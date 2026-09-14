---
id: TASK-070.01
title: >-
  Give every gate a stable key and a machine-readable timing sink in
  scripts/gates.sh
status: Done
assignee:
  - '@agent'
created_date: '2026-09-14 15:13'
updated_date: '2026-09-14 15:37'
labels:
  - planned
dependencies: []
parent_task_id: TASK-070
priority: high
type: chore
ordinal: 121800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Additive change to scripts/gates.sh that makes gate identity and gate cost addressable. Today a gate is identified only by its display banner, and its only recorded cost is the human line `--- 0.89s`, which can only be joined to a gate positionally - a child process printing a lookalike line silently corrupts it. Add a validated key per gate line, an opt-in GATES_TIMINGS_FILE sink that writes key-and-seconds directly, and replace the SECONDS-based tier total (an integer counter printed through %.1f, so it can only ever print X.0) with a real sub-second measurement. Nothing consumes these yet; TASK-070.02 is the first consumer, so this commit cannot break a build.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `gate` takes a validated unique key per gate (`^[a-z0-9][a-z0-9-]{0,39}$`) and exits 2 naming the offender on a duplicate or malformed key; all 22 gates carry keys and no executed command, tier or ordering changes - proven by diffing `--dry-run ci` output modulo the new key field.
- [x] #2 `--list` and `--dry-run` both expose the key, and the matrix block embedded in doc-001 is re-copied byte-exact in this commit (diff against `bash scripts/gates.sh --list` prints nothing).
- [x] #3 With GATES_TIMINGS_FILE set, a commit-tier run appends one tab-separated line per completed gate plus a final tier line; the file has exactly as many gate lines as the tier reported, and each seconds value matches that gate's own `--- N.NNs` line to rounding.
- [x] #4 The tier total is no longer floored to whole seconds: measured across a commit and a push run, it now carries sub-second resolution and is never below the sum of its own gate lines by more than the documented overhead.
- [x] #5 `bash scripts/gates.sh commit` and `lefthook run pre-commit -f` both exit 0 with unchanged human output apart from the corrected total.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# Shape

Three additions to `scripts/gates.sh`, none of which changes what any caller executes: a stable key
per gate, a machine-readable timing sink, and an honest sub-second tier total. Nothing consumes them
yet - `scripts/gate-costs.sh` (TASK-070.02) is the first consumer - so this commit cannot break a
build. Additive only, same shape as TASK-061.01.

## Step 1 - one stable key per gate

Change the signature to:

    gate <min-tier> <key> "<banner>" [-C dir] <command...>

The key is the gate's identity; the banner stays display-only. Two reasons not to key on the banner:
prose has to cite a cost inside a sentence (`the provenance reader, {{gate:...}}, reads only`), and
a banner reworded for readability must not orphan the measurement recorded against it. That is the
same argument that put the gate list in a script rather than in YAML names.

Validate inside `gate()`: key matches `^[a-z0-9][a-z0-9-]{0,39}$`, and a repeated key exits 2 naming
both offenders. `=== gate definition parses ===` already runs `bash -n` on this file every commit,
so a malformed key is caught at the cheapest possible point once `gate()` enforces it at call time.

Keys, in declaration order (this is the mapping, do not renumber later without re-recording):

    gate-definition-parses        docs-artifact-names        elf-provenance-selftest
    elf-staleness-selftest        load-addresses-selftest    cargo-fmt
    cargo-fmt-firmware            cargo-clippy               cargo-clippy-log-usb
    cargo-clippy-log-defmt        clippy-pod-hw              dump-reassemble-selftest
    cargo-doc                     cargo-doc-all-features     firmware-cross-compile
    firmware-cross-compile-rtt    firmware-elf-provenance    image-load-addresses
    firmware-clippy               firmware-clippy-rtt        cargo-test
    cargo-test-pod-hw

`--list` gains a leading `key` column (`gates.sh:131-134` is the single row formatter, header at
`:162-163`); `--dry-run` gains the same key as its second field (`:136`). Both are the interfaces
TASK-070.02 parses, so keep them fixed-width/tabbed exactly as you print them and say so in the
comment above each.

## Step 2 - machine-readable timings, no parsing of human output

Today the only record of a gate's cost is the human line `--- 0.89s` (`:152`), and pairing those with
banners is positional: a child that prints `--- 1.23s` or echoes a banner would silently corrupt the
join, and `cargo test` / `cargo doc` are exactly that hazard class. Do not make anyone parse stdout.

Add an opt-in sink: when `GATES_TIMINGS_FILE` is set, append, per completed gate,

    gate<TAB><key><TAB><seconds to 3 decimals>

and after the last gate,

    tier<TAB><tier><TAB><seconds to 3 decimals>

Reuse the microseconds already read at `:145` and `:151`; do not add a second clock. Append with
`>>` and let the writer truncate first, so a partial run is visibly partial. The human `--- N.NNs`
line and the `tier %s: %d gates, %.1fs` summary both stay: the hook's contract is still a quiet run
and a true exit code.

## Step 3 - the tier total is currently a floored whole-second number

Found while planning this ticket, and it explains part of the drift TASK-070 is about. The summary
at `:386-387` formats bash's `SECONDS`, an integer counter, through `%.1f` - so it can only ever
print `X.0`, and the true elapsed sits anywhere in `[X, X+1)`. Measured during planning: a 2437 ms
sleep leaves `SECONDS=2`. That is why TASK-068 published `commit 4 s` against a gate sum of
4.15-4.37 s, and why TASK-064.01 saw `145.0s` where TASK-068 saw `143-144 s`. Take start
microseconds from `EPOCHREALTIME` before the first gate runs and print the real difference, so the
total and the parts become comparable. Keep the printed format.

## Step 4 - carry the embedded matrix forward once more, by hand

`doc-001:253-280` embeds `--list` output verbatim and nothing verifies it (that changes in
TASK-070.04). Since the row format changes here, re-copy it in this commit and prove it:

    diff <(sed -n '254,279p' "backlog/docs/doc-001 - Asperitas-Project-Plan.md") <(bash scripts/gates.sh --list)

Line numbers shift when the key column lands; recount them and update the `sed` range rather than
trusting the ones above.

## Do not

- Do not change any executed command, tier assignment, or ordering. Ordering rules 1 and 2 in the
  header are load-bearing; prove independence with a diff of `--dry-run ci` modulo the new key field.
- Do not touch `lefthook.yml`, `.github/workflows/ci.yml`, or any other script. No new gate either -
  that is TASK-070.04.
- Do not delete the human timing lines. They are what makes a red tier diagnosable from the log.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
AC#1 keys: gate() now takes <min-tier> <key> <banner>; validates ^[a-z0-9][a-z0-9-]{0,39}$ and uniqueness against a SEEN_KEYS map recording the declaring line. Proved by injection into a copy of the file: 'bad gate key "Docs_Artifact!" at line 236' exit 2; 'duplicate gate key "elf-provenance-selftest": used at line 255 and again at line 296' exit 2. Both fire in --list and --dry-run too, so the cheapest command that touches gates.sh catches it. Ordering/tier/command independence proved mechanically: bash scripts/gates.sh --dry-run ci before the change vs after with the new field cut out (cut -f1,3) diffs empty across all 22 lines.

AC#2 --list gains a leading fixed-width key column (%-27s, widest key is firmware-cross-compile-rtt at 26); --dry-run gains the key as its second tab-separated field. Both are commented as machine-read interfaces whose columns are contract, not taste. doc-001's matrix block re-copied: diff <(sed -n '254,279p' doc-001) <(bash scripts/gates.sh --list) prints nothing (re-checked after the comment edits landed). Longest generated line is now 112 chars inside a fenced block that already sits among 100-236 char lines elsewhere in the file.

AC#3 GATES_TIMINGS_FILE sink over a real commit run: 12 gate lines + 1 tier line, matching the 12 '--- N.NNs' lines the same run printed, each agreeing to rounding (e.g. elf-staleness-selftest 0.853 vs 0.85s, cargo-fmt-firmware 0.394 vs 0.39s). Appended, never truncated, so a mid-tier death leaves a partial file; one EPOCHREALTIME read per gate feeds both the human line and the sink.

AC#4 The total no longer reads SECONDS through %.1f. Commit run: 4.750 s against a gate sum of 4.618 (overhead 132 ms / 12 gates). Push run: 77.547 s against 77.322 (225 ms / 21 gates), i.e. ~11 ms per gate, which is the awk that formats each timing line. That overhead figure is written in the code where TIER_START_US is set, so a future run can tell an honest gap from a leak. For contrast the same-shape old script printed 'tier commit: 12 gates, 4.0s' on a run whose gates summed past 4.3 - the flooring the header now names.

AC#5 bash scripts/gates.sh commit exit 0; lefthook run pre-commit -f exit 0 (gates 4.32 s). Human output diffed against HEAD's gates.sh run side by side: the only differences are per-gate timings that vary run to run, cargo's own Finished-in lines, and the tier total (4.0s floored vs 4.8s real).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
scripts/gates.sh: every gate now carries a validated unique key beside its banner, --list/--dry-run expose it as a fixed-width/tabbed column, GATES_TIMINGS_FILE writes machine-readable per-gate and per-tier seconds without anyone parsing stdout, and the tier total is a real sub-second measurement instead of bash's integer SECONDS wearing a decimal point. Additive: no command, tier or ordering changed, proved by diffing --dry-run ci modulo the new field. doc-001's embedded matrix re-copied byte-exact.
<!-- SECTION:FINAL_SUMMARY:END -->
