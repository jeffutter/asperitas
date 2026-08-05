---
id: TASK-018
title: 'Pod BSP: control surface for the Daisy Pod'
status: To Do
assignee:
  - '@human'
created_date: '2026-08-05 17:25'
labels:
  - planned
dependencies: []
documentation:
  - docs/reference/daisy-pod.md
  - docs/reference/rust-daisy-stack.md
priority: high
type: feature
ordinal: 26000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
First half of milestone M3 in doc-001. The Daisy Pod carrier board supplies the whole control surface — 2 knobs, a rotary encoder with click, 2 buttons, 2 RGB LEDs — and none of it is reachable from Rust today.

daisy-embassy has no Pod support at all: `src/pins/` contains only `pins_seed.rs` and `pins_patch_sm.rs`. The pin map and the control drivers have to be written on our side. It is small and largely declarative work (GPIO plus ADC channel assignment), and docs/reference/daisy-pod.md flags it as a good candidate to contribute upstream once it is proven on hardware.

The pin map itself is already transcribed and cross-checked against libDaisy Rev3/Rev4 in docs/reference/daisy-pod.md — that table is the source of truth for the subtasks, not a fresh reading of libDaisy.

Umbrella ticket. It is `@human` because it inherits the strictest assignee among its children: TASK-018.04 needs the board and hands, so this parent cannot be closed by an agent.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All subtasks are Done
<!-- AC:END -->
