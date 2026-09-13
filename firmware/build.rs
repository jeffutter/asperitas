use std::env;
use std::path::PathBuf;

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("memory.x"), include_str!("memory.x")).unwrap();

    // Without this, the copy written above is inert: nothing tells the linker
    // to look in OUT_DIR. The build still worked only because the linker also
    // searches the crate root, where memory.x happens to live. Emitting the
    // search path makes the copy actually authoritative, and keeps ours ahead
    // of the memory.x that embassy-stm32's `memory-x` feature generates.
    println!("cargo:rustc-link-search={}", out.display());

    // defmt 1.x puts one byte of every format string's metadata in its own `.defmt.<level>.<json>`
    // input section and consolidates them into a single `.defmt` output section with a linker
    // fragment that `defmt`'s own build script drops next to `-Tlink.x`. Without that fragment the
    // fragments never merge: the ELF carries ~107 loose one-byte `.defmt.*` sections and no
    // `.defmt`, and probe-rs 0.32 refuses the image outright — "Failed to parse defmt data /
    // defmt version found, but no `.defmt` section - check your linker configuration" — before it
    // even looks for a probe.
    //
    // Conditional rather than unconditional in `.cargo/config.toml` because the fragment also
    // emits an ALLOC-free `.defmt` section, which perturbs the layout of the console-only build we
    // promise is unchanged (`make build FEATURES="seed3"`). Linking it costs nothing at runtime:
    // the section is `(INFO)`, so it stays in the ELF where probe-rs reads it and out of flash.
    if env::var_os("CARGO_FEATURE_LOG_DEFMT").is_some() {
        println!("cargo:rustc-link-arg=-Tdefmt.x");
    }

    provenance();

    println!("cargo:rerun-if-changed=memory.x");
    // Printing any rerun-if directive opts out of the default "rerun if anything changed", so the
    // feature test above needs its own trigger or a feature flip would reuse the stale link line.
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_LOG_DEFMT");
}

/// Stamp every firmware ELF with the cargo cfg set it was linked against.
///
/// `target/thumbv7em-none-eabihf/release/main` is one name for two different images:
/// `scripts/gates.sh` builds the console and the RTT-only cfg sets back to back, and the top-level
/// name alternates between the two `deps/main-<hash>` artifacts it hardlinks. Neither the path nor
/// the mtime says which one is sitting there, and no host-side record can either -- those gates run
/// raw cargo, never `make`, so anything a recipe writes beside the ELF describes some other
/// artifact than the name currently holds. So the answer travels inside the image, where it cannot
/// disagree with the bytes it travels in.
///
/// The carrier is a non-allocated ARM note: `%note` with an empty flags field gives the section
/// VMA and LMA 0x0, exactly like defmt's own `.defmt`, so `objcopy -O binary` leaves it out of what
/// gets flashed and no KEEP fragment is needed for it to survive `-Tlink.x`, our `memory.x`
/// SECTIONS block and `-Tdefmt.x`.
///
/// Read by `scripts/elf-provenance.sh`, filed as TASK-062.02, which is the only thing in the repo
/// that parses this blob.
fn provenance() {
    // Section name, format tag and key order are defined here and nowhere else. The blob leads with
    // the tag so a later format change is detected by its absence instead of being misparsed.
    const SECTION: &str = ".asp.prov";
    const TAG: &str = "asp-prov1";

    let mut features: Vec<String> = env::vars_os()
        .filter_map(|(k, _)| k.into_string().ok())
        .filter_map(|k| {
            k.strip_prefix("CARGO_FEATURE_")
                .map(|f| f.to_lowercase().replace('-', "_"))
        })
        // Cargo reports the implicit `default` feature as CARGO_FEATURE_DEFAULT. The `default=`
        // line below already records it; leaving it in the list would state the fact twice.
        .filter(|feature| feature != "default")
        .collect();
    features.sort();

    let default = if env::var_os("CARGO_FEATURE_DEFAULT").is_some() {
        "1"
    } else {
        "0"
    };
    // Carried in the blob but deliberately not enforced by the reader: it selects which frames got
    // compiled in, not which cfg set this is. See the note above `elf-check` in firmware/Makefile.
    let defmt_log = env::var("DEFMT_LOG").unwrap_or_default();

    // One `format!` emits the section directive and the blob together so they cannot drift apart.
    // Two different kinds of newline here, and mixing them up breaks the build in a way that reads
    // like a tokeniser bug: the wrapper needs REAL newlines to be valid Rust source, while the blob
    // carries escaped `\n` pairs for the assembler to expand, so the generated file keeps its
    // `.asciz` on one line and the bytes in the ELF come out with real newlines.
    let body = format!(
        "{TAG}\\ndefault={default}\\nfeatures={}\\ndefmt_log={defmt_log}\\n",
        features.join(",")
    );
    let asm = format!(
        "core::arch::global_asm!(r#\"\n.section {SECTION}, \"\", %note\n.balign 4\n.asciz \"{body}\"\n\"#);\n"
    );

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("asp_pro.inc"), asm).unwrap();

    // Emitting any rerun-if directive opts out of cargo's default "rerun if anything changed", so a
    // value baked into the blob needs its own trigger or a stale one gets reused. Flipping features
    // needs none: that changes the unit's metadata hash, and cargo reruns the script regardless.
    println!("cargo:rerun-if-env-changed=DEFMT_LOG");
}
