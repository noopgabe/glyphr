use std::sync::LazyLock;

use rkyv::rancor::Error as RkyvError;
use rkyv::util::AlignedVec;

use crate::glyph::{Glyph, StoredGlyph};

/// The full glyph set, embedded from `data/glyphs.bin` at compile time and
/// materialized once on first use. Fully offline at runtime.
pub static GLYPHS: LazyLock<Vec<Glyph>> = LazyLock::new(|| {
    // `include_bytes!` places the data at a static offset inside the .rodata
    // section, whose alignment is not guaranteed to satisfy rkyv's
    // byte-checker (it needs >=4-byte alignment). Copy into an aligned
    // backing buffer before decoding.
    let raw: &[u8] = include_bytes!("../data/glyphs.bin");
    let mut aligned = AlignedVec::<16>::with_capacity(raw.len());
    aligned.extend_from_slice(raw);
    let stored: Vec<StoredGlyph> =
        rkyv::from_bytes::<Vec<StoredGlyph>, RkyvError>(&aligned)
            .expect("data/glyphs.bin is not a valid rkyv snapshot");
    stored.into_iter().map(StoredGlyph::into_glyph).collect()
});

pub fn glyphs() -> &'static [Glyph] {
    &GLYPHS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn snapshot_loads_and_is_well_formed() {
        let g = glyphs();
        assert!(!g.is_empty(), "snapshot is empty");
        let mut seen = HashSet::new();
        for x in g {
            assert!(!x.name.is_empty(), "empty name");
            assert!(
                char::from_u32(x.codepoint).is_some(),
                "invalid codepoint 0x{:x} for {}",
                x.codepoint,
                x.name
            );
            assert!(seen.insert(&x.name), "duplicate name {}", x.name);
        }
    }
}
