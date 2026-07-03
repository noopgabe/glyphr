//! Generates `data/glyphs.bin` from a pinned Nerd Fonts release.
//!
//! The main binary stays fully offline; this is a manual build/dev tool, run with
//! network access to refresh the committed snapshot:
//!
//! ```text
//! cargo run --release --bin gen-snapshot -- --release <tag> -o data/glyphs.bin
//! ```
//!
//! On disk, each entry is just `[name, codepoint]`. The icon-set token and any
//! search aliases are derived from `name` at runtime by [`crate::glyph::Glyph`].

use std::fs;
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use rkyv::rancor::Error as RkyvError;
use serde_json::Value;

use glyphr::glyph::StoredGlyph;

/// Relative path within the nerd-fonts repo used to build the snapshot. The
/// generator parses the object keyed by icon name that nerd-fonts commits at
/// the repository root.
const DEFAULT_SOURCE_PATH: &str = "glyphnames.json";

#[derive(Parser)]
#[command(
    version,
    about = "Regenerate glyphr's glyph snapshot from a pinned Nerd Fonts release"
)]
struct Args {
    /// Pinned nerd-fonts release tag, e.g. `v3.4.0`.
    #[arg(long, default_value = "v3.4.0")]
    release: String,
    /// Override the upstream JSON URL entirely.
    #[arg(long)]
    url: Option<String>,
    /// Read upstream JSON from a local file instead of fetching.
    #[arg(long)]
    input: Option<String>,
    /// Output path (relative to the cwd).
    #[arg(short, long, default_value = "data/glyphs.bin")]
    out: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let url = args.url.unwrap_or_else(|| {
        format!(
            "https://raw.githubusercontent.com/ryanoasis/nerd-fonts/{}/{}",
            args.release, DEFAULT_SOURCE_PATH
        )
    });

    let raw = match &args.input {
        Some(p) => fs::read_to_string(p).with_context(|| format!("read {p}"))?,
        None => {
            let out = Command::new("curl")
                .args(["-fsSL", &url])
                .output()
                .context("failed to run curl")?;
            if !out.status.success() {
                return Err(anyhow!(
                    "curl failed ({}): {}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
            String::from_utf8(out.stdout).context("upstream was not valid UTF-8")?
        }
    };

    let value: Value = serde_json::from_str(&raw).context("upstream was not valid JSON")?;
    let glyphs = normalize(&value)?;
    if glyphs.is_empty() {
        return Err(anyhow!(
            "normalized glyph set is empty; check the upstream schema (url={url})"
        ));
    }

    if let Some(parent) = std::path::Path::new(&args.out).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).ok();
        }
    }
    let bytes = rkyv::to_bytes::<RkyvError>(&glyphs).context("failed to encode rkyv snapshot")?;
    fs::write(&args.out, &bytes).with_context(|| format!("write {}", args.out))?;
    println!(
        "wrote {} glyphs ({} bytes) to {} (source: {url})",
        glyphs.len(),
        bytes.len(),
        args.out
    );
    Ok(())
}

/// Pull just `name` and `codepoint` out of upstream. The icon-set token
/// (`"cod"`, `"md"`, etc.) and the underscored descriptor alias are both
/// recoverable from the canonical nf-name at runtime.
fn normalize(value: &Value) -> Result<Vec<StoredGlyph>> {
    let obj = value
        .as_object()
        .ok_or_else(|| anyhow!("upstream glyphnames JSON must be an object"))?;

    let mut out = Vec::with_capacity(obj.len());
    for (key, item) in obj {
        if key == "METADATA" {
            continue;
        }
        let name = format!("nf-{key}");
        let codepoint = parse_codepoint(item)?;
        out.push(StoredGlyph { name, codepoint });
    }
    Ok(out)
}

fn parse_codepoint(item: &Value) -> Result<u32> {
    if let Some(i) = item.get("codepoint").and_then(|v| v.as_u64()) {
        return Ok(i as u32);
    }
    let s = item
        .get("codepoint")
        .or_else(|| item.get("code"))
        .or_else(|| item.get("hex"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("glyph item missing codepoint"))?;
    let t = s
        .trim()
        .trim_start_matches("U+")
        .trim_start_matches("u+")
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    u32::from_str_radix(t, 16).map_err(|e| anyhow!("bad hex `{s}`: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_object_schema() {
        let v = json!({
            "METADATA": { "version": "3.4.0" },
            "cod-folder": { "char": "", "code": "ea83" },
            "fa-cogs": { "char": "", "code": "f085", "aliases": ["cogs", "settings"] }
        });
        let mut g = normalize(&v).unwrap();
        g.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(g.len(), 2);
        // nf-cod-folder sorts before nf-fa-cogs.
        assert_eq!(g[0].name, "nf-cod-folder");
        assert_eq!(g[0].codepoint, 0xea83);
        assert_eq!(g[1].name, "nf-fa-cogs");
        assert_eq!(g[1].codepoint, 0xf085);
    }

    #[test]
    fn skips_metadata_key() {
        let v = json!({ "METADATA": { "version": "1" }, "dev-git": { "code": "e702" } });
        let g = normalize(&v).unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].name, "nf-dev-git");
        assert_eq!(g[0].codepoint, 0xe702);
    }

    #[test]
    fn rejects_non_object_upstream() {
        assert!(normalize(&json!([{ "name": "nf-x", "code": "e000" }])).is_err());
    }

    #[test]
    fn empty_object_yields_empty_vec() {
        // empty input normalizes to an empty Vec (NOT an error); the CLI
        // rejects empty results separately via the `is_empty()` guard in main.
        assert!(normalize(&json!({})).unwrap().is_empty());
    }

    #[test]
    fn plane16_codepoint_parsed() {
        // Material Design icons live in the supplementary plane, e.g. U+F024B.
        let v = json!({ "md-folder": { "code": "f024b" } });
        let g = normalize(&v).unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].codepoint, 0xf024b);
    }
}
