---
id: TASK-070.03
title: >-
  Convert every published gate and tier cost figure into a ledger key across the
  eight publisher files
status: Done
assignee:
  - '@agent'
created_date: '2026-09-14 15:16'
updated_date: '2026-09-16 08:52'
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
- [x] #1 `bash scripts/gate-costs.sh --refresh` followed by `--check` exits 0 on a clean tree, and grep over the eight guarded files finds no wall-clock duration literal outside a generated region other than reasoned, counted exemptions.
- [x] #2 Every figure that carried an argument survives as either a key or words: the cheapest-first placement case for the three selftests, the ordering-rule evidence for clippy-after-build, the read-only case for the provenance gate, the cold-page-in explanation for the 0.98 s outlier, and pod-hw's priced CI-only exclusion with its reopen condition (TASK-061 AC #3 asked for priced, not absent).
- [x] #3 The live contradictions are gone rather than reconciled by hand: elf-staleness stated once (was 0.9 vs 0.85, plus a sentence asserting what gates.sh prints), image load addresses once (was 0.70/0.71 vs 0.72), the firmware-clippy idle pair once (0.25 vs 0.42/0.22), the cargo-metadata component once (one at ~85 ms vs two, and 82 ms elsewhere), and the per-child cost once (0.15 s vs ~120 ms).
- [x] #4 Component micro-costs become dated ledger components stated in one place each, not prose duplicated across files; the single-sample approximation caveat stays visible in rendered prose.
- [x] #5 Stale cross-references found while planning get fixed in passing: doc-001:250-251 still tells the reader to regenerate the matrix with gates.sh --list, and gates.sh:87 cites check-doc-artifact-names.sh:37-38 when that code sits at :49-50.
- [x] #6 Proving the guard before trusting it: hand-typing a wrong figure into a gates.sh comment and changing one ledger value without re-rendering both turn --check red naming file and line; both mutations reverted afterwards.
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

## Implementation Notes

### What shipped

One commit, eight guarded files, no wall-clock numeral left outside the ledger. Every published figure
is now a token the renderer owns: `VALUE {{gate:key}}`, `{{tier:t}}`, `{{component:key}}` or
`{{meta:measured}}`. 49 tokens across the eight files, over 21 distinct keys, plus doc-001's gate table
as a generated region between markers. Eight reasoned exemptions remain, one per figure this check
cannot see because it is not a cost claim (`grep 'gate-costs:exempt'`): cargo's own `Finished in 0.29s`,
the 2975 s mtime gap that motivated TASK-056, three historical totals quoted as evidence for why
`SECONDS` left the formatter, the audio block latency that follows from the sample rate, and two quotes
of figures another ticket shipped. A marker on a line holding no duration is reported as dead, so the
set cannot quietly rot into decoration.

`lefthook.yml` carries zero tokens by choice. Its paragraph argues that the hook is mute for as long as
the tier costs; the sentence now says the number lives in the ledger and points there, rather than
restating one it would have to keep warm.

### Two live defects the conversion exposed, both in code TASK-070.02 shipped

Both were found by reading rendered output rather than by the checker, and both passed `--check` at the
time. That is worth stating plainly: a check that grades self-consistency grades self-consistency.

1. **`strip_owned()` kept a digit.** Substitution leaves the token standing after the figure it wrote,
   so rendering a rendered line must reproduce it exactly, which means stripping the old figure first.
   The trailing-figure regex has an alternative that matches empty at line start; when the figure sat at
   the head of a line, the strip consumed nothing but the space before the token, and the new figure was
   appended after the old digit. Hand-typing `0.85 s` at the start of a doc-001 sentence rendered
   `00.86 s` - and `--check` called it clean, because the file on disk matched what the renderer
   produced. That doubled figure went out live in the previous commit. Fixed with two guards (an exact
   match preceded by a digit, comma or dot is refused; a trail whose first character is not a separator
   means the empty alternative fired, so strip the whole tail), plus case `line_start_figure`, which is
   red on revert and shows `rendered: 00.89 s`.
2. **The generated matrix marked tier membership backwards.** `generate_gate_matrix()` printed
   `(1 <= rank ? "yes" : "-")` where `rank` is the gate's own `min_tier`, so a commit-tier gate read
   `yes | - | -` and the ci-only `pod-hw` test read `yes | yes | yes`. In the plan that is the claim
   that `cargo test` runs in pre-commit and that the cheap structural gates do not run in CI - the exact
   inverse of `gates.sh --list`, and precisely the species of published false statement this ticket
   exists to retire. `--check` could not see it: it compares rendered bytes against disk, and the
   generator was consistently wrong. Direction corrected to `rank <= N`, and case `matrix_membership`
   added, which renders a fixture with one gate per tier and asserts each row's marks equal what its
   tier implies. Red on revert, naming all four rows.

### Mutation proofs (AC #6), run against the committed tree

Hand-typed figure, `scripts/gates.sh:229` changed from `~11 ms` to `~40 ms`:

```
scripts/gates.sh:229: renders differently from the file on disk.
  on disk:    # ~40 ms {{component:gates-sh-per-gate-spawn}} per gate, ...
  rendered:   # ~11 ms {{component:gates-sh-per-gate-spawn}} per gate, ...
```

Ledger moved without re-rendering, `elf-provenance-selftest` commit cost set to `0.55`: three findings
in one run - `scripts/gates.sh:54`, `scripts/elf-provenance.sh:266`, and
`backlog/docs/doc-001…:263` - which is the point of the whole design. One measurement backs citations
in three places, so one stale measurement produces three named findings rather than one plausible-
looking document. Both mutations reverted; `--check` green afterwards.

### Measurement numbers (AC #1)

`bash scripts/gate-costs.sh --refresh` inside `nix develop .#default` on a clean tree, then `--check`:
exit 0, "8 guarded files render byte-identical to docs/gate-costs.json, which prices all 22 live gates".
`measured_utc` 2026-09-16T08:48:27Z. commit 4.311 s, push 77.229 s, ci 143.696 s. Selftests 0.884,
0.845, 0.601 s; the two `cargo test` invocations 66.44 and 66.507 s, which is why `ci` is where it is.
`--selftest` 58 cases, 0 failures.

### Held back for TASK-070.04 deliberately

The `gate commit gate-costs-current` line in `scripts/gates.sh`, the sentence in `lefthook.yml` that
says the commit tier fails on a stale figure, and doc-001's "Who owns each number" subsection. Wiring
belongs to .04, and a commit that claims the check runs as a gate before it does would be the same kind
of false publication this ticket is fixing. This commit ships the check green and un-wired.

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Every published gate and tier cost figure in this repo is now a key into `docs/gate-costs.json`. The
eight guarded files hold 49 tokens over 21 keys, doc-001's gate table is a generated region, and eight
figures that are not cost claims stay literals with a reason the checker can check. `--refresh` followed
by `--check` exits 0 on a clean tree: no wall-clock numeral outside the ledger, outside a generated
region, or outside an exemption.

Converting the text was the small half. The value came from reading what the renderer actually produced,
which turned up two live defects in code that had already shipped and passed its own check: the
line-start strip that rendered `0.85 s` as `00.86 s` and called it clean (the doubled figure went out in
the previous commit), and a tier-membership comparison written backwards, which published "cargo test
runs in pre-commit" in the project plan while `gates.sh --list` said the opposite. Both fixed, each with
a case that goes red on revert.

Proof the guard bites before trusting it: a hand-typed `~40 ms` in a `gates.sh` comment reports file,
line, disk bytes and rendered bytes; moving one ledger value without re-rendering names all three sites
that cite it in a single run. Both reverted, green after.

Numbers from the real command on a clean tree: measured 2026-09-16T08:48:27Z, commit 4.311 s, push
77.229 s, ci 143.696 s, 22 gates priced per paying tier, selftests 0.884 / 0.845 / 0.601 s, the two
`cargo test` runs 66.44 and 66.507 s. Selftest 58 cases, 0 failures. Stale cross-references fixed in
passing. Ships un-wired: registering `--check` as a commit-tier gate, and writing down who owns each
number, is TASK-070.04.
<!-- SECTION:FINAL_SUMMARY:END -->
