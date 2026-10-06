---
id: TASK-070.04
title: >-
  Wire the cost check as a commit-tier gate, prove it goes red, and write down
  who owns each number
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-14 15:17'
updated_date: '2026-10-06 05:16'
labels:
  - planned
dependencies:
  - TASK-070.03
parent_task_id: TASK-070
priority: medium
type: chore
ordinal: 124800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Makes TASK-070's mechanism load-bearing. Register scripts/gate-costs.sh --check in scripts/gates.sh's commit tier (unfiltered, per lefthook.yml:23-27), prove with deliberate mutations that it catches every drift class it claims to - watched-green is the failure mode this repo keeps hitting - and replace the scattered regenerate-it-with-that-command sentences in doc-001 section 5 with one subsection naming the owner of each figure and the one command that reproduces them all. Adding a gate invalidates its own measurement set, so this commit re-times everything (~4 min) after adding the line.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A new commit-tier gate runs scripts/gate-costs.sh --check unfiltered, placed beside the other cheap structural checks ahead of every cargo invocation; bash scripts/gates.sh commit exits 0 and the added cost is measured three times against the ~4 s commit budget and quoted in the header comment.
- [x] #2 Adding the gate line is followed by a full --refresh in the same commit, so the ledger contains the new gate and the tier lands green rather than red on its own missing key.
- [x] #3 Eight mutations run and observed, each reverted, with the observed output quoted in the ticket notes: hand-typed figure in a comment; ledger value changed without re-rendering; gate added with no ledger entry; gate removed leaving its entry; a gate's command edited so only its digest moves (only that gate reports); a banner reworded with the key kept (stays green); ledger deleted (exit 2, not a silent pass); a duration literal in firmware/Makefile outside a generated region, then exempted with a reason and counted in the summary.
- [x] #4 doc-001 gains an ownership subsection stating: docs/gate-costs.json owns every measured number with date, host, environment and sample count; scripts/gate-costs.sh --refresh is the one command that reproduces what the docs publish; prose elsewhere holds keys; --check never writes and costs about N s while only a person runs --refresh; re-timing stays out of the hook because four minutes does not belong in pre-commit; runner-side figures stay with TASK-052/TASK-063 and why; and a stored figure is meaningless without the tier that paid it.
- [x] #5 The now-redundant number-carrying sentences in lefthook.yml's and gates.sh's headers are replaced by a single clause pointing at the owner, keeping the trade they argue for (stdout buffering until the tier ends) rather than the price list.
- [x] #6 TASK-070's three acceptance criteria are verified against the tree rather than against this plan, and the final summary records which published figures moved when the fresh measurement disagreed with the prose.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit trailed Task-Id: TASK-070.04. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Review (TASK-070.03 checkpoint, 2026-09-16T09:26Z): the working tree already carries substantial uncommitted work matching this ticket's scope (GATE_COSTS_BOOTSTRAP env var + usage text + cmd_check wiring in scripts/gate-costs.sh, the commit-tier gate line + re-refreshed ledger in scripts/gates.sh, and matching edits to lefthook.yml, .github/workflows/ci.yml, docs/gate-costs.json, firmware/Makefile, scripts/check-elf-staleness.sh, scripts/elf-provenance.sh, backlog/docs/doc-001). None of AC #1-6 are checked and status is still To Do, so this was not done through the normal execute flow and never got committed. Before starting fresh: run 'git status' / 'git diff' and read what is already there -- it may satisfy some ACs already (AC #1's bootstrap seam in particular looks implemented) rather than needing to be redone from scratch.

Adopted the pre-existing uncommitted diff (gate-costs-current gate, GATE_COSTS_BOOTSTRAP seam, ledger measured 2026-09-16T09:14Z with all 13 commit gates, doc-001 "Who owns each number", header clauses in gates.sh/lefthook.yml) as the implementation; nothing was redone. Verified against the tree: `scripts/gate-costs.sh --check` exit 0 in 0.51 s ("8 guarded files render byte-identical ... prices all 23 live gates; 9 reasoned duration exemption(s)"); `bash scripts/gates.sh commit` green, "tier commit: 13 gates, 7.1s".

AC#3 mutations, each run in a scratch copy of the tree (GATE_COSTS_ROOT), real tree untouched, so nothing to revert:
1. hand-typed "3.7 s" in a gates.sh comment -> exit 1: "scripts/gates.sh:264: wall-clock duration literal ' 3.7 s ' sits outside a generated region owned by no key."
2. ledger elf-provenance-selftest commit cost set to 9.99 without re-render -> exit 1, 3 findings: gates.sh:55, elf-provenance.sh:266, doc-001:264 "renders differently from the file on disk" (0.89 s vs ~10 s).
3. stub `gate commit probe-costs` with no entry -> exit 1: "live gate 'probe-costs' (min_tier commit) has no entry in docs/gate-costs.json" plus tier gate-count mismatches 13/22/23 vs 14/23/24, and doc-001 matrix stale.
4. elf-staleness-selftest line deleted, entry kept -> exit 1: "docs/gate-costs.json still prices 'elf-staleness-selftest', which gates.sh --list no longer offers", plus count mismatches 12/21/22.
5. `--release` added to cargo-clippy command -> exit 1, exactly one finding: "gate 'cargo-clippy' now runs a different command than the one its stored cost measured (ledger 196d9a67d5f6..., current c3d7d96573eb...)". No other gate named.
6. banner reworded, key kept -> DEVIATES FROM THE CRITERION AS WRITTEN: exit 1, one finding, doc-001:263 "renders differently from the file on disk" (matrix banner column). Cost, key and digest findings: none. `--render` (sub-second, no re-timing) rewrites that one file and `--check` is then exit 0. Reason: doc-001's generated gate matrix prints live banners, so a reword makes that generated view stale. Property that matters holds (key stays identity, cost not orphaned, no re-measure needed); literal "stays green with no action" does not. Left as is: a matrix showing stale banners would be the worse trade.
7. docs/gate-costs.json removed -> exit 2: "gate-costs: no docs/gate-costs.json. Write one with: scripts/gate-costs.sh --refresh". Not a silent pass. Did not separately observe gates.sh's tier-level report of this (fixture has no cargo workspace).
8. "## it takes 12 s to flash" appended to firmware/Makefile -> exit 1: "firmware/Makefile:453: wall-clock duration literal ' 12 s ' sits outside a generated region". With gate-costs:exempt reason="..." on the line -> exit 0, summary "10 reasoned duration exemption(s) in use" (was 9).

Added cost of the gate: 0.464/0.457/0.457 s (header comment, three commit runs); ledger publishes 0.48 s; this session's --check: 0.51 s total.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Landed the commit-tier gate gate-costs-current (third in the tier, unfiltered, ~0.46 s), the GATE_COSTS_BOOTSTRAP seam for pricing a new gate, a ledger measured with the gate in place, the doc-001 'Who owns each number' subsection, and header clauses pointing at the owner. Seven of eight mutations behave as specified; the banner reword (#6) turns red on the generated doc-001 matrix until --render is run (cheap, no re-time), which differs from 'stays green' - see notes. Figures that moved when the fresh measurement disagreed with the prose: elf-provenance-selftest 0.88->0.89 s, elf-staleness-selftest 0.84->0.85 s, cargo-clippy and cargo-clippy-log-usb 0.20->0.21 s, firmware-clippy 0.23->0.24 s, dump-reassemble-selftest 0.68->0.71 s, firmware-cross-compile-rtt 0.12->0.13 s, cargo-test-pod-hw ~67->~66 s; new gate-costs-current 0.48 s; counts commit 12->13, push 21->22, ci 22->23. TASK-070 ACs verified against the tree: #1 --check clean with 9 counted exemptions; #2 mutation 3 shows a new gate turns the tier red naming it; #3 doc-001 names the owner and the other seven files cite keys. This ticket's ledger was measured 2026-09-16 and was not re-timed in this session.
<!-- SECTION:FINAL_SUMMARY:END -->
