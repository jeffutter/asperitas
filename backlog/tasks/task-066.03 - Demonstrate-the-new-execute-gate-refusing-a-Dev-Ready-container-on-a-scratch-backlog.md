---
id: TASK-066.03
title: >-
  Demonstrate the new execute gate refusing a Dev Ready container, on a scratch
  backlog
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-13 09:08'
updated_date: '2026-09-13 09:10'
labels:
  - planned
dependencies:
  - TASK-066.02
parent_task_id: TASK-066
priority: medium
type: task
ordinal: 112800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-066 AC #3 says demonstrated, not asserted: compiling is not evidence. This ticket produces the
artifact - a real run of the gated execute stage refusing a container that sits in Dev Ready with an
open child, forcing it to Blocked, noting why, and continuing to the eligible sibling.

Everything happens in a throwaway project directory holding a copy of this repo's `backlog/`, so the
live board and the running loop are never raced. Verified 2026-09-13: `cd /tmp/x && backlog task view
TASK-059 --plain` and `./backlog/unblocked-todo.sh "To Do"` both work against a copied `backlog/`
alone, and ralph keys its state by absolute cwd (`index.ts:112-114`), so `/tmp/...` gets its own
`~/.pi/agent/ralph/-tmp-...` state directory and cannot touch asperitas'.

Depends on TASK-066.02 being deployed. Writes the evidence into TASK-066's notes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Red state captured before the fix is exercised: for the same scratch board, `backlog task list -s "Dev Ready" --plain` shows the ineligible container while `./backlog/unblocked-todo.sh "Dev Ready"` omits it. Both quoted verbatim in the notes.
- [ ] #2 Running ralph against the scratch project produces a gate history entry that refuses the container, forces it to Blocked with its dependency list intact, appends exactly one comment naming the reason token, and selects the eligible sibling in the same pass.
- [ ] #3 Idempotency shown on a second pass: the note sentence occurs exactly once in the corrected task file and no further status change happens.
- [ ] #4 Nothing outside the scratch directory changed: the real repo's git status shows only backlog/*.md edits, TASK-059's status and assignee are unchanged, and the scratch ralph state directory is removed afterwards.
- [ ] #5 TASK-066's notes record the refused id, the status forced, the verbatim note text, and the corrected citation about .claude/workflows/ralph-backlog-loop.js (l.215-232 is a post-plan verify reached from the Plan branch at l.472, not a pre-execute guard).
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Why a scratch project

`stateDirFor(cwd)` (TASK-066.02 anchor l.112-114) maps the absolute cwd to
`~/.pi/agent/ralph/<path-with-dashes>`, so a demo directory gets its own `state.json` and
`history.jsonl` and cannot perturb the live asperitas run. Measured 2026-09-13 in a throwaway copy:
`cd /tmp/x && backlog task view TASK-059 --plain` resolves entirely from the copied `backlog/`, and
`./backlog/unblocked-todo.sh "To Do"` lists correctly from there. Nothing else from the repo is
needed for the selection path.

Never hand-edit the real board for this: the orchestrating loop is a live process and will race any
status you push by hand.

## Build the fixture

```bash
D=/tmp/t066-demo; rm -rf "$D"; mkdir -p "$D"
cp -R /Users/jeffutter/src/asperitas/backlog "$D/backlog"
rm "$D"/backlog/tasks/*.md            # keep completed/ and archive/: the script reads them for status
cd "$D" && git init -q && git add -A && git commit -qm "t066 gate fixture"
```

Create exactly four tickets in `$D` with `backlog task create` / `task edit -s`:

1. `FIX-01` - container, `@agent`, status Dev Ready, one child `FIX-01.01` left in To Do. Give it a
   real dependency on some Done ticket copied into `completed/`, so the dependency-preservation check
   in the notes has something to compare against.
2. `FIX-02` - leaf, `@human`, Dev Ready, no dependencies.
3. `FIX-03` - leaf, `@agent`, Dev Ready, no dependencies. This one must get picked.
4. Nothing else in Dev Ready.

Record the baseline: `git rev-parse HEAD`, and `shasum -a 256 backlog/tasks/*` into a file.

## Red state, captured before anything runs

Quote both outputs verbatim in the notes - this is the bug in one screen:

```bash
cd /tmp/t066-demo
backlog task list -s "Dev Ready" --plain      # shows FIX-01, FIX-02, FIX-03
./backlog/unblocked-todo.sh "Dev Ready"       # shows only FIX-03
```

Confirm the check is not vacuously passing: `./backlog/unblocked-todo.sh "Dev Ready" --explain` must
name `container-children-open` for FIX-01 and `assignee-human` for FIX-02.

## Run the gate

Launch detached, then poll - do not block a foreground call on a long-running loop:

```bash
cd /tmp/t066-demo
nohup pi -p "/ralph 1" > /tmp/t066-demo.log 2>&1 &
```

Poll `~/.pi/agent/ralph/-tmp-t066-demo/history.jsonl` every ~30 s for a `"kind":"gate"` entry; as soon
as the expected entries exist, stop the run (`/ralph-stop` is interactive-only, so kill the process
group) and clean up any surviving workers: `pgrep -f t066-demo`. Ping intercom before launching and
again when it returns.

If print mode cannot start the loop at all (no state directory appears), record that finding and fall
back to loading a scratch copy of the deployed `index.ts` with `pickExecutableTicket` temporarily
exported, driven by a two-line extension passed via `-e`, invoked against `$D`. Label that evidence as
weaker: it exercises the gate but not the `runLoop` wiring, and the temporary export must not survive
into the committed source.

## Evidence to put in the notes

1. The gate history lines, raw JSON, showing `corrected: FIX-01 -> Blocked (container-children-open)`,
   `held: FIX-02 (assignee-human, status unchanged)`, and the pick of FIX-03.
2. The comment as it actually landed in `backlog/tasks/FIX-01*.md`, quoted.
3. `git diff` in `$D` limited to the fixture files: status line changed, one comment appended, and the
   `dependencies:` block byte-unchanged (this is AC #2's "preserving its existing dependency list").
4. Second pass: rerun the same selection and show `grep -c "<the exact sentence>" <file>` is still 1,
   i.e. no duplicate note and no re-block.
5. Non-interference: `git -C /Users/jeffutter/src/asperitas status --porcelain` listing only
   `backlog/` paths, `backlog task view TASK-059 --plain | head -8` showing the same status and
   assignee as before, and `ls ~/.pi/agent/ralph/` after cleanup.

Then `rm -rf /tmp/t066-demo ~/.pi/agent/ralph/-tmp-t066-demo`.

## Handoff to TASK-066

Copy the refused id, forced status, verbatim note text and the four proofs above into TASK-066's
notes, and correct the citation there: `.claude/workflows/ralph-backlog-loop.js:215-232`
(`verifyAndCorrectPrompt`) is reached from the **Plan** branch at l.472, after planning succeeds - it
is not a pre-execute guard, and the Claude harness never runs the script with `"Dev Ready"` on the way
into Execute. Its real Execute-side protections are the `humanOnly` filter at l.338-358 and the
`blocked-reverted` backstop at l.363-379. Say plainly that pi's gate is therefore stricter than the
Claude loop's, not merely equal to it.
<!-- SECTION:PLAN:END -->
