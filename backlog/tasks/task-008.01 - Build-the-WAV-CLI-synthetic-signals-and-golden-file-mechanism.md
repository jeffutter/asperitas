---
id: TASK-008.01
title: 'Build the WAV CLI, synthetic signals, and golden-file mechanism'
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-08-01 05:57'
updated_date: '2026-08-01 18:22'
labels:
  - planned
dependencies: []
parent_task_id: TASK-008
type: feature
ordinal: 18000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`asperitas process in.wav out.wav --param damping=0.3`

Use `hound` for WAV I/O. Handle sample-rate mismatch by calling `set_sample_rate` with the file's actual rate rather than resampling or refusing — the trait was designed for exactly this.

Parameter parsing is the first consumer of the knob-to-parameter mapping policy later shared with the Pod and the TUI. Keep that mapping somewhere both can reach; do not bury it in CLI argument handling.

Generate the synthetic corpus from the CLI itself: impulses, sweeps, plucked-string stubs. Deterministic and diffable.

Golden-file regression: freeze known-good output for an input + parameter set, assert future output matches within float tolerance. **Goldens regenerate only by an explicit command, never automatically** — a golden diff in review means 'listen to this before accepting it', not 'run the update command'. That distinction is the entire value of the mechanism.

Analysis output (impulse/frequency response, RMS, spectrogram) is a later ticket; leave room in the command structure but do not build it now.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `asperitas process in.wav out.wav` runs a Processor over a file and writes valid output
- [ ] #2 Parameters settable from the command line via a mapping shared with future hosts, not CLI-local
- [ ] #3 A WAV at a rate other than 48 kHz is handled via `set_sample_rate`, not rejected
- [ ] #4 The CLI generates the synthetic test signals
- [ ] #5 Golden-file tests pass over synthetic signals, and regenerating goldens requires an explicit opt-in command
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation Plan

### Overview

Transform the skeleton `asperitas-cli` into a functional WAV processing tool with two subcommands (`process`, `generate`), a shared parameter-parsing module, synthetic signal generators, and golden-file regression tests. Adds `clap 4` (CLI arg parsing) and `hound 3` (WAV I/O) as dependencies.

### Architecture Decisions

**Binary name:** Keep `asperitas-cli` internally; users invoke via `cargo run -p asperitas-cli -- <args>`. The `[[bin]]` name stays `asperitas-cli` (with hyphen) to match the crate name convention.

**Parameter mapping location:** Each processor gets a `parse_params_from_cli(pairs: &[(String, String)]) -> Result<Self::Params, String>` static helper method on its own type in `asperitas-dsp`. This keeps the mapping close to the Params definition (so future Pod/TUI hosts import it from `asperitas-dsp`) while letting each processor own its own key names. The CLI dispatches to the right parser via a `match` on `--processor`.

**Synthetic signal generation:** Lives in `asperitas-cli/src/synth.rs`. Pure functions returning `Vec<f32>` mono samples. A wrapper stereo-doubles them. Deterministic: Karplus-Strong uses a fixed LCG seed. All generators output at a specified sample rate and duration.

**Golden-file storage:** `audio/goldens/` directory in the repo root. Files named by convention: `{processor}_{param_digest}_{signal_type}.wav`. Goldens are ~160 KB each (1 s, stereo, 16-bit, 48 kHz). Six goldens total (~1 MB) — small enough for git without LFS.

**Golden tolerance:** Per-sample absolute delta < 1e-5 (well above floating-point noise, well below audible threshold at 16-bit). Compares frame-by-frame after confirming matching length and spec.

### Files to Create/Modify

#### 1. `crates/asperitas-cli/Cargo.toml` — add dependencies

Add `clap = { version = "4", features = ["derive"] }` and `hound = "3.5"`. Also add a `[lib]` section so integration tests can import internal modules.

#### 2. `crates/asperitas-dsp/src/gain.rs` — add `parse_params_from_cli`

Add a static helper to `Gain` that parses CLI key=value pairs into `GainParams`. Recognized key: `gain_db`. Returns descriptive error for unknown keys or bad values.

#### 3. `crates/asperitas-dsp/src/filter.rs` — add `parse_params_from_cli`

Same pattern for `OnePoleLowPass`. Recognized key: `cutoff_hz`.

#### 4. `crates/asperitas-cli/src/lib.rs` — new library root

Minimal `pub mod synth; pub mod wav_io;` so integration tests can link against the library.

#### 5. `crates/asperitas-cli/src/wav_io.rs` — new, WAV read/write helpers

`read_wav(path) -> Result<(WavSpec, Vec<Frame>), String>` — validates stereo + 16-bit PCM, converts i16 interleaved samples to f32 Frames scaled to [-1, 1].

`write_wav(path, spec, frames) -> Result<(), String>` — clamps f32 to ±1.0, scales to i16, writes interleaved stereo.

#### 6. `crates/asperitas-cli/src/process.rs` — new, `run_process()` function

Takes ProcessArgs (input path, output path, processor name, param list). Reads input WAV, dispatches to correct processor by name string, calls `set_sample_rate` with file's actual rate (AC#3), sets params via shared parser (AC#2), runs `process_block`, writes output (AC#1).

#### 7. `crates/asperitas-cli/src/synth.rs` — new, synthetic signal generators

Three generators, all returning `Vec<f32>` mono at a given sample rate:
- **Impulse:** single sample = 1.0, rest zeros
- **Logarithmic sine sweep:** phase integral of log chirp, default 20 Hz → 20 kHz, 1 second
- **Karplus-Strong pluck:** deterministic LCG-seeded noise buffer, 440 Hz default, damping 0.996, 2 seconds

Plus `to_stereo(mono: &[f32]) -> Vec<Frame>` wrapper.

#### 8. `crates/asperitas-cli/src/generate.rs` — new, `run_generate()` function

Clap-derived args: output path, signal type (impulse/sweep/pluck), duration, sample rate. Dispatches to correct generator, stereo-doubles, writes WAV.

#### 9. `crates/asperitas-cli/src/main.rs` — rewrite with clap derive

Top-level Cli with `Process` and `Generate` subcommands. Dispatches to respective module functions.

#### 10. `crates/asperitas-cli/tests/golden_tests.rs` — new, integration tests

Generates deterministic synthetic signals, processes them through Gain and OnePoleLowPass with default params, compares against golden WAV files in `audio/goldens/`. Uses `UPDATE_GOLDENS=1` env var for explicit opt-in regeneration (AC#5). Six test cases: gain×{impulse,sweep,pluck} and filter×{impulse,sweep,pluck}.

#### 11. `audio/goldens/.gitkeep` — new directory marker

Golden WAVs generated during implementation via `UPDATE_GOLDENS=1 cargo test -p asperitas-cli`.

### Implementation Order

1. Cargo.toml — add clap, hound dependencies + [lib] section
2. lib.rs — minimal library root exposing synth and wav_io
3. wav_io.rs — read/write helpers (needed by everything downstream)
4. synth.rs — signal generators (needed by generate command and tests)
5. gain.rs / filter.rs in asperitas-dsp — add parse_params_from_cli methods
6. process.rs — run_process() wiring
7. generate.rs — run_generate() wiring
8. main.rs — clap CLI shell tying modules together
9. Verify build: `cargo build -p asperitas-cli`
10. Smoke test: generate an impulse, process it, verify output WAV is valid
11. golden_tests.rs — integration tests with UPDATE_GOLDENS mechanism
12. Generate initial goldens: `UPDATE_GOLDENS=1 cargo test -p asperitas-cli`
13. Verify goldens pass: `cargo test -p asperitas-cli` (without UPDATE_GOLDENS)
14. Clippy: `cargo clippy -p asperitas-cli --all-targets -- -D warnings`

### Risks & Mitigations

| Risk | Mitigation |
|------|-----------|
| Karplus-Strong buffer shift is O(n) per sample | Acceptable for test generation (not real-time). Replace with ring buffer if needed later. |
| hound may not support all WAV variants | We only read/write 16-bit PCM stereo. Explicit validation rejects others with clear errors. |
| Golden files drift due to compiler changes | Unlikely for f32 ops on same target. Investigate before regenerating. |
| Log sweep phase integral edge cases | Clamp to reasonable defaults (f_start >= 20 Hz, f_end <= 20 kHz). |

### Why No Sub-Tickets

All five acceptance criteria share infrastructure (WAV I/O, CLI skeleton, synth generators). None can be independently tested until the full chain exists. The individual modules are small (<100 lines each) and ship atomically, following the same pattern as TASK-007.
<!-- SECTION:PLAN:END -->
