---
id: TASK-004
title: 'First light: blinky on Seed3, flashed over DFU without a probe'
status: Dev Ready
assignee:
  - '@human'
created_date: '2026-08-01 05:45'
updated_date: '2026-08-01 15:40'
labels: []
dependencies:
  - TASK-002
documentation:
  - docs/reference/daisy-seed3.md
priority: high
type: feature
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Get arbitrary Rust code running on the Seed3. This is the ticket that proves the board, the USB-C DFU path, and the cross-compiled binary all work — before any audio is involved.

**No debug probe available for several weeks**, so the stock daisy-embassy workflow (`cargo run` with `probe-rs run --chip STM32H750IBKx`) is unavailable. Flash by DFU instead:

1. `cargo objcopy --release --features seed3 -- -O binary firmware.bin`
2. Hold `BOOT`, tap `RESET`, release `BOOT` — the board enumerates as an STM32 DFU device
3. `dfu-util -a 0 -s 0x08000000:leave -D firmware.bin`

Address `0x08000000` is the STM32H750's 128 KB internal flash via the built-in system bootloader. That's enough for now; larger applications later need the Daisy bootloader, which relocates into QSPI.

Base the application on daisy-embassy's `examples/blinky.rs`. Keep it in `firmware/src/bin/` so it survives as a known-good diagnostic once real firmware exists — when something later goes wrong, being able to flash a thing that definitely works is worth a lot.

Document the exact working flash procedure (including anything the BOOT/RESET dance needs on this specific board) in docs/reference/daisy-seed3.md, correcting it if reality differs from what's written there.

**If this fails**, the board itself is suspect. Blinky in C++ via libDaisy is a valid isolation step here — libDaisy has no Seed3 *codec* support, but blinky touches no codec. That is the only place in this project where the C++ fallback is available.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All subtasks complete
<!-- AC:END -->
