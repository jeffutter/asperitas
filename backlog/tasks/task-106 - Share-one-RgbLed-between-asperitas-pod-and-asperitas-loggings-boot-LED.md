---
id: TASK-106
title: Share one RgbLed between asperitas-pod and asperitas-logging's boot LED
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - pod
  - logging
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
- [ ] #1 One RgbLed driver and one polarity constant
- [ ] #2 LED 1 boot, running and panic states re-checked (hardware recheck ticket)
<!-- AC:END -->
