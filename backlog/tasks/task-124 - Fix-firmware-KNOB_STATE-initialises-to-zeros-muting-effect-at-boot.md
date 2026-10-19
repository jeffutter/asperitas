---
id: TASK-124
title: 'Fix: firmware KNOB_STATE initialises to zeros, muting effect at boot'
status: Done
assignee:
  - '@agent'
created_date: '2026-10-17 01:16'
updated_date: '2026-10-19 03:28'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 242800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
firmware/src/bin/main.rs: KNOB_STATE starts at [0.0,0.0] and the audio callback passes it to Asperitas::set_controls before knob_poll_task writes, driving 20 Hz cutoff / -60 dB gain (audible dip/thump). Initialise from Controls::default().knobs. Also KnobState::write takes no critical section though its comment says all accesses do, so the audio ISR can read a torn pair.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
SHIPPED by f611bce (in HEAD 61fbf4ce8579b41b04197199ec5dde97541ae6d5) - main.rs seeds KNOB_STATE from Controls::default().knobs before audio starts, and KnobState uses per-knob AtomicU32 so reads are never torn
<!-- SECTION:FINAL_SUMMARY:END -->
