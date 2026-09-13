---
id: TASK-062.01
title: >-
  Stamp every firmware ELF with its cfg set: a non-allocated .asp.prov note
  section written by build.rs
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-13 12:54'
updated_date: '2026-09-13 13:09'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-062
priority: medium
ordinal: 114800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Producer half of TASK-062: make every firmware ELF carry the cargo cfg set it was linked with, inside the ELF itself, in a section that costs no flash and is invisible to the board. Nothing reads it yet - TASK-062.02 files the reader.

Mechanism (measured end-to-end on a copy of this tree in /tmp/pl62fw, rust/cargo 1.97.1, llvm-objcopy 22.1.6): firmware/build.rs generates a second OUT_DIR artifact, asp_pro.inc, containing one global_asm! that opens a NON-ALLOCATED ARM note section (.section .asp.prov, "", %note) and emits a short key=value blob assembled from CARGO_FEATURE_*, the default-features state, and DEFMT_LOG. Each of the six firmware/src/bin/*.rs gains exactly one line: include!(concat!(env!("OUT_DIR"), "/asp_pro.inc"));. There are no [[bin]] stanzas to edit - the six bins are auto-discovered from src/bin/.

Why the provenance rides in the ELF and not in a host-side record: scripts/gates.sh:235 and :241-242 build both cfg sets with RAW cargo, never through make, and whichever ran last is what target/thumbv7em-none-eabihf/release/main names (they are hardlinks to different deps/main-<hash>). Anything written by a make recipe therefore describes some other artifact than the one the name currently holds. A section written at link time cannot disagree with the bytes it travels in, survives cargo clean of the sibling cfg set, and needs no assumption about cargo's private layout.

Rejected, with reasons: (a) dual [[bin]] targets with required-features, because it breaks the six-bin inventory, scripts/check-doc-artifact-names.sh's doc-name gate and several quoted error texts; (b) a sidecar next to the ELF, per the paragraph above; (c) reading .fingerprint/*/bin-main.json, which works today (research matched the top-level ELF against deps/main-* and correctly reported ["log-defmt","seed3"]) but pins us to a cargo-private schema and stops working the moment the artifact is copied or the cache pruned. Keep it as a diagnostic, not as the mechanism.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All six release ELFs (blinky ledtest main panictest podtest rig) carry a .asp.prov section whose VMA and LMA are 0x0, i.e. non-allocated exactly like .defmt. Evidence is the rust-objdump -h section table pasted for all six, not a claim.
- [ ] #2 rust-objcopy --dump-section .asp.prov=<out> <elf> yields a blob that differs between the two cfg sets, in the canonical form asp-prov1 / default=<0|1> / features=<sorted comma list, lowercased, hyphens to underscores, the implicit "default" feature omitted because it is already the default= field> / defmt_log=<verbatim>. cargo build --release --features seed3 must report default=1 features=log_usb,seed3 and --no-default-features --features "seed3 log-defmt" must report default=0 features=log_defmt,seed3. Both were measured on a patched copy of this tree before this ticket existed (the raw dump there also listed default inside the feature list; drop that duplicate).
- [ ] #3 Flash cost is a measurement, not a promise. Record .text and .rodata size plus $(BINARY).bin byte count for all six bins before and after. Pre-change baseline measured in /tmp/pl62fw: five bins byte-for-byte unchanged, main grew .text from 0x11cf4 to 0x11d90 (+156 B) because the asm blob is one more input object, taking main.bin from 88,613 to 88,773 bytes. Land only with every bin under +512 B and the numbers in the notes; if main grows more than that, try the linker-fragment route in the notes before landing anything.
- [ ] #4 Boardless proof that probe-rs still parses the image: make probe-rtt-list FEATURES="seed3 log-defmt" NO_DEFAULT=1 with no board attached ends at "No connected probes were found", NOT at an ELF parse error. Paste stderr and rc. Measured once already on a provenance-bearing RTT-only ELF.
- [ ] #5 make -n build flash flash-all check stays byte-identical to HEAD (the DFU guard TASK-053 set and TASK-056 restates), and scripts/gates.sh ci is green.
- [ ] #6 Section name, format tag and key order are defined in exactly one place in firmware/build.rs, the blob starts with an asp-prov1 tag so a future format change is detectable by its absence, and the comment there names the reader (TASK-062.02) rather than restating what the reader does.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Follow the measured recipe in Implementation Notes - it was run against a copy of this tree and produced the two distinct blobs, so there is nothing left to discover.

1. Add provenance() to firmware/build.rs beside the existing memory.x write, call it from main() before the rerun-if lines, and emit cargo:rerun-if-env-changed=DEFMT_LOG inside it. Keep the generated text in one format! call so the section directive and the blob cannot drift apart.
2. Add the single include! line to each of the six src/bin/*.rs, after the #![...] attributes. No Cargo.toml change: the bins are auto-discovered.
3. Build both cfg sets, then take AC #1/#2/#3 evidence: rust-objdump -h for all six, the two dumps, and the before/after size table. Take the "before" numbers from a pristine checkout build FIRST - comparing a patched ELF against an ELF of the other cfg set looks like a 156 KB regression and is not one.
4. Take AC #4 with no board attached. It is a parse test, not a flash test: probe-rs must reach "No connected probes were found".
5. Prove AC #5 by diffing make -n output against git show HEAD:firmware/Makefile expansion, the way TASK-053 recorded it, then run scripts/gates.sh ci.
6. Commit with the size table in the commit body. Do not touch elf-check, the Makefile, gates.sh or any doc - the reader is TASK-062.02 and prose that describes the reader belongs there.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Measured recipe (verified working in /tmp/pl62fw against this tree; nothing in the repo was touched)

build.rs appends, next to the existing memory.x write and the conditional -Tdefmt.x at build.rs:28
(the note in the research brief said line 81 - wrong, it is 28):

    fn provenance() {
        let mut feats: Vec<String> = std::env::vars_os()
            .filter_map(|(k, _)| k.into_string().ok())
            .filter_map(|k| k.strip_prefix("CARGO_FEATURE_")
                     .map(|f| f.to_lowercase().replace('-', "_")))
            .collect();
        feats.sort();
        let default = if env::var_os("CARGO_FEATURE_DEFAULT").is_some() { "1" } else { "0" };
        let defmt_log = env::var("DEFMT_LOG").unwrap_or_default();
        // key=value lines, NUL terminated, leading format tag
        let body = format!("asp-prov1\ndefault={default}\nfeatures={}\ndefmt_log={defmt_log}\n", feats.join(","));
        let out = std::env::var("OUT_DIR").unwrap();
        std::fs::write(format!("{out}/asp_pro.inc"),
            format!("core::arch::global_asm!(r#\"\n.section .asp.prov, \"\", %note\n.balign 4\n.asciz \"{body}\"\n\"#);\n"))
            .unwrap();
        println!("cargo:rerun-if-env-changed=DEFMT_LOG");
    }

The sketch leaves the implicit default token inside feats; drop it there so default= stays the only
place defaults are recorded, which is what AC #2 asks for.

Add one call to it from main(), plus one include! line in each of the six src/bin/*.rs, placed
after the #![...] attribute block.

## Gotchas, all measured here

- ARM wants %note, not @note, for the section type in the third operand. With an empty flags field
  the section comes out non-allocated: rust-objdump -h shows VMA/LMA 0x0 right beside .defmt.
- No KEEP fragment is needed. The section survives cortex-m-rt's -Tlink.x, our memory.x SECTIONS
  block and -Tdefmt.x untouched. If a future toolchain drops it, defmt's own route is available
  and is already proven twice in this file: write OUT_DIR/asp-prov.x containing
  `.asp_prov (INFO) : { KEEP(*(SORT(.asp.prov*))) }` and emit cargo:rustc-link-arg=-Tasp-prov.x.
  build.rs:13 already puts OUT_DIR on the linker search path, and memory.x:33-36 records that
  link.x performs exactly one INCLUDE memory.x, which is why a second fragment needs its own -T.
  That variant is ALLOC-free too but costs an extra link arg; do not reach for it pre-emptively.
- build.rs emits rerun-if directives today (build.rs:31-34), and emitting ANY of them opts out of
  cargo's default change detection, hence the DEFMT_LOG directive above. Flipping features does not
  need one: a feature flip changes the unit's metadata hash and cargo reruns the script anyway
  (three distinct OUT_DIRs with three distinct blobs observed).
- LOG_CHANNEL does not exist in this repository - grep finds zero hits, including in backlog and
  docs. The channel is selected by log-usb / log-defmt plus --no-default-features, so it arrives as
  CARGO_FEATURE_LOG_USB / CARGO_FEATURE_LOG_DEFMT. Nothing to read for it. Do not implement a
  LOG_CHANNEL axis.
- Host read must be rust-objcopy --dump-section .asp.prov=out FILE. rust-readelf and readelf are
  NOT in the dev shell (flake.nix:33-59 installs cargo-binutils + llvm-tools-preview); bare
  llvm-objcopy is not on PATH either. On LLVM 22 the dump needs no dummy output-file argument.
  Never use `-O binary --only-section` for this: it prints nothing for a non-alloc section, and
  scripts/check-doc-artifact-names.sh rule R3 forbids --only-section in the build recipe anyway.
- Prose about the dump must spell it `rust-objcopy`: rule R2 of
  scripts/check-doc-artifact-names.sh fails any `cargo objcopy` token in README.md or docs/**.
- Symbol heuristics are traps, which is why an explicit string is worth its 156 bytes: .defmt
  presence really does imply log-defmt (without -Tdefmt.x there is no consolidated .defmt at all),
  but embassy_usb_synopsys_otg symbols appear even in the --no-default-features build because
  daisy-embassy's seed3 pulls the USB stack independently, and nothing in the symbol table
  distinguishes seed3, slow-boot or stim-* at all.
- One build-script run per package covers all six bins, so the blob cannot vary per binary within
  one cargo invocation. Fine here, but it means `--bin main` and `--bins` never disagree.
<!-- SECTION:NOTES:END -->
