---
id: TASK-061
title: 'Define the gate set once, so ci.yml and lefthook cannot diverge again'
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-13 00:10'
updated_date: '2026-09-13 05:39'
labels:
  - planned
dependencies:
  - TASK-060
  - TASK-061.01
  - TASK-061.02
references:
  - .github/workflows/ci.yml
  - lefthook.yml
  - scripts/check-doc-artifact-names.sh
priority: medium
type: chore
ordinal: 97800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-060 lands firmware fmt and clippy into three call sites that are already near-duplicates of each other, and the reason it needs three is the bug: `lefthook.yml`'s pre-commit/pre-push lists and `.github/workflows/ci.yml`'s single bash step implement the same idea twice, by hand, with no cross-reference. Every check added since has drifted between them.

The drift record, all verified 2026-09-12:

- TASK-018.01's fixup added the `asperitas-pod/pod-hw` clippy/test pair to **CI only** (`c44b9c1`, now `ci.yml:40-41,57-58`). Still missing from pre-push after this ticket, deliberately, with a reason - see TASK-060.04.
- `dump_reassemble --selftest` and the RTT-only cross-compile: **CI only**.
- TASK-049 widened doc links on **both** sides, which is the exception that proves the rule - it took a ticket to remember to do it.
- TASK-060 itself exists because firmware was invisible to both lists, and four closed tickets each declined to fix that.

Two consolidation shapes, both measured as viable, neither decided here on purpose:

1. **One script, two callers.** Extract ci.yml's ordered check list into `scripts/check-all.sh`; ci.yml calls it inside `nix develop .#default`, and lefthook's pre-push runs the same file. Precedent in-repo: `scripts/check-doc-artifact-names.sh` (TASK-058) is already shared this way, and `backlog/unblocked-todo.sh` opens with a war story about exactly this failure mode ("the same rules implemented twice drift; only one of the two copies gets noticed when that happens"). Deep-module shape: one command, no configuration, callers cannot get it wrong.
2. **CI delegates to the hooks.** `lefthook run pre-push --all-files` inside the nix shell makes CI execute the hook definitions instead of restating them. Caveats found: commands with no file template can still be skipped when their filtered file set is empty (lefthook #1038, #554), `--all-files` does *not* bypass `root:` filtering (verified against 2.1.10 source and probes), and `--force/-f` is the escape hatch. Also loses GitHub's per-step output unless the checks stay separate jobs.

Constraints either shape must respect, measured or read from lefthook 2.1.10 today:

- Commands run **sequentially** unless the hook sets `parallel: true` (`controller.go:100-106`), and are sorted by `priority`, then leading digits in the name, then name ascending (`command.go:36-92`) - **not** yml order. `lefthook dump` shows the current pre-commit order as `clippy, clippy-log-defmt, clippy-log-usb, doc-artifact-names, fmt-check`.
- `root:` sets cwd *and* filters paths; an empty filtered set skips the job silently with exit 0 (`docs/configuration/root.md:11`, plus probes). Any consolidated definition must not put a lint behind a filter.
- A manual `lefthook run pre-commit` on a clean tree is a no-op for every command, so a CI-side invocation must pass `--force` or `--all-files` explicitly.
- Local cost baseline for whatever replaces the current lists: the verbatim ci.yml suite is 138 s warm, 133 s of which is the two `cargo test --workspace` runs. CI itself has **no cache at all** today (no `actions/cache`, no `Swatinem/rust-cache`, no sccache), so upstream prior art is on the table if wall time becomes the objection: Swatinem/rust-cache supports multiple workspaces (`workspaces: ".\nfirmware -> target"`) and nix shells (PR #290), or `RUSTC_WRAPPER=sccache`. daisy-embassy's own CI runs a separate `cargo fmt -- --check` job plus one `cargo clippy --features X -- deny=warnings` per feature variant with `dtolnay/rust-toolchain` pinning the thumbv7em target; other Embassy-with-firmware repos keep nested workspaces and use `working-directory:` rather than folding them.

Out of scope and owned elsewhere: what the checks *are* (TASK-060 decides that), firmware docs being ungated by `cargo doc` (TASK-049's note), pedantic clippy adoption, and `make elf-check`'s mtime test (TASK-056).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 One definition of the gate set exists, and both .github/workflows/ci.yml and lefthook.yml invoke it rather than restating individual cargo checks.
- [x] #2 Behaviour is preserved: every check that ran before still runs, measured local warm cost is no worse than the 138 s baseline, and any newly shared item's cost delta is recorded per call site.
- [x] #3 Deliberate exclusions survive consolidation as explicit commented exceptions rather than silent absences - the pod-hw test pair from TASK-060.04 is the known case.
- [x] #4 No lint job sits behind a lefthook path filter or any other condition that can skip it silently with exit 0.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Decision: shape 1, one script, three callers

`scripts/gates.sh <commit|push|ci>` becomes the only place a check is named. `.github/workflows/ci.yml`
runs the `ci` tier; lefthook's `pre-commit` runs `commit` and `pre-push` runs `push`. Each caller is
one line, so AC #1 holds and there is nothing left to restate.

Shape 2 (`lefthook run pre-push --all-files` inside CI) was rejected on measured behaviour, not
taste:

- Commands are sorted by priority, then leading digits, then name - **not** YAML order. Verified with
  `lefthook dump`: today's pre-commit really runs `ci-steps-parse, clippy, clippy-firmware,
  clippy-firmware-rtt, clippy-log-defmt, clippy-log-usb, doc-artifact-names, fmt-check,
  fmt-check-firmware`, i.e. the 0.2 s docs gate runs eighth behind every clippy, and in pre-push the
  cross-clippies run *before* the cross-builds whose artifacts they are supposed to reuse
  (`.github/ci-steps.sh:90-92` says otherwise). Two of this repo's cost decisions - cheapest-first
  fail-fast, build-before-lint artifact reuse - are simply unexpressible in hook config. A script can
  say them.
- Path filtering skips silently with exit 0. One job still uses it today (`lefthook.yml:143-146`), so
  AC #4 is already violated at HEAD and `doc-001:231-233`'s claim that no lint job does is false.
  `--all-files` does not bypass `root:`; only `-f` does.
- `lefthook run` re-syncs `.git/hooks` unless told not to, which would have CI rewriting its own
  checkout mid-run (`--no-auto-install` exists, but it is one more thing a caller must remember -
  exactly the class of mistake this ticket is meant to end).
- Per-step GitHub output disappears either way, and shape 2 buys nothing that shape 1 does not.

What shape 1 gives up, recorded so nobody re-litigates it: lefthook captures a command's stdout and
replays it at the end, so a hook now prints nothing until the tier finishes (~70 s for push). In
exchange the log gets per-gate headers and per-gate wall times that never existed before, plus
deterministic ordering and fail-fast that alphabetical command names could not provide. The escape
hatch (`parallel: true` + `priority: 100000`, streams while preserving declared order, but the first
failing job may not be reported first) is written into `lefthook.yml` as a comment rather than used.

The tier argument is the one knob, and it is genuine rather than decorative: the three call sites
have budgets two orders of magnitude apart (≈1 s / ≈70 s / ≈138 s warm local), and a commit loop that
runs nine gates cannot pay for a test suite. Cumulative tiers (`commit ⊂ push ⊂ ci`) match how the
lists actually relate today - 9 / 16 / 17 gates, pre-commit a strict subset of pre-push, pre-push and
CI differing by exactly two items, both deliberate and both commented. No argument means `ci`, so
forgetting the argument can never mean "ran less than everything".

## Sub-tickets

Both are `@agent`, both planned, executed in this order (`.02` depends on `.01`; both block here):

- **TASK-061.01** - write `scripts/gates.sh`: cumulative tiers, repo-root anchoring, verbatim banner
  labels, per-gate timings, scoped firmware subshell, thumbv7em preflight, `--dry-run` / `--list`.
  Additive: nothing calls it yet, so it cannot break a build, and equivalence against today's three
  lists is proven mechanically by diffing `--dry-run` output against the extracted `run:` lines and
  ci-steps.sh blocks. That diff is AC #2's evidence.
- **TASK-061.02** - point `ci.yml` and both hook stages at it, `git rm .github/ci-steps.sh`, delete
  25 duplicated `run:` lines and the last `root:`, prove the hooks really fire and propagate failure,
  measure all three tiers warm, and re-point every live prose reference (doc-001, `firmware/Makefile`,
  `scripts/check-doc-artifact-names.sh`) plus a comment on each open ticket that still names the dead
  path.

Split where the risk changes shape: `.01` is "is the new definition equivalent", `.02` is "does
everything that calls it survive the switch". They are separate commits because a bug found during
`.02` should cost one small revert, not the loss of the equivalence work.

## Integration verification (what closes this ticket)

1. `scripts/gates.sh --list` regenerated into doc-001, and the matrix compared cell-by-cell against
   the census: 9 / 16 / 17 gates, with `cargo test --workspace --features asperitas-pod/pod-hw` the
   only single-tier item and the self-parse gate the only net-new one.
2. All three tiers green end to end inside `nix develop .#default` on a clean tree, with the printed
   per-gate times pasted into the notes and the `ci` tier ≤ 138 s warm.
3. Hook wiring observed, not inferred: `lefthook dump` shows one command per stage, `grep -c 'root:'
   lefthook.yml` prints 0, `lefthook run pre-commit` / `pre-push` execute rather than skip, an empty
   `git commit` shows the gate output, and a deliberately broken gate aborts a commit non-zero.
4. Bench untouched: `git status --porcelain` empty after the runs, and
   `firmware/target/thumbv7em-none-eabihf/release/main` back to its starting sha256. `make -C firmware
   elf-check` is red on a clean tree before this work starts (mtime-only drift, rc=2) and is expected
   to be exactly as red afterwards - TASK-056 and TASK-062 own it.
5. AC #3 durability check: grep the consolidated file for the pod-hw exception and confirm the
   exclusion reads as a priced decision with a reopen condition, not an omission. Also confirm the
   other asymmetry is written down - `bash -n` used to exist only in the hooks and now runs in all
   three tiers as the first gate.

## Deliberate human-owned work, and why it is not a child of this ticket

Nothing here needs hands: every criterion is provable locally, including the wall-time figures
("local warm" is what the baseline itself is labelled as). What cannot be done by an agent is pushing
main and watching GitHub actually execute the thing - main sits ~91 commits ahead of origin/main,
`ci.yml` triggers only on push/PR with no `workflow_dispatch`, and the last real CI run predates the
breakage that TASK-060 found. That observation is already owned by **TASK-063** (@human, filed by
TASK-060's integration pass for exactly this purpose), so filing a second one would duplicate it.
Instead TASK-063 now depends on TASK-061, so the single pending push happens against the final shape,
and its criteria get a comment naming `scripts/gates.sh ci` and asking for the per-gate timings the
new script prints - which also discharges most of TASK-052's long-standing debt of runner-side
figures. This ticket is *not* gated on that push: making agent work wait on a human is how the loop
stalls, and TASK-060 set the same precedent.

## Out of scope

What the checks *are* (TASK-060), firmware docs being ungated by `cargo doc` (TASK-049's note),
pedantic clippy adoption, `elf-check`'s mtime test (TASK-056) and cfg blindness (TASK-062), CI caching
(`Swatinem/rust-cache` multi-workspace or `RUSTC_WRAPPER=sccache` - worth its own ticket, and a
prerequisite for any idea about fanning the gate list out into one job per gate, since CI has no
cache at all today and 17 jobs would mean 17 cold builds), folding `firmware/` into the root workspace
(rejected at TASK-060:116), and `cargo hack --feature-powerset` collapsing the feature-variant lints
(exponential, and this ticket is about one definition, not fewer checks).

One correction to carry forward: this description's drift record cites `ci.yml:40-41,57-58` for the
pod-hw pair. Those coordinates died in 70c6fc6; the pair now lives at `.github/ci-steps.sh:45-46` and
`:62-63`, and will live in `scripts/gates.sh` after TASK-061.02.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Integration verification (umbrella pass, all figures LOCAL warm, aarch64-darwin, inside `nix develop .#default`, clean tree)

Both leaves carry their own evidence; this pass re-ran the five closure checks named in the plan
rather than trusting them, and one new defect fell out.

### 1. Matrix vs census, cell by cell

`scripts/gates.sh --list` output diffs byte-identical against the block doc-001 embeds (extracted at
its fence and compared to the live command - only difference is the blank line before the closing
fence). Counts: commit 9, push 16, ci 17. Exactly one row has `min = ci`, the `pod-hw` test, so it is
the only single-tier item. Then an independent set-diff, written fresh rather than reusing `.01`'s
script, against the pre-consolidation lists extracted from `24049b3` (`git show` of the old
`lefthook.yml` with each job's `root:` rendered as the `cd <dir> && ` prefix the script uses, and of
the old `.github/ci-steps.sh` with the subshell's inherited cwd made explicit):

    ### tier commit: old 9  vs new 9   IDENTICAL SETS
    ### tier push:   old 16 vs new 16  IDENTICAL SETS
    ### tier ci:     old 16 vs new 17  DIFF: + PARSE-GATE

One difference across three tiers, and it is the net-new self-parse gate (CI never had a parse gate;
the hooks' `bash -n .github/ci-steps.sh` could only become `bash -n "$BASH_SOURCE"`). Zero
unexplained membership change.

### 2. All three tiers green end to end, plus both hook stages

Five runs back to back, all rc 0, wall clock including nix shell startup in brackets:

| run | gates | script-reported | wall |
| --- | --- | --- | --- |
| `gates.sh commit` | 9 | 2.0 s | [3 s] |
| `lefthook run pre-commit -f` | 9 | 2.0 s | [2 s] |
| `gates.sh push` | 16 | 73.0 s | [74 s] |
| `lefthook run pre-push -f` | 16 | 73.0 s | [74 s] |
| `gates.sh ci` | 17 | 140.0 s | [140 s] |

Per-gate seconds from the `ci` run, in execution order: gate definition parses 0.01, docs artifact
names 0.16, cargo fmt 0.25, cargo fmt (firmware workspace) 0.41, cargo clippy 0.35, clippy log-usb
0.21, clippy log-defmt 0.20, clippy pod-hw 0.22, dump_reassemble --selftest 0.57, cargo doc 1.33,
cargo doc (all features) 1.58, firmware cross-compile 0.34, firmware cross-compile (RTT-only) 0.13,
firmware clippy 0.43, firmware clippy (RTT-only) 0.22, cargo test 66.30, cargo test (pod-hw) 66.52.
The two test invocations are 132.8 of 140.0 s - still the whole cost story, and why the pod-hw test
stays ci-only.

Against AC #2's 138 s bar: re-measured the OLD list in this same session (`git show
24049b3:.github/ci-steps.sh`, run from the repo root in the same shell) at **141 s**, against the new
`ci` tier's **140 s** ten minutes earlier. Parity, slightly better, with one more gate. The quoted
138 s is optimistic against this machine today - the old list itself does not reproduce it - which is
why every figure above is paired within one session. Runner-side numbers stay owed to TASK-052 /
TASK-063; no agent can observe a runner.

### 3. Hook wiring observed

`lefthook dump` prints the entire effective config and it is four lines of substance:
`min_version: 2.1.10` plus one `gates` command per stage. Every filtering or conditional key greps to
zero in `lefthook.yml`: `root:`, `glob:`, `files:`, `local:`, `staged_files`, `push_files`,
`parallel:`, `priority:`, `only:`, `stdout:`. Both stages execute rather than skip under `-f` (9 and
16 banners with timings above). Failure propagation re-proven at the umbrella level rather than
inherited from `.02`: planted `gate commit "=== PLANTED FAILING GATE TASK-061 INTEGRATION ===" false`
as the second commit gate, staged a file, ran a real `git commit` inside the nix shell -> rc 1
printing `*** gate failed: ...` and `*** tier: commit (1 of 2 gates completed before it)`, HEAD
unchanged at `9283670`. `scripts/gates.sh` restored byte-identical (`cmp`), probe file unstaged and
deleted, the string greps to zero outside completed ticket prose, nothing committed.

### 4. Bench left as found

Before any run: ELF digest `9b60b8ffde4d2270e9a043feb300f50283762cdeee537ca81537f36b937fe80b`,
`make -C firmware elf-check` rc 2 ("older than src/bin/podtest.rs"). After all five runs the ELF was
`16dc9e5c...`, the RTT image, exactly as ordering rule 1 predicts when a tier ends with the RTT-only
build. Restored with `rm -f` plus `make -C firmware build-elf` -> digest back to `9b60b8ff...`, and
elf-check rc 2 with a message diff-identical to the before-capture. Still red, still for the mtime
reason TASK-056 owns. `git status --porcelain` after every run listed only files this ticket itself
was changing.

### 5. Exclusions read as decisions

`grep` on the consolidated file finds the exception at its own gate: priced (67 s warm, more than
every other push-tier gate combined), scoped (its compile-time half runs in push as the pod-hw
clippy, so only runtime coverage stays remote), attributed (CI is the authority, TASK-018.01's fixup
made the split at `c44b9c1`, the loop that writes most commits here never pushes), and carrying a
reopen condition stated as a condition. The other asymmetry is written down too: `bash -n` used to
exist only in the hooks and is now the first gate of all three tiers, with the six-day
apostrophe-in-`bash -c '...'` outage it exists to catch recorded next to it.

## New finding: `forward_stderr: true` is a key lefthook has never had

Every command in `lefthook.yml` carried `forward_stderr: true`, copied forward by at least four
tickets (TASK-036.05, TASK-043, TASK-049, TASK-060.02 each add commands "with `forward_stderr: true`
like their neighbours"). It does nothing. Measured three ways:

- Not in lefthook at all, three ways. **Schema:** the published `schema.json` lists eighteen
  snake_case option keys (`stage_fixed`, `exclude_tags`, `use_stdin`, `no_auto_install`, `min_version`
  ...) and `forward_stderr` is not one of them. **Source:** `grep -rin "forward"` over the whole
  unpacked tree - 117 Go files, `schema.json`, the docs - returns exactly one hit, the word
  "forward" in prose on the `interactive:` page. **Binary:** `strings` over the pinned
  `lefthook-2.1.10` executable finds `forward_stderr` zero times where the real keys
  `stage_fixed` (6), `exclude_tags` (3) and `interactive` (7) show up as tag strings. The source
  grepped is what nixpkgs currently serves, version 2.1.12 by its CHANGELOG; the binary tested is
  2.1.10, what the flake pins and what `lefthook --version` reports.
- Unknown keys are accepted silently: a scratch config with `bogus_key_xyz: true` dumps clean and
  exits 0, and `lefthook dump` omits both keys without a word of warning. That is how a key that has
  never existed survived a dozen edits.
- The behaviour it names happens anyway: a command writing to stderr and exiting 3 printed both its
  stdout and its stderr through lefthook's capture and turned exit status 3 into lefthook rc 1, with
  no such key present.

Removed from both stages. Left behind, in the file header, is the measurement and the reason it
matters here: this file's contract after this ticket is that a line in it means something, and a key
that silently means nothing is the same bug the ticket was filed for - two spellings of one intent,
only one of which gets noticed. `lefthook dump` after the edit still shows one `gates` command per
stage, and `lefthook run pre-commit -f` still executes all nine gates at rc 0.

## Verdict on the four acceptance criteria

1. One definition, both callers invoking it: holds, and the umbrella re-derived it from the dead
   lists rather than from the leaves' claim.
2. Behaviour preserved: holds on sets (identical but for the net-new parse gate) and on cost (140 s
   new against 141 s old, same session, same machine, one gate richer). Per-call-site deltas were
   already recorded per gate in `.02`; nothing here contradicts them.
3. Exclusions explicit: holds, and now also covers the phantom key, which was an exclusion nobody
   chose.
4. Nothing skippable with exit 0: holds. `lefthook dump` is the proof, being the whole effective
   config. The one skip that survives - lefthook skipping a stage whose staged-file set is empty,
   unconfigurable in 2.1.10 - is benign because an empty commit changes no tree, and is written up in
   `lefthook.yml` and doc-001.

One follow-up filed: **TASK-064** (parent @human, children TASK-064.01 @agent writes the cache layer
for both workspaces, TASK-064.02 @human measures cold against warm on a runner). The plan named CI
caching out of scope for this ticket and worth its own ticket; it had no owner anywhere outside
this ticket's prose, which is how the last four gaps stayed unfixed, and this pass re-measured why it
matters - 132.8 s of the 140 s tier is two `cargo test` runs that a runner will do from cold every
push. Everything else already has an owner: the phantom key is fixed here, `elf-check`'s mtime and cfg
blindness belong to TASK-056 and TASK-062, and the runner-side figures to TASK-052 and TASK-063.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Both leaves landed before this pass, so the umbrella did what an umbrella is for: re-ran the five closure checks itself instead of inheriting them. One definition of the gate set now stands, `scripts/gates.sh`, with `.github/workflows/ci.yml` asking for `ci` and lefthook's two stages asking for `commit` and `push`; `lefthook dump` prints four lines of substance and no check is named anywhere else. Membership re-derived from the dead lists at `24049b3` with a fresh extraction script rather than `.01`'s: identical sets in all three tiers, 9 / 16 / 17 gates, the only difference across all three being the net-new self-parse gate CI never had. Cost re-measured paired inside one session rather than against a quote: old CI list 141 s against the new `ci` tier's 140 s ten minutes apart, one gate richer, with the two `cargo test` invocations still 132.8 of those 140 s - the ticket's 138 s bar does not reproduce on this machine even for the list it was measured from, which is why every figure here sits next to its own baseline. All five runs green end to end (commit 2.0 s, push 73.0 s, ci 140.0 s, both hook stages matching their tiers), per-gate timings recorded, and a planted failing gate aborted a real `git commit` at rc 1 naming itself with HEAD unmoved and `gates.sh` restored byte-identical. Bench as found: ELF back to `9b60b8ff`, `elf-check` rc 2 with a message diff-identical to the before-capture, tree clean after every run. The exception survives as a priced decision at its own gate - 67 s warm, compile-time half already in push, reopen condition stated as a condition - and the other asymmetry is written down too: `bash -n` now runs first in all three tiers where it used to exist only in the hooks. One defect fell out of asking a question nobody had asked: every command in `lefthook.yml` carried `forward_stderr: true`, copied forward by at least four tickets as something their neighbours had, and the key exists in no lefthook release - absent from the published `schema.json`'s eighteen option keys, from all 117 Go files and docs of the current source, and zero times in the pinned 2.1.10 binary where real keys show up as tag strings - while unrecognised keys are accepted silently, so it survived a dozen edits saying nothing. Removed from both stages with the measurement in the file header, since a config line that means nothing is the same disease this ticket was filed for: two spellings of one intent where only one gets noticed. One follow-up was filed rather than left in this ticket's prose: TASK-064, split into an agent half that writes the cache keys for both workspaces and a human half that measures cold against warm on a runner, because CI still has no cache at all and seventeen gates against no cache is why nobody may fan them out yet. `elf-check`'s mtime and cfg blindness stay where they were, with TASK-056 and TASK-062.
<!-- SECTION:FINAL_SUMMARY:END -->
