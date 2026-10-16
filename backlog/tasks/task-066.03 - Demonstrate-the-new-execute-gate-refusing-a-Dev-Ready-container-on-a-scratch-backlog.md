---
id: TASK-066.03
title: >-
  Demonstrate the new execute gate refusing a Dev Ready container, on a scratch
  backlog
status: Done
assignee:
  - '@agent'
created_date: '2026-09-13 09:08'
updated_date: '2026-10-16 16:36'
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
- [x] #1 Red state captured before the fix is exercised: for the same scratch board, `backlog task list -s "Dev Ready" --plain` shows the ineligible container while `./backlog/unblocked-todo.sh "Dev Ready"` omits it. Both quoted verbatim in the notes.
- [x] #2 Running ralph against the scratch project produces a gate history entry that refuses the container, forces it to Blocked with its dependency list intact, appends exactly one comment naming the reason token, and selects the eligible sibling in the same pass.
- [x] #3 Idempotency shown on a second pass: the note sentence occurs exactly once in the corrected task file and no further status change happens.
- [x] #4 Nothing outside the scratch directory changed: the real repo's git status shows only backlog/*.md edits, TASK-059's status and assignee are unchanged, and the scratch ralph state directory is removed afterwards.
- [x] #5 TASK-066's notes record the refused id, the status forced, the verbatim note text, and the corrected citation about .claude/workflows/ralph-backlog-loop.js (l.215-232 is a post-plan verify reached from the Plan branch at l.472, not a pre-execute guard).
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 5c5c4e3. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Evidence (2026-10-16). Scratch board /tmp/t066-demo (copy of backlog/ minus tasks, fixture sha 24dba8f). Fixture: TASK-044 container (@agent, Dev Ready, depends on TASK-043, child TASK-044.01 To Do), TASK-045 leaf @human Dev Ready, TASK-046 leaf @agent Dev Ready.

RED (AC1) - `backlog task list -s "Dev Ready" --plain`:
  TASK-044 - Fixture container / TASK-045 - Fixture human leaf / TASK-046 - Fixture agent leaf
`./backlog/unblocked-todo.sh "Dev Ready"`: `TASK-046 - Fixture agent leaf`. `--explain`: `TASK-044|container-children-open`, `TASK-045|assignee-human`, `TASK-046|eligible`.

DEVIATION (weaker evidence, as the plan allowed): `pi -p "/ralph 1"` could not run the loop. It crashed in postStatusMessage (index.ts:2698, via runLoop:2848) with a stale-ctx assertActive error before any gate entry, and no models are configured here (1Password CLI has no account). Fallback used: a scratch copy of the deployed index.ts (chmod u+w copy under /tmp, deleted afterwards; nothing committed) with a env-triggered driver that calls pickExecutableTicket twice against the scratch cwd. This exercises the gate, not the runLoop wiring.

AC2 - gate history.jsonl (raw, pass 1):
{"kind":"gate","ticket":"TASK-044","outcome":"ok","summary":"corrected: TASK-044 -> Blocked (container-children-open)"}
{"kind":"gate","ticket":"TASK-045","outcome":"ok","summary":"held: TASK-045 (assignee-human, status unchanged)"}
and the same pass returned pick TASK-046 (Fixture agent leaf). git diff of TASK-044: status Dev Ready -> Blocked, updated_date bumped, one comment appended, dependencies block (`- TASK-043`) unchanged. Comment text: "ralph: planning marked this Dev Ready, but it fails the eligibility check (reason: container-children-open), so it was forced to Blocked so Execute never receives it. It should be re-checked once that clears." TASK-045 left untouched (held, not mutated).

AC3 - pass 2: pick again TASK-046; only a repeat `held: TASK-045` entry; TASK-044 no longer in Dev Ready so no re-block; `grep -c "reason: container-children-open"` on TASK-044 file = 1. (The sentence-level idempotency inside forceTicketToBlockedWithNote was not separately re-invoked on an already-Blocked file.)

AC4 - real repo `git status --porcelain` showed only backlog/*.md (this ticket); TASK-059 untouched by me (Status: Blocked before and after); scratch dir and both ralph state dirs (-tmp-t066-demo from the driver, -private-tmp-t066-demo from the crashed print-mode run) removed, `ls ~/.pi/agent/ralph/` shows only the pre-existing asperitas/home-manager/meethook dirs.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Built a scratch board, captured red state, ran pi's deployed pickExecutableTicket gate directly (print-mode /ralph crashes), showed container forced to Blocked with one note, human leaf held, agent leaf picked, idempotent on second pass, scratch state removed, evidence copied to TASK-066 with the corrected workflow citation.
<!-- SECTION:FINAL_SUMMARY:END -->
