---
id: TASK-019.02
title: Hear knobs changing the sound on hardware
status: To Do
assignee:
  - '@human'
created_date: '2026-08-05 17:27'
labels: []
dependencies:
  - TASK-019.01
  - TASK-018.04
documentation:
  - docs/reference/daisy-pod.md
parent_task_id: TASK-019
priority: high
type: feature
ordinal: 33000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Requires the board, the Pod, ears, and a signal source. Milestone M3 is met when turning a knob changes what you hear — not when it compiles.

GAIN STAGING FIRST. The Pod 3.5 mm input is line level, not hi-Z instrument level (docs/reference/daisy-pod.md). Feed it from a DI box, a preamp, or an audio interface line output. Thin, quiet, noisy audio is the expected symptom of plugging a pickup straight in, and doc-001 section 7 flags it as a Medium risk precisely because it reads as a DSP bug.

COMPARE AGAINST THE CLI. Run the same processor with the same parameter values through `asperitas process` over a file from audio/instruments/, and check the device is doing the same thing. That comparison is the entire reason TASK-019.01 puts the mapping in asperitas-dsp instead of the firmware — this is where the sharing pays off, or where it turns out not to have worked.

ZIPPER NOISE IS THE SPECIFIC FAILURE TO LISTEN FOR. Audible stepping, crackling, or grit while a knob is moving means smoothing is missing or applied at the wrong rate. doc-001 section 6 lists this as the property that catches missing smoothing; on hardware you simply hear it.

If TASK-018.04 recorded knob jitter, this is where you find out whether that number was small enough. Jitter that was invisible as a logged value can be plainly audible once it modulates a filter cutoff.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: turning knob 1 audibly changes the sound in the expected direction across its full travel
- [ ] #2 HUMAN: turning knob 2 audibly changes its parameter across its full travel
- [ ] #3 HUMAN: no zipper noise, stepping, or crackle while a knob is in motion
- [ ] #4 HUMAN: knob jitter is not audible as unwanted modulation when a knob is held still
- [ ] #5 HUMAN: no dropouts, glitches, or underruns across several minutes of continuous audio, including while knobs are being moved
- [ ] #6 HUMAN: device output and asperitas-cli output agree to the ear on the same source file with the same parameter values
- [ ] #7 HUMAN: behaviour survives a reset and a power cycle
- [ ] #8 Any gain-staging or mapping finding worth keeping is written into docs/reference/daisy-pod.md or the shared mapping documentation
<!-- AC:END -->
