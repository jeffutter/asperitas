---
id: TASK-145.06
title: >-
  HUMAN: rerun the rig excerpt comparison on the board at the Controls the live
  equivalence test uses
status: To Do
assignee:
  - '@human'
created_date: '2026-10-19 22:10'
labels:
  - m4
  - live
  - rig
  - hardware
dependencies:
  - TASK-145.02
  - TASK-145.05
documentation:
  - docs/reference/asperitas-rig.md
  - docs/reference/flashing.md
  - audio/README.md
parent_task_id: TASK-145
priority: medium
type: task
ordinal: 280600
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Device half of the M4 exit criterion. TASK-145.05 shows the live engine is bit-identical to `asperitas-cli process` and to the no_std `ReplayFx` the rig firmware runs. This ticket shows the board, running that code, agrees with the same reference on real audio: the rig excerpt comparison (`asperitas-measure analyze --excerpt`, reference = the CLI path at the session's effect and knob level).

Why a fresh bench run rather than folding into an existing recheck: the last processed-excerpt comparisons (TASK-035.07.03, TASK-019.03.04 hold captures, 2026-10-15) ran on an earlier build where `CTL FX` chose a lone gain or low-pass; the current rig runs the full `Asperitas` patch with knob overrides, and no open recheck ticket covers it. TASK-145.02 may also have touched code the firmware links; its notes say whether `firmware/src` changed.

Tolerance: the comparison's documented limits (`asperitas-measure` thresholds: residual RMS below -25 dB and peak below -15 dB relative to the reference, loop gain on baseline). This is limited by the analog loopback, not float precision; the float-level claim rests on TASK-145.05 plus the device running the same `asperitas-dsp` code.

Suggested sessions, one knob per hold session (the session takes one `--knob`), on the stim-excerpt rig image with an installed corpus excerpt: patch with knob 0 held at the normalised level of cutoff_hz=2000, and patch with knob 1 held at the level of gain_db=-6 (compute both with the TASK-145.02 translation and record them). Command shapes are in docs/reference/asperitas-rig.md (Bench session, analyze --excerpt). Commit captures, sidecars and reports under captures/TASK-145.06/.

An agent may prepare the commands and analyse the captures with the owner live, but the board work and the close are the owner's.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: the rig image built from HEAD after TASK-145.02 and TASK-145.05 is flashed, and the knob levels used are recorded in the notes with the real-unit values they map to
- [ ] #2 HUMAN: a hold session with the patch selected and knob 0 at the cutoff_hz=2000 level, and one with knob 1 at the gain_db=-6 level, each pass asperitas-measure analyze --excerpt
- [ ] #3 HUMAN: a negative control (one capture analysed against the other setting, or against bypass) fails as it should
- [ ] #4 HUMAN: captures, sidecars and reports are committed under captures/TASK-145.06/
- [ ] #5 HUMAN: if TASK-145.02 changed firmware/src, main is flashed and both Pod knobs are heard to move cutoff and gain as before; otherwise the notes say it was not needed
<!-- AC:END -->
