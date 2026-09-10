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

    println!("cargo:rerun-if-changed=memory.x");
    // Printing any rerun-if directive opts out of the default "rerun if anything changed", so the
    // feature test above needs its own trigger or a feature flip would reuse the stale link line.
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_LOG_DEFMT");
}
