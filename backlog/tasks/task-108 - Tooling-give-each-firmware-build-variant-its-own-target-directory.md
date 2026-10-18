---
id: TASK-108
title: 'Tooling: give each firmware build variant its own target directory'
status: Blocked
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-18 21:01'
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
SHIPPED by d470c41 (AC3 still open, see notes). This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented: scripts/variant-dir.sh names target/<layout>-<default|nodefault>-<features>[-tag]; Makefile ELF/build/check/clippy/objcopy use it via --target-dir; gates.sh passes vd(...) to every firmware cargo gate, ordering rule 1, RTT_ONLY residue and the firmware-elf-provenance gate are gone (gate counts now commit 16, push 32, ci 33); check-bootloaded-builds.sh builds each row into its own dir (no provenance call); check-image-load-addresses.sh gained --target-dir (gate reads the RTT-only variant, which links all six bins); elf-provenance.sh lost check/compare_request/expected_from_metadata and keeps show+digest (selftest 14 cases pass); Makefile elf-check keeps only the source-digest half. AC2: elf-check digest refusal is covered by check-elf-staleness --selftest (12 cases pass) and the ELF path is derived from LAYOUT/NO_DEFAULT/FEATURES (make elf-check LAYOUT=bootloaded with no such build reports no ELF). Also fixed three private intra-doc links in asperitas-logging that made the push-tier cargo-doc-all-features gate red on HEAD. AC3 NOT met: scripts/gates.sh push, cold variant dirs, passes 31 of 32 gates; the last, cargo test, fails on the pre-existing TASK-123 (measure reference test reads a deleted golden). Timings from that run (s): bootloaded-builds 81, each rig/cross build ~18, rig clippy ~16, firmware clippy ~10; firmware/target grew 5.5G -> 14G. Remaining once TASK-123 is Done: rerun gates.sh push, push main, read CI with gh run view and record the CI duration, and check that Swatinem rust-cache (ci.yml workspaces firmware -> target) actually saves the nested target/<variant> dirs; if not, list them or accept cold CI builds.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Per-variant cargo target dirs landed in d470c41 (variant-dir.sh, Makefile, gates.sh, bootloaded builds, load-address check, provenance reader trimmed to show+digest). AC1 and AC2 met; AC3 (push tier green, CI time recorded) is waiting on TASK-123, the pre-existing cargo test failure.
<!-- SECTION:FINAL_SUMMARY:END -->
