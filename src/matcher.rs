use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Matcher, Utf32Str};

use crate::glyph::Glyph;

/// One match result returned from [`filter`]. Holds a glyph index plus the
/// character positions (in `glyph.name`) that the search needle matched, so the
/// UI can highlight them without re-running the matcher.
#[derive(Debug, Clone)]
pub struct Hit {
    pub glyph_idx: usize,
    pub name_indices: Vec<u32>,
}

/// Split a raw query like `"md>folder"` into a (set prefix, search needle)
/// pair. The split happens on the first `>`. Both halves are trimmed and the
/// prefix is lowercased. An empty normalized prefix becomes `None`; an empty
/// needle stays a `String`.
pub fn split_query(q: &str) -> (Option<String>, String) {
    let t = q.trim();
    let (raw_left, raw_right) = match t.find('>') {
        Some(i) => (Some(&t[..i]), &t[i + 1..]),
        None => (None, t),
    };
    let left = raw_left
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase());
    let right = raw_right.trim();
    (left, right.to_string())
}

/// Build the filtered, score-ordered list of glyph hits for `query`.
///
/// Recognizes a leading `set>` prefix and restricts candidates to glyphs whose
/// `set` starts with that prefix (case-insensitive). The remainder of the
/// query is fuzzy-matched against the canonical name, the hex codepoint, and
/// every alias.
///
/// An empty/blank query returns every glyph in natural (snapshot) order with no
/// scoring and no highlight indices.
pub fn filter(glyphs: &[Glyph], query: &str, matcher: &mut Matcher) -> Vec<Hit> {
    let (set_prefix, search) = split_query(query);
    let needle_text = search.trim();

    if set_prefix.is_none() && needle_text.is_empty() {
        return (0..glyphs.len())
            .map(|i| Hit {
                glyph_idx: i,
                name_indices: Vec::new(),
            })
            .collect();
    }

    let pattern = Pattern::parse(needle_text, CaseMatching::Smart, Normalization::Smart);

    let mut scored: Vec<(usize, u32, Vec<u32>)> = Vec::new();
    for (i, g) in glyphs.iter().enumerate() {
        if let Some(p) = &set_prefix {
            if !g.set().to_ascii_lowercase().starts_with(p.as_str()) {
                continue;
            }
        }
        let mut best: Option<u32> = None;
        for s in g.search_strings() {
            if let Some(sc) = pattern.score(Utf32Str::Ascii(s.as_bytes()), matcher) {
                best = Some(best.map_or(sc, |b| u32::max(b, sc)));
            }
        }
        let Some(score) = best else { continue };

        // Highlight positions on the canonical name. Lowercase the needle so
        // the case-insensitive `fuzzy_indices`-from-`Config::DEFAULT` returns
        // characters that always correspond to a real substring of `name`.
        let mut name_idx = Vec::new();
        if !needle_text.is_empty() {
            let chars: Vec<char> = g.name.chars().collect();
            matcher.fuzzy_indices(
                Utf32Str::Unicode(&chars),
                Utf32Str::Ascii(needle_text.to_lowercase().as_bytes()),
                &mut name_idx,
            );
        }
        scored.push((i, score, name_idx));
    }
    // Higher score first; ties broken by name length then name for stability.
    scored.sort_by(|(ia, sa, _), (ib, sb, _)| {
        sb.cmp(sa)
            .then_with(|| glyphs[*ia].name.len().cmp(&glyphs[*ib].name.len()))
            .then_with(|| glyphs[*ia].name.cmp(&glyphs[*ib].name))
    });
    scored
        .into_iter()
        .map(|(i, _, name_indices)| Hit {
            glyph_idx: i,
            name_indices,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glyph::Glyph;
    use nucleo_matcher::Config;

    fn fx() -> Vec<Glyph> {
        vec![
            Glyph {
                name: "nf-cod-folder".into(),
                codepoint: 0xf07d,
            },
            Glyph {
                name: "nf-fa-cogs".into(),
                codepoint: 0xf085,
            },
            Glyph {
                name: "nf-dev-github".into(),
                codepoint: 0xe70e,
            },
            Glyph {
                name: "nf-md-cog".into(),
                codepoint: 0xf0493,
            },
        ]
    }

    fn fresh() -> Matcher {
        Matcher::new(Config::DEFAULT)
    }

    #[test]
    fn empty_returns_all() {
        let v = filter(&fx(), "", &mut fresh());
        assert_eq!(v.len(), 4);
        assert!(v.iter().all(|h| h.name_indices.is_empty()));
    }

    #[test]
    fn whitespace_returns_all() {
        let v = filter(&fx(), "   ", &mut fresh());
        assert_eq!(v.len(), 4);
    }

    #[test]
    fn name_match_findable() {
        let v = filter(&fx(), "folder", &mut fresh());
        let idxs: Vec<usize> = v.iter().map(|h| h.glyph_idx).collect();
        assert!(idxs.contains(&0), "{idxs:?}");
    }

    #[test]
    fn codepoint_match_findable() {
        let g = &fx()[0];
        let hex = format!("{:x}", g.codepoint).to_ascii_lowercase();
        let v = filter(&fx(), &hex, &mut fresh());
        assert!(v.iter().any(|h| h.glyph_idx == 0));
    }

    #[test]
    fn alias_match_findable() {
        let v = filter(&fx(), "github", &mut fresh());
        assert!(v.iter().any(|h| h.glyph_idx == 2));
    }

    #[test]
    fn no_match_returns_empty() {
        let v = filter(&fx(), "zzzznotathing", &mut fresh());
        assert!(v.is_empty());
    }

    #[test]
    fn match_highlight_in_name() {
        let v = filter(&fx(), "folder", &mut fresh());
        let hit = v.iter().find(|h| h.glyph_idx == 0).unwrap();
        // "folder" should fall within `nf-cod-folder` (chars indices 7..13).
        let max = fx()[0].name.chars().count();
        assert!(hit.name_indices.iter().all(|&i| (i as usize) < max));
        // We don't pin the exact positions (nucleo may reorder across versions);
        // every idx must be in the "folder" substring.
        for &i in &hit.name_indices {
            let c = fx()[0].name.chars().nth(i as usize).unwrap();
            assert!(
                "folder".contains(c),
                "index {i} -> {c:?} not in 'folder'",
            );
        }
    }

    #[test]
    fn split_query_recognizes_set_prefix() {
        assert_eq!(split_query("md>folder"), (Some("md".into()), "folder".into()));
        assert_eq!(split_query("  fa >  cog  "), (Some("fa".into()), "cog".into()));
        assert_eq!(split_query(">folder"), (None, "folder".into()));
        assert_eq!(split_query("md>"), (Some("md".into()), "".into()));
        assert_eq!(split_query("folder"), (None, "folder".into()));
        assert_eq!(split_query(""), (None, "".into()));
    }

    #[test]
    fn set_prefix_restricts_candidates() {
        let v = filter(&fx(), "md>", &mut fresh());
        let idxs: Vec<usize> = v.iter().map(|h| h.glyph_idx).collect();
        assert_eq!(idxs, vec![3]);

        let v = filter(&fx(), "md>cog", &mut fresh());
        let idxs: Vec<usize> = v.iter().map(|h| h.glyph_idx).collect();
        assert_eq!(idxs, vec![3]);
        let hit = &v[0];
        // The matched indices should cover the "cog" tail (positions 7..10).
        for &i in &hit.name_indices {
            let c = fx()[3].name.chars().nth(i as usize).unwrap();
            assert!("cog".contains(c), "index {i} -> {c:?} not in 'cog'");
        }
    }

    #[test]
    fn set_prefix_starts_with_match() {
        let g = vec![Glyph {
            name: "nf-seti-folder".into(),
            codepoint: 0xe7eb,
        }];
        let v = filter(&g, "set>folder", &mut fresh());
        assert!(v.iter().any(|h| h.glyph_idx == 0));
    }

    #[test]
    fn unmatched_set_prefix_is_empty() {
        let v = filter(&fx(), "pl>folder", &mut fresh());
        assert!(v.is_empty());
    }
}
