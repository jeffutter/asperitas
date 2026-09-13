---
id: TASK-067
title: Commit the provenance-reader checks so 7251cd8's claim about tests becomes true
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-13 14:51'
updated_date: '2026-09-13 14:51'
labels: []
dependencies: []
priority: low
ordinal: 117800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-062.02's commit message (7251cd8, `TASK-062.02: teach elf-check which cfg set an ELF was built for`)
says the `/dev/null` guard and the exit-code paths are "asserted in the script's own tests, which also
cover console agreement, an injected third cfg set refused while naming both sides, the
`--no-default-features --features seed3` pair deriving default=0/features=seed3 and refusing the
console ELF, and all four exit-code paths."

No such file exists. `grep -rn elf-provenance scripts/` finds only the script itself, and no gate runs
it beyond the one push-tier line TASK-062.03 added. The behaviours are real - TASK-062.02's
implementation notes record every one of them measured by hand on 2026-09-13 - but they live in a
ticket's notes rather than in something that runs, so the next edit to build.rs's blob encoding or to
`normalize_features` has nothing between it and a silent mislabel at the bench.

Make the claim true by committing the checks. Two shapes are open and the planner should pick one with
its eyes open: reuse the two ELFs the cross-build pair already produced (free, but ties the selftest's
position to sitting after the pair, like the provenance gate does) or build synthetic blob fixtures
(independent of build order, but then it exercises the parser and not the stamp). The `dump_reassemble
--selftest` gate is the precedent to imitate: one binary, one flag, one gate line, ~0.7 s.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `scripts/elf-provenance.sh --selftest` (or an equivalent sibling invoked the same way) prints one prefixed line per case, exits 0 when all pass, 1 on any failure while reporting every failure in one run, and 2 when it cannot run at all. Follow scripts/check-doc-artifact-names.sh's header conventions for that exit-code contract.
- [ ] #2 Cases asserted rather than printed for a human to read: console agreement; RTT-only agreement; a third cfg set refused while naming both sides; `--no-default-features --features seed3` deriving default=0 features=seed3; a feature outside the default closure (e.g. stim_ess) present in the derived expectation; missing ELF -> 2; ELF with no .asp.prov section -> 2; blob carrying a foreign format tag -> 2; reading a blob leaves the ELF's mtime AND sha256 unchanged, which is what the trailing /dev/null argument buys.
- [ ] #3 It builds nothing and stays cheap. Choose either reuse of the artifacts the two cross-build gates produce, placed after them, or fixture-based parsing placed anywhere, then record the measured warm cost and the reason for the position. Budget: under 1 s warm, no cargo build invocation, no reaching into target/<triple>/release/.fingerprint.
- [ ] #4 Registered as exactly one new gate line in scripts/gates.sh; doc-001's matrix block regenerated from `scripts/gates.sh --list` rather than hand-edited, with its counts and measured cost lines updated in the same commit.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Origin: TASK-062's umbrella run, 2026-09-13. The measurements this ticket turns into code are written out in TASK-062.02's implementation notes; do not rediscover them.
<!-- SECTION:NOTES:END -->
