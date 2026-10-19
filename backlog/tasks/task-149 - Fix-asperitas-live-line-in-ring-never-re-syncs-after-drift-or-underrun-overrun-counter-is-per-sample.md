---
id: TASK-149
title: >-
  Fix: asperitas-live line-in ring never re-syncs after drift or underrun;
  overrun counter is per-sample
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-19 23:35'
updated_date: '2026-10-19 23:56'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 284700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The output callback trims the ring once at start (started flag) and never again, so independent input/output clocks or an input stall leave it permanently starved (constant zero-fill) or permanently full (max latency). Re-prime/trim on underrun and when occupancy exceeds a threshold. Also: --buffer 0 sizes the ring assuming a 512-frame host buffer, wrong for large default buffers; size from the actual first callback or a larger floor. Overruns are counted per rejected sample while underruns count per chunk; use push_slice and count events so they are comparable.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Output callback re-primes after an underrun and trims the ring when occupancy exceeds a high-water mark, tested via a pure policy unit
- [x] #2 Ring sizing no longer assumes 512 frames for BufferSize::Default
- [x] #3 Overruns and underruns both count events (per callback chunk), not per sample
- [x] #4 cargo test, clippy, fmt pass for asperitas-live
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by d0678d8. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added ring.rs (pure RingSync policy, 4 unit tests); backend output callback re-primes after underrun and trims above high-water; default-buffer ring is 16384 frames; overruns/underruns/trims count callbacks. Docs updated. Not exercised on a real device.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Line-in ring now re-syncs via RingSync (re-prime on underrun, trim above high-water), sizes without the 512 guess, and counts events per callback; trims added to the exit line.
<!-- SECTION:FINAL_SUMMARY:END -->
