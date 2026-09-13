---
id: TASK-061.02
title: >-
  Point ci.yml and both lefthook stages at scripts/gates.sh, and delete the
  lists they were restating
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-13 03:10'
updated_date: '2026-09-13 03:11'
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
- [ ] #1 The workflows single step runs "nix develop .#default --command bash scripts/gates.sh ci", its comment says where checks are added now, and .github/ci-steps.sh is deleted in the same commit with its header rationale (an inline single-quoted bash -c string cannot carry comments; cost figures are local warm) moved into the script rather than lost.
- [ ] #2 lefthook.yml holds exactly one command per stage, each invoking the script with its own tier. No root:, glob:, files:, local: or staged/push_files template survives anywhere in the file (prove with a grep count of 0), min_version rises to the 2.1.10 the flake pins, and no parallel:, priority:, only: or stdout: key is added - only: reports success over a failed child exit code.
- [ ] #3 Hook wiring observed rather than inferred: lefthook dump shows one command per stage; lefthook run pre-commit and lefthook run pre-push on a clean tree execute the script (its own headers print, nothing says skipped) and exit 0; an empty git commit shows gate output; a deliberately broken gate aborts a commit non-zero naming the gate that died, then gets reverted and never committed.
- [ ] #4 Cost recorded per tier before and after, warm local inside nix develop (TASK-061 AC #2): baseline taken from the OLD lists first, then the three tiers, with each delta explained - alphabetical-versus-cheapest-first in commit, build-before-cross-clippy artifact reuse in push, membership otherwise unchanged - and the ci tier no worse than the 138 s warm baseline.
- [ ] #5 Every live reference re-pointed: doc-001 at :33, :113-123, :229-257 and :326 with the tier matrix regenerated from --list and marked as generated; firmware/Makefile:270-275; the stale framing sentence at scripts/check-doc-artifact-names.sh:30-32 while keeping the load-bearing make -n warning. Open tickets that name the dead path (TASK-030, TASK-038.03.02, TASK-038.03.02.04, TASK-052, TASK-056, TASK-062, TASK-063) each get a task comment, not a silent edit to someone elses acceptance criteria. Coordinates inside completed tickets stay untouched as history.
- [ ] #6 Bench left as found: make -C firmware elf-check status captured before and after (it is red on a clean tree today for TASK-056/TASK-062 reasons and must stay exactly that red, unfixed here), git status --porcelain empty after every green tier run, and the firmware ELF digest restored.
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
