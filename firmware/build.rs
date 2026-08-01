use std::env;
use std::path::PathBuf;

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("memory.x"), include_str!("memory.x")).unwrap();
    println!("cargo:rerun-if-changed=memory.x");
}
