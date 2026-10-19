---
id: TASK-106
title: Share one RgbLed between asperitas-pod and asperitas-logging's boot LED
status: Done
assignee:
  - '@human'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-19 16:42'
labels:
  - review-2026-10
  - pod
  - logging
  - planned
dependencies:
  - TASK-094
priority: low
type: chore
ordinal: 208800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
LED_ACTIVE_LOW is defined in pod led.rs:35 and logging led.rs:39; the channel-drive code is duplicated (pod led.rs:116-131, logging led.rs:208-246) and LED 1 hardware lives in the logging crate. logging led.rs:292-295 calls Panicked unreachable, but main.rs:223 and rig.rs:2341 set it without halting.

Change: one RgbLed in asperitas-pod with logging's boot-led composing it; fix the comment. The boot LED is on the panic path and hardware-verified, so re-check its states on the board.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Subtask TASK-106.01 (LED 1 hardware recheck) is done; the RgbLed code change shipped in e1fb8ae
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED (AC1) by the commit carrying Task-Id TASK-106. AC2 remains for the hardware recheck ticket.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
AC1 done: asperitas_pod::led::RgbLed is the single channel driver and owns the only LED_ACTIVE_LOW (now private). Led2 and logging BootLed compose it; blink_task Panicked arm comment corrected (reachable when a binary sets Panicked without halting); daisy-pod.md points at the pod file. Gates (commit tier) and firmware clippy for all bins pass. AC2 is a HUMAN board recheck covered by TASK-120 and is intentionally left unchecked, so the ticket is not marked Done.

Split: code change (old AC1) is shipped; hardware recheck (old AC2) moved to TASK-106.01 (@human). Parent is an umbrella and inherits @human so the loop stops re-selecting it.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
The code change (one asperitas_pod::led::RgbLed driver and one polarity constant, shipped in e1fb8ae) is done, and the LED 1 hardware recheck (TASK-106.01) was closed with the owner on 2026-10-19.
<!-- SECTION:FINAL_SUMMARY:END -->
