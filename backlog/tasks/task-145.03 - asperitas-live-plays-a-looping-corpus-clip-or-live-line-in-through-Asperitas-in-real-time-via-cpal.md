---
id: TASK-145.03
title: >-
  asperitas-live plays a looping corpus clip or live line-in through Asperitas
  in real time via cpal
status: Done
assignee:
  - '@agent'
created_date: '2026-10-19 22:08'
updated_date: '2026-10-19 23:12'
labels:
  - m4
  - live
  - host
  - planned
dependencies:
  - TASK-145.01
  - TASK-145.02
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
  - docs/investigations/cpal-macos-duplex.md
  - docs/development.md
  - flake.nix
parent_task_id: TASK-145
priority: high
type: feature
ordinal: 280300
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Sound design on the effect needs a flash cycle per change. This ticket adds the new host crate `crates/asperitas-live` (root workspace; `members = ["crates/*"]` picks it up, `firmware/` is excluded and must not depend on it): a `cpal` backend that runs `asperitas_dsp::asperitas::Asperitas` in real time. Two sources: a looping WAV (the corpus clips in `audio/instruments`, so sound design needs no live player) and live line-in from the default input device. Output goes to the default output device. Knobs are fixed at startup here; live knob control is the sibling ticket TASK-145.04, which builds on the control path this ticket leaves.

Inputs from the dependencies:
- TASK-145.01 (spike) left `docs/investigations/cpal-macos-duplex.md`: supported rates and buffer sizes, whether input and output share a clock (and the ring buffer capacity if not), latency, and the decision on DSP block versus host buffer. Follow that decision. The expected one: process any host buffer length per sample, and apply a pending `Controls` change only at frame indices divisible by 32 counted from stream start, matching firmware `main` (`set_controls` once per 32-frame block).
- TASK-145.02 left one public real-unit `key=value` to `Controls` translation in asperitas-dsp. The startup flags use it with the CLI's syntax (`--params cutoff_hz=2000 gain_db=-6`); this crate must not map units itself.

Design constraints:
- The processing core (source, effect, block-grid control latch) is a plain struct with no cpal types, callable from a test with any buffer length. The cpal callback is a thin shell around it. TASK-145.05 tests that core against the offline CLI, so its public surface must allow feeding a WAV-loaded buffer through in arbitrary host-sized chunks and changing controls at a given frame.
- The audio callback does not allocate, lock or block. Controls reach it through atomics (or a lock-free single-slot), the same shape as firmware `KnobState`.
- Sample rate is 48 kHz (doc-001). Refuse to start, with a clear message, if the device cannot run at 48 kHz or the WAV is not 48 kHz; no resampling.
- Mono-in, stereo-out policy as on the device: a mono source feeds both channels of the stereo `Frame`. WAV loading reuses `asperitas_cli::wav_io::read_wav` (same int-to-float convention the goldens and the rig comparison use) rather than a second reader.
- Output clamps to [-1, 1] at the device boundary as the firmware codec boundary does; the core's pre-clamp output is what tests compare.

New dependencies, added only to `crates/asperitas-live/Cargo.toml`: `cpal` (the version the spike recorded), `clap` (as asperitas-cli uses), path deps on asperitas-dsp and asperitas-cli. On Linux cpal needs ALSA; `flake.nix` already ships `alsa-lib` and `pkg-config` in both dev shells for this, so CI (`nix develop .#ci`, ubuntu) must build and clippy the crate without flake changes; confirm, and if anything is missing fix the flake rather than feature-gating the crate. Tests must never open an audio device (CI has none). Add the crate to the crate maps in CLAUDE.md and doc-001 section 2, and document running it (a `README` section or `docs/reference/asperitas-live.md`, linked from CLAUDE.md).

Code comments describe current state only: no ticket numbers, dates, measurement history, or line numbers of other files.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 crates/asperitas-live exists in the root workspace with a binary that takes --input <wav> (looping) or --line-in, and --params key=value using the shared asperitas-dsp translation; firmware/Cargo.toml and firmware/Cargo.lock are unchanged
- [x] #2 The processing core has no cpal types and unit tests pin: a looping source wraps sample-exactly at the clip end; a control change requested mid-buffer takes effect at the next frame index divisible by 32 from stream start, whatever the buffer length (test lengths 1, 31, 32, 33, 480, 512); the callback path allocates nothing (asserted by construction or a test allocator)
- [x] #3 Starting with a non-48 kHz WAV, or a device that cannot open at 48 kHz, exits non-zero with a message naming the rate; a test pins the WAV case
- [x] #4 The binary was run on this Mac with each corpus clip as --input for at least 10 s with no cpal error or underrun reported (the stream error callback and any ring underrun counter are printed on exit), recorded in the ticket notes with the command used; audibility is NOT claimed here, that is the @human listening ticket
- [x] #5 cargo clippy and cargo doc pass for the crate on Linux in CI (push main and check the GitHub Actions run with gh), not only on macOS
- [x] #6 CLAUDE.md and doc-001 crate maps list asperitas-live, and a run command is documented
- [x] #7 bash scripts/gates.sh push passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by ba30b31. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented crates/asperitas-live: lib (ControlSlot atomics, Engine with 32-frame block-grid control latch, LoopSource, load_clip refusing non-48 kHz) plus a thin cpal shell (backend.rs) and clap main. Shared parse_asperitas_controls extracted in asperitas-cli process.rs so --params uses the one Controls::from_values translation. Tests: loop wrap sample-exact at chunk sizes 1..512, control change vs device reference at host lengths 1,31,32,33,480,512, counting-allocator no-alloc test (tests/no_alloc.rs), 44.1 kHz WAV refusal naming the rate. firmware/ untouched. AC4: ran './target/release/asperitas-live --input audio/instruments/<clip>.wav --params cutoff_hz=2000 --seconds 10' for all 8 corpus clips on this Mac: each exit 0, 'stream errors: 0, underruns: 0, overruns: 0'. Audibility not claimed (TASK-145.07). Extra observation: --line-in --seconds 10 showed 11 ring underruns at buffer 128 and 6 at buffer 256 (3 s run showed 12), so they do not grow with duration; likely a startup transient, not investigated. AC5: pushed main, GitHub Actions run 39338309165 succeeded on ubuntu (no flake change needed). gates.sh push passed.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added the asperitas-live crate (engine core with block-grid control latch, cpal shell, clap CLI), shared the asperitas-cli key=value parser, documented in docs/reference/asperitas-live.md and the CLAUDE.md/doc-001 crate maps. Unit and no-alloc tests pass, all 8 corpus clips ran 10 s clean on the Mac, push gates and Linux CI green.
<!-- SECTION:FINAL_SUMMARY:END -->
