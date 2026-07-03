use rkyv::{Archive, Deserialize, Serialize};

/// One Nerd Font glyph as stored in the vendored rkyv snapshot. Only fields
/// that survive a round trip upstream are persisted; everything else (set
/// token, search aliases) is derived from `name` at runtime.
#[derive(Archive, Serialize, Deserialize)]
pub struct StoredGlyph {
    /// Canonical nf name, e.g. `nf-cod-folder`.
    pub name: String,
    /// Unicode codepoint of the glyph.
    pub codepoint: u32,
}

/// The runtime view of a glyph used throughout the app. `set` and `aliases`
/// are derived from `name` on demand — see [`Glyph::set`] and
/// [`Glyph::aliases`] — and are never persisted in the snapshot.
#[derive(Debug, Clone)]
pub struct Glyph {
    pub name: String,
    pub codepoint: u32,
}

impl StoredGlyph {
    /// Convert the on-disk form into the runtime [`Glyph`] by deriving the
    /// set token and any search aliases from the name.
    pub fn into_glyph(self) -> Glyph {
        Glyph {
            name: self.name,
            codepoint: self.codepoint,
        }
    }
}

/// Icon-set token from a canonical nf name, e.g. `nf-cod-folder` -> `"cod"`.
/// Returns the empty slice if the name has no `nf-` prefix or no `-` after it.
pub fn set_of(name: &str) -> &str {
    let rest = name.strip_prefix("nf-").unwrap_or(name);
    rest.split_once('-').map(|(s, _)| s).unwrap_or(rest)
}

/// Default search alias from a canonical nf name, e.g.
/// `nf-cod-left_hard_divider` -> `["left hard divider"]`. Returns an empty
/// `Vec` if there is no glyph descriptor after `nf-{set}-`, or if it would be
/// fully made of underscores (whose replacement is all whitespace).
pub fn aliases_of(name: &str) -> Vec<String> {
    let prefix_len = "nf-".len() + set_of(name).len() + 1;
    match name.get(prefix_len..) {
        Some(desc) if !desc.is_empty() => {
            let spaced = desc.replace('_', " ");
            if spaced.trim().is_empty() {
                Vec::new()
            } else {
                vec![spaced]
            }
        }
        _ => Vec::new(),
    }
}

impl Glyph {
    pub fn char(&self) -> Option<char> {
        char::from_u32(self.codepoint)
    }

    /// Icon-set token, derived from `name`.
    pub fn set(&self) -> &str {
        set_of(&self.name)
    }

    /// Search needle aliases, derived from `name` (currently always 0 or 1
    /// entries — the underscored descriptor with underscores collapsed to
    /// spaces).
    pub fn aliases(&self) -> Vec<String> {
        aliases_of(&self.name)
    }

    /// Lowercased strings the matcher searches across: the canonical name, the
    /// codepoint in lower hex, and any aliases.
    pub fn search_strings(&self) -> Vec<String> {
        let mut v = vec![self.name.to_lowercase(), format!("{:x}", self.codepoint)];
        v.extend(self.aliases().into_iter().map(|a| a.to_lowercase()));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_round_trip() {
        let g = Glyph {
            name: "nf-cod-folder".into(),
            codepoint: 0xf07d,
        };
        assert_eq!(g.char(), Some('\u{f07d}'));
    }

    #[test]
    fn invalid_codepoint_is_none() {
        let g = Glyph {
            name: "nf-bogus".into(),
            codepoint: 0xd801, // surrogate
        };
        assert_eq!(g.char(), None);
    }

    #[test]
    fn set_derived_from_name() {
        assert_eq!(set_of("nf-cod-folder"), "cod");
        assert_eq!(set_of("nf-md-cog"), "md");
        // split_once on the first '-' only — the Seti icon set is just "set",
        // not "set-i"; the upstream `seti-foo` icon name has a hyphen too.
        assert_eq!(set_of("nf-seti-foo"), "seti");
        assert_eq!(set_of("nf"), "nf"); // no prefix-split -> whole string
        assert_eq!(set_of("not-nf"), "not"); // falls back when no `nf-`
    }

    #[test]
    fn alias_collapses_underscores() {
        let v = aliases_of("nf-cod-left_hard_divider");
        assert_eq!(v, vec!["left hard divider".to_string()]);
    }

    #[test]
    fn alias_simple_uses_suffix_as_is() {
        // No underscores → suffix is the alias directly.
        assert_eq!(aliases_of("nf-cod-folder"), vec!["folder".to_string()]);
        assert_eq!(aliases_of("nf-md-cog"), vec!["cog".to_string()]);
    }

    #[test]
    fn alias_empty_when_only_set() {
        // No descriptor after `nf-{set}-` → empty alias list.
        let v = aliases_of("nf-cod-");
        assert!(v.is_empty(), "got {v:?}");
        // And when the suffix is only underscores, the spaced form is empty.
        let v = aliases_of("nf-cod-___");
        assert!(v.is_empty(), "got {v:?}");
    }

    #[test]
    fn stored_glyph_into_glyph_strips_fields() {
        let s = StoredGlyph {
            name: "nf-cod-folder".into(),
            codepoint: 0xf07d,
        };
        let g = s.into_glyph();
        assert_eq!(g.name, "nf-cod-folder");
        assert_eq!(g.codepoint, 0xf07d);
        assert_eq!(g.set(), "cod");
    }
}
