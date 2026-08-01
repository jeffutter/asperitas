---
id: TASK-005.01
title: Implement audio passthrough against the seed3 feature
status: To Do
assignee:
  - '@agent'
created_date: '2026-08-01 05:57'
labels: []
dependencies: []
documentation:
  - docs/reference/daisy-seed3.md
parent_task_id: TASK-005
type: feature
ordinal: 13000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build daisy-embassy's passthrough against `--features seed3` from the pinned PR #80 commit.

```rust
interface.start_callback(|input, output| {
    output.copy_from_slice(input);
}).await
```

Target 48 kHz with a 48-sample block, matching libDaisy's Pod default and the configuration PR #80 verified. The Seed3 codec can do 192 kHz/32-bit; there is no musical reason to spend the CPU here.

If passthrough later turns out not to work on real hardware, we own `src/codec/tac5242.rs` in the pinned fork. The codec is hardware-strapped so there is no I2C register map to reverse — a failure would be in SAI configuration. Expected SAI settings are recorded in docs/reference/daisy-seed3.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Passthrough firmware compiles with --features seed3 at 48 kHz / 48-sample blocks
- [ ] #2 A flashable .bin is produced by the TASK-004.01 pipeline
<!-- AC:END -->
