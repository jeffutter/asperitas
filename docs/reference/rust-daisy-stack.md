# Rust Daisy Stack — Ecosystem Reference

Snapshot taken 2026-08-01. **This file describes a fast-moving situation** — especially
the Seed3 support status below. Re-check before relying on it. Stable hardware facts
live in [daisy-seed3.md](./daisy-seed3.md).

## Board support: use `daisy-embassy`

[`daisy-embassy`](https://github.com/daisy-embassy/daisy-embassy) is the primary BSP and
is linked from Electro-Smith's own software page. Built on the Embassy async runtime.
Provides a `new_daisy_board!` macro that moves `embassy_stm32::Peripherals` into typed
builders (`AudioPeripherals`, `FlashBuilder`, …).

Board selection is by Cargo feature:

| Feature | Board | Codec |
|---|---|---|
| `seed` | Seed Rev4 | AK4556 |
| `seed_1_1` | Seed Rev5 | WM8731 |
| `seed_1_2` | Seed Rev7 | PCM3060 |
| `patch_sm` | Patch SM | PCM3060 |
| `seed3` | **Seed3 — unmerged, see below** | TAC5242 |

Current release line is `0.3.0` (unreleased); `0.2.2` is the last published version.

The audio API is a callback over a block:

```rust
interface.start_callback(|input, output| {
    output.copy_from_slice(input);
}).await
```

Useful examples in the repo: `blinky.rs`, `passthrough.rs`, `triangle_wave_tx.rs`,
`looper.rs`, `sdram.rs`, `flash.rs`, `usb_serial.rs`, `usb_midi.rs`, `usb_uac.rs`.

## Seed3 support status — PR #80

This is the load-bearing fact for this project.

- **Issue [#79](https://github.com/daisy-embassy/daisy-embassy/issues/79)** — "Seed3
  Support", tracking issue. Confirms same CPU, "mostly compatible".
- **PR [#80](https://github.com/daisy-embassy/daisy-embassy/pull/80)** — "Add Daisy
  Seed3 TAC5242 support". Opened **2026-08-01** by `landakram`.
  - Branch: `landakram/daisy-embassy` @ `dev/seed3`, head `477083b0227d`
  - State: **open, mergeable, no reviews**, 2 commits, 12 files, +240 / −13
  - Adds `src/codec/tac5242.rs` and a `seed3` feature; SDRAM and pinout reused unchanged

The author's own hardware verification — **on a Seed3 in a Daisy Pod**, the same
configuration as this project:

- board boots and resets reliably
- Pod buttons, encoder, LEDs and both knobs work
- USB connects and reconnects correctly
- SDRAM passes a bounded destructive test (all 32 data bits, power-of-two offsets for
  address aliasing, 4096 values across the full 64 MiB, cache maintenance before
  readback)
- stereo in and out at 48 kHz with correct left/right channels
- line-out to line-in loopback passes stereo audio repeatably

**Consequence for us:** the Seed3 codec work is already done. Pin the dependency to the
PR branch rather than writing a driver. Because the PR is brand new and unreviewed it
may be force-pushed or reworked, so pin to the **specific commit SHA**, not the branch
name, and revisit when it merges.

**Contribution opportunity:** an unreviewed PR whose author is asking for confirmation is
exactly where an independent test report on identical hardware has the most value. This
is the cheapest possible route to the "contribute back upstream" goal — it costs a
comment, not a driver. Same pattern as the existing call for `0.3.0` test reports in
[issue #59](https://github.com/daisy-embassy/daisy-embassy/issues/59).

## Alternative / superseded BSPs

- [`zlosynth/daisy`](https://github.com/zlosynth/daisy) — sync (non-async), `no_std`,
  `embedded_hal`-based. One of the foundations daisy-embassy was built from. Still
  maintained. Reasonable fallback if Embassy's async model proves a poor fit.
- [`electro-smith/libdaisy-rust`](https://github.com/electro-smith/libdaisy-rust) —
  last pushed 2025-03; effectively dormant.
- [`antoinevg/daisy_bsp`](https://github.com/antoinevg/daisy_bsp) — the ~2020–2022
  generation. Out of date; this is most likely the "old Rust project" already
  encountered.

## DSP libraries

**There is no Rust equivalent of DaisySP.** No 1:1 port exists, and none is in progress.

- [`dasp`](https://github.com/RustAudio/dasp) (RustAudio) — low-level PCM primitives:
  ring buffers, interpolation, signal combinators, frame/sample traits. A foundation
  rather than a module library. `no_std` capable. **This project's chosen base.**
- `fundsp` — general-purpose, combinator-based graph API with batteries included.
  Active community. `no_std` behaviour on a 480 MHz M7 with hard-float is unverified.
- `valib` — smaller, newer, musical-DSP-focused building blocks.
- `hexodsp` — fuller modular DSP / audio-graph library, built for the HexoSynth plugin;
  `no_std` compatibility not confirmed.
- [`mi-plaits-dsp-rs`](https://github.com/sourcebox/mi-plaits-dsp-rs) — a genuine native
  Rust port of Mutable Instruments Plaits, `no_std` compatible. Plaits is a synth voice,
  not a texture processor, so it does not cover Clouds/Rings-style effects.
- **No Rust port of Clouds or Rings exists.** Building either means writing it from
  scratch informed by the original C++ in
  [`pichenettes/eurorack`](https://github.com/pichenettes/eurorack), which is reportedly
  short and readable.

## Host-side crates

- `cpal` — real-time audio I/O for the live desktop TUI
- `hound` — WAV read/write for the offline CLI and golden-file tests
- `proptest` — property testing
- `ratatui` — TUI

## Toolchain

- Target: `thumbv7em-none-eabihf` (Cortex-M7F, hard float)
- `probe-rs` — flash + `defmt`/RTT logging; **requires an ST-Link probe**
- `dfu-util` — probe-free flashing over the Seed3's USB-C; see
  [daisy-seed3.md](./daisy-seed3.md#flashing-without-a-debug-probe)
- `cargo-binutils` / `llvm-tools` — `objcopy` to produce the raw `.bin` DFU needs
