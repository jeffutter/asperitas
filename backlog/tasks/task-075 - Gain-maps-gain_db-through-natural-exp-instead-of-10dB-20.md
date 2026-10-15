---
id: TASK-075
title: Gain maps gain_db through natural exp instead of 10^(dB/20)
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-15 20:33'
labels: []
dependencies: []
priority: medium
ordinal: 168800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
crates/asperitas-dsp/src/gain.rs set_params uses libm::expf(gain_db/20), so gain_db 12 yields x1.82 (5.2 dB), not x3.98. Found in TASK-035.04. Fix to 10^(dB/20) and check dependent tests/docs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 set_params converts dB to linear as 10^(dB/20), with a unit test at +6.02 dB giving ~x2
- [ ] #2 cargo test, clippy and fmt pass; asperitas-measure param_step tests re-baselined if needed
<!-- AC:END -->
