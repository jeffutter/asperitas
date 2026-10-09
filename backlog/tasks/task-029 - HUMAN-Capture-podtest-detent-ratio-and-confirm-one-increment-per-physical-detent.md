---
id: TASK-029
title: >-
  HUMAN: Capture podtest detent ratio and confirm one increment per physical
  detent
status: Blocked
assignee:
  - '@human'
created_date: '2026-09-09 00:27'
updated_date: '2026-10-09 10:55'
labels:
  - review-followup
dependencies:
  - TASK-024
  - TASK-031
references:
  - ~/podtest.log
documentation:
  - docs/reference/daisy-pod.md
priority: high
type: task
ordinal: 120
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-024 fixes ENCODER_LUT so the Pod's detented encoder reports one ControlEvent::EncoderDelta per physical detent instead of four. Its hardware verification was moved here out of that ticket, because a criterion prefixed `HUMAN:` cannot be satisfied by agent work and leaving it on an `@agent` ticket means either a false Done or a stalled loop.

This ticket is the standing record that the detent ratio is code-verified (host unit tests replaying the captured transition sequences, TASK-024 AC #3) but not yet verified on a real encoder.

Protocol matches the 2026-08-08 captures referenced in TASK-018.04 and TASK-024: flash the podtest build containing the fix, turn the encoder deliberately, and record the USB CDC serial output.

Can share one board session with TASK-027, which needs a fresh podtest capture for the ~1 kHz poll rate anyway. Unlike that pair there is no ordering trap here — TASK-024 is the only prerequisite.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: a fresh podtest capture, taken with the podtest build that contains the TASK-024 fix and using the same protocol as the 2026-08-08 captures, is attached or summarized in this ticket
- [ ] #2 HUMAN: ten deliberate clockwise detents produce a net +10 EncoderDelta and ten deliberate counter-clockwise detents produce a net -10, direction positive clockwise
- [ ] #3 The observed per-detent transition-cluster sizes are recorded in this ticket's implementation notes as numbers, including whether any cluster registered fewer than four transitions — this is the evidence for whether the remainder-carrying accumulator or the rest-state full-step variant was the right call (see TASK-024's design constraint)
- [ ] #4 Any detent counted short or long is filed as a new bug ticket with the timestamps from the capture, rather than checked off or patched from memory
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Dependencies updated 2026-09-09 to include TASK-031. This ticket keeps its @human assignment — turning ten deliberate detents in each direction is actuation, not analysis, and no rig exists for it. But criterion #1's capture should be taken through the rig runner rather than by hand once TASK-031 lands, so the detent counts rest on integrity-checked records instead of a serial transcript that loses roughly 8.8% of its lines. Criterion #3's cluster arithmetic is agent work either way.

The bench left rigged by TASK-027.01 can be reused for this session; both need a fresh podtest capture and neither needs anything touched in between.

TASK-029.01 landed: podtest now emits '[podtest] t=<ms> ENCRAW AB=<bits>' once per raw 2-bit encoder state change, alongside the existing decoded 'ENC {:+}' detent line. Reading a capture: a physical click is the run of ENCRAW lines between two stable stretches (expect up to 4, e.g. 00->01->11->10 clockwise); the state held while nothing is turning is the mechanical rest position, and the first ENCRAW line after boot reports the state the harness booted in.

TASK-030 (self-verifying log records) has NOT landed, so these lines ride the same USB CDC path that lost ~8.8% of the 2026-08-08 capture's lines. A dropped line inside a burst is indistinguishable from a transition that never happened, so do not read one run's cluster length as final: turn each direction at least twice and compare the runs, and record the comparison here. If the runs disagree, the capture lost lines -- report it rather than concluding the ratio or cluster shape, and mention it so TASK-029.01's fallback (drop the ENCRAW timestamp, or coalesce a burst into one line per cluster) can be taken.

2026-10-09: Set to Blocked at the owner's request while they have no physical access to the board. This ticket flashes or runs firmware on the Seed3, which can wedge it (debug port stuck until USB-C is replugged, bad bootloader or link layout) in a way that needs hands to recover. Return to To Do when the owner is back at the bench.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-09 01:28
---
Subtask added 2026-09-09 while planning TASK-024: **TASK-029.01** (Add transition-gated raw A/B encoder logging to podtest) is now a prerequisite of this capture. It exists because AC #3 asks for per-detent transition-cluster sizes and no current log line can produce them — `podtest.rs:199` prints only the decoded sum (`ENC {:+}`), never the 2-bit A/B state. Without that subtask this ticket can verify the net ratio (+10/-10, AC #2) but not AC #3, and the encoder's mechanical rest state stays unmeasured — which is the fact that would decide whether TASK-024 should have used the rest-state full-step decoder instead of the remainder-carrying accumulator.

Also note the original evidence is gone: `~/podtest.log` no longer exists on disk, so the 2026-08-08 cluster arrays quoted in TASK-024 are the only surviving record, and they are aggregate rather than raw. Do one fresh capture that serves this ticket, TASK-027 (poll rate) and TASK-029.01's own verification, in the same board session.
---
<!-- COMMENTS:END -->
