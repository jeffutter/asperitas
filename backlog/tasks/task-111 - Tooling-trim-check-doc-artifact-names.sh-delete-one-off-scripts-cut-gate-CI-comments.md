---
id: TASK-111
title: >-
  Tooling: trim check-doc-artifact-names.sh, delete one-off scripts, cut gate/CI
  comments
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-19 03:10'
labels:
  - review-2026-10
  - tooling
  - planned
dependencies: []
priority: low
type: chore
ordinal: 213800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
check-doc-artifact-names.sh R3 (no --only-section in the Makefile) is a stand-in its header says TASK-059.02 replaced, and image-load-addresses R3 now proves the image length; R1/R2 lint docs against a one-off incident; its line references are wrong (:17 cites Makefile:118-120, build recipe is at 160-162; :36 cites :89, 'all:' is at 126). scripts/bench-030-04.py and scripts/knob-interval.py are single-ticket tools nothing references; rig-session.py is covered by the rig-session ticket (python3 is not in the flake). Comment drift: lefthook.yml:1-60 (60 of 70 lines are commentary on lefthook internals), ci.yml:16-29,36-104, gates.sh header (331 comment lines of 573). Confirmed stale: doc-001:412 says CI runs .#default but ci.yml:146 uses .#ci; doc-001:390 names firmware/memory.x which no longer exists; lefthook.yml:2 cites flake.nix:67 (install is at :118); ci.yml cites flake.nix:25-35 (toolchain is at 45-52); gate-costs.sh:68 cites Makefile:284 (:309).

Change: delete R3 (keep R1 as a small grep or delete the script); delete the two one-off scripts; keep a short 'why' plus ticket ID at each gate, cut lefthook.yml to ~5 lines of comments.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The listed scripts and checks are removed or reduced
- [x] #2 No stale line-number citations remain in lefthook.yml, ci.yml, gates.sh
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 507c972. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
check-doc-artifact-names.sh: R3 and its pass deleted, R1/R2 kept with a short header. bench-030-04.py, knob-interval.py and the tracked __pycache__ removed. lefthook.yml cut to 6 comment lines, ci.yml and gates.sh comments trimmed to short whys with no line citations and no ticket IDs (project rule for code comments). doc-001 staleness (.#default, memory.x) was already fixed before this run. Stale refs in docs/development.md, firmware/Makefile and check-image-load-addresses.sh updated. gates.sh --list output unchanged; gates.sh commit passes.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Removed the R3 check, two one-off scripts and stale cache files; trimmed comments in lefthook.yml, ci.yml, gates.sh and removed all line-number citations. Gate list and behaviour unchanged.
<!-- SECTION:FINAL_SUMMARY:END -->
