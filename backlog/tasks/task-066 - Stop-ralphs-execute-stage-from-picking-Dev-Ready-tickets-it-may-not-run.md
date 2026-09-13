---
id: TASK-066
title: Stop ralph's execute stage from picking Dev Ready tickets it may not run
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-13 08:09'
updated_date: '2026-09-13 09:14'
labels:
  - planned
dependencies:
  - TASK-066.01
  - TASK-066.02
  - TASK-066.03
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Shape of the fix

Both guards already exist; the execute stage just does not consult them. So this is not new policy,
it is one call site moved to where the policy lives. Three leaves, each independently shippable, in
that order:

1. **TASK-066.01** `unblocked-todo.sh --explain` - one call reports `id|reason` per task in a status,
   using the rules the script already applies, with default output byte-identical. Home-manager repo.
2. **TASK-066.02** the pi extension - `pickExecutableTicket(pi, cwd, state)` answers "what may Execute
   run next, and if nothing, why", owning raw-list ordering, the verdict map, corrections, history
   entries and fail-closed behavior. `runLoop` shrinks to a switch on its result. Home-manager repo.
3. **TASK-066.03** the demonstration on a scratch copy of `backlog/`, whose evidence lands back here.

`.02 --dep .01` (it consumes the reason vocabulary), `.03 --dep .02`; this ticket depends on all
three, so the leaves surface first. Nothing here touches Rust, firmware or this repo's gates: the code
lives in `~/.config/home-manager/modules/home/languages/ai/pi-extensions/ralph/` and reaches the loop
only through `~/bin/rebuild`, because `~/.pi/agent/extensions/ralph/*` are `/nix/store` symlinks.

## Why one entry point rather than a filter helper at three call sites

The active-pool decision has five parts that always travel together - which status wins, in what
order, who is eligible, what to do about the ineligible ones, and what to do when eligibility cannot
be read. Splitting them across `runLoop` would leave the reasoning implicit at each site and put shell
parsing in more than one place. One function, longer is fine, keeps the invariant in one head and one
diff. The reason tokens come from the script rather than being re-derived in TypeScript for the same
reason the script exists at all: a verdict and its explanation must not be able to disagree.

## Decisions taken at planning time

1. **Ordering preserved.** Candidates come from the raw `backlog task list` priority order intersected
   with the eligible set. The script emits filename/ID order, so adopting its order would silently
   change which ticket runs first (measured divergence on today's board).
2. **Only planning errors get corrected.** Dependencies-unresolved or container-children-open in Dev
   Ready means planning was wrong → force Blocked + one comment naming the token, and the existing
   promote step returns it once the condition clears. `@human`-only ineligibility is a person's
   decision, not a bug: skip without mutating, matching how the Claude harness filters `humanOnly`
   without touching status. Ineligible In Progress is likewise held, never yanked mid-flight.
   **This reads narrower than AC #2 as written**, which asks for correction of any failed check; the
   reasoning is in `.02`'s plan and in the code comment. Overrule by editing that ticket if you want
   the blunt version.
3. **Fail closed.** A failing or timed-out filter call mutates nothing, executes nothing, and must
   never reach the "no unblocked tickets remain" finish. It records a failed gate entry and feeds the
   existing `MAX_CONSECUTIVE_FAILURES = 2` guard, so two bad reads stop the run naming the cause. Today
   `listUnblockedByStatus` swallows exactly this and reads as an empty board.
4. **Notes use `--comment`, never `--notes`** (`--notes` replaces implementation notes and would
   clobber planner prose). Idempotency is grep-before-append on `task view` output.
5. **AC #3's "show the diff is empty" is measured as fragile** and is proven instead by: dependency
   block byte-unchanged in the diff, and the note sentence occurring exactly once after a second pass.
   Status round-trips rewrite the NOTES marker block cosmetically, so a byte-empty diff is not
   achievable. Restoring TASK-059 to Dev Ready against a recorded sha is rejected outright: the live
   loop races it.
6. **Needs Plan stays unfiltered**, deliberately diverging from the Claude harness - planning the
   `@human` container TASK-059 is precisely what produced the three children that unblocked it.

## Corrected premise

This ticket's description cites `.claude/workflows/ralph-backlog-loop.js:219-227` as an Execute-stage
guard. It is not: those lines are `verifyAndCorrectPrompt()`, invoked only from the **Plan** branch at
l.472 after a successful plan. The Claude harness never runs the script with `"Dev Ready"` on the way
into Execute; its Execute-side protections are the `humanOnly` filter (l.338-358) and the
`blocked-reverted` backstop (l.363-379). The parity argument survives - pi is missing both - but the
new gate ends up stricter than the sibling, not equal to it. TASK-066.03 records this alongside the
evidence.

## Verification

`.01` proves its own non-regression by diffing captured output before/after across four statuses and
three assignee modes. `.02` gets the load smoke test (`pi --no-extensions -e <path> -p
"/ralph-progress"`, grepping for `Failed to load extension` because it exits 0 even on a parse error);
there is no type-check or unit-test harness for these extensions and none is introduced here. `.03`
supplies the behavioral artifact AC #3 demands. Final state check: home-manager commits recorded in
these notes, deployed copies byte-identical to source, and `git -C /Users/jeffutter/src/asperitas
status --porcelain` showing nothing outside `backlog/`.

## Out of scope (AC #5, restated)

What `unblocked-todo.sh` considers ready (`.01` adds reporting only), Choose/review/promote logic, and
`.claude/workflows/ralph-backlog-loop.js`.
<!-- SECTION:PLAN:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-13 09:12
---
Planned 2026-09-13 into three leaves (.01 script --explain, .02 the extension gate, .03 the demonstration), all @agent and planned; execution order .01 -> .02 -> .03.

Two things planning changed about this ticket as written, both recorded in full in the Implementation Plan:

1. The citation is wrong. .claude/workflows/ralph-backlog-loop.js:219-227 is verifyAndCorrectPrompt(), reached only from the Plan branch at l.472 after a plan succeeds - not a pre-execute guard. The Claude harness never runs unblocked-todo.sh with "Dev Ready" on the way into Execute; what it does have, and pi lacks, is the humanOnly filter over In Progress/Dev Ready (l.338-358) and the blocked-reverted backstop (l.363-379). Parity still holds as a goal; the new gate will be stricter than the sibling, not equal.

2. AC #2 read literally forces Blocked for all three failed checks. The plan corrects only dependencies-unresolved and container-children-open - planning errors that the existing Blocked->To Do promotion step undoes automatically - and skips an @human-only-ineligible Dev Ready ticket without touching its status, because that state was set by a person and blocking it fights both their intent and the promotion step. Say so if you want the blunt version instead.
---

created: 2026-09-13 09:13
---
Deliberately left in To Do rather than Dev Ready, which is what the planning step would normally do. This ticket is a container whose three children are open, so Dev Ready is the exact state whose handling it exists to fix: today's execute stage takes the first row of `backlog task list -s "Dev Ready"` unchecked, would hand this umbrella to /backlog-execute, get no commit, and two consecutive failures halt the whole run (MAX_CONSECUTIVE_FAILURES = 2). To Do also matches how the other planned umbrellas here sit (TASK-018, TASK-019, TASK-064): invisible to Execute, and held back from Choose by unblocked-todo.sh's container rule until the leaves land. Execution order is .01 -> .02 -> .03, all three planned and @agent.
---
<!-- COMMENTS:END -->
