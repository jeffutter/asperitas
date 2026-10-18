---
id: TASK-107
title: >-
  Tooling: delete the gate cost ledger (gate-costs.sh, gate-costs.json,
  {{gate:...}} keys)
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-18 20:42'
labels:
  - review-2026-10
  - tooling
  - planned
dependencies: []
priority: medium
type: chore
ordinal: 209800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
scripts/gate-costs.sh is 2,204 lines guarding timing numbers in comments. No build/flash/audio failure has depended on them. It is a commit-tier gate (gates.sh:283): editing any gate command fails the commit with DIGEST_CHANGED (gate-costs.sh:831-832) until a ~4-minute --refresh; adding a gate needs the GATE_COSTS_BOOTSTRAP=1 two-step (gates.sh:274-282). It has a live bug its own check cannot see: doc-001:338 reads 'measured 2026-09-16 2026-10-08 2026-10-15 2026-10-16 {{meta:measured}}' because strip_owned (gate-costs.sh:404-420) strips only figures matching the current value, so each refresh adds a date. Facts it does not guard have gone stale (see the doc tickets).

Change: delete the script, the ledger, the exempt markers and every cost figure in prose; gates.sh already prints a time per gate and per tier.
DECISION NEEDED: do you want gate timings recorded anywhere (nothing, an untracked last-run file, or a CI job summary)?

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 gate-costs.sh, docs/gate-costs.json and the key citations are gone; the commit tier is green
- [x] #2 Editing or adding a gate no longer requires a refresh step
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-107. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-16 owner decision: Gate timings do not need to be recorded anywhere; gates.sh already prints a time per gate and per tier. Delete the ledger.

Deleted scripts/gate-costs.sh and docs/gate-costs.json, the gate-costs-current gate, every {{gate/tier/component/meta}} key and gate-costs:exempt marker, and the cost figures they guarded (gates.sh, lefthook.yml, ci.yml, firmware/Makefile, three checker headers, doc-001 priced matrix and ownership section). Kept GATES_TIMINGS_FILE sink in gates.sh and the runner-side CI observations in doc-001. Verified: bash scripts/gates.sh commit green (16 gates), no stale-reference grep hits. Adding a gate now needs only a gate line.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Removed the gate cost ledger and all its citations; commit tier is green with no refresh step.
<!-- SECTION:FINAL_SUMMARY:END -->
