---
id: TASK-008.01
title: 'Build the WAV CLI, synthetic signals, and golden-file mechanism'
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-08-01 05:57'
updated_date: '2026-08-01 18:11'
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
