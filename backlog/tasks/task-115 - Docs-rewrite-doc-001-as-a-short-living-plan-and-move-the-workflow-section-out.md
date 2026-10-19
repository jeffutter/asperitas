---
id: TASK-115
title: 'Docs: rewrite doc-001 as a short living plan and move the workflow section out'
status: Done
assignee:
  - '@human'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-19 18:25'
labels:
  - review-2026-10
  - docs
  - planned
dependencies:
  - TASK-107
priority: high
type: chore
ordinal: 217800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
doc-001 no longer describes what was built. False statements: 48-sample blocks (:39-40; the block is 32, rust-daisy-stack.md:57, main.rs:84), 'from scratch on dasp' (:27,511; no crate depends on dasp), 'Debug probe: none for weeks' (:30), the PR #80 SHA pin (:60-61, risk row :508; PR #80 merged and the build uses master). The crate layout (:81-112) promises asperitas-tui and asperitas-bench and never mentions asperitas-logging, -pod, -rig or -measure; milestones record no status for M0-M3 and omit the measurement-rig and host-control effort; M4 has no tickets and promises 'the same parameter model as the device' which no document defines (Pod mapping, CLI --params and CTL FX are three separate surfaces). Section 5 (218-445, 230 of 530 lines) is generated gate-cost tables plus CI history, has a render glitch at :338, cites firmware/memory.x (:390,401, which does not exist) and gives 469,763,536 bytes at :393 where daisy-seed3.md:124 and check-doc-artifact-names.sh say 469,763,480.

Change: move section 5 to docs/development.md (or the gates.sh header), rewrite the plan with superseded decisions marked, the real crate map, M0-M3 marked done, and concrete M4/M5 scope (TUI crate name, shared params model, cpal backend, what 'same as the device' means; the resonator Processor and its analysis). The M4/M5 scope needs the owner.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 doc-001 is under ~250 lines, factually matches the repo, and marks M0-M3 done
- [x] #2 Workflow/gates content lives outside the plan; no cites of firmware/memory.x
- [x] #3 M4 scope is stated concretely (reviewed with the owner)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against e349f24

Approach: single-session docs edit of backlog/docs/doc-001 (currently 427 lines; section 5 'Development workflow' is lines 218-346).
1. Move section 5 (Nix, lefthook, CI) to docs/development.md, dropping generated gate-cost tables (ledger deleted in TASK-107) and the line-338 render glitch; remove firmware/memory.x cites (real files: firmware/memory-internal.x, memory-bootloaded.x); use 469,763,480 bytes to match docs/reference/daisy-seed3.md and check-doc-artifact-names.sh. Check lefthook.yml/scripts/gates for doc-001 references and repoint.
2. Fix false statements in doc-001: block size 32 (not 48), drop 'from scratch on dasp', drop 'no debug probe', drop PR #80 SHA pin and risk row (merged, build uses master).
3. Replace crate layout with real map: asperitas-cli, -dsp, -host, -logging, -measure, -pod, -replay, -rig, -wire, plus firmware/.
4. Milestones: mark M0-M3 done; add the measurement-rig/host-control effort; keep M4-M7 short.
5. M4/M5 concrete scope (TUI crate name, shared params model, cpal backend, meaning of 'same as device', resonator Processor): draft from repo state and mark as proposal needing owner review (AC3 is HUMAN-reviewed; do not tick AC3 unattended).
6. Verify: wc -l < ~250, grep for memory.x, 48-sample, dasp, SHA pin; run the repo doc-artifact-names check script and gates for docs.
Final: tick AC1, AC2; leave AC3 for owner sign-off.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Section 5 moved to docs/development.md (condensed, no timings, no gate-cost tables, memory.x cites replaced by memory-internal.x/memory-bootloaded.x, 469,763,480). doc-001 rewritten to 181 lines: real crate map, M0-M3 marked done, measurement rig effort added, PR #80/dasp/probe/48-sample claims fixed. M4/M5 scope is drafted as a proposal marked needing owner review; AC3 deliberately left unchecked (HUMAN review, cannot be satisfied unattended). Also repointed the gates.sh --list usage text and an led.rs comment that cited doc-001.

2026-10-19, live session with the owner: walked the M4 scope in doc-001 (asperitas-live crate, Controls and ParamSpec as the one shared parameter model, live host versus device exit criterion, M4 before M5). The owner confirmed each point as written and signed off AC #3. The exit criterion stays as drafted, live host against device.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
doc-001 is a short living plan with the workflow section moved to docs/development.md. The owner reviewed and confirmed the M4 scope as written in a live session, closing the last criterion.
<!-- SECTION:FINAL_SUMMARY:END -->
