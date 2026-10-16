---
id: TASK-115
title: 'Docs: rewrite doc-001 as a short living plan and move the workflow section out'
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - docs
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
- [ ] #1 doc-001 is under ~250 lines, factually matches the repo, and marks M0-M3 done
- [ ] #2 Workflow/gates content lives outside the plan; no cites of firmware/memory.x
- [ ] #3 M4 scope is stated concretely (reviewed with the owner)
<!-- AC:END -->
