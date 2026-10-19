use std::env;
use std::path::PathBuf;

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    // Two link layouts share the one file name the linker looks for. `internal` (the default) is the
    // 128 KB internal flash that DFU-to-0x08000000 loads; `bootloaded` is the AXI-SRAM image the
    // stock Daisy bootloader copies out of QSPI (memory-bootloaded.x). Selected per build with
    // ASPERITAS_LAYOUT, which `make LAYOUT=...` sets.
    let layout = env::var("ASPERITAS_LAYOUT").unwrap_or_default();
    let memory_x = match layout.as_str() {
        "" | "internal" => include_str!("memory-internal.x"),
        "bootloaded" => include_str!("memory-bootloaded.x"),
        other => panic!("ASPERITAS_LAYOUT={other:?}: expected \"internal\" or \"bootloaded\""),
    };
    let layout = if layout.is_empty() {
        "internal"
    } else {
        layout.as_str()
    };
    std::fs::write(out.join("memory.x"), memory_x).unwrap();
    // Reaching the compiler as an env var makes the layout part of the crate's fingerprint, so
    // flipping it relinks the binary even though no source file changed.
    println!("cargo:rustc-env=ASPERITAS_LAYOUT={layout}");
    println!("cargo:rerun-if-env-changed=ASPERITAS_LAYOUT");

    // Without this, the copy written above is inert: nothing tells the linker
    // to look in OUT_DIR. The build still worked only because the linker also
    // searches the crate root. Emitting the search path makes the copy actually authoritative, and
    // keeps ours ahead of the memory.x that embassy-stm32's `memory-x` feature generates.
    //
    // The crate root must therefore hold NO file named memory.x: measured with the layouts split,
    // lld resolves `INCLUDE memory.x` against the working directory before any -L path, so a
    // root-level memory.x silently beat the selected layout (an image stamped bootloaded that linked
    // for internal flash). The two layouts are memory-internal.x and memory-bootloaded.x for that reason.
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

    provenance(layout);
    version();
    rig_flash_ban();

    println!("cargo:rerun-if-changed=memory-internal.x");
    println!("cargo:rerun-if-changed=memory-bootloaded.x");
    // Printing any rerun-if directive opts out of the default "rerun if anything changed", so the
    // feature test above needs its own trigger or a feature flip would reuse the stale link line.
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_LOG_DEFMT");
}

/// Embed in every firmware ELF with the cargo cfg set it was linked against.
///
/// The record travels inside the image, where it cannot disagree with the bytes it travels in: no
/// host-side record can describe an artifact that was built by raw cargo, as the gates do, rather
/// than by a `make` recipe that could write something beside the ELF.
///
/// The carrier is a non-allocated ARM note: `%note` with an empty flags field gives the section
/// VMA and LMA 0x0, exactly like defmt's own `.defmt`, so `objcopy -O binary` leaves it out of what
/// gets flashed and no KEEP fragment is needed for it to survive `-Tlink.x`, our `memory.x`
/// SECTIONS block and `-Tdefmt.x`.
///
/// The blob also carries `source_digest`, the digest of the inputs listed in `elf-inputs.manifest`
/// (see `digest_inputs`), so the record of which sources built the image travels with it.
///
/// Read by `scripts/elf-provenance.sh`, which is the only thing in the repo that parses this blob.
fn provenance(layout: &str) {
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
    let source_digest = digest_inputs(&["digest"]);
    let body = format!(
        "{TAG}\\ndefault={default}\\nfeatures={}\\ndefmt_log={defmt_log}\\nlayout={layout}\\nsource_digest={source_digest}\\n",
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

    // The digest is a function of files, so each one needs a trigger, and so does every directory
    // that contains one: a file that did not exist at the last link is named by no earlier directive,
    // but adding it moves its directory's mtime. The set is listed by the same script that computed
    // the digest, from the same manifest, so the triggers and the digest cannot disagree. The script
    // and manifest are inputs to the answer too.
    for path in digest_inputs(&["list"]).lines() {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-changed=elf-inputs.manifest");
    println!("cargo:rerun-if-changed=../scripts/elf-inputs-digest.sh");
}

/// Run `scripts/elf-inputs-digest.sh` and return its trimmed stdout. The input set is defined in
/// `elf-inputs.manifest` and implemented once, in that script, which the Makefile and the elf-check
/// side also call; re-implementing it here in Rust would be the second hand-kept copy that can drift
/// from the checker. A failure stops the build: an ELF carrying a guessed digest is worse than
/// none.
fn digest_inputs(args: &[&str]) -> String {
    let script = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("../scripts/elf-inputs-digest.sh");
    let out = std::process::Command::new("bash")
        .arg(&script)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("cannot run {}: {e}", script.display()));
    if !out.status.success() {
        panic!(
            "{} {} failed: {}",
            script.display(),
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// Export `ASPERITAS_VERSION` for `CTL VER`: the package version, plus `git describe` when a
/// repository and git are available. Wire-safe by construction (printable, no space, at most 32
/// bytes) because `control::VersionText` rejects anything else and the reply is useless then.
fn version() {
    let pkg = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let describe = std::process::Command::new("git")
        .args(["describe", "--always", "--dirty", "--tags"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let mut v = match describe {
        Some(d) => format!("{pkg}+{d}"),
        None => pkg,
    };
    v.retain(|c| c.is_ascii_graphic());
    v.truncate(32);
    println!("cargo:rustc-env=ASPERITAS_VERSION={v}");
    // HEAD and its reflog move on commit/checkout; the index/worktree state behind `-dirty` has no cheap trigger,
    // so a dirty marker can lag until the next build that reruns this script.
    // Only paths that exist: cargo reruns a script every build for a rerun-if-changed path that is
    // missing (a worktree's `.git` is a file, a source tarball has none).
    for p in ["../.git/HEAD", "../.git/logs/HEAD"] {
        if std::path::Path::new(p).exists() {
            println!("cargo:rerun-if-changed={p}");
        }
    }
}

/// Fail the build if any source under `src/bin/rig/` calls daisy-embassy's blocking flash API.
///
/// The blocking `read` / `write` / `erase` (and `read_uuid`) spin on the chip's status register with
/// no bound, and `FlashBuilder::build` is the only way to get a handle that offers nothing else. rig
/// runs a measurement whose console must keep talking, so it uses the async calls only; the reasons
/// are in `src/bin/rig/install.rs`. A type cannot enforce that - `Flash<Async>` still carries
/// the blocking methods - so this does it by text, on every firmware build and clippy run, which is
/// every tier of `scripts/gates.sh` and CI.
///
/// Line comments are stripped first, so prose may name the banned calls. The patterns are broad on
/// purpose: rig has no other `.read(` / `.write(` / `.erase(` receiver today, and a future one that
/// trips this is cheaper to rename than a hang is to find at the bench.
fn rig_flash_ban() {
    const RIG_DIR: &str = "src/bin/rig";
    const BANNED: [&str; 6] = [
        ".read(",
        ".write(",
        ".erase(",
        ".read_uuid(",
        ".build()",
        "FlashBuilder::build(",
    ];
    println!("cargo:rerun-if-changed={RIG_DIR}");
    let mut sources: Vec<_> = std::fs::read_dir(RIG_DIR)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    sources.sort();
    for path in sources {
        println!("cargo:rerun-if-changed={}", path.display());
        let source = std::fs::read_to_string(&path).unwrap();
        for (n, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if let Some(pattern) = BANNED.iter().find(|pattern| code.contains(**pattern)) {
                panic!(
                    "{}:{}: `{pattern}` - rig may only use daisy-embassy's async flash API \
                     (read_async / write_async / erase_async on a build_async handle); see \
                     rig_flash_ban in firmware/build.rs",
                    path.display(),
                    n + 1
                );
            }
        }
    }
}
