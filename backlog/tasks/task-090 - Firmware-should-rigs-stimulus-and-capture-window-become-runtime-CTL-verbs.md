---
id: TASK-090
title: 'Firmware: should rig''s stimulus and capture window become runtime CTL verbs?'
status: Backlog
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - firmware
  - needs-decision
dependencies:
  - TASK-085
priority: low
type: chore
ordinal: 192800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Cargo features stim-sine, stim-ess, stim-pulse, stim-excerpt and unsmoothed-params are accepted by every binary and mean nothing to five of them (Cargo.toml:32-50); rig.rs:144-176 const-asserts mutual exclusion; env vars ASP_RIG_CAPTURE_SECONDS (rig.rs:366) and ASP_RIG_EXCERPT_SLOT (rig.rs:1607). The reason given for compile-time stimulus choice ('no inbound channel', rig.rs:163) no longer holds now that CTL exists. Runtime CTL STIM and CTL WINDOW would remove four features, two env vars, the exclusion assert and many build/clippy gate rows.

DECISION NEEDED: compile-time image identity (current) versus runtime choice; flash-size impact is unmeasured. At minimum document the features as rig-only.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Decision recorded; features either documented rig-only or replaced by CTL verbs, with the gate matrix updated
<!-- AC:END -->
