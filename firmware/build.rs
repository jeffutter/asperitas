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

    println!("cargo:rerun-if-changed=memory.x");
}
