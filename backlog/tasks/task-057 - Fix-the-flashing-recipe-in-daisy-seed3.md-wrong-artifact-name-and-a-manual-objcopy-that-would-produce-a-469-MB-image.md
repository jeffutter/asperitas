---
id: TASK-057
title: >-
  Fix the flashing recipe in daisy-seed3.md: wrong artifact name, and a manual
  objcopy that would produce a 469 MB image
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-12 19:18'
labels: []
dependencies: []
priority: medium
ordinal: 89800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while correcting the size claims in this same document under TASK-055. That ticket's sweep was
scoped to byte counts; these lines are stale in a way that costs an afternoon.

docs/reference/daisy-seed3.md:95-96 documents the step-by-step route as:

    make build BINARY=blinky   # produces firmware.bin via cargo objcopy
    make flash                 # dfu-util -a 0 -s 0x08000000:leave -D firmware.bin

Neither half is true since the Makefile started naming each image after its binary. `build` writes
`$(BINARY).bin` (firmware/Makefile:102-103), so `make build BINARY=blinky` produces `blinky.bin`; and
`flash` reads `$(BINARY).bin` (:132) with `BINARY` defaulting to `main` (:15), so plain `make flash`
re-flashes `main.bin`, not the blinky image the line above just built. Following the snippet as written
flashes the application while the reader believes they flashed the test binary. The Makefile comment at
:11 calls this exact trap out: "A single name would be a trap on this project."

Worse, the manual route at :104-108:

    cargo objcopy --release --features seed3 --bin blinky -- -O binary firmware.bin
    dfu-util -a 0 -s 0x08000000:leave -D firmware.bin

omits the defaults/target-dir flags and the entire `--only-section` list, which is load-bearing rather
than cosmetic. Per firmware/Makefile:86-89, llvm-objcopy's `-O binary` spans lowest to highest VMA, so
it covers the gap between FLASH (0x08000000) and RAM (0x24000000) and emits a ~469 MB file. A reader
typing this gets a 469 MB image, then a dfu-util failure or worse.

Scope is this document's build/flash quickstart only. No firmware behavior changes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 #1 Every artifact name in the build/flash quickstart matches what firmware/Makefile produces and consumes today, quoted line by line against the build and flash rules, and the step-by-step snippet flashes the image it just built rather than the default main.bin.
- [ ] #2 #2 The manual cargo objcopy route either carries the flags the Makefile actually passes (CARGO_DEFAULTS, FEATURES, and all six --only-section entries) or is removed in favor of the make targets. State explicitly that omitting --only-section yields a ~469 MB image and why, so nobody re-simplifies it later.
- [ ] #3 #3 Any command in this document that a reader can run against the device is checked for the same drift, and each surviving one is dated with what it was verified against.
- [ ] #4 #4 Host gates green in nix develop: fmt and the workspace clippy/doc invocations from ci.yml, plus both cross-compiles even though no firmware logic is touched.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Every command in the touched section copy-pasteable against the current Makefile
<!-- DOD:END -->
