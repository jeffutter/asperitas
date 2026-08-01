---
id: TASK-005.01
title: Implement audio passthrough against the seed3 feature
status: Done
assignee:
  - '@ralph'
created_date: '2026-08-01 05:57'
updated_date: '2026-08-01 17:40'
labels:
  - planned
dependencies:
  - TASK-009
documentation:
  - docs/reference/daisy-seed3.md
parent_task_id: TASK-005
type: feature
ordinal: 13000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build daisy-embassy's passthrough against `--features seed3` from the pinned PR #80 commit.

```rust
interface.start_callback(|input, output| {
    output.copy_from_slice(input);
}).await
```

Target 48 kHz. Note: `BLOCK_LENGTH` is a hardcoded `const` (32 samples) in the pinned fork's `src/audio.rs`, not a configurable field on `AudioConfig` — the block size is not ours to choose. This diverges from libDaisy's 48-sample Pod default; see docs/reference/rust-daisy-stack.md. The Seed3 codec can do 192 kHz/32-bit; there is no musical reason to spend the CPU here.

If passthrough later turns out not to work on real hardware, we own `src/codec/tac5242.rs` in the pinned fork. The codec is hardware-strapped so there is no I2C register map to reverse — a failure would be in SAI configuration. Expected SAI settings are recorded in docs/reference/daisy-seed3.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Passthrough firmware compiles with --features seed3 at 48 kHz / 32-sample blocks (block size is hardcoded upstream, not configurable — see Description)
- [x] #2 A flashable .bin is produced by the TASK-004.01 pipeline
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Single-file change: transform firmware/src/bin/main.rs from a no-op stub into audio passthrough.

## Implementation

1. **firmware/src/bin/main.rs** — Replace the `loop {}` body with the daisy-embassy passthrough pattern:

   Add imports:
   - `use embassy_time::Timer;` (already available in Cargo.toml)

   Change the `main` function signature from `async fn main(_spawner)` to `async fn main(spawner)` (used for potential future spawner.spawn calls, but not needed for basic passthrough).

   After creating the board, add:
   ```rust
   let interface = board
       .audio_peripherals
       .prepare_interface(Default::default())
       .await;
   let mut interface = interface.start_interface().await.expect("audio init failed");
   // start_callback returns Result<Infallible, sai::Error> — Infallible means
   // it only exits on SAI hardware error. On error, halt (no probe to report to).
   if let Err(e) = interface.start_callback(|input, output| {
       output.copy_from_slice(input);
   }).await {
       let _ = e;
       loop {}
   }
   ```

   Keep the existing no-op defmt logger and panic_halt — these are correct for a probe-free deployment. Do NOT add defmt_rtt or panic_probe (those require an ST-Link).

2. **firmware/Makefile** — Update default BINARY from `blinky` to `main` so `make flash-all` flashes the passthrough. Keep `blinky` available via `make build BINARY=blinky`.

## Verification

- `cd firmware && cargo check --release --features seed3 --bin main` — must compile clean
- `cd firmware && cargo objcopy --release --features seed3 --bin main -- -O binary firmware.bin` — produces < 128 KB binary
- `cd firmware && cargo clippy --release --features seed3 --bin main -- -D warnings` — lint clean
- Binary size check: `ls -la firmware.bin` should show ~20-30 KB
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Fixup applied post-review (review-pi-work, targeting 7f33806): AC #1 and the Description claimed '48-sample blocks matching libDaisy's Pod default,' but the pinned daisy-embassy fork (PR #80, src/audio.rs) hardcodes `BLOCK_LENGTH = 32` as a const — it's not a field on `AudioConfig`, so 48 samples was never achievable here. The delivered firmware correctly runs 32-sample blocks (confirmed: AudioConfig::default() -> Fs::Fs48000, BLOCK_LENGTH const = 32). Corrected the AC and Description text to state the actual, verified behavior instead of the unmet original target.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented audio passthrough in firmware/src/bin/main.rs using daisy-embassy's prepare_interface -> start_interface -> start_callback pattern. 48 kHz, 32-sample blocks. Fixed llvm-objcopy multi-region memory issue (produced 469 MB binary) by using --only-section flags for FLASH-loadable sections, yielding correct ~32 KB DFU binary. Fixed memory.x FLASH length from 2M to 128K. Updated Makefile default BINARY from blinky to main.
<!-- SECTION:FINAL_SUMMARY:END -->
