---
id: TASK-070.03
title: >-
  Convert every published gate and tier cost figure into a ledger key across the
  eight publisher files
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-14 15:16'
updated_date: '2026-09-14 15:16'
labels:
  - planned
dependencies:
  - TASK-070.02
parent_task_id: TASK-070
priority: medium
type: chore
ordinal: 123800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Text surgery that makes scripts/gate-costs.sh --check green for the first time. Eight files publish costs today, not the four the parent ticket names: doc-001, gates.sh, lefthook.yml, .github/workflows/ci.yml, firmware/Makefile, and the headers of elf-provenance.sh, check-elf-staleness.sh and check-image-load-addresses.sh. Replace every wall-clock literal that states a gate or tier cost with a ledger key, keep every argument those numbers were carrying, and delete the duplicate copies. One commit: a half-converted repo cannot pass the check.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `bash scripts/gate-costs.sh --refresh` followed by `--check` exits 0 on a clean tree, and grep over the eight guarded files finds no wall-clock duration literal outside a generated region other than reasoned, counted exemptions.
- [ ] #2 Every figure that carried an argument survives as either a key or words: the cheapest-first placement case for the three selftests, the ordering-rule evidence for clippy-after-build, the read-only case for the provenance gate, the cold-page-in explanation for the 0.98 s outlier, and pod-hw's priced CI-only exclusion with its reopen condition (TASK-061 AC #3 asked for priced, not absent).
- [ ] #3 The live contradictions are gone rather than reconciled by hand: elf-staleness stated once (was 0.9 vs 0.85, plus a sentence asserting what gates.sh prints), image load addresses once (was 0.70/0.71 vs 0.72), the firmware-clippy idle pair once (0.25 vs 0.42/0.22), the cargo-metadata component once (one at ~85 ms vs two, and 82 ms elsewhere), and the per-child cost once (0.15 s vs ~120 ms).
- [ ] #4 Component micro-costs become dated ledger components stated in one place each, not prose duplicated across files; the single-sample approximation caveat stays visible in rendered prose.
- [ ] #5 Stale cross-references found while planning get fixed in passing: doc-001:250-251 still tells the reader to regenerate the matrix with gates.sh --list, and gates.sh:87 cites check-doc-artifact-names.sh:37-38 when that code sits at :49-50.
- [ ] #6 Proving the guard before trusting it: hand-typing a wrong figure into a gates.sh comment and changing one ledger value without re-rendering both turn --check red naming file and line; both mutations reverted afterwards.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# Shape

Text surgery across eight files, mechanical in mechanism and judgment-heavy in execution: every
duration literal that states a gate or tier cost becomes a ledger key, each argument those numbers
were carrying survives, and `scripts/gate-costs.sh --check` goes green for the first time. Do it in
one commit: a half-converted repo cannot pass the check, and every commit here is expected to be
green.

Order of work: run `--refresh` once first so the ledger holds real current numbers (it will also
disprove several published figures on the spot - record what moved), then convert, then re-render,
then `--check`.

## The rule you are applying

A wall-clock duration may appear literally in exactly one file, `docs/gate-costs.json`. Everywhere
else it is a key. Where a number was doing *argument* work rather than data work, keep the argument
and drop the numeral ("most of it is make parsing the Makefile ten times" needs no seconds). Where an
exemption is genuinely right, mark it with a reason; do not use exemptions to avoid a rewrite.

Do not "fix" a figure while you are converting it. If the fresh measurement contradicts the prose,
the ledger wins and the sentence gets the new value through its key. That contradiction is the thing
TASK-070 exists to end, not a side effect to smooth over.

## Inventory to convert (measured during planning; recount, do not trust these line numbers blindly)

**Tier totals** - `gates.sh:20`, `lefthook.yml:12`, `doc-001:282-283`, `doc-001:336-337`; ci total
also at `.github/workflows/ci.yml:67-68`. Four copies of commit/push, two of ci.

**Per-gate costs**

| gate | sites | today |
|---|---|---|
| elf-provenance-selftest | `gates.sh:202-203`, `elf-provenance.sh:265-269`, `doc-001:285,293` | 0.89 / 0.89 / 0.9 |
| elf-staleness-selftest | `gates.sh:217`, `check-elf-staleness.sh:37-39`, `doc-001:285,295` | **0.9 vs 0.85 vs 0.85**, plus a stale 0.4 named in prose |
| load-addresses-selftest | `gates.sh:239-240`, `doc-001:285,297-298,302-304` | 0.6 plus a competing 0.59-0.62 range and a 0.98 outlier |
| firmware-elf-provenance | `gates.sh:323`, `doc-001:318` | 0.08 in ci / 0.09 in push vs one context-free 0.09 |
| image-load-addresses | `gates.sh:341-342`, `doc-001:327` | **0.70/0.71 vs 0.72** |
| firmware-clippy pair | `gates.sh:348-350`, `firmware/Makefile:436-437` | one idle figure vs two (0.25 vs 0.42+0.22) |
| cargo-test-pod-hw | `gates.sh:364`, `lefthook.yml:46`, `doc-001:352` | 67 s in three files |

**Derived/comparison claims** - `doc-001:283` and `ci.yml:67-69` both state "the two cargo tests are
133 of the 144 s"; `gates.sh:364` and `doc-001:352` both state "more than every [other] push-tier
gate combined". The second is an argument with no numeral needed once the reader can see the table;
keep the claim, generate the number if it stays.

**Component micro-costs** - `gates.sh:203` (~50 ms objcopy, ~85 ms cargo metadata),
`elf-provenance.sh:163,252,255,267` (82 vs 85 ms, one vs two invocations),
`check-elf-staleness.sh:34` (~40 ms make), `gates.sh:239` (~58 ms objdump),
`check-image-load-addresses.sh:70` (~120 ms per child vs `doc-001:298`'s 0.15 s),
`gates.sh:320` (0.07 s relink), `gates.sh:341-342` (~58 ms, ~30 ms). These become dated `components`
entries, stated once each, in the file whose script they describe.

**Cheap extras, same defect class, take them while you are here** - `doc-001:250-251` still tells the
reader to regenerate the matrix with `gates.sh --list`, which stops being true once the renderer owns
the block; `gates.sh:87` cites `check-doc-artifact-names.sh:37-38` when that code is at `:49-50`.
Optional if cheap: `firmware/Makefile:284,288,349-350` duplicate quoted build timings that
`docs/reference/daisy-seed3.md:835-836` repeats verbatim.

## What must survive the edit

The comments in `gates.sh` are the reason this repo knows what it knows. Concretely: the cheapest-first
placement argument for the three selftests (`doc-001:285-291`), the ordering-rule evidence behind
placing clippy after both cross-builds (`gates.sh:348-350`), the read-only argument for
`firmware-elf-provenance` (`gates.sh:320-323`, `doc-001:318-319`), the cold-page-in explanation for
the 0.98 s outlier (`doc-001:302-304`), and above all the priced CI-only exclusion for pod-hw with its
reopen condition (`gates.sh:364-366`, `doc-001:352-355`). TASK-061 AC #3 asked for that gate to be
"priced rather than merely absent"; a conversion that deletes the price fails this ticket even if the
checker goes green. Keep the distribution honesty too - the single-sample approximation caveat belongs
in the rendered text via `{{meta:...}}`, not deleted.

## Verify

`--check` exits 0. Then prove the guard works before trusting it: hand-type a wrong figure into a
`gates.sh` comment and confirm exit 1 naming the file and line; change one ledger value and confirm
both the ledger-side and prose-side reports. Revert both. Finally `bash scripts/gates.sh commit` green
end to end.
<!-- SECTION:PLAN:END -->
