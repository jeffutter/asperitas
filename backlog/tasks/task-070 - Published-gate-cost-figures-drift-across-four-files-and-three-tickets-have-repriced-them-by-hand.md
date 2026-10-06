---
id: TASK-070
title: >-
  Published gate cost figures drift across four files, and three tickets have
  repriced them by hand
status: Done
assignee:
  - '@agent'
created_date: '2026-09-14 09:44'
updated_date: '2026-10-06 05:31'
labels:
  - planned
dependencies:
  - TASK-070.01
  - TASK-070.02
  - TASK-070.03
  - TASK-070.04
priority: medium
type: chore
ordinal: 120800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Gate and tier cost figures are stated as fact in eight tracked files with no generator behind any of them, so three tickets have repriced them by hand and two live contradictions remain unnoted. Fix: measurements become data in one committed ledger written by a command that actually times the tiers; prose cites keys; a cheap commit-tier check renders prose from the ledger and fails on any byte of difference, which covers stale values and hand-typed numbers with one rule. Numbers stop being restated, not merely re-checked.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Every published gate or tier cost figure is either generated from a measurement or explicitly marked approximate with the date, host and method that measured it; grep over the guarded files finds no wall-clock duration literal outside a generated region other than reasoned, counted exemptions.
- [x] #2 One command reproduces every figure the docs publish (scripts/gate-costs.sh --refresh), and adding a gate turns the commit tier red naming that gate, with no person having to notice the prose disagreed first.
- [x] #3 The eight files that publish costs today - doc-001, scripts/gates.sh, lefthook.yml, .github/workflows/ci.yml, firmware/Makefile and the headers of elf-provenance.sh, check-elf-staleness.sh and check-image-load-addresses.sh - no longer restate any number independently, and doc-001 names which file owns each figure and how the others refer to it.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# What is actually broken

Measured while planning this ticket, not copied from the description:

- Two live contradictions, one of them a sentence about another file that is false about its own text.
  `gates.sh:217` prices elf-staleness at 0.9 s while `check-elf-staleness.sh:37` prices it at 0.85 s
  *and asserts that gates.sh reports 0.84 and 0.85 s across three runs*, which gates.sh's own text does
  not say. TASK-068 fixed one of the pair and left the other. `gates.sh:341` says 0.70 standalone /
  0.71 in push; `doc-001:327` says 0.72. Component figures disagree too: `gates.sh:203` says one
  `cargo metadata` at ~85 ms, `elf-provenance.sh:252,267` say two, `:163` says 82 ms.
- Tier totals live in three files each (`gates.sh:20`, `lefthook.yml:12`, `doc-001:282` + `:337`; ci in
  `doc-001:283` and `ci.yml:68`), pod-hw's 67 s in three, the 133-of-144 split in two.
- The tier x gate matrix **is** generated and **is** currently byte-identical to `gates.sh --list` -
  verified with `diff` while planning - and nothing checks it. doc-001:250 says "generated, do not
  hand-edit"; the only thing that has ever enforced that is a human remembering to re-copy it in
  TASK-068's commit.
- Eight files publish costs, not four: add `.github/workflows/ci.yml`, `firmware/Makefile:436-437`,
  `scripts/elf-provenance.sh` and `scripts/check-image-load-addresses.sh`.
- The published tier total is a floored whole-second number wearing a decimal point: `gates.sh:386-387`
  formats bash's integer `SECONDS` through `%.1f`, so it can only print `X.0` while the truth sits in
  `[X, X+1)`. That is part of why TASK-068 published commit `4 s` against a gate sum of 4.15-4.37 s and
  why TASK-064.01 saw `145.0s` a day after TASK-068 saw `143-144 s`.

So the defect is not drift, which is survivable. It is that eight files state figures as fact with no
generator behind any of them, which means the next ticket has to decide whether to trust prose or pay
four minutes to time it. Three tickets already paid that; TASK-060.04's notes observed that every prior
inventory here went stale within two tickets.

# Approach: numbers become data, prose holds keys

One committed measurement ledger, `docs/gate-costs.json`, written by a command that actually times the
tiers. Every place that quotes a figure cites a key instead, and a cheap commit-tier check renders the
prose from the ledger and fails on any byte of difference - which covers stale values and hand-typed
numbers with one rule. Adding a gate then goes red immediately because the ledger has no entry for it,
at a cost of about 0.1 s rather than four minutes.

Two alternatives rejected, both worth keeping on record so nobody retries them:

- **Re-time in the hook.** Four minutes does not belong in pre-commit (TASK-068's non-goal still
  holds), and lefthook's constraint makes the obvious shape unsafe: `lefthook.yml:23-27` records that a
  path-filtered job with an empty set exits 0 without running, which is precisely how this check would
  turn decorative.
- **Date-based staleness** ("older than the newest gate addition"). This is the ticket's own suggestion
  and it should be refused on precedent: TASK-056 spent its existence replacing mtime freshness with a
  content digest after a bulk mtime refresh caused a spurious failure, and added a text-based tripwire
  so `-newer` never returns. A calendar rule reintroduces exactly the dependence that ticket removed,
  and needs a clock to be wrong in a new way. Use per-gate command digests instead: same mechanism the
  repo already trusts (`firmware/Makefile:221-274`), finer-grained (retiering or rewording a gate does
  not invalidate twenty unrelated measurements), and it satisfies AC #1 without a date at all.

Keeping keys inline rather than centralizing every figure into one doc-001 table is the one place this
plan spends complexity to buy something: `gates.sh`'s comments are priced arguments ("a cfg switch
finishes in {{...}} by re-uplifting the cached hardlink, so the check would grade its own side effect")
and stripping their numbers guts the reasoning. With keys, the numeral still appears in exactly one
file and the argument stays where it is read.

# Children and order

Strictly sequential; each child blocks the parent and the next child depends on the previous one. All
four are planned, none needs hands.

1. **TASK-070.01** - `gates.sh`: stable per-gate keys, an opt-in machine-readable timing sink, and a
   sub-second tier total. Additive; nothing consumes it yet. Removes the fragile positional pairing of
   banners to `--- N.NNs` lines, where any child printing a lookalike line silently corrupts the join.
2. **TASK-070.02** - `scripts/gate-costs.sh`: ledger schema, `--refresh` (measure then publish),
   renderer, `--check`, and a fixture-driven `--selftest`. Ships un-wired, so it cannot break anything.
   Depends on 01 for keys and the timing sink.
3. **TASK-070.03** - convert all eight publisher files to keys and generated regions, preserving every
   argument the numbers carried, including pod-hw's priced CI-only exclusion with its reopen condition
   (TASK-061 AC #3 asked for that gate to be priced rather than merely absent).
4. **TASK-070.04** - register `--check` as a commit-tier gate, prove it red under eight deliberate
   mutations, and write the ownership subsection AC #3 asks for. Registering adds a gate, so this commit
   also re-times everything (~4 min).

# Integration and verification

The parent is done when, on a clean tree: `bash scripts/gate-costs.sh --refresh && bash
scripts/gate-costs.sh --check` exits 0; `grep` over the eight guarded files finds no wall-clock literal
outside a generated region except reasoned, counted exemptions; `bash scripts/gates.sh commit` is green
and its added cost is measured and quoted; and doc-001 names one owner per figure plus the command that
reproduces it. The mutation list in TASK-070.04 step 2 is the real evidence - watched-green is the
failure mode this repo keeps hitting, from `elf-provenance.sh`'s cases that were described as asserted
but never run (TASK-067) to the matrix block that has claimed to be generated since TASK-061.02.

# Assignment

No `@human` child. Nothing here needs the board, ears, an instrument or a bench decision: re-timing is
`nix develop .#default --command bash scripts/gate-costs.sh --refresh`, roughly four minutes of cargo
and clippy on the dev machine, which an agent can run and read. Runner-side figures stay deferred to
TASK-052/TASK-063, which are already `@human` for the right reason - `ci.yml` has no
`workflow_dispatch`, no artifact upload and no `permissions:` block, so nobody can observe or write back
from a runner. The one judgment call, whether single-sample timing is honest enough to publish, is
answered inside the design (mark it approximate, store the sample count and host, keep the distribution
caveat in the rendered prose) rather than escalated.
<!-- SECTION:PLAN:END -->

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
