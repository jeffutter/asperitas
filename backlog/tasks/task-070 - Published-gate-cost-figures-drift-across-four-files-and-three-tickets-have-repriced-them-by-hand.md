---
id: TASK-070
title: >-
  Published gate cost figures drift across four files, and three tickets have
  repriced them by hand
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-14 09:44'
updated_date: '2026-09-14 09:46'
labels: []
dependencies: []
priority: medium
type: chore
ordinal: 120800
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every published gate or tier cost figure is either generated from a measurement or explicitly marked approximate with the date it was measured; grep shows no figure stated as fact in more than one file.
- [ ] #2 One command reproduces every figure the docs publish, so adding a gate makes the stale ones visible without a person first noticing the prose disagreed.
- [ ] #3 doc-001, gates.sh's header, lefthook.yml's header and each script's own header no longer restate the same number independently: say which file owns each figure and how the others refer to it.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Evidence, all measured in TASK-068 on 2026-09-14

Three separate contradictions existed at once:

- `scripts/check-elf-staleness.sh:37` said "Measured 0.4 s warm for ten cases" while `scripts/gates.sh`
  priced the same gate at 0.9 s. Measured truth: 0.84-0.85 s inside a tier run. TASK-068 corrected the
  line by hand.
- doc-001 published push 75 s and ci 146 s; three timed runs each gave 76-77 s and 143-144 s, and the two
  `cargo test` runs were 133 s of the 144 rather than "136 of those 146".
- The same figures live in four places that must move together: doc-001 (three separate passages),
  gates.sh's header, lefthook.yml's header, and each script's own selftest header. TASK-068 had to touch
  all four, and its plan records that TASK-056 had done exactly the same before it.

## Why it recurs

`gates.sh` already prints a per-gate `--- N.NNs` line and a `tier <name>: N gates, M.Ms` total on every
run, so the data is fresh at every single commit. What does not exist is any link from that output to the
prose. Adding a gate in one file silently invalidates a sentence in three others and nothing fails, so the
next ticket that reads a figure has to decide whether to trust it or time it again.

TASK-068 deliberately did not solve this and said so in its non-goals. Its reasoning still holds: pricing
22 gates costs about four minutes, which does not belong in a pre-commit hook. Any fix here has to be
cheaper than a full tier run, or has to live somewhere other than pre-commit.

## Constraint on any solution

Do not reintroduce a filtered or skippable job to do it. lefthook.yml:26-38 records that lefthook skips a
stage whose staged-file set is empty, exiting 0 without running the check, which is precisely how a lint
comes to read green having checked nothing. Whatever checks the figures must be either unfiltered or
outside the hook.

## A cheaper angle worth trying first

The expensive part is timing; the stale part is only the prose. Consider separating them: let the docs
state a figure as "measured <date>, re-time with <command>" and have a cheap check assert only that a
recorded measurement exists and is not older than the newest gate addition, rather than re-timing on every
commit. That converts a four-minute gate into a staleness comparison, and it removes the possibility of
two files disagreeing, which is the actual defect - not drift itself.
<!-- SECTION:NOTES:END -->
