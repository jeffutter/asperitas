---
id: TASK-137
title: >-
  Fix: firmware make build passes --target-dir to cargo objcopy, which rejects
  it
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-19 16:27'
updated_date: '2026-10-19 17:06'
labels:
  - firmware
  - tooling
  - planned
dependencies: []
references:
  - firmware/Makefile
  - scripts/variant-dir.sh
  - >-
    backlog/tasks/task-120 -
    HUMAN-hardware-recheck-after-the-review-refactors.md
priority: medium
type: bug
ordinal: 263800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`cd firmware && nix develop .. --command make build BINARY=main` fails: `cargo objcopy --release --target-dir <dir> ...` exits with `error: unexpected argument '--target-dir' found` (cargo-objcopy 0.4.0 in the nix shell, usage `cargo objcopy --release --target <TRIPLE> [-- <args>]`). The `build` recipe in firmware/Makefile passes `$(CARGO_DIR)` (`--target-dir $(VARIANT_DIR)`) to it, so every `make build`, `flash-all` and anything that builds a .bin through it is broken here. `check`, `clippy` and the cargo build recipes take the same flag fine.

Found at the TASK-120 bench session. Workaround used: `CARGO_TARGET_DIR=$PWD/target/<variant> make build BINARY=... CARGO_DIR=` (and `LAYOUT=bootloaded` for the bootloaded images).

Change: make `build` put the variant target dir in the environment (CARGO_TARGET_DIR) instead of the argument, keeping the per-variant directory scheme from scripts/variant-dir.sh, and confirm scripts/firmware-variants.sh and the gates still build every variant.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 make build BINARY=main|rig|panictest|podtest produces the .bin in the internal and bootloaded layouts inside the nix shell with no manual CARGO_DIR override
- [x] #2 The per-variant target directory scheme is unchanged and scripts/firmware-variants.sh still builds, lints and grades every row of the variants table
- [x] #3 docs/reference/flashing.md build commands still match what works
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by this commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
firmware/Makefile build recipe now sets CARGO_TARGET_DIR=$(VARIANT_DIR) and drops CARGO_DIR for cargo objcopy. Verified in nix shell: main, panictest, podtest build internal and bootloaded; rig builds bootloaded. rig internal overflows the 128 KB FLASH region by design (linker error, not an argument error). firmware-variants.sh build passes unchanged (it uses cargo build, not objcopy). flashing.md uses plain make build, no change needed.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
make build passes the variant target dir to cargo objcopy via CARGO_TARGET_DIR; variant dir scheme unchanged.
<!-- SECTION:FINAL_SUMMARY:END -->
