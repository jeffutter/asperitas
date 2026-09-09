---
id: TASK-019.03
title: Verify knob-driven behaviour objectively through the Pod self-loopback
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 01:27'
updated_date: '2026-09-09 02:14'
labels: []
dependencies:
  - TASK-032
  - TASK-035
  - TASK-038
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
- [ ] #1 With a stimulus excerpt replayed from QSPI by TASK-038 and its return captured, each mapped parameter is swept under host control across its full range and the measured response moves monotonically in the intended direction, with no flat regions.
- [ ] #2 A continuous run of at least five minutes shows no silence dropouts and no discontinuity candidates above threshold, including while parameters are moving.
- [ ] #3 The step-artifact detector reports nothing for the shipped smoothing and does report for an intentionally unsmoothed build, so the clean result is known to be evidence rather than an insensitive test.
- [ ] #4 Device-through-loopback output agrees with asperitas-cli output within the tolerance defined by TASK-035 on at least one named excerpt, and on more where the QSPI budget allows — the count is bounded by flash shared with the firmware image, not by patience.
- [ ] #5 Capture artifacts and metric summaries are committed and referenced from this ticket, per the standard that a measurement is the evidence and a successful build is not.
- [ ] #6 Any disagreement found is filed as its own bug ticket rather than resolved by loosening a threshold.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Renamed 2026-09-09 from 'the loopback bench' to the Pod self-loopback, and added TASK-038 as a dependency, when the externally purchased audio interface was dropped in favour of patching the Pod's own output into its input. Nothing in the criteria changed; what changed is where the samples come from and the fact that they no longer cross a second clock domain.

Stimulus budget corrected 2026-09-09: criterion #4 originally asked for three files from audio/instruments/. The 8 MB QSPI flash also carries the firmware image via XIP, so realistic excerpt space is on the order of a megabyte or two — roughly one to two 10 s mono 16-bit excerpts. Asking for three would have quietly meant overwriting the application. Criterion #4 now names one excerpt as the requirement and treats extra coverage as budget permitting.

Sweeping parameters under host control still exercises smoothing, since TASK-032 injects upstream of it; the excerpt choice affects signal realism only.
<!-- SECTION:NOTES:END -->
