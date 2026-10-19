---
id: TASK-135
title: 'Fix: rig session passes when the dump ends before the SET schedule completes'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-19 14:02'
updated_date: '2026-10-19 14:23'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 261800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In crates/asperitas-rig/src/session.rs, Session::outcome checks o.set_acks == self.sets_sent, but sets_sent is only what was actually sent. Since the timed-SET loop stops sending once released (dump ended early), a truncated sweep with every sent SET acked reports Outcome::Pass although the plan's SETs were not all sent. Compare against the planned timed SET count (timeline.timed.len()) so a cut-short schedule is a Fail, and add a test.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-135. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
SessionRun gained sets_planned (pre + timed); outcome() requires sets_sent == sets_planned. Existing early-dump test now expects Fail.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
A session whose dump ends before the SET schedule completes now fails instead of passing.
<!-- SECTION:FINAL_SUMMARY:END -->
