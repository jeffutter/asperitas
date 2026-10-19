---
id: TASK-137
title: >-
  Fix: firmware make build passes --target-dir to cargo objcopy, which rejects
  it
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-19 16:27'
labels:
  - firmware
  - tooling
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
- [ ] #1 make build BINARY=main|rig|panictest|podtest produces the .bin in the internal and bootloaded layouts inside the nix shell with no manual CARGO_DIR override
- [ ] #2 The per-variant target directory scheme is unchanged and scripts/firmware-variants.sh still builds, lints and grades every row of the variants table
- [ ] #3 docs/reference/flashing.md build commands still match what works
<!-- AC:END -->
