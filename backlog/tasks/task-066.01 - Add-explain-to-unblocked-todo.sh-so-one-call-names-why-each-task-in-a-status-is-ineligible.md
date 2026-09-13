---
id: TASK-066.01
title: >-
  Add --explain to unblocked-todo.sh so one call names why each task in a status
  is ineligible
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-13 09:03'
updated_date: '2026-09-13 09:05'
labels:
  - planned
dependencies: []
parent_task_id: TASK-066
priority: high
type: chore
ordinal: 110800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`unblocked-todo.sh` already computes all three eligibility rules (dependencies Done, no unfinished
descendant, assignee not `@human`) but reports only a binary verdict: a task is either printed or
not. Its caller in ralph's pi extension therefore cannot say *why* a Dev Ready ticket was refused,
which TASK-066 needs to write a self-describing correction note. Reconstructing the reason in
TypeScript would be a second copy of these rules - the exact drift this script's header exists to
prevent (its own comment records that the archive-shadowing fix `bc5fe61` landed on the repo copy
only, and the assignee filter landed on the deployed copy only).

Add `--explain`: one invocation prints `id|reason` for every task in the target status, so the
caller gets the verdict and the reason for the whole pool in a single ~5 s call. Default output must
not change by a single byte; what the script considers ready is out of scope (TASK-066 AC #5).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 unblocked-todo.sh <status> --explain prints exactly one "id|reason" line per task whose .status matches <status>, including ineligible tasks and tasks the assignee mode would hide, with reason limited to eligible | dependencies-unresolved | container-children-open | assignee-human and precedence dependencies > container > assignee; exit 0.
- [ ] #2 Default output is byte-identical to the pre-change output for To Do, Dev Ready, Blocked and Needs Plan at all three assignee modes, proven by diffing recorded captures, and the recorded diffs are shown empty in the notes.
- [ ] #3 --explain is parsed before the positional catch-all, so it can never be swallowed into TARGET_STATUS; a still-unknown flag keeps today's behavior (documented, not fixed here).
- [ ] #4 The header comment documents the mode and the reason vocabulary, and states that reasons are computed in this file so verdict and explanation cannot disagree.
- [ ] #5 Landed in the home-manager source, applied with ~/bin/rebuild, and verified byte-identical at ~/.pi/agent/extensions/ralph/unblocked-todo.sh; notes carry the home-manager commit sha and confirm the asperitas repo got no code change outside backlog/.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Where this lands

One file: `~/.config/home-manager/modules/home/languages/ai/pi-extensions/ralph/unblocked-todo.sh`
(153 lines), deployed by `modules/home/languages/ai.nix:495-498` to
`~/.pi/agent/extensions/ralph/unblocked-todo.sh`. The asperitas repo receives no code change; its
`backlog/unblocked-todo.sh` is a 34-line wrapper that `exec`s the deployed path with argv forwarded
verbatim (`cd "$(dirname "${BASH_SOURCE[0]}")/.."` then `exec "$target" "$@"`), so it picks up the
new flag for free.

Anchors verified in the current script (2026-09-13):

- l.32-33 `set -euo pipefail`, `cd backlog` - the script is invoked with cwd at the project root.
- l.35-52 arg parse. The catch-all at l.47-50 makes **any unrecognized argument become the
  status**, which is why `--limit 3` silently yields empty output with exit 0 today. `--explain`
  must be matched explicitly in this case statement, before the catch-all.
- l.55-61 unknown `--assignee` mode exits 1 (deliberate: a typo'd mode must not mean "no filter").
- l.63-83 builds `status_of[$id]` from `archive/tasks/ completed/ tasks/` in that order, so a live
  task beats an archived stub sharing its ID.
- Rule 1 dependencies l.93-102: every `.dependencies` entry must resolve to exactly `Done`; an
  unknown ID becomes the literal `MISSING` and therefore blocks.
- Rule 2 container l.104-124: parenthood is the ID string, any map key matching `$id.*` (trailing
  dot) is a descendant, transitively - `TASK-038.03.02.04` makes `TASK-038` a container.
- Rule 3 assignee l.132-148: skipped entirely when mode is `all`; normalizes by stripping `@`,
  deleting whitespace, lowercasing, comparing to `human`; one `@human` in a list is contagious.
- l.150-153 the only write: `echo "$id - $title"`. Output order is filename glob order, i.e. ID
  order, not priority order. Callers do their own ordering.

## Change

1. Add `EXPLAIN=false` beside `TARGET_STATUS`/`ASSIGNEE_MODE`, and a `--explain) EXPLAIN=true; shift ;;`
   branch in the case at l.38, ahead of the `*)` catch-all.
2. Per candidate, carry `reason=eligible` next to the existing `blocked=false`. Set it at each place
   a rule flips `blocked`, with precedence dependencies > container > assignee, and never overwrite a
   non-`eligible` value. Fixed lowercase vocabulary, no others:
   `eligible`, `dependencies-unresolved`, `container-children-open`, `assignee-human`.
3. At the print point (l.150-153): if `EXPLAIN=true`, print `"$id|$reason"` for **every** task whose
   `.status` equals `TARGET_STATUS` - including ineligible ones and ones the assignee mode would hide
   - one line per task, nothing else on stdout, exit 0. If false, keep the existing behavior
   untouched. Note the status-match guard near l.89 currently `continue`s non-matching tasks; explain
   mode must still respect it (it is a per-status report, not a full-board dump).
4. Extend the header comment (l.1-31) with the new mode and the vocabulary, in the same voice as the
   TASK-004 paragraph, and record why the reasons are computed here rather than re-derived by
   callers: the verdict and its explanation must not be able to disagree.

Keep the three rules themselves byte-for-byte unchanged. Changing what counts as ready is TASK-066
AC #5's explicit out-of-scope line.

## Verification - prove the default output did not move

Capture before and after over the real board, at every status and all three assignee modes, and diff:

```bash
cd /Users/jeffutter/src/asperitas
for st in "To Do" "Dev Ready" "Blocked" "Needs Plan"; do for m in agent human all; do
  ./backlog/unblocked-todo.sh "$st" --assignee "$m" > "/tmp/before-$st-$m.txt" 2>&1 || true
done; done
# ...edit + ~/bin/rebuild...
# same loop writing /tmp/after-... ; then diff each pair. All diffs must be empty.
```

Then exercise the new mode against known ground truth already measured on this board:

- `./backlog/unblocked-todo.sh "To Do" --explain` - `TASK-038.06` must read
  `dependencies-unresolved` (dep `TASK-038.04` is To Do) and `TASK-064` must read
  `container-children-open` (child `TASK-064.02` open) while `TASK-064.01` reads `eligible`.
- `--assignee all` must not change any reason: reasons come from the rules, the mode only hides
  lines in listing mode.
- A crafted fixture in a scratch copy of `backlog/` (never the real board) for the combined case: a
  task that is both `@human` and has an open child must report `container-children-open`, proving
  the precedence.
- Exit codes: unknown status stays exit 0 with no lines; missing `backlog/` directory stays exit 1.
- Requires bash 5 (`declare -A`) plus `yq` (mikefarah) and `jq`; `/bin/bash` 3.2 on macOS fails with
  exit 2, which is pre-existing, not something this change may introduce.

## Deploy and commit

1. Edit the home-manager source. Syntax check by loading it: `bash -n` on the file, then confirm the
   wrapper still runs from a subdirectory: `cd backlog && ./unblocked-todo.sh "To Do"`.
2. `~/bin/rebuild`, then verify the deployed bytes moved: `shasum -a 256` must match between
   `~/.config/home-manager/modules/home/languages/ai/pi-extensions/ralph/unblocked-todo.sh` and
   `~/.pi/agent/extensions/ralph/unblocked-todo.sh`.
3. Commit in the **home-manager** repo (`git -C ~/.config/home-manager`), subject style follows
   history: `ralph: name the unmet condition in unblocked-todo.sh with --explain`. Record the sha in
   this ticket's notes, plus the confirmation that the asperitas repo received no code change beyond
   `backlog/`.

## Out of scope

Any change to what counts as ready; fixing the unknown-flag-becomes-status trap; touching
`.claude/workflows/ralph-backlog-loop.js`; wiring the flag into the pi extension (that is the
dependent ticket).
<!-- SECTION:PLAN:END -->
