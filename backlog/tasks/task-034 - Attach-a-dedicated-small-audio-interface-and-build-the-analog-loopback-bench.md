---
id: TASK-034
title: Attach a dedicated small audio interface and build the analog loopback bench
status: To Do
assignee:
  - '@human'
created_date: '2026-09-09 01:26'
labels: []
dependencies: []
documentation:
  - docs/reference/daisy-pod.md
priority: high
type: chore
ordinal: 44000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Objective audio verification needs a known stimulus into the Pod input and a recording of what comes back out. Until that exists, every DSP criterion is a listening test, and the project cannot tell a regression from a bad cable.

A deliberately small interface is chosen over the larger ones already in storage on purpose: the friction of dragging a rack unit out and re-cabling it is exactly what keeps verification manual, and fidelity short of the TAC5242's −120 dB noise floor is acceptable for level, glitch, monotonicity and device-versus-CLI comparison work. What that trade-off costs is stated honestly in TASK-035 rather than hidden: absolute noise-floor claims become inadmissible until better conversion exists, and those stay human judgement.

Constraints that disqualify candidate hardware, since this is a purchase decision:
- Inputs must accept line level. Instrument- or microphone-only inputs are useless — the Pod's output is line level (docs/reference/daisy-pod.md records the converse trap for its input).
- Two inputs and two outputs are needed for stereo; single-input interfaces cannot capture a stereo field.
- Class-compliant with no driver install, so it can be driven headlessly from a script.
- Bus-powered from the same port it lives in, since it stays attached.

Two traps to characterize now rather than discover mid-measurement. First, grounding: the interface and the Pod share one host's USB ground, and any resulting hum reads as DSP noise — the same class of misdiagnosis the Pod's line-level input has already caused once. Second, clock domains: the two converters run independently, roughly tens of parts per million apart, which is a few samples per second of slip, so long captures drift and any alignment has to tolerate that.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: an interface meeting the constraints above is acquired and left permanently attached to the host, wired both directions: host output to Pod input, Pod output to host input.
- [ ] #2 HUMAN: a direct loopback baseline is captured with the device bypassed — host output plugged straight into host input — and its level, noise floor and distortion observations are recorded as the reference later measurements compare against.
- [ ] #3 HUMAN: end-to-end levels through the Pod are recorded at a fixed sine and nominal settings, confirming gain staging per docs/reference/daisy-pod.md and confirming no clipping.
- [ ] #4 HUMAN: hum or buzz attributable to grounding is characterized — either eliminated, or documented with its measured amplitude and source.
- [ ] #5 HUMAN: sample rate is fixed at 48 kHz on host and device with no resampling in the path, and observed clock drift over a capture of at least 60 s is recorded as a number so later analysis knows its budget.
- [ ] #6 Wiring, adapter types, nominal levels, drift figure and the hum outcome are written into docs/reference as the lab setup note.
<!-- AC:END -->
