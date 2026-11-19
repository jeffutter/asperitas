---
id: TASK-152.05
title: >-
  HUMAN: run an automated knob session on the board and compare it with the
  offline render
status: Done
assignee:
  - '@human'
created_date: '2026-11-13T22:01:00Z'
updated_date: '2026-11-19T03:36:00Z'
labels:
  - automation
  - rig
  - hardware
dependencies:
  - TASK-152.04
  - TASK-152.02
documentation:
  - docs/reference/asperitas-rig.md
  - docs/reference/flashing.md
  - audio/README.md
parent_task_id: TASK-152
priority: medium
type: task
ordinal: 292700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The device half of this umbrella: the offline render with an automation file (TASK-152.01) is the stand-in the agents will rely on, and the rig session (TASK-152.04) lets the board run the same file. This ticket shows the board, with the knobs moving, agrees with the offline render on real audio, the way TASK-145.06 did for held knobs. It is @human because it needs the Pod rigged with its self-loopback cable and the board flashed.

Board set-up is the one used in TASK-145.06: the rig `stim-excerpt` image from HEAD, bootloaded layout, an installed corpus excerpt (check the console's EXCPLAY crc16 against the clip the reference uses; a slot can hold a different clip than assumed), the ST-Link attached for `--probe-reset`, and `--device` set to the Seed3's console node when two usbmodem nodes are present. An agent may prepare the commands, build and flash, and analyse the captures with the owner live; the close is the owner's.

Suggested automation: one file that moves both knobs, with a ramp on knob 0 (cutoff, log taper), a step on knob 1 (gain) and a hold, long enough to cover the excerpt. Commit captures, sidecars and reports under captures/TASK-152.05/.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 HUMAN: the rig image built from HEAD after TASK-152.04 is flashed with an excerpt in the slot the reference uses, and the automation file used is committed beside the captures
- [x] #2 HUMAN: an automated session with moving knobs passes asperitas-measure analyze --excerpt against the offline render of the same clip and file, within the comparison's documented limits
- [x] #3 HUMAN: a negative control fails as it should (the same capture analysed against a different automation file or against bypass)
- [x] #4 HUMAN: captures, sidecars and reports are committed under captures/TASK-152.05/ and the ticket notes record the commands used
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-18: Closed live with the owner, who asked for the ticket to be closed once every criterion was met; each criterion's evidence was shown in the session.

Setup: rig built from d6202ab (`ASP_RIG_CAPTURE_SECONDS=10 make build LAYOUT=bootloaded BINARY=rig FEATURES="seed3 stim-excerpt"`), flashed with `dfu-util -w -a 0 -s 0x90040000:leave -D rig-bootloaded.bin`. Slot 0 already held mandolin_single_note_soft (EXCPLAY crc16=15fd want=15fd clean=1). Console /dev/cu.usbmodem101. The ST-Link could not reach the chip (JtagGetIdcodeError), so each session followed `asperitas-rig restart` (software reset) instead of `--probe-reset`. The firmware is unchanged by the fix below (host only), so the flashed image is the HEAD image.

Automation: captures/TASK-152.05/knobs.txt (cutoff ramp 8000 to 400 Hz arriving at 0.5 s, gain step to -9 dB at 1.5 s, cutoff ramp to 4000 Hz arriving at 2.0 s, then hold; clip is 3.0 s).

Commands: `asperitas-rig restart --device /dev/cu.usbmodem101`, then `asperitas-rig session --out captures/TASK-152.05/2026-11-18-rig-automation-excerpt.console.zst --device /dev/cu.usbmodem101 --fx-f 1 --max-seconds 60 automation captures/TASK-152.05/knobs.txt`, then `asperitas-measure analyze <capture> --excerpt audio/instruments/mandolin_single_note_soft.wav`.

First two runs failed (captures/TASK-152.05/failed-late-clock/): alignment slip at window 1 (0.52 and 0.78 samples). Cause, verified on the board: the session started the script clock at the first CAPSTAT with delivered>0, which arrives once a second (3205 ms), while the clip started at about 2305 ms, so every knob change landed about 0.9 s late in clip time. A gain-step script showed the device step at 1.40 s instead of 0.5 s; rendering the step at 1.41 s dropped the residual from -11.0 to -30.4 dB. Also found: the session released the knobs 0.1 s after the last SET while the render holds the last value. Fixed in a7d3a95 (clock from the `rig: capture armed` log record, release at dump end).

Result after the fix: session sets_sent=203 set_acks=203 rel_acks=2 device_errors=0, arm_t_ms=2305. analyze --excerpt passes: delay 81 samples, loop gain deviation 0.036 dB, residual RMS -29.7 dB (limit -25), peak -26.8 dB (limit -15). Run once; repeatability on the fixed build was not measured.

Negative controls (both fail, alignment slip): the same capture against a different automation file (1.8 samples) and against bypass (2.9 samples). Made from copies of the capture with an edited sidecar; reports in captures/TASK-152.05/negative-control-vs-*.report.json (their `capture` field points at the scratch copy).

Not covered: a cutoff-step-only script was not run on the board.
<!-- SECTION:NOTES:END -->
