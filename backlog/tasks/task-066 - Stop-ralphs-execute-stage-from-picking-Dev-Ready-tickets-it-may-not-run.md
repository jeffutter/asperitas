---
id: TASK-066
title: Stop ralph's execute stage from picking Dev Ready tickets it may not run
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-13 08:09'
labels: []
dependencies: []
references:
  - >-
    ~/.config/home-manager/modules/home/languages/ai/pi-extensions/ralph/index.ts
  - .claude/workflows/ralph-backlog-loop.js
  - backlog/unblocked-todo.sh
priority: high
type: chore
ordinal: 109800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Ralph's execute stage picked TASK-059 and handed it to `/backlog-execute`. TASK-059 is a container
whose only acceptance criterion is "all three subtasks are Done", two of which were open, and whose
assignee is `@human` because one child is bench work. An agent cannot move it forward at all, and
CLAUDE.md forbids it from closing it. That is the TASK-004 failure mode written up in CLAUDE.md and
in the header of `unblocked-todo.sh`: the executor spins, produces no commit, trips the
"claimed success but no commit landed" guard, and gets re-picked until the failure-streak guard
halts the run. It was avoided this time only because the executor read the ticket and chose to park
it rather than pretend.

The guards already exist; the execute stage just doesn't use them.

- `~/.pi/agent/extensions/ralph/unblocked-todo.sh` implements exactly the three checks needed: all
  dependencies Done, no unfinished child ("container tickets are held back in every mode"), and the
  assignee split, with `agent` the default precisely because of TASK-004. Verified 2026-09-13:
  `./backlog/unblocked-todo.sh "Dev Ready"` prints nothing while TASK-059 sat in Dev Ready, and
  `--assignee all` prints nothing either. The tool got it right; the caller ignored it.
- The pi extension selects the work differently. `index.ts:2030-2033` does
  `findFirstByStatus(pi, cwd, "In Progress") ?? findFirstByStatus(pi, cwd, "Dev Ready")`, and
  `findFirstByStatus` (around `index.ts:580`) is a bare `backlog task list -s <status> --plain`
  grepping for the first ID. No dependency check, no container check, no assignee filter.
- The Claude workflow for the same loop already solved this.
  `.claude/workflows/ralph-backlog-loop.js:219-227` runs `./backlog/unblocked-todo.sh "Dev Ready"`
  before handing anything to Execute, and if a ticket is in Dev Ready but not listed there it forces
  it to Blocked and appends a note saying planning marked it ready wrongly. Two harnesses, one
  correct and one not, is the drift pattern `backlog/unblocked-todo.sh`'s header was written about.

Fix the pi extension to route the execute-stage selection through the same rules, so the two
harnesses agree again.

## Where the code lives

This is NOT a change inside this repo. The editable source is
`~/.config/home-manager/modules/home/languages/ai/pi-extensions/ralph/index.ts`;
`~/.pi/agent/extensions/ralph/index.ts` is a `/nix/store` symlink generated from it, so editing the
deployed path fails or is silently discarded. Sequence: edit the repo copy, run `~/bin/rebuild`,
then verify the deployed copy changed. Committing is therefore in the home-manager repo, not here -
this ticket exists because this project's backlog is where the failure was observed and where the
loop that wastes iterations on it runs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Both execute-stage picks (In Progress and Dev Ready) are filtered by the same rules as unblocked-todo.sh: all dependencies Done, no unfinished child, and assignee not @human. Implement it by calling the deployed script rather than reimplementing the checks in TypeScript - the header of backlog/unblocked-todo.sh documents why two copies of these rules drifted once already.
- [ ] #2 A Dev Ready ticket that fails the check is corrected rather than merely skipped, matching .claude/workflows/ralph-backlog-loop.js:219-227: set status Blocked preserving its existing dependency list, and append one implementation note naming the unmet condition. Selection then continues with the next candidate instead of stalling the step.
- [ ] #3 Demonstrated, not asserted. Reproduce the condition on a real container with an unfinished child in Dev Ready (or restore TASK-059 to Dev Ready briefly against a recorded sha), run the loop far enough to reach the selection, and record in the notes what it did: the ticket it refused, the status it forced, and the note text. Then restore the prior status and dependency list and show the diff is empty.
- [ ] #4 The change is made in the home-manager source, applied with ~/bin/rebuild, and verified present in the deployed /nix/store copy. Notes record the home-manager commit sha and confirm this repo received no code change beyond its own backlog files.
- [ ] #5 Out of scope, stated in the notes rather than done: changing what unblocked-todo.sh itself considers ready, and touching .claude/workflows/ralph-backlog-loop.js, which already has the guard.
<!-- AC:END -->
