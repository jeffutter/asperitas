---
id: TASK-066.02
title: >-
  Gate ralph's execute-stage picks on unblocked-todo.sh, correcting
  wrongly-Dev-Ready tickets to Blocked
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-13 09:06'
updated_date: '2026-09-13 09:07'
labels:
  - planned
dependencies:
  - TASK-066.01
parent_task_id: TASK-066
priority: high
type: task
ordinal: 111800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Ralph's execute stage picks work with `findFirstByStatus`, a bare `backlog task list -s <status>
--plain` that takes the first row: no dependency check, no unfinished-child hold-back, no assignee
filter. Every guard against handing an unexecutable ticket to `/backlog-execute` lives in
`unblocked-todo.sh`, and the execute stage never calls it. That is how TASK-059 - an `@human`
container with two open children - reached an executor, the failure mode CLAUDE.md documents for
TASK-004.

Route both active-pool picks (In Progress, then Dev Ready) through the deployed script via the new
`--explain` mode from TASK-066.01, correct wrongly-Dev-Ready tickets to Blocked with one
self-describing comment, and fail closed when the filter itself cannot run.

Lives in `~/.config/home-manager/modules/home/languages/ai/pi-extensions/ralph/index.ts`; the
asperitas repo gets no code change. Depends on TASK-066.01 for the reason vocabulary.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Both execute-stage picks (In Progress and Dev Ready) are gated by the deployed unblocked-todo.sh --explain verdict: all dependencies Done, no unfinished descendant, assignee not @agent-hostile. No eligibility rule is reimplemented in TypeScript.
- [ ] #2 Selection order is unchanged: candidates come from the raw `backlog task list` priority ordering intersected with the eligible set, not from the script's filename ordering.
- [ ] #3 A Dev Ready candidate that fails on dependencies or an unfinished child is forced to Blocked with its dependency list intact, plus exactly one appended comment naming the machine-readable reason; selection then continues with the next candidate in the same pass rather than stalling.
- [ ] #4 An @human-only-ineligible Dev Ready candidate, and any ineligible In Progress candidate, is skipped without mutating its status, and the skip is recorded in history.jsonl.
- [ ] #5 If the filter call fails or times out, nothing is mutated and no candidate is executed: the step records a failed gate entry and feeds the existing failure-streak guard, so two consecutive filter failures stop the run with a named cause instead of declaring the board drained or falling through to Choose.
- [ ] #6 The correction is idempotent: re-running the gate on an already-corrected ticket appends no second note, proven by a grep count of the exact sentence.
- [ ] #7 StepKind gains a gate kind and buildFinalSummary counts it; /ralph-progress still renders.
- [ ] #8 The Needs Plan pick is deliberately left unfiltered, with a comment saying why (planning an @human container is how TASK-059 got its three children), recording the deviation from the Claude harness.
- [ ] #9 Landed in the home-manager source, applied with ~/bin/rebuild, verified present at ~/.pi/agent/extensions/ralph/index.ts, committed in the home-manager repo with the sha in the notes, and confirmed to leave this repo's code untouched outside backlog/.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Where this lands

One file: `~/.config/home-manager/modules/home/languages/ai/pi-extensions/ralph/index.ts` (2505
lines), deployed by `modules/home/languages/ai.nix:481` to
`~/.pi/agent/extensions/ralph/index.ts`. Line numbers below were verified against that file on
2026-09-13 (deployed copy byte-identical, sha256 `63ddf8d3...`).

Anchors:

- l.110-114 `RALPH_STATE_ROOT` + `stateDirFor(cwd)` - state/history keyed by absolute cwd with
  slashes rewritten to dashes. Relevant to TASK-066.03's scratch-project demonstration.
- l.123-135 `UNBLOCKED_TODO_SCRIPT = join(homedir(), ".pi/agent/extensions/ralph/unblocked-todo.sh")`
  - already an absolute path into the deployed copy, which is what AC #1 asks us to call.
- l.213 `MAX_CONSECUTIVE_FAILURES = 2`; l.1781-1796 `stoppedByFailureStreak(cwd, state, key, ok)`.
- l.236-254 `StepKind` union and `RalphHistoryEntry`; l.380-394 `recordHistory`; l.1884-1955
  `buildFinalSummary` (its `distinctTickets(kind, outcome)` helper at ~l.1898-1912); l.2187-2216
  dashboard lines print `entry.kind` raw, so a new kind needs no dashboard change.
- l.463-554 `execCapture` returns `ExecResult { ok, killed, stdout, stderr, failure }`.
- l.556-575 `parsePlainTaskList` / `parseUnblockedList`; l.577-592 `findFirstByStatus` (the bug);
  l.606-622 `listUnblockedByStatus` (calls the script but destructures only `{ stdout }`, so a failed
  or timed-out filter reads as "nothing eligible" - leave its other callers alone, do not reuse it).
- l.717-733 `setTicketStatus` - today the only ticket mutator in the file.
- l.2029-2041 the execute pick inside `runLoop`; l.2043-2051 Needs Plan fallback; l.2053-2086 the
  To Do path, including the drained finish at l.2065-2079.

## The one question this module answers

Add a single entry point owning the whole active-pool decision, so `runLoop` does not have to know
about statuses, ordering, reasons, corrections or fail-closed behavior:

```ts
type GateReason =
  | "eligible"
  | "dependencies-unresolved"
  | "container-children-open"
  | "assignee-human";

type ActivePick =
  | { kind: "ticket"; ticket: Ticket }
  | { kind: "empty" }
  | { kind: "filter-failed"; detail: string };

async function pickExecutableTicket(
  pi: ExtensionAPI, cwd: string, state: RalphState,
): Promise<ActivePick>
```

Its docstring must carry the reasoning, not just the contract: why the invariant was enforced only
at Choose and trusted at Execute, and what TASK-059/TASK-004 cost.

Internals:

1. `gateVerdicts(pi, cwd, status, assignee)` runs the script with
   `[status, "--assignee", assignee, "--explain"]`, timeout `30_000`, and **checks `res.ok` and
   `!res.killed`** - returning `{ ok: false, detail }` built from `res.failure`/`stderr` instead of
   swallowing it the way `listUnblockedByStatus` does. Returns a `Map<id, GateReason>` on success.
2. For each status in `["In Progress", "Dev Ready"]`: read the raw list with the existing
   `backlog task list -s <status> --plain` shape and `parsePlainTaskList`. If it is empty, skip the
   status without calling the script - In Progress usually is empty, which keeps steady-state cost
   at one ~5 s script call per iteration (measured 5-6 s over 134 task files).
3. On `filter-failed`, return immediately. Never mutate, never fall through to Choose.
4. Walk the raw list in its own order and return the first candidate whose reason is `eligible`.
   Raw order is backlog's priority grouping (HIGH before MEDIUM); the script prints filename/ID
   order, so intersecting the two preserves today's choice. Adopting the script's order would
   silently change which ticket runs first - measured divergence: the script places `TASK-063`
   ahead of HIGH tickets that `backlog task list` puts last.
5. Otherwise act on each ineligible candidate in raw order, then return `{ kind: "empty" }`.

## What counts as a correction (the asymmetry, decided here)

- Dev Ready + `dependencies-unresolved` or `container-children-open` → **correct**: planning was
  wrong about readiness. Force Blocked and note it. Self-healing: `promoteUnblockedBlockedTickets`
  (l.654-681) lists Blocked with `--assignee all` and promotes once deps land.
- Dev Ready + `assignee-human` only → **hold, do not mutate**. A person put that ticket there; the
  Claude harness likewise filters `humanOnly` without touching status
  (`.claude/workflows/ralph-backlog-loop.js:338-358`). Blocking it would fight both that intent and
  the promotion step above, for a state that is not a planning error.
- In Progress + anything → **hold, never correct**. Work may be genuinely in flight in another
  session; yanking its status mid-flight is worse than the thing being fixed.
- Anything while the filter itself failed → **no mutation at all**.

Cap corrections at `MAX_CORRECTIONS_PER_PASS = 5` per pass so a bad board cannot be mass-Blocked off
one unreadable read; leftovers stay Dev Ready and are seen next iteration. Record every action as a
history entry (below). Skipping an @human ticket costs nothing extra per iteration because one
`--explain` call already carries verdicts for the whole pool - no per-candidate subprocess, so no
new budget guard is needed beyond the cap.

## Correction mechanics

```ts
async function forceTicketToBlockedWithNote(
  pi: ExtensionAPI, cwd: string, id: string, reason: GateReason,
): Promise<boolean>
```

- Read `backlog task view <id> --plain` once; if it already contains the exact sentence below, skip
  the append (idempotency). Apply `-s "Blocked"` only if the ticket is not already Blocked.
- Status via the existing `setTicketStatus`. `-s` touches nothing else, so "preserving its existing
  dependency list" holds by construction - prove it in TASK-066.03 rather than adding code.
- Append with `backlog task edit <id> --comment "<sentence>"`. Use `--comment`, **not** `--notes`:
  `--notes` *replaces* implementation notes and would clobber the planner's prose, and `task view`
  prints Comments, which is what makes the grep-before-append check work.
- Fixed sentence, one constant, machine-readable token inside it:
  `ralph: planning marked this Dev Ready, but it fails the eligibility check (reason: <token>), so
  it was forced to Blocked so Execute never receives it. It should be re-checked once that clears.`

## History and reporting

- Add `"gate"` to `StepKind` (l.238). One kind for all three actions; `outcome` plus a fixed summary
  prefix distinguishes them, and both stay greppable in `history.jsonl`:
  `corrected: <id> -> Blocked (<reason>)`, `held: <id> (<reason>, status unchanged)`,
  `filter-failed: <detail>`.
- Count gated tickets in `buildFinalSummary` alongside the existing `execute`/`plan`/`promote`
  counts. Confirm `/ralph-progress` renders (it prints `kind` raw, so it needs no edit - verify, do
  not assume).

## Wiring in runLoop (l.2029-2041)

```ts
const pick = await pickExecutableTicket(pi, cwd, state);
if (pick.kind === "filter-failed") {
  await recordHistory(cwd, state, {
    kind: "gate", outcome: "failed", summary: `filter-failed: ${pick.detail}`,
  });
  await persist(cwd, state); renderWidget(ctx, state);
  if (stoppedByFailureStreak(cwd, state, "execute-gate", false)) break;
  continue;
}
if (pick.kind === "ticket") { /* existing doExecute body, unchanged */ }
```

Two consecutive filter failures then stop the run naming the real cause, reusing the machinery that
already exists, instead of choosing fresh work off a board it cannot read. A filter failure must
never reach the drained finish at l.2065-2079 ("no unblocked tickets remain") - that claim is exactly
what a broken filter must not be allowed to make.

Leave the Needs Plan pick at l.2043 alone and say so in a comment: filtering it by assignee would
have stopped the planning run that gave TASK-059 its three children, and containers legitimately get
planned. This is a deliberate deviation from `.claude/workflows/ralph-backlog-loop.js:338-358`, which
filters `humanOnly` out of every branch.

## Checks available here (real ones, not assumed)

- Load smoke test, measured recipe: `cd /Users/jeffutter/src/asperitas && pi --no-extensions -e
  ~/.pi/agent/extensions/ralph/index.ts -p "/ralph-progress"`. A broken file prints `Error: Failed to
  load extension "...": ParseError ...` but **exits 0**, so grep the output for `Failed to load
  extension`; a clean load prints nothing.
- There is no type-check path to add casually: standalone `node --import tsx` import of index.ts
  fails with `Cannot find module '@earendil-works/pi-coding-agent'` from any cwd, and no tsconfig
  exists under `modules/home/languages/ai/pi-extensions`. Do not invent a test harness here - no test
  dir exists for these extensions. Behavioral proof is TASK-066.03.
- `~/bin/rebuild` can take several minutes: ping intercom before running it, per the loop's ping rules.

## Deploy and commit

Edit source → load smoke test → `~/bin/rebuild` → confirm the deployed copy changed (`shasum -a 256`
on source vs `~/.pi/agent/extensions/ralph/index.ts`) → commit in the home-manager repo with subject
`ralph: gate the execute stage on unblocked-todo.sh before handing work to /backlog-execute` → record
the sha in this ticket's notes along with `git -C /Users/jeffutter/src/asperitas status --porcelain`
showing only `backlog/` changes.

## Out of scope

Needs Plan selection, Choose, review, promote; changing `unblocked-todo.sh` semantics (TASK-066.01
owns that file); `.claude/workflows/ralph-backlog-loop.js`.
<!-- SECTION:PLAN:END -->
