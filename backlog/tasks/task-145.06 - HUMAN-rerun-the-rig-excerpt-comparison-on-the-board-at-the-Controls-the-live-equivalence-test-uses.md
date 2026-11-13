---
id: TASK-145.06
title: >-
  HUMAN: rerun the rig excerpt comparison on the board at the Controls the live
  equivalence test uses
status: Done
assignee:
  - '@human'
created_date: 2026-10-19 22:10
updated_date: '2026-11-13T18:37:00Z'
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
- [x] #1 HUMAN: the rig image built from HEAD after TASK-145.02 and TASK-145.05 is flashed, and the knob levels used are recorded in the notes with the real-unit values they map to
- [x] #2 HUMAN: a hold session with the patch selected and knob 0 at the cutoff_hz=2000 level, and one with knob 1 at the gain_db=-6 level, each pass asperitas-measure analyze --excerpt
- [x] #3 HUMAN: a negative control (one capture analysed against the other setting, or against bypass) fails as it should
- [x] #4 HUMAN: captures, sidecars and reports are committed under captures/TASK-145.06/
- [x] #5 HUMAN: if TASK-145.02 changed firmware/src, main is flashed and both Pod knobs are heard to move cutoff and gain as before; otherwise the notes say it was not needed
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-13: Closed live with the owner, who walked through and agreed each criterion.

Setup: rig built from HEAD (`seed3 stim-excerpt`, bootloaded, ASP_RIG_CAPTURE_SECONDS=10), flashed by probe reset + dfu-util -w. Console is /dev/cu.usbmodem101 (the other usbmodem node is the ST-Link; pass --device). Slot 0 held octave_mandolin_single_note_soft (crc16 3add), not the mandolin clip; the first runs were compared against the wrong clip and failed alignment. mandolin_single_note_soft was reinstalled (EXCOK crc16=15fd) and both sessions rerun. A bypass capture also slipped (80.94 vs 80.54 samples) against the octave clip on both current and pre-2026-10-16 firmware, so that clip is a poor alignment target.

Knob levels (ParamSpec): knob 0 = 0.666667 -> cutoff_hz 2000 (log, 20..20000); knob 1 = 0.75 -> gain_db -6 (linear, -60..12).

Results vs asperitas-cli, full Asperitas patch: cutoff 2 kHz delay 81, loop gain -0.009 dB vs model, residual RMS -32.2 dB, peak -29.6 dB, pass. Gain -6 dB delay 81, +0.005 dB, RMS -30.4 dB, peak -21.9 dB, pass. EXCPLAY clean=1, no overruns or drops.

Negative controls (all fail): gain capture vs cutoff setting - alignment slip; cutoff capture vs bypass - alignment slip; gain capture vs bypass - loop gain 6.0 dB off baseline. Reports: captures/TASK-145.06/negative-control-*.report.json.

AC #5: TASK-145.02 changed only a doc comment under firmware/src, so main was not flashed.
<!-- SECTION:NOTES:END -->
