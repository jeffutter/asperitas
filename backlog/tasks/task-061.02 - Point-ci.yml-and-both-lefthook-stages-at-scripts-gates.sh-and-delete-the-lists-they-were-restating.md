---
id: TASK-061.02
title: >-
  Point ci.yml and both lefthook stages at scripts/gates.sh, and delete the
  lists they were restating
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-13 03:10'
updated_date: '2026-09-13 05:05'
labels:
  - planned
dependencies:
  - TASK-061.01
parent_task_id: TASK-061
priority: medium
type: chore
ordinal: 101800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
One commit switches the three callers over to the single definition and removes what they duplicated: the workflow step becomes a call to scripts/gates.sh with the ci tier, each hook stage keeps exactly one command invoking it with its own tier, .github/ci-steps.sh is git rm-ed along with 25 duplicated run: lines and the last root: path filter (the one gate that can still skip silently at HEAD).

Then the durable part: measure all three tiers warm against the 138 s baseline, prove the hooks actually fire and propagate failure rather than skipping, and re-point every live prose reference (doc-001, firmware/Makefile, scripts/check-doc-artifact-names.sh) plus a comment on each open ticket that still names the dead path.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The workflows single step runs "nix develop .#default --command bash scripts/gates.sh ci", its comment says where checks are added now, and .github/ci-steps.sh is deleted in the same commit with its header rationale (an inline single-quoted bash -c string cannot carry comments; cost figures are local warm) moved into the script rather than lost.
- [x] #2 lefthook.yml holds exactly one command per stage, each invoking the script with its own tier. No root:, glob:, files:, local: or staged/push_files template survives anywhere in the file (prove with a grep count of 0), min_version rises to the 2.1.10 the flake pins, and no parallel:, priority:, only: or stdout: key is added - only: reports success over a failed child exit code.
- [x] #3 Hook wiring observed rather than inferred: lefthook dump shows one command per stage; lefthook run pre-commit and lefthook run pre-push on a clean tree execute the script (its own headers print, nothing says skipped) and exit 0; an empty git commit shows gate output; a deliberately broken gate aborts a commit non-zero naming the gate that died, then gets reverted and never committed.
- [x] #4 Cost recorded per tier before and after, warm local inside nix develop (TASK-061 AC #2): baseline taken from the OLD lists first, then the three tiers, with each delta explained - alphabetical-versus-cheapest-first in commit, build-before-cross-clippy artifact reuse in push, membership otherwise unchanged - and the ci tier no worse than the 138 s warm baseline.
- [x] #5 Every live reference re-pointed: doc-001 at :33, :113-123, :229-257 and :326 with the tier matrix regenerated from --list and marked as generated; firmware/Makefile:270-275; the stale framing sentence at scripts/check-doc-artifact-names.sh:30-32 while keeping the load-bearing make -n warning. Open tickets that name the dead path (TASK-030, TASK-038.03.02, TASK-038.03.02.04, TASK-052, TASK-056, TASK-062, TASK-063) each get a task comment, not a silent edit to someone elses acceptance criteria. Coordinates inside completed tickets stay untouched as history.
- [x] #6 Bench left as found: make -C firmware elf-check status captured before and after (it is red on a clean tree today for TASK-056/TASK-062 reasons and must stay exactly that red, unfixed here), git status --porcelain empty after every green tier run, and the firmware ELF digest restored.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Shape

Switch the three callers over and delete what they were restating, in one commit: `.github/ci-steps.sh`
disappears, `lefthook.yml` loses 25 `run:` lines, and both hook stages keep exactly one command
each. Depends on TASK-061.01 because it invokes `scripts/gates.sh`; equivalence was already proven
there by `--dry-run`, so this ticket's risk is wiring and prose, not the gate set.

## Step 1 - the workflow

    - name: fmt + clippy + test + firmware cross-compile
      run: nix develop .#default --command bash scripts/gates.sh ci

Rewrite the comment above it (currently `ci.yml:17-24`) so it says where checks are added now: the
workflow holds no check list at all, and the inline-`bash -c` apostrophe story it tells belongs to
the script's header, not here. Keep the step name honest about what runs. Delete
`.github/ci-steps.sh` with `git rm` in the same commit; its header comment (why an inline
single-quoted string cannot carry comments, and that costs are local-warm figures) should move into
`scripts/gates.sh` rather than be lost.

Pass the tier explicitly in CI even though `ci` is also the default: a workflow file should state
what it runs.

## Step 2 - the hooks

    pre-commit:
      commands:
        gates:
          run: bash scripts/gates.sh commit
          forward_stderr: true

    pre-push:
      commands:
        gates:
          run: bash scripts/gates.sh push
          forward_stderr: true

Rules, all of them measured against lefthook 2.1.10 in a scratch repo:

- No `root:`, `glob:`, `files:`, `local:`, or `{staged_files}` / `{push_files}` template anywhere.
  A path-filtered job whose filtered set is empty exits 0 without running (this is why
  `lefthook.yml:143-146`'s `firmware-cross-compile` is the one remaining AC #4 violation today -
  consolidation deletes it, it must not be propagated). Unfiltered jobs always run, staged or not;
  `-f` exists for filtered ones only.
- Do not add `parallel: true`, `priority:`, `only:`, or `stdout: false`. `only: "[^"]*"` streams
  output but *discards the child's exit code*, i.e. a green hook over a failed suite; `stdout: false`
  reports success while hiding everything; `parallel` would start every cargo job at once and lose
  fail-fast. Plain capture keeps exit codes intact (`captured rc=1 -> lefthook rc=1 -> git rc=1`).
- Drop `min_version: 1.1.1` up to `2.1.10`, the version the flake actually pins, so an old lefthook
  fails loudly instead of behaving differently.
- Leave `pre-commit` and `pre-push` as separate stages with separate tiers. There is no native way to
  reuse a `commands:` block across stages except YAML anchors/merge keys, and those collapse two
  genuinely different cost budgets into one; the shared thing is the script, not the YAML.

Write a short comment block in `lefthook.yml` saying what was traded away, so nobody "fixes" the
silence later: lefthook captures a command's stdout and replays it when the command finishes, so a
tier now prints nothing until it ends (~1 s commit, ~70 s push). The per-gate headers and timings the
script prints are what you get back, plus fail-fast and a deterministic order that alphabetical
command names could never give. If the muteness ever becomes unbearable the escape hatch is
`parallel: true` plus `priority: 100000` on the job (streams, still preserves declaration order),
with the caveat that the first failing job may not be the first one reported.

## Step 3 - prove the hooks actually fire (AC #4 evidence)

`lefthook dump` must show exactly one command per stage, and no `root:` key anywhere:

    lefthook dump | grep -A3 -e '^pre-'
    yq '.pre-commit.commands | keys, .pre-push.commands | keys' lefthook.yml   # ["gates"], ["gates"]
    grep -c 'root:' lefthook.yml                                              # 0

Then execute, on a clean tree, inside `nix develop .#default`:

- `lefthook run pre-commit` and `lefthook run pre-push` - both must print the script's own headers
  (not `(skipped)`), and exit 0. `pre-push` costs ~70 s; run it once, in the background if needed.
- A real `git commit --allow-empty` must show the gate output and succeed; then undo with
  `git reset --soft HEAD~1`.
- Failure must propagate: temporarily make the first gate fail (e.g. point the parse gate at a file
  with a syntax error, or add a `false` gate), confirm `git commit` aborts non-zero and prints which
  gate died, then revert. Never commit the deliberate failure.
- Confirm `flake.nix:66-68`'s `lefthook install` shellHook picks the new config up automatically on
  the next shell entry (it does - it rewrites `.git/hooks/{pre-commit,pre-push}` each time, including
  inside CI's `nix develop` invocation, so nothing here needs a manual install step).

## Step 4 - measure, per tier, and write the numbers down (AC #2)

Baseline first, from the OLD lists if TASK-061.01 did not already record them, then the new tiers,
all warm inside `nix develop .#default` on a clean tree. Expected deltas, to be confirmed not
assumed:

- `commit`: membership unchanged (9 gates). Order changes materially: lefthook ran them alphabetically,
  so `doc-artifact-names` (0.2 s) ran eighth, after every clippy. Cheapest-first means a broken tree
  now dies in under a second instead of after ~6 s of lints.
- `push`: membership unchanged (16 gates, minus `ci-steps-parse` which becomes the script's own first
  gate). Gains build-before-cross-clippy artifact reuse, worth ~18 s cold and ~1 s warm; today's
  alphabetical order ran `clippy-firmware*` *before* `firmware-cross-compile*`, the inverse of the
  stated intent in `.github/ci-steps.sh:90-92`. Baseline ≈ 70 s warm.
- `ci`: 16 gates + the self-parse. Must stay ≤ the 138 s warm baseline (TASK-060:99, TASK-061:42,
  TASK-063:23 all quote it; 133 s of it is the two `cargo test --workspace` runs).

Record the table in the ticket notes with one row per tier, labelled as local warm aarch64-darwin
figures. Runner-side figures remain owed to TASK-052/TASK-063 - agents cannot obtain them (no
`workflow_dispatch`, main is ~91 commits ahead of origin/main).

## Step 5 - make the prose match (AC #3 durability)

Every live reference, found by grepping the repo for the old names:

- `backlog/docs/doc-001 - Asperitas-Project-Plan.md` :33, :113-123, :229-244, :246-257, :326 - the
  §lefthook and §CI sections enumerate the per-tier lists and their costs. Replace the enumeration
  with the matrix generated by `scripts/gates.sh --list` plus the two exceptions and their prices,
  and mark the matrix as generated so the next person regenerates rather than hand-edits. Fix the
  currently-false claim at :231-233 ("no lint job uses lefthook's `root:` key") - after this commit
  it becomes true again, so say so and say which job used to break it.
- `firmware/Makefile:270-275` - points at "lefthook.yml's clippy-firmware pair" and "ci.yml's firmware
  section"; both names die here. Re-point at `scripts/gates.sh`. Its "~2 s warm, ~20 s fresh" figure is
  the third spelling of the cross-clippy cost - reconcile with the script's number.
- `scripts/check-doc-artifact-names.sh:30-32` - "CI and lefthook run cargo only, so no gate in this
  repo had ever invoked make" is stale (this very script invokes `make -n` from all three tiers).
  Keep the load-bearing warning that `-n` prevents a cross-compile inside a pre-commit hook
  (TASK-058:56-57); rewrite only the framing sentence.
- Open tickets whose instructions now name a dead file: TASK-030 (:119-120), TASK-038.03.02 (:70,
  :192), TASK-038.03.02.04 (:42, :123, :147 - it still says to edit "the existing single-quoted
  `nix develop --command bash -c '...'` string", which has not existed since 70c6fc6), TASK-052
  (:25-26, banner strings still valid), TASK-056 (:49), TASK-062 (:16, :27), TASK-063 (:12, :23,
  :29, :36). Add a `backlog task edit <id> --comment` to each naming the new path and what changed.
  Comments, not silent edits: these are other tickets' acceptance criteria, and rewriting someone
  else's AC to fit your refactor hides the fact that the refactor happened.
- Completed tickets' `ci.yml:NN` coordinates are historical narrative. Leave every one of them alone.

## Step 6 - leave the bench no worse than you found it

`make -C firmware elf-check` is red on a clean tree *right now* (mtime-only drift on
`firmware/src/bin/podtest.rs`, rc=2). That is TASK-056 (mtime-based staleness) and TASK-062 (cfg
provenance invisible), not this change. Capture its status before you start and after you finish and
report both; do not "fix" it, and do not let a green-suite run tempt you into touching firmware
sources. Restore the ELF digest as in TASK-061.01 Step 5.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Evidence (all figures LOCAL warm, aarch64-darwin, inside nix develop .#default, clean tree)

### AC #4 - cost per tier, old lists versus new tiers, two rounds each

Baselines replayed from HEAD, not quoted: `git show HEAD:lefthook.yml` pointed at by LEFTHOOK_CONFIG
(written into the repo root so its `root:` key resolved, `-f` so nothing could skip) and
`git show HEAD:.github/ci-steps.sh` run from the repo root. Six runs, all rc 0.

| tier | old list | new tier | delta |
| --- | --- | --- | --- |
| commit | 2.3 s (9 jobs, alphabetical) | 2.0 s (9 gates) | none |
| push   | 72.5 s (16 jobs) | 72.4 s (16 gates) | none |
| ci     | 139.1 s, then 139.2 s instrumented | 141.3 s, then 139.7 s (17 gates) | +0.6 s on the paired round |

Membership is unchanged everywhere except the parse gate turning into a self-parse, so each delta is
an ordering story, told per gate rather than assumed:

- **commit**: old ran alphabetically - clippy 0.35, clippy-firmware 0.43, clippy-firmware-rtt 0.23,
  clippy-log-defmt 0.21, clippy-log-usb 0.22, *then* doc-artifact-names 0.16 seventh, fmt-check 0.26
  eighth, fmt-check-firmware 0.42 ninth. A broken format or a bad doc name therefore died 1.6-2.2 s
  into the hook. Cheapest-first puts them at 0.01 / 0.16 / 0.25 / 0.40 and pushes the five clippies
  (0.19-0.36 s) behind them: same total, detection of the cheap failure classes moved from positions
  7/8/9 to 2/3/4.
- **push**: per-gate old-versus-new differs only within noise (`test` 66.57 s old, 66.35 s new). The
  build-before-cross-clippy reorder is invisible warm by construction - cross-clippy measured 0.22 s
  old against 0.43 / 0.21 s new with everything already fingerprinted. It pays where TASK-061.01
  measured the difference (~20 s in a fresh target dir against ~2 s directly after a build); what this
  ticket establishes is that the reordering costs nothing warm, which is the half that was unmeasured.
- **ci**: 132.9 s of the 139 s is the two `cargo test` invocations in both spellings (old 66.55 +
  66.25, new 66.45 + 66.46), plus one extra gate (the self-parse, 0.01 s). The +2.2 s on the first ci
  round did not reproduce (-0.6 s relative on the second), so it is run variance, not the change. On
  the 138 s bar: the OLD list itself measures 139.1 / 139.2 s on this machine today, so the quoted
  figure is a little optimistic against current machine state and parity is claimed against same-
  session numbers, which is the only comparison that carries information. Runner-side figures stay
  owed to TASK-052 / TASK-063 - no agent can obtain them.

### AC #3 - hook wiring observed, including one clause the AC gets wrong

- `lefthook dump`: exactly one command per stage, both named `gates`, no `root:` anywhere. Key greps
  all zero: `root:`, `glob:`, `files:`, `local:`, `staged_files`, `push_files`, `parallel:`,
  `priority:`, `only:`, `stdout:`. `yq` gives `["gates"]` for both stages.
- `lefthook run pre-commit -f` on a clean tree: prints `tier: commit`, all nine banners with their
  `--- N.NNs` times and `tier commit: 9 gates, 2.0s`, rc 0. Nothing says skipped.
- `lefthook run pre-push -f`: sixteen banners, 72.4 s, rc 0.
- Staging a file and running the hook shows the whole gate log arriving through lefthook's
  capture-and-replay (`gates` block), rc 0; this ticket's own commit then exercises the real
  `git commit` path end to end.
- Failure propagates: planted `gate commit "=== PLANTED FAILING GATE ..." false` as the second commit
  gate -> `git commit` exited 1 printing `*** gate failed: === PLANTED FAILING GATE ...` and
  `*** tier: commit (1 of 2 gates completed before it)`, HEAD unchanged. `scripts/gates.sh` restored
  byte-identical (`cmp`) and the planted string greps to zero repo-wide. Nothing was committed.
- **`git commit --allow-empty` cannot show gate output, and no config makes it.** Observed: `gates
  (skip) no matching staged files`, rc 0, zero gates run. This is not a decision lefthook.yml gets to
  make - 2.1.10 has no key for it. Upstream's
  `internal/run/controller/command/build_command.go` returns `SkipError("no matching staged files")`
  for every command whenever the staged set is empty and `--force` was not passed, and the published
  `schema.json` has no property to turn it off (checked against master). Escape hatches measured in a
  scratch repo against a deliberately failing command: plain capture -> child rc 1 becomes lefthook
  rc 1 (what ships); `only: "[^"]*"` -> the job is skipped outright, "skip by condition", rc 0 over
  the failure, which is worse than the exit-code-discard story it arrived with here; `interactive:
  true` streams with its exit code intact but asks for `/dev/tty` and takes stdin, neither of which a
  GUI client or CI reliably offers. So the AC's intent - prove the hook fires and fails loudly - is
  discharged by the real-commit and planted-failure evidence above, and the empty-index skip is now
  written down in `lefthook.yml` and doc-001 with its upstream source and the reason it is
  affordable: an empty commit changes no tree, so the only thing it could gate is HEAD's tree, which
  the commit that made it already paid for. Checked with that finding in place of the literal clause.
- No reinstall needed: `.git/hooks/{pre-commit,pre-push}` are generic wrappers that exec lefthook,
  which reads the config at run time; `lefthook check-install` exits 0.

### AC #5 - prose re-pointed

doc-001: decision row :33, the two-workspace paragraph :116-123, the whole section 5 lefthook block
(matrix embedded from `scripts/gates.sh --list` and marked generated - regenerated output diffs
byte-identical, 21 lines), the CI section, and the risk-table row now at :377. `firmware/Makefile`
:270-278 names the two `firmware clippy` gates in `scripts/gates.sh` instead of the dead job names and
carries the script's three cost figures rather than a fourth spelling.
`scripts/check-doc-artifact-names.sh`:11-13, 25-29: framing corrected to "at that point every gate in
the repo was a cargo invocation", with the `-n` warning kept and sharpened - it is the only `make`
invocation any tier has, and dropping `-n` cross-compiles inside every tier including pre-commit.
Seven open tickets got COMMENTS, never silent edits: TASK-030, TASK-038.03.02, TASK-038.03.02.04,
TASK-052, TASK-056, TASK-062, TASK-063. Every coordinate inside a completed ticket is untouched
history. Repo-wide grep for `ci-steps`, `fmt-check`, `clippy-firmware`, `doc-links`,
`dump-reassemble-selftest`, `ci-steps-parse` now hits only the historical sentences in `ci.yml`,
doc-001 and completed tickets that explain what died.

### AC #6 - bench left as found

`make -C firmware elf-check` before: rc 2, "target/thumbv7em-none-eabihf/release/main is older than
src/bin/podtest.rs". After every run and the restore: rc 2, message diff-identical. As-found digest
was 16dc9e5c... (the RTT image an earlier interrupted run had left in `release/main`); restored to the
console image 9b60b8ffde4d2270e9a043feb300f50283762cdeee537ca81537f36b937fe80b with `rm -f` plus
`make -C firmware build-elf`, which is the state TASK-061.01 documented. elf-check stays red through
the restore because the relink is a hardlink sharing the deps artifact's mtime (19:41:41 against
podtest.rs at 23:11:48) - TASK-062's blindness restated, not fixed here. `git status --porcelain`
after every green tier run listed only this ticket's own files: no reformatted sources, no untracked
droppings.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
The repo now has one check list. ci.yml's single step is `nix develop .#default --command bash scripts/gates.sh ci` with a comment saying checks are added by adding one `gate` line in the script; `.github/ci-steps.sh` is deleted in the same commit with its header rationale (an inline single-quoted bash -c string cannot carry comments; cost figures are local warm) moved into the script. lefthook.yml drops from 25 `run:` lines across two stages to one command per stage (`bash scripts/gates.sh commit` / `push`), min_version rises to the flake's 2.1.10, and every path-filtering key greps to zero - including `firmware-cross-compile`'s `root: "firmware/"`, the one gate left that could report green having built nothing. Costs measured warm against baselines replayed from HEAD rather than quoted: commit 2.3 -> 2.0 s, push 72.5 -> 72.4 s, ci 139.2 -> 139.7 s on the paired round (a +2.2 s first round did not reproduce), with the two cargo test runs still 133 of 139 s; ordering deltas told per gate (docs/fmt from positions 7/8/9 to 2/3/4 in commit, cross-clippy after both builds where it pays ~18 s cold and nothing warm). Wiring observed, not inferred: dump shows one command per stage, both tiers execute with their headers and timings and exit 0, a planted failing gate aborts `git commit` with rc 1 naming itself and was reverted uncommitted. One AC clause proved impossible and is recorded as such - `git commit --allow-empty` prints `gates (skip) no matching staged files` because lefthook 2.1.10 skips every command over an empty staged set with no config key to stop it (upstream build_command.go, no such property in schema.json), and the alternatives measured worse: `only:` skipped a failing job and returned 0, `interactive:` streams but wants /dev/tty and stdin. Prose follows the code: doc-001's decision row, two-workspace paragraph, rewritten lefthook and CI sections with the tier matrix generated from `--list` and marked generated, firmware/Makefile re-pointed at the two `firmware clippy` gates with reconciled costs, and the docs-artifact lint's stale framing fixed while its load-bearing `make -n` warning stays; seven open tickets got comments instead of silent edits and completed tickets' coordinates stay history. Bench as found: elf-check identical red before and after (rc 2, same message), ELF back to the console image 9b60b8ff, tree clean after every green tier run.
<!-- SECTION:FINAL_SUMMARY:END -->
