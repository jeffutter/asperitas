//! Doc-consistency check: the layout table in `docs/reference/daisy-seed3.md` ("Instrument excerpts
//! in QSPI") must quote the `excerpt` module's constants. The module owns the numbers; this test
//! fails when the doc is left behind.
//!
//! Run with: `cargo test -p asperitas-wire --test excerpt_docs`

use asperitas_wire::excerpt::{
    slot_base, BOOTLOADER_END, EXCERPT_AREA_START, EXC_CHUNK_RAW, HEADER_LEN, PCM_CAPACITY,
    RESERVED_SECTOR_START, SLOT_COUNT, SLOT_STRIDE,
};

const DOC: &str = include_str!("../../../docs/reference/daisy-seed3.md");

/// The excerpt section, from its heading to the next level-2 heading.
fn section() -> &'static str {
    let start = DOC
        .find("\n## Instrument excerpts in QSPI\n")
        .expect("doc has the 'Instrument excerpts in QSPI' section");
    let rest = &DOC[start + 1..];
    let end = rest[3..].find("\n## ").map_or(rest.len(), |i| i + 3);
    &rest[..end]
}

fn thousands(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[test]
fn layout_table_quotes_the_module_constants() {
    let doc = section();
    let last = slot_base(SLOT_COUNT - 1).expect("last slot exists");
    let wanted = [
        format!("`0x{BOOTLOADER_END:X}`"),
        format!("`0x{EXCERPT_AREA_START:X}`"),
        format!("`0x{SLOT_STRIDE:X}`"),
        format!("{SLOT_COUNT} slots"),
        format!("`0x{last:X}`"),
        format!("`0x{RESERVED_SECTOR_START:X}`"),
        format!("{} B per slot", thousands(PCM_CAPACITY)),
        format!("{HEADER_LEN} B used"),
        format!("{EXC_CHUNK_RAW} raw bytes"),
    ];
    for needle in &wanted {
        assert!(
            doc.contains(needle.as_str()),
            "docs/reference/daisy-seed3.md section 'Instrument excerpts in QSPI' no longer \
             contains {needle:?}; update the table to match crates/asperitas-wire/src/excerpt.rs"
        );
    }
}
