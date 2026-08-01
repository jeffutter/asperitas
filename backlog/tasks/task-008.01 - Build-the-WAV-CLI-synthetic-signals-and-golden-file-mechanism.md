---
id: TASK-008.01
title: 'Build the WAV CLI, synthetic signals, and golden-file mechanism'
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-08-01 05:57'
updated_date: '2026-08-01 18:20'
labels: []
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

Transform the skeleton  into a functional WAV processing tool with two subcommands (, ), a shared parameter-parsing module, synthetic signal generators, and golden-file regression tests. Adds  (CLI arg parsing) and  (WAV I/O) as dependencies.

### Architecture Decisions

**Binary name:** Keep  internally; users invoke it via . The  name stays  (with hyphen) to match the crate name convention.

**Parameter mapping location:** Each processor gets a  static helper method on its own type in . This keeps the mapping close to the Params definition (so future Pod/TUI hosts import it from ) while letting each processor own its own key names. The CLI dispatches to the right parser via a  on .

**Synthetic signal generation:** Lives in . Pure functions returning  mono samples. A wrapper stereo-doubles them. Deterministic: Karplus-Strong uses a fixed LCG seed. All generators output at a specified sample rate and duration.

**Golden-file storage:**  directory in the repo root. Files named by convention: . Goldens are ~160 KB each (1 s, stereo, 16-bit, 48 kHz). Six goldens total (~1 MB) — small enough for git without LFS.

**Golden tolerance:** Per-sample absolute delta < 1e-5 (well above floating-point noise, well below audible threshold at 16-bit). Compares frame-by-frame after confirming matching length and spec.

### Files to Create/Modify

#### 1.  — add dependencies

No dev-dependencies needed beyond what's already in the workspace.

#### 2.  — add 

Add a static helper to :

#### 3.  — add 

Same pattern for :

#### 4.  — rewrite with clap derive

Structure using clap's derive API:

:
-  — input WAV path
-  — output WAV path
-  — processor name
-  — repeated 

Wait, clap's  splits a single arg. Better: use a custom value parser or just accept  and split internally. Actually, simplest approach:

Main dispatch:

#### 5.  — new, WAV read/write helpers

Read WAV into , write  to WAV:

Key design points:
- Validates stereo + 16-bit PCM only (keeps it simple; mono-to-stereo conversion is a later enhancement)
- Converts i16 ↔ f32 with proper scaling ( on read,  + clamp on write)
- Error messages include paths for debugging

#### 6.  — new,  function

This satisfies AC#1 (process runs), AC#2 (shared param mapping), and AC#3 (set_sample_rate with file's actual rate).

#### 7.  — new, synthetic signal generators

Three generators, all returning  mono at a given sample rate:

**Impulse (Dirac delta):**

**Logarithmic sine sweep:**

Default: 20 Hz → 20 kHz, 1 second.

**Karplus-Strong plucked string:**

Default: 440 Hz, damping 0.996, 2 seconds.

**Stereo doubler wrapper:**

#### 8.  — new,  function

#### 9.  — new, integration tests

**Golden regeneration:** When  is set, tests write the actual output instead of comparing. Implemented via a helper:

This satisfies AC#5: goldens regenerate only with explicit 
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 12 tests
test gain_silence_in_silence_out ... ok
test filter_param_change_smooth ... ok
test gain_param_change_smooth ... ok
test filter_silence_in_silence_out ... ok
test gain_reset_idempotent ... ok
test filter_output_always_finite ... ok
test gain_output_bounded ... ok
test filter_block_equals_tick ... ok
test gain_block_equals_tick ... ok
test gain_output_always_finite ... ok
test filter_output_bounded ... ok
test filter_reset_idempotent ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.51s

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s.

#### 10.  — new directory with 

Initial commit includes . Golden WAVs are generated by running 
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s during implementation.

#### 11.  — new (for test imports)

Since , , etc. are in  siblings, the integration tests can't access them directly. Two options:
- Make a  re-exporting the modules (tests link against  as a lib)
- Duplicate the WAV read logic in tests (DRY violation)

Go with option A — create a minimal :

And update  to also declare :

### Implementation Order

1. **Cargo.toml** — add ,  dependencies +  section
2. **lib.rs** — minimal library root exposing  and 
3. **wav_io.rs** — read/write helpers (needed by everything downstream)
4. **synth.rs** — signal generators (needed by  command and tests)
5. **gain.rs / filter.rs** — add  methods in 
6. **process.rs** —  wiring
7. **generate.rs** —  wiring
8. **main.rs** — clap CLI shell tying modules together
9. **Verify build:** 
10. **Smoke test:** generate an impulse, process it, verify output WAV is valid
11. **golden_tests.rs** — integration tests with UPDATE_GOLDENS mechanism
12. **Generate initial goldens:** 
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
13. **Verify goldens pass:** 
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s (without UPDATE_GOLDENS)
14. **Clippy:** 

### Risks & Mitigations

| Risk | Mitigation |
|------|-----------|
| Karplus-Strong buffer shift is O(n) per sample — slow for long outputs | Acceptable for test generation (not real-time). If performance matters, replace with ring buffer later. |
| hound may not support all WAV variants (e.g., float32 WAVs) | We only read/write 16-bit PCM stereo. Explicit validation rejects others with clear errors. |
| Golden files drift due to compiler/toolchain changes | Unlikely for integer-arithmetic-only processing (Gain and OnePoleLowPass use f32 ops, but results are deterministic across compilers for the same target). If drift occurs, investigate before regenerating. |
| Log sweep phase integral formula may have edge cases | Clamp to reasonable defaults (f_start ≥ 20 Hz, f_end ≤ 20 kHz). Test with known-good values first. |

### Why No Sub-Tickets

All five acceptance criteria share infrastructure (WAV I/O, CLI skeleton, synth generators). None can be independently tested until the full chain exists. The individual modules are small (<100 lines each) and ship atomically.
<!-- SECTION:PLAN:END -->
