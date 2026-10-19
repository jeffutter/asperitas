---
id: TASK-140
title: 'Fix: restore sweep-shape/level coverage and verify goldens after stimulus move'
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-19 17:22'
labels:
  - review-fix
dependencies: []
ordinal: 266800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Commit 7131e5a deleted crates/asperitas-cli/tests/stimulus_shape_tests.rs (device vs golden sweep shape, dBFS level scaling, fades-only difference) and changed synth.rs: sample_count now rounds instead of truncating (0.1 s at 44.1 kHz went 4409 -> 4410) and generate_sweep builds Chirp from N/fs with different phase grouping. No goldens were checked against the old implementation. Re-establish equivalent coverage and confirm goldens still pass.
<!-- SECTION:DESCRIPTION:END -->
