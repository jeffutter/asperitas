---
id: TASK-019.03
title: Verify knob-driven behaviour objectively through the Pod self-loopback
status: Done
assignee:
  - '@human'
created_date: '2026-09-09 01:27'
updated_date: '2026-10-16 03:36'
labels:
  - planned
dependencies:
  - TASK-019.03.04
references:
  - audio/captures/2026-10-07-rig-sine-300s.console.zst
documentation:
  - docs/reference/daisy-pod.md
parent_task_id: TASK-019
priority: high
type: feature
ordinal: 46000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-019.02 asks eight questions at once and several of them are measurements wearing a listening-test label: whether a parameter moves the signal in the expected direction across its travel, whether glitches appear over minutes of running, and whether the device agrees with asperitas-cli on the same source file. Those have measurable answers, and leaving them bundled with genuine judgement is what keeps the whole milestone waiting on a person.

This ticket takes the measurable half now that the rig can drive a parameter and record the result. What remains in TASK-019.02 is what only ears decide: whether the effect is musically right, whether it responds well to real playing, whether the sound is the intended sound. That split is deliberate — the criteria staying human are the ones where a number would be a proxy nobody checked.

Host override must enter upstream of smoothing, per TASK-032's constraint, or these measurements describe a signal path the product does not ship.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 With a stimulus excerpt replayed from QSPI by TASK-038 and its return captured, each mapped parameter is swept under host control across its full range and the measured response moves monotonically in the intended direction, with no flat regions.
- [x] #2 A continuous run of at least five minutes shows no silence dropouts and no discontinuity candidates above threshold, including while parameters are moving.
- [x] #3 The step-artifact detector reports nothing for the shipped smoothing and does report for an intentionally unsmoothed build, so the clean result is known to be evidence rather than an insensitive test.
- [x] #4 Device-through-loopback output agrees with asperitas-cli output within the tolerance defined by TASK-035 on at least one named excerpt, and on more where the QSPI budget allows — the count is bounded by flash shared with the firmware image, not by patience.
- [x] #5 Capture artifacts and metric summaries are committed and referenced from this ticket, per the standard that a measurement is the evidence and a successful build is not.
- [x] #6 Any disagreement found is filed as its own bug ticket rather than resolved by loosening a threshold.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 4fab0e4

Approach: the rig can only apply a static replay effect (CTL FX) and cannot be swept during capture, while main.rs has the live override path but no loopback capture. Gaps: (a) live override on the rig plus an unsmoothed feature build (.01); (b) sweep monotonic/flat-region analysis (.02); (c) soak analysis with expect-failure sensitivity (.03); (d) hardware run and committed evidence (.04, HUMAN). Order: .01 and .02 in parallel, .03 after .01, .04 last. Parent is @human because .04 is @human (CLAUDE.md inheritance rule); all direct work is in children, so status is Blocked. AC #4 and #6 are covered by .04; AC #5 by .04.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Renamed 2026-09-09 from 'the loopback bench' to the Pod self-loopback, and added TASK-038 as a dependency, when the externally purchased audio interface was dropped in favour of patching the Pod's own output into its input. Nothing in the criteria changed; what changed is where the samples come from and the fact that they no longer cross a second clock domain.

Stimulus budget corrected 2026-09-09: criterion #4 originally asked for three files from audio/instruments/. The 8 MB QSPI flash also carries the firmware image via XIP, so realistic excerpt space is on the order of a megabyte or two — roughly one to two 10 s mono 16-bit excerpts. Asking for three would have quietly meant overwriting the application. Criterion #4 now names one excerpt as the requirement and treats extra coverage as budget permitting.

Correction 2026-10-15 (TASK-038.04.06): the XIP clause above is wrong for this stack. The default layout links the application at 0x08000000 in internal flash and has no execute-in-place path from QSPI; the bootloaded layout has the stock Daisy bootloader copy an image from QSPI at 0x90040000 into AXI SRAM, which still is not XIP. Either way the excerpt area (0x100000 to 0x7FF000 within the QSPI, 14 slots of 516,096 bytes each) is a separate region, so the budget is far larger than one to two clips. See "Instrument excerpts in QSPI" in docs/reference/daisy-seed3.md.

Sweeping parameters under host control still exercises smoothing, since TASK-032 injects upstream of it; the excerpt choice affects signal realism only.

Loopback evidence from TASK-038.05 (2026-10-07/08), the measured baseline this ticket compares against:
- Archived capture: audio/captures/2026-10-07-rig-sine-300s.console.zst (rig at eed633b, sine -20 dBFS 1 kHz, 300 s, 879/879 blocks). Re-derive the PCM with: zstd -dc <file> | cargo run -p asperitas-logging --release --example dump_reassemble -- --out capture.pcm
- Loop gain -0.41 dB at -20 dBFS / 1 kHz; flat to +/-0.03 dB from 150 Hz to 11 kHz (ESS capture); noise + distortion after removing the tone 0.35 LSB RMS (~76 dB, the 16-bit floor); DC -0.49 LSB; channel mapping straight, no crosstalk above the floor.
- Timing: max_block_us 72 of 666, worst_gap_us 714 of 832; dump 69.4 s for 300 s (414.9 kB/s PCM).
- Summary table: README 'Measurement rig'. Full analysis: TASK-038.05 notes. Setup: docs/reference/daisy-pod.md 'Self-loopback channel mapping (measured)'.
- Caveats: the ESS build misses its first 2 s (20-112 Hz) until TASK-038.08 lands. Timing figures are only valid after a RESET-button boot, not straight after a probe flash.

2026-10-15: bench evidence for TASK-019.03.04 is in commit 2c3bb45 (audio/captures/2026-10-15-rig-{sweep,soak,hold}-*.console.zst, captures/task-019-03-04-*.json, docs/reference/asperitas-rig.md 'Measured on the board'). Gain and low-pass sweeps pass; the 300 s triangle soak passes; the unsmoothed build fires the step detector on the same snap schedule where the smoothed build does not; device agrees with asperitas-cli on the mandolin excerpt at held settings through SET. Awaiting the owner's sign-off on TASK-019.03.04.

2026-10-15: closed with the owner, who agreed all six criteria were met after TASK-019.03.04 was signed off. Children Done: .01 live override and unsmoothed build, .02 sweep analysis, .03 soak analysis, .04 bench run, .05 effect after any stimulus generator (added when .04 found the effect existed only on the one-shot excerpt replay). AC mapping: #1 sweeps for gain (sine) and low-pass (pulse train) pass monotonic, no flat region; wording note: the criterion names an excerpt replayed from QSPI, but the sweeps need a steady tone, so they ran on the sine and pulse train by agreement, and the excerpt was used for the held-setting comparisons; #2 300 s triangle soak with 5001 SET frames, no silence, no discontinuity, no step; #3 step detector 0 events on the smoothed build and 4 on the unsmoothed build on an identical snap schedule; #4 mandolin_single_note_soft agrees with asperitas-cli at held gain +6 dB and low-pass 2 kHz through SET, and at three FX settings in TASK-035.07.03; #5 evidence in 2c3bb45; #6 no disagreement found, none filed, no threshold changed.
<!-- SECTION:NOTES:END -->
