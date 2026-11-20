---
id: TASK-155.08
title: Golden regression tests for the resonator against the committed corpus
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-11-20T00:26:00Z'
labels:
  - m5
  - resonator
  - testing
dependencies:
  - TASK-155.06
documentation:
  - audio/README.md
  - crates/asperitas-cli/tests/golden_tests.rs
parent_task_id: TASK-155
priority: medium
type: feature
ordinal: 303700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Golden-file regression is the project's guard against silent DSP changes: frozen output for an input plus parameter set, regenerated only on purpose, where a golden diff means "listen before accepting". The resonator has none yet, so a later tweak to its numerics (or to the shared smoothing) could change its sound unnoticed.

Add goldens for the resonator through the CLI render (TASK-155.06) over corpus clips in audio/instruments and the synthetic signals, at a small set of parameter sets that cover the default voicing and the extremes of damping and structure, following the layout, regeneration opt-in and size policy in audio/README.md and the existing `golden_tests.rs`. Clips are short (2-5 s), mono, 48 kHz.

Out of scope: the resonator's DSP tests (they live with the DSP tickets), rig or device captures.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Resonator goldens exist for the default voicing and for extremes of damping and structure, over at least one corpus instrument clip and one synthetic signal, in audio/goldens following the existing layout
- [ ] #2 cargo test -p asperitas-cli verifies them, and a deliberate change to the resonator's output makes a golden test fail (shown once, then reverted)
- [ ] #3 Regeneration is opt-in only, as for the existing goldens, and a golden is never rewritten by a normal test run
- [ ] #4 Total committed golden size stays within the policy in audio/README.md, and the new goldens and how to regenerate them are documented there
- [ ] #5 Existing goldens are untouched and still pass
- [ ] #6 bash scripts/gates.sh push passes
<!-- AC:END -->
