---
id: TASK-107
title: >-
  Tooling: delete the gate cost ledger (gate-costs.sh, gate-costs.json,
  {{gate:...}} keys)
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - tooling
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
- [ ] #1 gate-costs.sh, docs/gate-costs.json and the key citations are gone; the commit tier is green
- [ ] #2 Editing or adding a gate no longer requires a refresh step
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-16 owner decision: Gate timings do not need to be recorded anywhere; gates.sh already prints a time per gate and per tier. Delete the ledger.
<!-- SECTION:NOTES:END -->
