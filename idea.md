# Daisy Seed 3 Project — Research Summary & Handoff Notes

Context: Jeff owns a Daisy Seed 3 (Electro-Smith, STM32H750 MCU) and wants to program it without
using C++. This document summarizes research and decisions made so far, for handoff to another
model to plan the actual build.

## Goal (not yet finalized)
An audio effect, likely inspired by the Chase Bliss Mood, for use with acoustic/clean instruments:
mandolin, octave mandolin, upright bass, bass guitar, jazz guitar. Interest is in "musically
interactive" effects — ones that observe/react to playing dynamics rather than static processing.
Not interested in distortion/overdrive/fuzz.

**Candidate effect directions discussed (none chosen yet):**
- Sympathetic resonator (tuned bandpass/comb filter bank, Rings-like) — flagged as a good first
  project given no DSP background; well suited to plucked acoustic strings.
- Dynamics-driven ambient wash (envelope follower modulating a granular/multi-tap delay's density,
  feedback, pitch spread) — also flagged as a good first project.
- "Call and response" looper (silence-gate phrase detection, playback with time-stretch/pitch-shift/
  reverse after the player stops).
- Chord-aware harmonizer (pitch detection via autocorrelation/YIN, diatonic harmony generation) —
  flagged as high difficulty, especially for polyphonic jazz guitar input; more tractable on
  monophonic mandolin/bass lines.
- Wandering/random-walk modulation source (smoothed noise driving pitch/filter/pan) — identified as
  the general technique behind Mood's "unpredictable" character; cheap to build, useful as a
  building block for any of the above.

## Language / Toolchain Decision

**Decision: Rust**, not C++, not Zig.

- C++ ruled out per Jeff's preference (doesn't want to learn it).
- Zig investigated and ruled out for now: MicroZig's STM32 HAL currently only covers the F3 and L4
  families; there is no STM32H7 support and no existing peripheral drivers (audio SAI, QSPI, SDRAM,
  USB) for the Daisy's chip. Building this from scratch was judged a multi-week detour rather than a
  reasonable starting point.
- Clarified that Rust's *bare-metal embedded* cross-compilation story is not the same pain point as
  cross-compiling std-based targets (mobile/WASM with native deps) — it's `rustup target add
  thumbv7em-none-eabihf` + `cargo build --target ...`, with `daisy-embassy`'s examples already
  providing working `.cargo/config.toml` setup.

## Rust Crate / Library Landscape

- **`daisy-embassy`** — recommended primary BSP. Officially linked from Electro-Smith's own software
  page. Built on the Embassy async embedded framework (async Rust rather than interrupt-driven C++).
  Provides a `new_daisy_board!` macro for peripheral setup (audio, ADC, GPIO, flash). Actively
  maintained. Repo: github.com/daisy-embassy/daisy-embassy
- **`zlosynth/daisy`** — alternative sync (non-async), `no_std`, `embedded_hal`-based BSP. One of the
  foundations `daisy-embassy` was built from. Still maintained.
- Older/superseded: `antoinevg/daisy_bsp` and early "hello-daisy" forum starter projects (~2020–2022)
  — this is likely the "old Rust project" Jeff had already found and noted as out of date.
- **No Rust equivalent of DaisySP exists** (the official C++ DSP library). No 1:1 port. Options
  discussed:
  - `fundsp` — general-purpose DSP crate (oscillators, filters, envelopes, effects), combinator-based
    API, active community.
  - `valib` — smaller, newer, musical-DSP-focused building blocks.
  - `dasp` (RustAudio) — low-level PCM primitives (ring buffers, interpolation, signal combinators),
    more of a foundation than an off-the-shelf module library.
  - `hexodsp` — fuller modular DSP/audio-graph library, built primarily for the HexoSynth plugin
    rather than embedded; `no_std` compatibility not confirmed.
  - **`mi-plaits-dsp-rs`** (by sourcebox) — a genuine native Rust port of Mutable Instruments' Plaits
    macro-oscillator DSP code, `no_std` compatible. Confirmed to exist, but Plaits is a synth voice
    (oscillator engine), not a granular/texture processor, so it doesn't cover the Clouds-style effect
    use case directly.
  - **No Rust port of Clouds (or Rings) was found.** Generic no_std granular-synthesis crates exist
    (including one built specifically for Daisy Seed) but are not the actual Mutable Instruments
    algorithm — building a Clouds-like granular engine would mean writing it from scratch, informed
    by reading the original C++ source (which is reportedly short/readable, largely ported from
    Mutable Instruments' Plaits/Rings/Clouds and Csound/Soundpipe).
- **DaisyDuino** (Arduino wrapper around libDaisy + DaisySP) was raised as a middle-ground option if
  the real aversion turns out to be modern C++ complexity rather than the language itself — full
  DaisySP access with a shallower C-with-objects dialect. Not selected, but noted as a fallback if the
  from-scratch/Rust-crate DSP path feels too limiting once building starts.

## Key open uncertainty
Jeff has no prior DSP programming experience, and is weighing "write minimal DSP from scratch in
Rust" vs. "lean on a fuller-featured library" — concern being that thinner libraries could constrain
musical results while he's also learning DSP concepts for the first time. Not resolved; deferred
until there's a clearer picture of what's being built (see effect brainstorm above).

## Development / Testing Workflow Decision
**Decision: decouple DSP logic from hardware entirely** to avoid slow flash-and-listen iteration
cycles.

- Write the effect as a plain Rust crate (`process(&mut self, input: &[f32]) -> f32`-style interface)
  with no dependency on Daisy/embassy/hardware peripherals.
- Run/iterate on this crate on a laptop using **`cpal`** for real-time audio I/O (instrument plugged
  into a normal audio interface, `cargo run`, tweak, rerun) — no flashing required for sound design.
- Only wire the finished crate into `daisy-embassy`'s audio callback once satisfied with the sound on
  desktop.
- Additional testing techniques discussed:
  - Offline WAV-in/WAV-out batch processing (via `hound`) using pre-recorded reference riffs, to
    compare parameter sets without playing live each time.
  - Golden-file regression tests: freeze known-good output WAV for a given input + parameter set,
    assert future changes don't silently alter it (within float tolerance) — explicitly linked to
    Jeff's existing promptfoo/evaluation-driven habits.
  - Optional lightweight `egui` or CLI-driven live parameter tweaking on the desktop version.
- When actual hardware testing is needed (CPU headroom, ADC/pot jitter, latency, USB/audio glitches),
  `probe-rs` gives a fast `cargo run`-style flash+run loop without a BOOT/RESET dance, and the debug
  probe can coexist with audio interface + instrument connections.

## Sources Consulted (web search, not exhaustive citations)
- Electro-Smith's official Daisy software page (links `daisy-embassy` as a recommended option)
- `daisy-embassy` GitHub repo and README
- `zlosynth/daisy` crate on crates.io
- MicroZig project docs/DeepWiki (STM32 chip support scope)
- `awesome-audio-dsp` curated list (BillyDM) — DSP library landscape
- `mi-plaits-dsp-rs` GitHub repo and README (sourcebox)
- `electro-smith/DaisySP` and `DaisySP-LGPL` GitHub repos
- Mutable Instruments Clouds official documentation/manual (pichenettes.github.io)
- `pichenettes/eurorack` GitHub repo (original Mutable Instruments C++ source)
- Various crates.io listings (`dsp`, `dasp`, `dasp-rs`, `microdsp`)

## Explicitly Not Yet Decided
- Which specific effect to build first.
- Whether to use a general Rust DSP crate (`fundsp` et al.) or write DSP from scratch.
- Detailed architecture/design of the chosen effect.
- Hardware I/O specifics (pots, footswitches, display use, etc. on the Daisy Seed 3 board itself).
