---
id: doc-001
title: Asperitas Project Plan
type: specification
created_date: '2026-08-01 05:43'
---

# Asperitas - Project Plan

A musically-interactive audio effect for acoustic and clean electric instruments
(mandolin, octave mandolin, upright bass, bass guitar, jazz guitar), built in Rust for
the Electro-Smith Daisy Seed3 hosted in a Daisy Pod.

Hardware and ecosystem facts live in `docs/reference/` and are linked from `CLAUDE.md`. How the
repo is built and gated lives in `docs/development.md`. This document is the *plan*: decisions,
architecture and milestones, kept short and updated as work lands.

---

## 1. Decisions

| Question | Decision | Status |
|---|---|---|
| v1 effect | **Sympathetic resonator** - a bank of tuned comb/bandpass resonators excited by the input, Rings-like | Not started. The shipped effect is a one-pole low-pass then gain (`Asperitas` in `asperitas-dsp`), which proves the chain end to end |
| DSP approach | **Hand-written in `asperitas-dsp`**, `no_std`, no DSP crate dependency. Transliterating specific Mutable Instruments algorithms stays an option where their exact character is wanted | Holds. (`dasp` was considered and is not used.) |
| Board bring-up | **Rust-only.** C++ is not a fallback: libDaisy has no Seed3 support | Done. `daisy-embassy` Seed3 support (PR #80) merged; the build tracks `master` with the `seed3` feature, no SHA pin |
| Control surface | **Daisy Pod** - 2 knobs, encoder + click, 2 buttons, 2 RGB LEDs | Done (`asperitas-pod`, firmware `main`) |
| Debug | Flash by DFU over the onboard USB-C, or by ST-Link on the SWD pads. Log over the framed USB console or `defmt`/RTT | Both routes work; see `docs/reference/daisy-seed3.md` |
| Desktop tooling | WAV CLI first, then live `cpal` TUI, then analysis, then CPU-cost harness | CLI done; analysis lives in `asperitas-measure`; TUI and bench not started |
| Test corpus | Real instrument recordings committed to git, alongside synthetic signals | Done (`audio/`) |
| CI | `lefthook` locally and GitHub Actions, all executing `scripts/gates.sh` at ascending tiers | Done; see `docs/development.md` |

### Assumptions

- **48 kHz, 32-sample blocks, `f32` internally** (`BLOCK_LENGTH` in `firmware/src/audio.rs`).
  The codec can do 192 kHz/32-bit; there is no musical reason to pay for it.
- **Mono in, stereo out.** The instruments are mono sources; a resonator bank wants stereo spread
  on the output. The frame type is stereo throughout, so this is a policy, not a constraint.
- **`proptest`, not `quickcheck`**, for property tests.
- **Two Cargo workspaces**, host and firmware (see section 2).

---

## 2. Architecture

The organising principle: **the DSP knows nothing about the hardware.**

### Crate map

```
asperitas/
├── crates/                      # host workspace; the no_std ones also build for firmware
│   ├── asperitas-dsp/           # no_std. Processor trait, filter, gain, smoothing, the Asperitas
│   │                            #   patch, replay effect stage, stimulus generators
│   ├── asperitas-wire/          # no_std wire protocol: framed console records, CTL commands
│   ├── asperitas-logging/       # device logging: backends, USB CDC console, panic handler, LEDs
│   ├── asperitas-pod/           # Pod control surface drivers and the pure ControlDecoder
│   ├── asperitas-replay/        # QSPI-to-callback ping-pong staging for replaying captured audio
│   ├── asperitas-host/          # host tooling for the wire protocol (std)
│   ├── asperitas-rig/           # host capture runner: `asperitas-rig` binary, session sidecars
│   ├── asperitas-measure/       # host analysis of rig captures against limits
│   └── asperitas-cli/           # offline WAV processing through the dsp crate
├── firmware/                    # SEPARATE workspace, thumbv7em-none-eabihf: main plus rig,
│                                #   blinky, ledtest, podtest and panictest bins
├── audio/                       # instrument clips, goldens, committed rig captures
├── docs/reference/              # hardware and ecosystem findings
└── docs/development.md          # nix, gates, hooks, CI
```

**Two workspaces, deliberately.** A single workspace cannot cleanly hold both host and
`thumbv7em` targets: `forced-target` is unstable and a workspace-wide `[build] target` breaks the
host crates. `firmware/` is excluded from the root workspace and carries its own
`.cargo/config.toml`. Nothing reaches it by default, so `scripts/gates.sh` gives it its own fmt,
cross-clippy and cross-compile gates.

### The `Processor` boundary

Every other piece is organised around `Processor` in `crates/asperitas-dsp/src/processor.rs`:
`set_sample_rate`, `set_params`, per-sample `tick`, `reset`, and a provided in-place
`process_block`. The design notes still hold:

- **`tick` is the primitive; `process_block` is provided.** Implementers write the simple thing.
- **Parameters are an associated type, not a bag of floats.** The DSP owns parameter semantics
  (ranges and curves, `ParamSpec`); the caller owns the physical mapping.
- **No allocation, no `Result`.** Clamp, saturate, make degenerate values ordinary ones.
- **Smoothing lives inside processors**, so no host reimplements it.

---

## 3. Milestones

### M0 - Scaffold: done
Nix flake dev shell, two-workspace skeleton, lefthook, CI, all driven by `scripts/gates.sh`.

### M1 - First light on device: done
Blinky over DFU, audio passthrough with the `seed3` feature, USB CDC logging, plus the RTT route
through a probe.

### M2 - DSP spine and offline tooling: done
`Processor` trait, gain and one-pole filter, property and golden tests, the WAV CLI.

### M3 - Pod integration: done
Pod pin map, ADC knobs, encoder, buttons and LEDs; `Asperitas` wired into the firmware audio
callback with the knobs driving cutoff and gain. Hardware sign-off tracked in the backlog tickets.

### Measurement rig and host control: done / ongoing
Not in the original plan. A framed USB console (`asperitas-wire`, `asperitas-logging`), a rig
firmware that plays stimuli or replays captured audio through the effect stage, the host capture
runner (`asperitas-rig`) and the analysis crate (`asperitas-measure`). This is how the device is
verified without ears; committed captures live in `audio/captures`. Open work is tracked as
tickets, notably the HUMAN bench runs.

### M4 - Live desktop iteration: proposal, needs owner review
A `cpal`-backed live host so sound design stops requiring a flash cycle. Proposed scope:

- **Crate:** `asperitas-live` (a `cpal` backend running `Asperitas`; add `ratatui` only if a
  terminal UI earns its keep over plain CLI flags). Not `asperitas-tui` unless the TUI is the point.
- **Shared parameter model:** today three separate surfaces map to the same effect: the Pod knob
  mapping, `asperitas-cli --params`, and `CTL FX`. "Same as the device" is proposed to mean exactly
  one definition: `asperitas-dsp`'s `Controls` (normalised knobs in `[0, 1]`) and `ParamSpec`
  tables, with every host translating its own input into `Controls` and nothing else.
- **Exit criterion:** the live host and the device, given the same `Controls` and input, produce
  the same output within float tolerance.

### M5 - The resonator: proposal, needs owner review
A resonator `Processor` (comb/bandpass bank, excitation shaping, damping and structure controls,
stereo spread) with its own `Params`, added beside `Asperitas` rather than replacing it. Golden
regression tests against the committed corpus; impulse and frequency response output from the CLI
or `asperitas-measure` to see what the bank does. The knob-to-parameter mapping for the Pod's two
knobs and encoder is an owner decision.

### M6 - Musical interactivity
Envelope follower and the smoothed random-walk modulation source, modulating resonator
parameters. This is what separates the project from a static filter bank.

### M7 - Polish and hardware
CPU-cost harness and headroom work, preset save/recall to QSPI flash, and only then enclosure,
footswitch, jacks and true bypass.

---

## 4. Testing strategy

Four layers, each catching what the others cannot.

- **Property tests (`proptest`):** output finite and bounded for any legal parameters; silence in,
  silence out once tails decay; `reset()` idempotent and deterministic; `process_block` agrees
  with repeated `tick`; parameter changes never produce discontinuities above a threshold (the
  test that catches missing smoothing).
- **Golden-file regression:** frozen output for an input plus parameter set, regenerated
  deliberately and never automatically. A golden diff means "listen before accepting".
  Corpus clips are short (2-5 s), mono, 48 kHz.
- **Unit tests:** coefficient computation, parameter curves, wire framing, decoders.
- **Analysis output:** impulse, frequency, RMS and spectrogram dumps, because seeing a DSP bug is
  easier than hearing it.

**Not automated:** anything needing a person at the board - hearing the result, pot jitter on a
real Pod, real instruments. Device behavior that can be measured goes through the rig instead;
`HUMAN:` acceptance criteria record the rest. Compiling is not evidence.

---

## 5. Risks

| Risk | Severity | Mitigation |
|---|---|---|
| Gain staging mistaken for DSP bugs | Medium | Pod input is line level, not hi-Z (`docs/reference/daisy-pod.md`). Use a DI/preamp; compare device against the CLI on identical source |
| No prior DSP experience | Medium | Analysis output makes behavior visible; a comb filter is a delay line plus feedback, so resonators are a gentle first algorithm |
| CPU headroom exhausted late | Medium | The CPU-cost harness is in M7; pull it forward if voice counts get ambitious |
| Committed audio corpus bloats the repo | Low | Short mono clips, deliberate regeneration only |
| Two-workspace friction | Low | Firmware has its own gates; residual gaps are listed in `docs/development.md` |

---

## 6. Deliberately deferred

- Enclosure, footswitch, true bypass, jacks, pedal versus desktop box - after M7.
- Transliterating Mutable Instruments algorithms versus staying hand-written.
- MIDI, SD card, and preset management beyond simple QSPI save/recall.
- Other effect directions in `idea.md` (call-and-response looper, chord-aware harmonizer, granular
  wash). The `Processor` boundary keeps these cheap: new implementations, not new architectures.
