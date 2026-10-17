---
id: TASK-082
title: >-
  DSP: one top-level effect that owns knob mapping, chaining and the parameter
  table; in-place block API
status: Blocked
assignee:
  - '@agent'
created_date: '2026-10-16 18:12'
updated_date: '2026-10-16 22:35'
labels:
  - review-2026-10
  - dsp
  - architecture
  - planned
dependencies:
  - TASK-082.01
  - TASK-082.02
  - TASK-082.03
  - TASK-082.04
priority: high
type: chore
ordinal: 184800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Nothing in asperitas-dsp owns 'the effect'. Which knob drives which parameter is decided in firmware main.rs:201-202; the filter-then-gain chain is hand-wired with three stack buffers (main.rs:205-213); rig has a second composition (replay_fx.rs + fx_chain.rs: one effect at a time, one knob, wire codes in FxKind::from_code); the CLI has its own string dispatch (process.rs:23-46); each processor has stringly parse_params_from_cli behind a std feature (gain.rs:98-119, filter.rs:109-130). So rig cannot measure what main ships, a new effect touches about eight places, and the M4 TUI would copy main.rs:201-202. The Processor block method also takes (&[Frame], &mut [Frame]) which cannot alias, forcing the three buffers (processor.rs:34-38).

Change: add one top-level processor (e.g. Asperitas) with set_controls(&Controls{knobs:[f32;N],..}) that owns the chain and maps controls to parameters; give each processor one declarative parameter table (name, unit, range, taper, default) from which the knob mapping, CLI key=value parsing (moved to the CLI), TUI labels and proptest strategies derive, so dsp no longer needs std; make the provided block method in place. main and rig run the same chain; ReplayFx/FxKind shrink to bypass-or-patch or go. No generic plugin graph.

DECISIONS NEEDED FIRST: the control-rate model for M6 (is modulation block-rate set_params through the smoothers, or per-sample modulation inputs?), and whether this top-level effect owns a soft-clip output stage.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 main.rs and rig run the same asperitas-dsp chain; the knob mapping lives in asperitas-dsp only
- [ ] #2 CLI process, the future TUI and the firmware build the chain from the same parameter table
- [ ] #3 Block processing is in place with no triple buffering in main.rs; max_block_us is not worse by more than a few us
- [ ] #4 Hardware recheck is covered by the review hardware-recheck ticket
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against d9ed6ab
Epic-style tracking ticket; all work is delegated. Owner decisions already recorded: block-rate modulation, no soft-clip stage.
Order: .01 (in-place block API) and .02 (parameter tables) are independent; .03 (Asperitas effect) needs both; .04 (move main, rig, CLI onto it) last.
Verification: host gate and goldens bit-identical after each step; all firmware variants build; max_block_us recheck on hardware is covered by TASK-120 (AC #4).
No remaining work outside the subtasks.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-16 owner decision: Modulation (M6) is block-rate: the envelope follower and random walk set parameters once per audio block through each processor's existing smoother; no per-sample modulation inputs. The top-level effect has no soft-clip stage; the hard clamp at the codec boundary stays. Revisit by ear in M5.
<!-- SECTION:NOTES:END -->
