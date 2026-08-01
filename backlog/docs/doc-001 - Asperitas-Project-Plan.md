---
id: doc-001
title: Asperitas Project Plan
type: specification
created_date: '2026-08-01 05:43'
---

# Asperitas — Project Plan

A musically-interactive audio effect for acoustic and clean electric instruments
(mandolin, octave mandolin, upright bass, bass guitar, jazz guitar), built in Rust for
the Electro-Smith Daisy Seed3 hosted in a Daisy Pod.

Hardware and ecosystem facts referenced throughout live in `docs/reference/` and are
linked from `CLAUDE.md`. This document is the *plan*; those are the *findings*.

---

## 1. Decisions

Settled through a design interview on 2026-08-01. Recorded here so they don't get
relitigated.

| Question | Decision |
|---|---|
| v1 effect | **Sympathetic resonator** — bank of tuned comb/bandpass resonators excited by the input, Rings-like |
| DSP approach | **From scratch on `dasp` primitives**, with the option to transliterate specific Mutable Instruments algorithms later where their exact character is wanted |
| Board bring-up | **Rust-only.** Blinky first, then audio. C++ is *not* a fallback (libDaisy has no Seed3 support). Contributing Seed3 support upstream is a stretch goal |
| Control surface | **Daisy Pod** — 2 knobs, encoder + click, 2 buttons, 2 RGB LEDs. No soldering, no enclosure decisions yet |
| Debug probe | **None for the first few weeks.** Flash by DFU over USB-C; debug over USB CDC serial with Pod LEDs as boot-stage fallback. `probe-rs` + `defmt` slots in later without rework |
| Desktop tooling | WAV CLI first, then live `cpal` TUI, then analysis output, then a CPU-cost harness |
| Test corpus | Real instrument recordings **committed to git**, alongside synthetic signals |
| CI | `lefthook` locally **and** GitHub Actions |

### Assumptions made without asking

State them so they're cheap to overturn:

- **48 kHz, 48-sample blocks, `f32` internally.** Matches libDaisy's Pod default block
  size and the sample rate PR #80 verified. 1 ms of block latency. The Seed3 codec can
  do 192 kHz/32-bit; there is no musical reason to pay for it here, and every reason to
  keep CPU headroom for resonator voices.
- **Mono in, stereo out.** The instruments are mono sources; a resonator bank wants
  stereo spread on the output. The frame type is stereo throughout so this is a policy,
  not a constraint.
- **`proptest`, not `quickcheck`**, for property tests.
- **Two Cargo workspaces**, host and firmware — see §3.

---

## 2. The one thing that makes this project tractable right now

**daisy-embassy PR #80 already implements Seed3 support, and its author tested it on a
Seed3 in a Daisy Pod** — this project's exact hardware. It is open, mergeable, and
unreviewed as of 2026-08-01.

This eliminates what would otherwise have been the dominant risk. The plan therefore:

1. Pins `daisy-embassy` to PR #80's **commit SHA** (not the branch — it's new and may be
   force-pushed), and revisits when it merges.
2. Treats "reproduce the author's test results and report them on the PR" as an early,
   cheap deliverable. An unreviewed PR whose author wants confirmation is where an
   independent report on identical hardware is worth the most.
3. Keeps a real fallback: if PR #80 doesn't work on our board, we own a `tac5242.rs` we
   can debug, and the codec is strapped so there's no I²C register map to reverse — see
   `docs/reference/daisy-seed3.md`.

The corollary risk is the reverse of the usual one: **libDaisy has no Seed3 support**, so
if audio doesn't work there is no C++ reference implementation to compare against.
Blinky in C++ remains available to prove the board is alive, since it touches no codec.

---

## 3. Architecture

The organising principle is the one already identified in `idea.md`: **the DSP knows
nothing about the hardware.** Everything else follows from that.

```
                      ┌───────────────────────────┐
                      │      asperitas-dsp        │
                      │  no_std, no hardware deps │
                      │  Processor trait,         │
                      │  resonator bank, filters, │
                      │  delay lines, envelopes,  │
                      │  modulation sources       │
                      └─────────────┬─────────────┘
             ┌──────────────┬───────┴───────┬──────────────┐
             │              │               │              │
   ┌─────────▼──────┐ ┌─────▼──────┐ ┌──────▼──────┐ ┌─────▼───────┐
   │ asperitas-cli  │ │ asperitas- │ │ asperitas-  │ │  firmware   │
   │ WAV in/out,    │ │ tui        │ │ bench       │ │ Seed3 + Pod │
   │ analysis       │ │ live cpal  │ │ CPU cost    │ │ thumbv7em   │
   └────────────────┘ └────────────┘ └─────────────┘ └─────────────┘
```

### Crate layout

```
asperitas/
├── crates/
│   ├── asperitas-dsp/     # no_std core. The whole point.
│   ├── asperitas-cli/     # offline WAV processing + analysis
│   ├── asperitas-tui/     # live cpal + ratatui
│   └── asperitas-bench/   # CPU cost harness
├── firmware/              # SEPARATE workspace, thumbv7em-none-eabihf
│   ├── .cargo/config.toml
│   └── src/
├── audio/                 # test corpus (see §6)
└── docs/reference/
```

**Two workspaces, deliberately.** A single workspace cannot cleanly hold both host and
`thumbv7em` targets — `forced-target` is unstable, and a workspace-wide
`[build] target` breaks the host crates. `firmware/` is excluded from the root workspace
and carries its own `.cargo/config.toml`. The cost is `cargo test` at the root not
covering firmware; the pre-push hook and CI compensate by cross-compiling explicitly.

### The `Processor` boundary

This is the interface every other piece is organised around, so it's worth getting
right. Sketch:

```rust
pub type Frame = [f32; 2];

pub trait Processor {
    type Params: Clone + Default;

    fn set_sample_rate(&mut self, hz: f32);
    fn set_params(&mut self, params: &Self::Params);
    fn tick(&mut self, input: Frame) -> Frame;
    fn reset(&mut self);

    /// Block processing, provided. Override only if a block-rate
    /// implementation is genuinely faster.
    fn process_block(&mut self, input: &[Frame], output: &mut [Frame]) { ... }
}
```

Design notes, per the project's design philosophy:

- **Per-sample `tick` is the primitive; `process_block` is provided.** Callers who want
  blocks get them free; implementers write the simple thing. Inverting this would force
  every processor to reimplement buffering.
- **Parameters are an associated type, not a bag of floats.** The knob-to-parameter
  mapping is a *policy* that belongs to the caller (Pod, CLI, TUI), not the DSP. The DSP
  owns the parameter *semantics*.
- **No allocation, no `Result`.** A real-time audio path that can fail or block is a
  design error. Errors are defined out of existence: clamp, saturate, and make
  degenerate parameter values into ordinary ones.
- **`set_sample_rate` is separate from construction** so the same processor instance can
  be reused across the 48 kHz device and whatever rate a WAV file happens to be.

Parameter smoothing lives inside processors, not in callers — pulling that complexity
down is what stops every one of the four hosts from reimplementing it.

---

## 4. Milestones

### M0 — Scaffold (tickets 1–3)

Nix flake dev shell, two-workspace skeleton, lefthook hooks, CI. Nothing runs yet; the
point is that everything after this is cheap.

### M1 — First light on device (tickets 4–6)

Blinky over DFU, then audio passthrough with `--features seed3`, then USB CDC serial
logging. **This is the milestone that de-risks the whole project**, and it depends on
almost nothing — it can run in parallel with M2. Ends with a test report posted to
daisy-embassy PR #80.

### M2 — DSP spine and offline tooling (tickets 7–8)

`Processor` trait, a trivial processor (gain/one-pole), property tests, and the WAV CLI.
Establishes the test harness the real DSP work will lean on.

### M3 — Pod integration

Pod BSP (pin map, ADC knobs, encoder, buttons, RGB LEDs), then wire `asperitas-dsp` into
the firmware audio callback with knobs driving parameters. First time the device makes a
sound you can change with your hands.

### M4 — Live desktop iteration

`cpal` TUI with the same parameter model as the device. From here, sound design stops
requiring a flash cycle. This is the point where the project starts being fun.

### M5 — The resonator

Comb/bandpass resonator bank, excitation shaping, damping and structure controls, stereo
spread. Golden-file regression tests against the committed corpus. Analysis output
(impulse/frequency response) to *see* what the bank does, which matters a lot given no
prior DSP background.

### M6 — Musical interactivity

Envelope follower and the smoothed random-walk modulation source from `idea.md`,
modulating resonator parameters. This is what separates the project from a static filter
bank and delivers the Mood-inspired brief.

### M7 — Polish and hardware

CPU-cost harness and headroom work, preset save/recall to QSPI flash, and only then any
decisions about enclosure, footswitch, jacks and true bypass — deferred until the effect
has told us what controls it actually needs.

---

## 5. Development workflow

### Nix

A single flake provides everything: Rust toolchain with `thumbv7em-none-eabihf`,
`clippy`, `rustfmt`, `rust-analyzer`, plus `dfu-util`, `probe-rs`, `cargo-binutils`,
`lefthook`, and the ALSA/pkg-config deps `cpal` needs. `direnv` is already wired up
(`.envrc` contains `use flake`).

Rust toolchain via `fenix` or `rust-overlay` — nixpkgs' `rustc` doesn't carry the
embedded target.

### lefthook

- **pre-commit** — `rustfmt --check` and `clippy` on changed crates. Fast enough not to
  be resented.
- **pre-push** — full `fmt` + `clippy -D warnings` across both workspaces, full test
  suite, and a `--target thumbv7em-none-eabihf` build. The cross-compile check is the
  important one: without it the firmware silently rots while all the work happens on
  desktop.

### CI

GitHub Actions re-runs the pre-push set. Same checks, but immune to `--no-verify`.

---

## 6. Testing strategy

Four layers, each catching what the others can't.

### Property tests (`proptest`)

Invariants that must hold for any processor and any input:

- output is always finite — no `NaN`, no infinity, for any parameter combination
- output is bounded (no runaway feedback under any legal parameter set)
- silence in, silence out, once any tail has decayed
- `reset()` is idempotent, and a reset processor given identical input produces
  identical output
- `process_block` agrees sample-for-sample with repeated `tick`
- parameter changes never produce discontinuities above a threshold (this is the test
  that catches missing smoothing)

Resonator-specific properties: energy decays for damping < 1; a resonator tuned to *f*
responds most strongly to input at *f*.

### Golden-file regression tests

Freeze known-good output for a given input + parameter set; assert future changes don't
silently alter it within float tolerance. This is what catches "it still works, but it
sounds different" — the failure mode property tests structurally cannot see.

**Corpus policy.** Real instrument recordings are committed to git alongside synthetic
signals (impulses, sweeps, plucked-string stubs). To keep this from becoming painful:

- keep clips **short (2–5 s)**, mono, 48 kHz
- goldens are regenerated deliberately via an explicit command, never automatically —
  every regeneration is a reviewable diff
- a golden diff in a PR means "listen to this before accepting it", not "run the update
  command"

### Unit tests

Ordinary tests for the parts with knowable correct answers: filter coefficient
computation, note/frequency conversion, parameter mapping curves, ring buffer indexing.

### Analysis output

Not a test, but the thing that makes DSP learnable without prior background: impulse
response, frequency response, RMS and spectrogram dumps from the CLI. Hearing that a
resonator bank is wrong is much harder than seeing it.

### What is deliberately *not* automated

Anything requiring the device: CPU headroom, ADC/pot jitter, real latency, USB/audio
glitching. These get a manual checklist per firmware ticket instead of a CI job that
would need hardware in the loop.

---

## 7. Risks

| Risk | Severity | Mitigation |
|---|---|---|
| PR #80 doesn't work on our board | High | It was tested on this exact Seed3-in-Pod configuration. If it fails, we own the code and the codec is strapped — no I²C to reverse. Blinky in C++ isolates "board dead" from "our Rust wrong" |
| PR #80 force-pushed or reworked | Medium | Pin to commit SHA `477083b0227d`, not the branch. Revisit on merge |
| No debug probe for weeks | Medium | USB CDC serial gives real text logging with no probe; Pod LEDs cover pre-USB boot stages. `probe-rs`/`defmt` slots in later without rework |
| Gain staging mistaken for DSP bugs | Medium | Pod input is line level, not hi-Z. Documented in `docs/reference/daisy-pod.md`. Use a DI/preamp; compare device against CLI on identical source |
| No prior DSP experience | Medium | From-scratch on `dasp` is the *learning* choice, not the fast one. Analysis output makes behaviour visible. Resonators are the right first algorithm — a comb filter is a delay line plus feedback |
| CPU headroom exhausted late | Medium | CPU-cost harness in M7 is arguably too late; if voice counts start feeling ambitious, pull it forward |
| Committed audio corpus bloats the repo | Low | Short mono clips, deliberate regeneration only |
| Two-workspace friction | Low | Pre-push and CI cross-compile explicitly, so the firmware can't rot unnoticed |

---

## 8. Deliberately deferred

Not decided, and not needing to be:

- Enclosure, footswitch, true bypass, jacks, and whether this ends up a pedal or a
  desktop box — deferred to M7, once the effect has revealed what controls it wants
- Whether to transliterate Mutable Instruments algorithms (Rings/Clouds) for their
  specific character, versus staying fully from-scratch
- MIDI, SD card, and preset management beyond simple QSPI save/recall
- Any of the other effect directions in `idea.md` (call-and-response looper,
  chord-aware harmonizer, granular wash). The `Processor` boundary is what keeps these
  cheap to try later — they become new implementations, not new architectures
