---
id: TASK-131
title: 'Fix: rig session keeps sending timed SETs after DUMPEND release'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-18 20:13'
updated_date: '2026-10-19 14:00'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 254800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In crates/asperitas-rig/src/session.rs drive loop, DUMPEND triggers release() but the timed-SET drain continues during dump_grace. A late SET re-engages the knob override on the device after the REL and nothing releases it again, and sets_sent/set_acks are inflated. Stop draining timeline.timed once released (or after dump_end), and add a test where the dump ends before the schedule finishes.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-131. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Gated the timed-SET drain in Runner::drive on !self.released; added test dump_ending_before_the_schedule_stops_the_sets (fails without the gate, passes with it). No acceptance criteria were defined.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
A released session no longer sends timed SETs, so a DUMPEND before the schedule ends leaves the knob released and sets_sent/set_acks accurate.
<!-- SECTION:FINAL_SUMMARY:END -->
