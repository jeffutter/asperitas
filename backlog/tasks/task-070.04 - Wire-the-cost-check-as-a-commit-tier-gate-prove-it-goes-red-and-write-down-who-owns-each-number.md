---
id: TASK-070.04
title: >-
  Wire the cost check as a commit-tier gate, prove it goes red, and write down
  who owns each number
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-14 15:17'
updated_date: '2026-09-16 09:26'
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
- [ ] #1 A new commit-tier gate runs scripts/gate-costs.sh --check unfiltered, placed beside the other cheap structural checks ahead of every cargo invocation; bash scripts/gates.sh commit exits 0 and the added cost is measured three times against the ~4 s commit budget and quoted in the header comment.
- [ ] #2 Adding the gate line is followed by a full --refresh in the same commit, so the ledger contains the new gate and the tier lands green rather than red on its own missing key.
- [ ] #3 Eight mutations run and observed, each reverted, with the observed output quoted in the ticket notes: hand-typed figure in a comment; ledger value changed without re-rendering; gate added with no ledger entry; gate removed leaving its entry; a gate's command edited so only its digest moves (only that gate reports); a banner reworded with the key kept (stays green); ledger deleted (exit 2, not a silent pass); a duration literal in firmware/Makefile outside a generated region, then exempted with a reason and counted in the summary.
- [ ] #4 doc-001 gains an ownership subsection stating: docs/gate-costs.json owns every measured number with date, host, environment and sample count; scripts/gate-costs.sh --refresh is the one command that reproduces what the docs publish; prose elsewhere holds keys; --check never writes and costs about N s while only a person runs --refresh; re-timing stays out of the hook because four minutes does not belong in pre-commit; runner-side figures stay with TASK-052/TASK-063 and why; and a stored figure is meaningless without the tier that paid it.
- [ ] #5 The now-redundant number-carrying sentences in lefthook.yml's and gates.sh's headers are replaced by a single clause pointing at the owner, keeping the trade they argue for (stdout buffering until the tier ends) rather than the price list.
- [ ] #6 TASK-070's three acceptance criteria are verified against the tree rather than against this plan, and the final summary records which published figures moved when the fresh measurement disagreed with the prose.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# Shape

Make the mechanism load-bearing: register `scripts/gate-costs.sh --check` as a commit-tier gate, prove
with deliberate mutations that it catches each drift class it claims to, and write down the ownership
rule AC #3 asks for so the next ticket does not have to rediscover it. This is the last child; nothing
here can go green before TASK-070.03 converts the prose.

## Step 1 - register the gate, and pay for it in the same commit

Add one line to `scripts/gates.sh`, third in the list, beside the other cheap structural checks and
ahead of every cargo invocation:

    gate commit gate-costs-current "=== gate costs current ===" scripts/gate-costs.sh --check

Placement follows the rule already written at `gates.sh:196-200`: cheapest thing that has no inputs to
wait for goes first, and this reads only `--list`, `--dry-run` and two files. It is unfiltered and
tier-honest - `lefthook.yml:23-27` records that a filtered job whose set is empty exits 0 having
checked nothing, which is exactly how this check would become decorative.

Adding a gate invalidates its own measurement set, so the run order inside this commit matters: add the
line, *then* run `bash scripts/gate-costs.sh --refresh` (~4 minutes: commit + push + ci tiers, detached
under `nohup` with the log polled, not blocking), then commit the ledger, the rendered prose and the
gate line together. Anyone who reverses that order gets a red tier from their own new gate naming a
missing key, which is correct behaviour, not a bug - say so in the commit message.

Record the added cost three times against the commit tier's ~4 s budget and put the numbers in the
header comment. If it lands above ~1 s, stop and work out why rather than shipping it anyway.

## Step 2 - prove it fails, in the way TASK-068 proved its parser

Watched-green is what got this repo here: `elf-provenance.sh`'s cases were described as asserted in a
commit message and nothing ran them (TASK-067), and doc-001's matrix has claimed to be generated since
TASK-061.02 with no check behind it. So each mutation gets run, observed red with the right words, and
reverted, and the observed output gets quoted in the ticket notes:

1. Hand-type a plausible figure into a `gates.sh` comment -> exit 1, names file and line.
2. Change one ledger `cost` value without re-rendering -> exit 1 on every prose site citing it.
3. Add a stub `gate commit probe-costs ...` line with no ledger entry -> exit 1 naming the key.
4. Delete a gate line but leave its ledger entry -> exit 1 as a dead figure.
5. Edit a gate's command (`--all-targets` onto a clippy line) leaving everything else -> exit 1 naming
   that one gate's digest, and *only* that gate's.
6. Reword a banner without touching the key -> still exit 0. Keys are identity; display text is free.
7. Remove `docs/gate-costs.json` -> exit 2, not a silent pass, and `gates.sh` reports the tier failed.
8. Put a duration literal in `firmware/Makefile` outside a generated region -> exit 1; add the reasoned
   exemption marker -> exit 0, with the exemption counted in the summary line.

Case 3 is the acceptance test for the ticket's real complaint: adding a gate makes the stale figures
visible with no human involved.

## Step 3 - write the ownership rule down once

Replace the scattered "regenerate it with that command" sentences in doc-001 section 5 with a short
subsection that states, per figure, who owns it and how the others refer to it:

- `docs/gate-costs.json` owns every measured number, its date, its host, and the method.
- `scripts/gate-costs.sh --refresh` is the one command that reproduces every figure the docs publish.
- Prose in `gates.sh`, `lefthook.yml`, `ci.yml`, `firmware/Makefile` and the checker headers holds keys
  inside its arguments; `doc-001`'s matrix and cost table are generated regions.
- `--check` runs in the commit tier, costs about N s, and never writes; only `--refresh` writes, and a
  person runs it.
- Re-timing costs about four minutes and stays out of the hook, which is TASK-068's non-goal kept
  rather than ignored. Runner-side figures stay with TASK-052/TASK-063 and why (`ci.yml` has no
  `workflow_dispatch`, no artifact upload and no `permissions:` block, so a runner cannot write a
  record back even if one wanted it to).
- Approximation policy: one warm sample per tier, last digit is noise, host and environment recorded
  in the ledger. Say plainly that a stored figure is meaningless without the tier that paid it, since
  the firmware-clippy pair spans ~20 s / ~2 s / 0.25 s depending on adjacency.

Then delete the now-redundant sentences in `lefthook.yml` and `gates.sh` headers that used to carry the
numbers, leaving each file pointing at the owner in one clause. `lefthook.yml`'s contract paragraph
about stdout buffering and the ~4 s / ~77 s silence is about a trade, not a price list - keep the
trade, key the numbers.

## Step 4 - close out the parent

Check TASK-070's three ACs against the tree, not against this plan: grep the guarded files for a
duration literal outside a generated region (expect zero plus the reasoned exemptions), run
`--refresh` followed by `--check` on a clean tree, and confirm all four files name one owner. Note in
the final summary what moved when the fresh measurement disagreed with the prose - there will be
something; three tickets have been repricing these by hand and TASK-064.01 already saw the ci tier
reported as 143-144 s and 145.0 s a day apart.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Review (TASK-070.03 checkpoint, 2026-09-16T09:26Z): the working tree already carries substantial uncommitted work matching this ticket's scope (GATE_COSTS_BOOTSTRAP env var + usage text + cmd_check wiring in scripts/gate-costs.sh, the commit-tier gate line + re-refreshed ledger in scripts/gates.sh, and matching edits to lefthook.yml, .github/workflows/ci.yml, docs/gate-costs.json, firmware/Makefile, scripts/check-elf-staleness.sh, scripts/elf-provenance.sh, backlog/docs/doc-001). None of AC #1-6 are checked and status is still To Do, so this was not done through the normal execute flow and never got committed. Before starting fresh: run 'git status' / 'git diff' and read what is already there -- it may satisfy some ACs already (AC #1's bootstrap seam in particular looks implemented) rather than needing to be redone from scratch.
<!-- SECTION:NOTES:END -->
