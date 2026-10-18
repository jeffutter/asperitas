//! Doc-consistency check: the layout table in the `excerpt` module's own rustdoc must quote the
//! module's constants. The constants own the numbers; this test fails when the prose is left behind.
//!
//! Run with: `cargo test -p asperitas-wire --test excerpt_docs`

use asperitas_wire::excerpt::{
    BOOTLOADER_END, EXCERPT_AREA_START, EXC_CHUNK_RAW, PCM_CAPACITY, RESERVED_SECTOR_START,
    SLOT_COUNT, SLOT_STRIDE,
};

const DOC: &str = include_str!("../src/excerpt.rs");

/// The module's `//!` header, up to the first line that is not a doc comment.
fn section() -> String {
    DOC.lines()
        .take_while(|l| l.starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n")
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
    let wanted = [
        format!("`0x000000..0x{BOOTLOADER_END:06X}`"),
        format!("`0x{EXCERPT_AREA_START:06X}"),
        format!("* 0x{SLOT_STRIDE:X}`"),
        format!("(0..{SLOT_COUNT})"),
        format!("`0x{RESERVED_SECTOR_START:X}..0x800000`"),
        format!("{} bytes", thousands(PCM_CAPACITY)),
        format!("is {EXC_CHUNK_RAW} rather"),
    ];
    for needle in &wanted {
        assert!(
            doc.contains(needle.as_str()),
            "the `excerpt` module docs no longer contain {needle:?}; update the layout prose to match \
             the constants"
        );
    }
}
