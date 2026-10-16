---
id: TASK-019.03.04
title: >-
  HUMAN: Run the knob-sweep, soak and unsmoothed captures on the board and
  commit the evidence
status: To Do
assignee:
  - '@human'
created_date: '2026-10-16 01:01'
updated_date: '2026-10-16 01:57'
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
<!-- SECTION:NOTES:END -->
