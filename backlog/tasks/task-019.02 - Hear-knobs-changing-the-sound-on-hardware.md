---
id: TASK-019.02
title: Hear knobs changing the sound on hardware
status: Blocked
assignee:
  - '@human'
created_date: '2026-08-05 17:27'
updated_date: '2026-10-09 10:55'
labels: []
dependencies:
  - TASK-019.01
  - TASK-019.03
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
- [ ] #1 HUMAN: turning knob 1 audibly changes the sound in the intended direction across its full travel, and the character of the change is what the mapping was meant to produce — not merely that some change occurs
- [ ] #2 HUMAN: turning knob 2 does the same for its mapped parameter
- [ ] #3 HUMAN: no zipper noise, stepping or crackle is audible by ear while a knob moves at playing speed, confirming that the automated detector's threshold from TASK-019.03 matches what a person actually hears
- [ ] #4 HUMAN: no unwanted modulation is audible while a knob is held still at an arbitrary position
- [ ] #5 HUMAN: behaviour survives unplugging and restoring power
- [ ] #6 Any gain-staging or mapping finding worth keeping is written into docs/reference/daisy-pod.md or the shared mapping documentation
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Restructured 2026-09-09: three of the original eight criteria were measurements rather than listening tests and moved to TASK-019.03, which the rig can run unattended — monotonic response across knob travel (was #1/#2), absence of dropouts and glitches over minutes of running (was #5), and agreement between device output and asperitas-cli on the same source file (was #6).

What stays here is what a number would only stand in for: whether the change has the intended musical character, whether the artefacts the detector is tuned against are actually audible, and whether behaviour survives a power cycle. Surviving a software restart moved to TASK-033, since that ticket exercises the host-initiated restart command.

Depends on TASK-019.03 so measurements pass before anyone is asked to listen — there is no point spending ears on a mapping a script can already show is non-monotonic.

2026-10-09: Set to Blocked at the owner's request while they have no physical access to the board. This ticket flashes or runs firmware on the Seed3, which can wedge it (debug port stuck until USB-C is replugged, bad bootloader or link layout) in a way that needs hands to recover. Return to To Do when the owner is back at the bench.
<!-- SECTION:NOTES:END -->
