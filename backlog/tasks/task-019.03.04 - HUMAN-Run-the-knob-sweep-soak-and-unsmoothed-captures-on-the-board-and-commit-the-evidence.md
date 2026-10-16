---
id: TASK-019.03.04
title: >-
  HUMAN: Run the knob-sweep, soak and unsmoothed captures on the board and
  commit the evidence
status: To Do
assignee:
  - '@human'
created_date: '2026-10-16 01:01'
updated_date: '2026-10-16 03:28'
labels:
  - task
dependencies:
  - TASK-019.03.01
  - TASK-019.03.02
  - TASK-019.03.03
  - TASK-019.03.05
parent_task_id: TASK-019.03
priority: high
ordinal: 176800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Needs the board and loopback cable. After TASK-019.03.01 to .03 land: (1) flash default rig, replay a named excerpt, sweep each mapped parameter full range with asperitas-rig override, run the sweep analysis (TASK-019.03.02); (2) run 5+ minutes with parameters moving, run soak analysis (.03); (3) flash the unsmoothed build and show soak analysis fails on it; (4) compare device-through-loopback with asperitas-cli at the swept settings on the named excerpt (more excerpts if QSPI budget allows); (5) commit captures and metric summaries and reference them from TASK-019.03; (6) file any disagreement as its own bug ticket rather than loosening a threshold. Boot via the RESET button before timing claims.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: Sweep captures for every mapped parameter pass the monotonic, no-flat-region analysis on the board
- [ ] #2 HUMAN: A 5+ minute swept capture passes soak analysis, and the unsmoothed build fails it
- [ ] #3 HUMAN: Device agrees with asperitas-cli within the TASK-035 tolerance on at least one named excerpt
- [ ] #4 HUMAN: Capture artifacts and summaries are committed and referenced from TASK-019.03; disagreements filed as bugs
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-15: not run yet. While preparing the unattended session the agent found the session cannot be done on the current firmware: the replay effect (CTL FX/SET/REL) only runs on the one-shot stim-excerpt replay (a 3 s clip, single pass), but the sweep and soak analyses judge a steady tone and the soak wants a 5+ minute sine. Agreed with the owner: TASK-019.03.05 makes the effect a stage after the sine generator first; this ticket now depends on it. Part of #3 is already covered: asperitas-cli agreement at three held settings on the mandolin excerpt passed on hardware in TASK-035.07.03 (gain +6 dB, gain -12 dB, low-pass 2 kHz). After .05 lands the session is: flash the sine+effect build by the probe-reset and dfu-util -w flow, sweep with asperitas-rig override --knob 0, run analyze --sweep, take the 5+ minute soak with analyze --soak, flash the unsmoothed build and run analyze --soak --expect-fail, commit the evidence.

2026-10-15 (agent ran the whole bench session unattended at the owner's request; NOT closed - the HUMAN criteria still need the owner's sign-off). Method: scripts/rig-session.py (probe reset, fresh BOOT record required, CTL FX on attach, CTL SET at 20/s, REL, single console reader); images flashed by probe-rs reset plus dfu-util -w, no button presses; cycle counter live (max_block_us nonzero). Evidence committed in 2c3bb45: audio/captures/2026-10-15-rig-{sweep,soak,hold}-*.console.zst and captures/task-019-03-04-*.json; full table and lessons in docs/reference/asperitas-rig.md 'Measured on the board (TASK-019.03.04)'. AC #1 (sweeps for every mapped parameter): gain sweep on the sine passes (rising, monotonic, no flat region, span 68.2 dB); low-pass sweep on the pulse train passes (span 36.9 dB; the bottom three buckets read the -84.3 dB measurement floor, under the 3-step flat limit). Both mapped parameters are swept; the live override serves knob 0 only. AC #2 (5+ minute swept soak passes, unsmoothed fails): a 300 s soak with a 250 s triangle 0.50-0.95-0.50 and 5001 SET frames passes (no silence, no discontinuity, largest jump 6.9 LSB, no step, largest 0.016 dB/ms). The unsmoothed build on an identical schedule with two +8.6 dB snaps fires the step detector 4 times at the snap edges (largest 8.65 dB/ms) and analyze --soak --expect-fail exits 0; the smoothed build on the same snap schedule has 0 step events (largest 2.1 dB/ms). Caveat the owner should weigh: the discontinuity detector flags any snap even on the smoothed build (72 candidates, all at the snaps), so the unsmoothed 'fail' demonstrated is the step detector's, which is what TASK-035.04 specified; the 5-minute pass is on the gentle triangle. Also two exploratory soaks failed on my own schedule and are kept as reports only: an 18 dB snap defeats the smoothing even for the step detector (6.3 dB/ms), and a ramp ending away from the base value snaps at REL (19 discontinuities at 261 s while the 250 s ramp itself was clean). No threshold was changed. AC #3 (device agrees with asperitas-cli within the TASK-035 tolerance on a named excerpt): mandolin_single_note_soft held through the SET path at gain +6 dB (loop gain within 0.03 dB of the model, residual RMS -30.7 dB) and low-pass 2 kHz (-32.9 dB), both pass; also passed at three FX settings in TASK-035.07.03. AC #4 (artifacts committed and referenced from TASK-019.03; disagreements filed): committed in 2c3bb45 and referenced from TASK-019.03; no disagreement between device and reference was found, so none filed. Callback cost with the effect on and the knob moving: max_block_us 52 (47 without the effect), worst_gap_us 696, no overruns, no drops; pulse-train build 88. Not done: no listening check (that is TASK-019.02).
<!-- SECTION:NOTES:END -->
