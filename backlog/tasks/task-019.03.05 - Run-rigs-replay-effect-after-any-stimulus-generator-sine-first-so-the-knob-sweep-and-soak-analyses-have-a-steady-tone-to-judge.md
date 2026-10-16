---
id: TASK-019.03.05
title: >-
  Run rig's replay effect after any stimulus generator (sine first), so the knob
  sweep and soak analyses have a steady tone to judge
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 01:57'
labels:
  - task
dependencies: []
references:
  - firmware/src/bin/rig.rs
  - crates/asperitas-dsp/src/replay_fx.rs
  - docs/reference/asperitas-rig.md
parent_task_id: TASK-019.03
priority: high
type: feature
ordinal: 177800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found 2026-10-15 while preparing TASK-019.03.04. `asperitas-measure analyze --soak` judges a 5+ minute SINE capture taken while a knob moves, and `--sweep` assumes a steady tone, but the `ReplayFx` effect (CTL FX / CTL SET / CTL REL, TASK-035.07.01 and TASK-019.03.01) only runs inside `ExcerptPlayer::process_block` in firmware/src/bin/rig.rs, on the `stim-excerpt` build. That build plays a 3 s mandolin clip ONCE and then goes silent (the replay core is a single pass). So on the current firmware a 5 minute soak with a moving knob cannot be captured, and a sweep over the clip measures the note's own decay, not the effect's response. Every other generator (sine, ESS sweep, pulse) has no effect stage, and without `stim-excerpt` every `CTL FX` is answered `CTLR ERR unknown-verb`.

Make the effect a stage that runs after whichever stimulus generator the build selected, starting with the sine (the default, which the soak and the TASK-034 baseline are defined on). The same controls must work there: `CTL FX f=.. v=..` before the capture arms, and `CTL SET k=00` / `CTL REL k=00` live during a capture. Knob 0 only stays as it is. Keep the excerpt replay working exactly as it does now (TASK-038.09, TASK-035.07.03 evidence must still reproduce).

Constraints to carry through: (1) with no `FX` sent (bypass, the default) every build is bit-identical to today, so the existing sine/sweep/pulse captures and the TASK-038.05 baseline do not change; (2) the effect runs after the generator and before the encoder and the capture is of the input, as now; (3) `unsmoothed-params` must still build the effect without smoothing so the soak's negative control can be taken on the sine path; (4) the SAI callback gains no await, lock-held flash access or allocation, and the added cost is estimated against the roughly 590 us of 666 us headroom (the 2026-10-15 replay measurement was max_block_us 19 with the effect on, 15 without); (5) RIGGEN/describe() states the active effect so a capture says what was played; (6) the rig build matrix in CI and in scripts/gates.sh covers the new combinations (sine plus effect, default and unsmoothed), and the docs in docs/reference/asperitas-rig.md say which builds accept which verbs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 rig builds with the replay effect applied after the sine generator (default features plus whatever new feature or cfg the design needs): `CTL FX` is accepted before the capture arms, answered `CTLR OK FX`, and `CTL SET k=00` / `CTL REL k=00` are accepted live during an armed capture; knob 1 and above still answer `bad-knob`
- [ ] #2 With no `FX` sent, the sine build's output is bit-identical to the build before this change (a host test over the pure processing path pins this, and the existing sine, sweep and pulse CI builds are unchanged), so the TASK-034/TASK-038.05 baseline and `asperitas-measure analyze` on the 2026-10-07 sine capture are untouched
- [ ] #3 The stimulus-then-effect chain is a host-testable unit in asperitas-dsp or asperitas-logging (not only in rig.rs): tests cover bypass identity, a gain and a low-pass setting on a sine, and a live knob change, with no board attached
- [ ] #4 `unsmoothed-params` builds the sine+effect path without smoothing, and a host test shows the soak step detector fires on the unsmoothed chain and not on the smoothed one, using the same asperitas-measure detector TASK-019.03.03 ships
- [ ] #5 The excerpt replay path still works unchanged: the existing excerpt host tests and the TASK-035.07 comparison tests pass, and its `CTL FX` behaviour (accepted before arm, `busy` after) is the same
- [ ] #6 RIGGEN or describe() names the active effect, CI and scripts/gates.sh build the new combinations (default and unsmoothed) and clippy is clean, the callback cost added is estimated in a comment against the headroom, and docs/reference/asperitas-rig.md states which builds accept which verbs; no bench claim is made (TASK-019.03.04 owns the hardware run)
<!-- AC:END -->
