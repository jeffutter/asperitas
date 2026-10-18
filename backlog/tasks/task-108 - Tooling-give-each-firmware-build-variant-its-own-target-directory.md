---
id: TASK-108
title: 'Tooling: give each firmware build variant its own target directory'
status: Blocked
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-18 21:00'
labels:
  - review-2026-10
  - tooling
  - firmware
  - planned
dependencies:
  - TASK-123
priority: medium
type: chore
ordinal: 210800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The console, RTT-only and bootloaded builds all write to target/thumbv7em-none-eabihf/release/<bin>. Machinery exists only to cope: gates.sh ordering rule 1 (gates.sh:27-36, 445-486), the firmware-elf-provenance gate, the build-then-check loop in check-bootloaded-builds.sh:5-7,22-23, and the cfg-closure check in make elf-check (Makefile:91, 388-392) that derives expected features via cargo metadata. The incident behind it (decoding bench output with the wrong ELF) was real, but this repo created the hazard.

Change: build each layout and log mode into its own --target-dir (Makefile and gates.sh), then delete ordering rule 1, the firmware-elf-provenance gate and the cfg-closure logic; keep the embedded source digest for probe-log. Cost is disk space and colder first builds.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Each variant builds into its own target dir; ordering rule 1 and the provenance gate are deleted
- [x] #2 probe-log still refuses a mismatched ELF
- [ ] #3 gates.sh push tier is green and CI time is recorded
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 8f8cbc7

Approach: one cargo target dir per build variant, named from the variant (layout + log mode, plus a feature tag for the rig stimulus builds), so no two builds ever write the same release/<bin>. Every consumer (Makefile ELF var, gates, check-bootloaded-builds.sh, check-image-load-addresses.sh) derives the dir from the same variant name. Not split: the pieces are tightly coupled (deleting the provenance machinery is only safe once the dirs exist), so one session.

Steps
1. Define the naming once. Variant dir = firmware/target-variants/<layout>-<console|rtt>[-<tag>] is NOT used; instead pass --target-dir firmware/target/<variant> (keeps CI cache key `firmware -> target` in .github/workflows/ci.yml working). Put a single bash helper (e.g. scripts/variant-dir.sh) and a Make function computing <variant> from LAYOUT, NO_DEFAULT/log-defmt and the stim feature; ELF = $(TARGET_DIR)/thumbv7em-none-eabihf/release/$(BINARY).
2. firmware/Makefile: add TARGET_DIR/--target-dir to build, build-elf, check, clippy and ELF_BUILD (CARGO_BUILD define); ELF var points into it. Delete the cfg-closure logic in elf-check (REQUESTED/PROV check call, cargo-metadata expected_from_metadata path) and the layout/cfg clause; KEEP the source-digest half of elf-check (build.rs .asp.prov digest vs scripts/elf-inputs-digest.sh) so probe-log still refuses a mismatched/stale ELF. Update the LAYOUT header comment (shared ELF path text) and the elf-check/probe-log doc comments.
3. scripts/gates.sh: add --target-dir to every firmware cargo build/clippy gate. Console and RTT clippy gates reuse the matching build dir. Delete ordering rule 1 (header lines ~27-36), the RTT_ONLY_FEATURES provenance residue comment and the firmware-elf-provenance gate; fix remaining comments that cite rule 1 or release/rig re-pointing (rig builds block, bootloaded-builds block). Keep rule 2 (clippy after builds) reworded. Re-run `scripts/gates.sh --list`/dry to confirm gate counts and update docs/development.md lines ~65-75 accordingly.
4. scripts/check-bootloaded-builds.sh: build each matrix row into its own dir; the build-then-grade unit is no longer required, but keep per-row grading (load addresses, REPLAY AXI-tail check) reading from that row's dir. Drop the elf-provenance.sh check call and the ordering note.
5. scripts/check-image-load-addresses.sh: derive_release_dir takes the variant dir (arg or env) instead of the single release dir; update its header/comments about release/main.
6. scripts/elf-provenance.sh: delete the `check` subcommand, expected_from_metadata, compare_request and their selftest cases (case_console_agreement, rtt_only_agreement, third_cfg_set_refused, feature_outside_default_closure, etc.); keep `digest` and `show` (probe-log needs digest; build.rs keeps stamping cfg/layout, harmless and useful for `show`). Update the selftest gate and docs/development.md "Three selftest gates" text if the selftest remains.
7. Docs: docs/reference/daisy-seed3.md and docs/development.md references to release/main, provenance gate, elf-check cfg clause; firmware/Cargo.toml comments at ~67/92 mention shared release/main; flake.nix line ~80 comment. Comments carry no ticket history.
8. Make flash/DFU .bin outputs (`cargo objcopy ... -O binary $(IMAGE)`) keep names; just add --target-dir.

Verification
- AC2: with two variants built (console, RTT-only), `make probe-log` against a deliberately stale ELF (touch/edit a source, no relink) refuses via the digest check; also confirm pointing ELF at the other variant's dir is impossible by construction (path is derived from FEATURES/NO_DEFAULT/LAYOUT). Keep or add an elf-check selftest/fixture for the digest refusal.
- Run scripts/elf-provenance.sh --selftest and check-elf-staleness selftest, `scripts/check-doc-artifact-names.sh`.
- AC3: run `scripts/gates.sh push` green (cold target dirs, so slower first run); then push main and read CI with `gh run list/view`; record push-tier wall time (GATES_TIMINGS_FILE) and CI duration in ticket notes, plus disk use of firmware/target before and after. Check ci.yml cache path still covers all variant dirs.

Risks: cold first builds and disk growth (accepted by ticket); Makefile callers elsewhere (docs, scripts/bench-030-04.py, probe-rtt-list.sh) that hardcode release/main; CI cache size.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented: scripts/variant-dir.sh names target/<layout>-<default|nodefault>-<features>[-tag]; Makefile ELF/build/check/clippy/objcopy use it via --target-dir; gates.sh passes vd(...) to every firmware cargo gate, ordering rule 1, RTT_ONLY residue and the firmware-elf-provenance gate are gone (gate counts now commit 16, push 32, ci 33); check-bootloaded-builds.sh builds each row into its own dir (no provenance call); check-image-load-addresses.sh gained --target-dir (gate reads the RTT-only variant, which links all six bins); elf-provenance.sh lost check/compare_request/expected_from_metadata and keeps show+digest (selftest 14 cases pass); Makefile elf-check keeps only the source-digest half. AC2: elf-check digest refusal is covered by check-elf-staleness --selftest (12 cases pass) and the ELF path is derived from LAYOUT/NO_DEFAULT/FEATURES (make elf-check LAYOUT=bootloaded with no such build reports no ELF). Also fixed three private intra-doc links in asperitas-logging that made the push-tier cargo-doc-all-features gate red on HEAD. AC3 NOT met: scripts/gates.sh push, cold variant dirs, passes 31 of 32 gates; the last, cargo test, fails on the pre-existing TASK-123 (measure reference test reads a deleted golden). Timings from that run (s): bootloaded-builds 81, each rig/cross build ~18, rig clippy ~16, firmware clippy ~10; firmware/target grew 5.5G -> 14G. Remaining once TASK-123 is Done: rerun gates.sh push, push main, read CI with gh run view and record the CI duration, and check that Swatinem rust-cache (ci.yml workspaces firmware -> target) actually saves the nested target/<variant> dirs; if not, list them or accept cold CI builds.
<!-- SECTION:NOTES:END -->
