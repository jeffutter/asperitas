export const meta = {
  name: "ralph-backlog-loop",
  description:
    "Priority-ordered autonomous backlog loop: execute > plan > choose",
  phases: [
    {
      title: "Setup",
      detail: "ensure required backlog statuses exist",
      model: "haiku",
    },
    {
      title: "State",
      detail:
        "one CLI-only sweep: In Progress / Dev Ready / Needs Plan tickets, dependency-unblocked To Do, priority-sorted To Do, no-ralph list, @human-assigned list",
      model: "haiku",
    },
    { title: "Execute", detail: "run /backlog-execute on one ticket" },
    {
      title: "Review",
      detail:
        "every 3 executes: review recent work and create follow-up tickets for issues",
    },
    {
      title: "Plan",
      detail:
        "run /backlog-planner; large tickets decomposed into sub-tasks that each re-enter the loop",
    },
    {
      title: "Choose",
      detail:
        "deterministically pick the priority-sorted To Do ticket whose dependencies are all Done, not @human-assigned, and move it to Needs Plan",
      model: "haiku",
    },
  ],
};

const MAX_ITERATIONS = (() => {
  if (typeof args === "number") return args;
  if (
    args &&
    typeof args === "object" &&
    typeof args.maxIterations === "number"
  )
    return args.maxIterations;
  return 25;
})();

const REVIEW_EVERY =
  args && typeof args === "object" && typeof args.reviewEvery === "number"
    ? args.reviewEvery
    : 4;

const SETUP_SCHEMA = {
  type: "object",
  properties: {
    statuses: { type: "array", items: { type: "string" } },
    changed: { type: "boolean" },
  },
  required: ["statuses", "changed"],
};

// One flat data-gathering call per iteration covers everything the loop needs
// to pick a branch AND (when it falls through to Choose) pick a target —
// instead of a separate State call every iteration plus a second Discover
// call only on iterations that reach Choose. All of this is pure CLI-output
// extraction, no judgment: the script does the actual branch/target decision
// with plain array/set logic, since asking an LLM to re-derive "are this
// ticket's dependencies all Done?" across many candidates is exactly how
// TASK-96.2 — which had two unresolved ones — got chosen as if it had none.
const STATE_SCHEMA = {
  type: "object",
  properties: {
    inProgress: {
      type: "array",
      items: { type: "string" },
      description: "Ticket IDs with status In Progress, in list order",
    },
    devReady: {
      type: "array",
      items: { type: "string" },
      description: "Ticket IDs with status Dev Ready, in list order",
    },
    needsPlan: {
      type: "array",
      items: { type: "string" },
      description: "Ticket IDs with status Needs Plan, in list order",
    },
    unblockedTodo: {
      type: "array",
      items: { type: "string" },
      description:
        'Every ticket ID from `./backlog/unblocked-todo.sh "To Do"`. Output lines are "ID - Title"; take the ID. These are To Do tickets whose dependencies are all Done.',
    },
    todoByPriority: {
      type: "array",
      items: { type: "string" },
      description:
        'Every ticket ID from `backlog task list -s "To Do" --sort priority --plain`, in the order shown (already sorted high -> medium -> low -> unset).',
    },
    noRalph: {
      type: "array",
      items: { type: "string" },
      description:
        'Every ticket ID from `backlog task list -s "To Do" -l no-ralph --plain` (may be empty).',
    },
    humanOnly: {
      type: "array",
      items: { type: "string" },
      description:
        'Every ticket ID from `backlog task list -a "@human" --plain`, across ALL statuses (the command groups by status; collect IDs from every group). These require a physical device, ears, instruments, or an owner decision, and the loop must never work them.',
    },
  },
  required: [
    "inProgress",
    "devReady",
    "needsPlan",
    "unblockedTodo",
    "todoByPriority",
    "noRalph",
    "humanOnly",
  ],
};

const MOVE_SCHEMA = {
  type: "object",
  properties: {
    ticketId: { type: "string" },
    success: { type: "boolean" },
  },
  required: ["ticketId", "success"],
};

const VERIFY_SCHEMA = {
  type: "object",
  properties: {
    ticketId: { type: "string" },
    status: {
      type: "string",
      description: "The ticket's current status field, exactly as shown",
    },
    isUnblocked: {
      type: "boolean",
      description:
        'True if the ticket ID appears in the output of `./backlog/unblocked-todo.sh "Dev Ready"`, meaning all its dependencies are Done. False if absent (it still has an unresolved dependency).',
    },
    corrected: {
      type: "boolean",
      description:
        "True if this call found status=Dev Ready and isUnblocked=false and fixed it (edited to Blocked + note), false otherwise (including when no fix was needed)",
    },
  },
  required: ["ticketId", "status", "isUnblocked", "corrected"],
};

const ACTION_SCHEMA = {
  type: "object",
  properties: {
    ticketId: { type: "string" },
    outcome: {
      type: "string",
      description: "One of: completed, blocked-reverted, planned, error",
    },
    summary: {
      type: "string",
      description: "One sentence describing what happened",
    },
  },
  required: ["ticketId", "outcome", "summary"],
};

const REVIEW_SCHEMA = {
  type: "object",
  properties: {
    ticketsReviewed: { type: "array", items: { type: "string" } },
    issuesFound: { type: "number" },
    ticketsCreated: { type: "array", items: { type: "string" } },
    summary: { type: "string" },
  },
  required: ["ticketsReviewed", "issuesFound", "ticketsCreated", "summary"],
};

const SETUP_PROMPT = `You're working in the asperitas repo at its root.

Read backlog/config.yml and look at the \`statuses\` list. This project's backlog work loop needs a 5-stage pipeline:
  To Do -> Needs Plan -> Dev Ready -> In Progress -> Done

"Blocked" is also a valid status (a dead-end for tickets that cannot proceed) — leave it alone if present.

If "Dev Ready" is not already in the statuses list, add it (insert it between "Needs Plan" and "In Progress") and save the file. Do not remove, rename, or reorder any other existing statuses.

Report via structured output:
- statuses: the final statuses list (after your edit, if any)
- changed: true if you modified the file, false if it already had everything needed`;

const STATE_PROMPT = `In the asperitas repo (repo root, no git submodules), run these seven commands and report each result as a plain ID list. This is pure data extraction — no filtering, judgment, or dependency reasoning needed, the caller does that:

1. backlog task list -s "In Progress" --plain -> inProgress
2. backlog task list -s "Dev Ready" --plain -> devReady
3. backlog task list -s "Needs Plan" --plain -> needsPlan
4. ./backlog/unblocked-todo.sh "To Do" -> unblockedTodo: every ticket ID from the output, which is one "ID - Title" line per ticket (take the part before the first " - "). If this command errors or is missing, report an EMPTY array and say so — do not substitute another command.
5. backlog task list -s "To Do" --sort priority --plain -> todoByPriority: every ticket ID, in the order shown
6. backlog task list -s "To Do" -l no-ralph --plain -> noRalph: every ticket ID shown (empty array if none)
7. backlog task list -a "@human" --plain -> humanOnly: every ticket ID shown, from EVERY status group in the output — do not pass -s, and do not stop at the first group (empty array if none)

Report all seven lists via the structured output.`;

function moveToNeedsPlanPrompt(ticketId) {
  return `You're working in the asperitas repo at its root.

Run: backlog task edit ${ticketId} -s "Needs Plan"

Report via structured output: ticketId "${ticketId}", success (true if the edit succeeded, false otherwise).`;
}

function verifyAndCorrectPrompt(ticketId) {
  return `You're working in the asperitas repo at its root.

Run: backlog task ${ticketId} --plain
Run: ./backlog/unblocked-todo.sh "Dev Ready"

Determine:
- status: this ticket's current status field, exactly as shown
- isUnblocked: true if "${ticketId}" appears in the script's output (one "ID - Title" line per ticket), false if absent — absent means it still has an unresolved, non-Done dependency

If status is exactly "Dev Ready" AND isUnblocked is false, the planner was wrong to mark it ready — correct it now, in this same turn:
  backlog task edit ${ticketId} -s "Blocked"
and append one implementation note: planning marked this Dev Ready, but its dependencies are not all Done, so it was forced to Blocked. It should be re-checked once its dependencies are Done.

Set corrected to true only if you made that fix just now; false otherwise (including when no fix was needed).

Report via structured output: ticketId "${ticketId}", status, isUnblocked, corrected.`;
}

function executePrompt(ticketId) {
  return `You're working in the asperitas repo at its root (no git submodules — commits happen directly here).

FIRST, a hard stop check. Run: backlog task ${ticketId} --plain

If its assignee is "@human", STOP IMMEDIATELY. Do not implement it, do not mark any acceptance criterion, do not change its status. This project targets physical hardware, and @human tickets require a board plugged in, ears, instruments, or a decision that is the owner's to make — an agent can only produce a false Done. Report outcome "error" with a summary saying the ticket is @human-assigned and was skipped.

Note that acceptance criteria prefixed "HUMAN:" can never be satisfied by writing code or watching a build succeed. If a ticket you are working carries one, leave it unchecked and do not mark the ticket Done.

Otherwise, proceed: use the Skill tool to invoke "/backlog-execute ${ticketId}". This skill will claim the ticket, implement the work, mark acceptance criteria, add implementation notes/summary, set the ticket status to Done, and commit the result — all per its own instructions. If the skill determines the ticket is blocked by new/unforeseen work discovered mid-implementation, it will revert the ticket's status to "To Do".

Dependency readiness is already guaranteed before a ticket reaches you: the Plan phase verifies every ticket it marks "Dev Ready" actually has all its dependencies Done before handing it off, so you should not need to re-check dependencies here. If you nonetheless find the ticket genuinely can't proceed, follow the skill's own blocked-handling behavior.

After the skill finishes, report via structured output:
- ticketId: "${ticketId}"
- outcome: "completed" if the ticket was finished and committed, "blocked-reverted" if it was reverted to To Do, or "error" if something went wrong
- summary: one sentence describing what happened`;
}

function planPrompt(ticketId) {
  return `You're working in the asperitas repo at its root (no git submodules).

First, run: backlog task ${ticketId} --plain
If it already carries the "planned" label AND its status is still "Needs Plan", a previous loop iteration already finished planning it but failed to move it out of "Needs Plan" — do NOT re-invoke the full planner skill (it's expensive and the work is already done). Just apply the status rule below directly, based on whether it has sub-tickets/direct work, and report outcome "planned".

Otherwise, use the Skill tool to invoke "/backlog-planner ${ticketId}". This skill researches the ticket, analyzes dependencies, and writes a detailed implementation plan.

IMPORTANT — large ticket decomposition: If the ticket is large or complex, the planner SHOULD break it down into sub-tasks. Each sub-task will independently enter the backlog loop as its own ticket, going through the full choose → plan → execute cycle on its own. Each sub-task starts at "To Do" and will be picked up by the loop naturally in a future iteration.

Once planning is complete, the ticket must NOT be left in "Needs Plan" — the loop re-queries that status every iteration, and a ticket left there gets re-selected and re-planned from scratch on the very next pass. Set its status to exactly one of:
- "Dev Ready" — if the ticket has direct implementation work of its own (with or without sub-tasks alongside it):
    backlog task edit ${ticketId} -s "Dev Ready"
- "Blocked" — if all its work was fully delegated to sub-tasks and it is now a pure tracking/epic ticket with no direct implementation of its own. This matches the project's existing convention for such tracking tickets (e.g. TASK-8). A future pass (or a human) is expected to promote it out of "Blocked" once its children are Done:
    backlog task edit ${ticketId} -s "Blocked"

Report via structured output:
- ticketId: "${ticketId}"
- outcome: "planned" if planning completed and the status was moved out of "Needs Plan" (to Dev Ready or Blocked), or "error" if something went wrong
- summary: one sentence describing what was planned (and any sub-tickets created)`;
}

function reviewPrompt(executedTicketIds) {
  return `You're working in the asperitas repo at its root.

The following tickets were recently implemented and committed: ${executedTicketIds.join(", ")}.

Review the work and create follow-up backlog tickets for any real issues found:

1. Find commits for this work:
     git log --oneline -20
   Identify commits related to the tickets listed above.

2. Use the Skill tool to invoke "/code-review" on the recent changes (medium effort is fine).

3. For each genuine issue (correctness bugs, significant inefficiencies, clear design problems — NOT nitpicks or style):
     backlog task create "Fix: <brief description>" --description "<detail>" --status "To Do" --label "review-fix"

   Only create tickets you'd actually want implemented. Err on the side of fewer, higher-signal tickets.

4. Report via structured output:
   - ticketsReviewed: the ticket IDs that were reviewed (${executedTicketIds.join(", ")})
   - issuesFound: number of issues that warranted new tickets
   - ticketsCreated: list of new ticket IDs created (empty array if none)
   - summary: one sentence summarizing the review outcome`;
}

phase("Setup");
const setup = await agent(SETUP_PROMPT, {
  schema: SETUP_SCHEMA,
  model: "haiku",
  phase: "Setup",
});
if (!setup) {
  return {
    stopReason: "setup-error",
    iterations: 0,
    results: [],
    table: "(setup failed)",
  };
}
log(
  `Setup: statuses = [${setup.statuses.join(", ")}]${setup.changed ? " (updated config.yml)" : ""}`,
);

const results = [];
let stopReason = "cap";
let executeCount = 0;
let executedSinceReview = [];
let lastExecuteTarget = null;
let lastExecuteOutcome = null;

for (let i = 0; i < MAX_ITERATIONS; i++) {
  phase("State");
  const state = await agent(STATE_PROMPT, {
    schema: STATE_SCHEMA,
    model: "haiku",
    phase: "State",
  });
  if (!state) {
    stopReason = "state-error";
    log("State detection failed; stopping.");
    break;
  }

  // @human tickets need a physical board, ears, instruments, or an owner
  // decision. An agent cannot satisfy their acceptance criteria — it can only
  // produce a false Done (compiling is not evidence that audio was heard). So
  // they are filtered out of every branch, not just Choose: a human moving one
  // into Dev Ready by hand must not hand it to Execute. Tickets with NO
  // assignee stay eligible, per the project convention in CLAUDE.md.
  const humanOnly = new Set(state.humanOnly);
  const agentInProgress = state.inProgress.filter((id) => !humanOnly.has(id));
  const agentDevReady = state.devReady.filter((id) => !humanOnly.has(id));
  const agentNeedsPlan = state.needsPlan.filter((id) => !humanOnly.has(id));

  const skippedHuman =
    state.inProgress.length -
    agentInProgress.length +
    (state.devReady.length - agentDevReady.length) +
    (state.needsPlan.length - agentNeedsPlan.length);
  if (skippedHuman > 0) {
    log(
      `Skipping ${skippedHuman} @human-assigned ticket(s) in the active pool; they need a person, not an agent.`,
    );
  }

  if (agentInProgress.length > 0 || agentDevReady.length > 0) {
    const target = agentInProgress[0] || agentDevReady[0];

    // Backstop: if the same ticket was blocked-reverted last iteration and is
    // still the top pick, the agent failed to move it out of the Dev Ready
    // pool. Don't burn the rest of the iteration cap re-trying it.
    if (
      target === lastExecuteTarget &&
      lastExecuteOutcome === "blocked-reverted"
    ) {
      results.push({
        ticketId: target,
        phase: "execute",
        outcome: "stuck",
        summary:
          "Same ticket re-selected immediately after a blocked-reverted outcome; it was not actually moved out of the Dev Ready pool. Stopping to avoid burning the iteration cap on repeats.",
      });
      stopReason = "stuck-ticket";
      break;
    }

    phase("Execute");
    log(`Iteration ${i + 1}: execute -> ${target}`);
    const outcome = await agent(executePrompt(target), {
      schema: ACTION_SCHEMA,
      phase: "Execute",
    });
    if (!outcome) {
      results.push({
        ticketId: target,
        phase: "execute",
        outcome: "error",
        summary: "subagent returned no result",
      });
      stopReason = "execute-error";
      break;
    }
    results.push({
      ticketId: target,
      phase: "execute",
      outcome: outcome.outcome,
      summary: outcome.summary,
    });
    lastExecuteTarget = target;
    lastExecuteOutcome = outcome.outcome;

    executeCount++;
    executedSinceReview.push(target);

    if (executeCount % REVIEW_EVERY === 0) {
      phase("Review");
      log(
        `Review triggered after ${REVIEW_EVERY} executes (${executedSinceReview.join(", ")})`,
      );
      const review = await agent(reviewPrompt(executedSinceReview), {
        schema: REVIEW_SCHEMA,
        phase: "Review",
      });
      executedSinceReview = [];
      if (review) {
        results.push({
          ticketId: "(review)",
          phase: "review",
          outcome:
            review.issuesFound > 0
              ? `${review.issuesFound} issue(s) queued`
              : "clean",
          summary: review.summary,
        });
        if (review.ticketsCreated.length > 0) {
          log(
            `Review created ${review.ticketsCreated.length} follow-up ticket(s): ${review.ticketsCreated.join(", ")}`,
          );
        }
      }
    }

    continue;
  }

  if (agentNeedsPlan.length > 0) {
    const target = agentNeedsPlan[0];
    phase("Plan");
    log(`Iteration ${i + 1}: plan -> ${target}`);
    const outcome = await agent(planPrompt(target), {
      schema: ACTION_SCHEMA,
      phase: "Plan",
    });
    if (!outcome) {
      results.push({
        ticketId: target,
        phase: "plan",
        outcome: "error",
        summary: "subagent returned no result",
      });
      stopReason = "plan-error";
      break;
    }
    results.push({
      ticketId: target,
      phase: "plan",
      outcome: outcome.outcome,
      summary: outcome.summary,
    });

    // Deterministic correctness check, not the planner's judgment call: the
    // planner decides Dev Ready vs Blocked based on "does it have direct work
    // vs is it a pure tracking ticket," which is orthogonal to "are its
    // declared dependencies actually Done." Verify independently and force
    // Blocked if the planner got that wrong (this is exactly how TASK-96.2
    // ended up in Dev Ready with two unresolved dependencies).
    if (outcome.outcome === "planned") {
      const verify = await agent(verifyAndCorrectPrompt(target), {
        schema: VERIFY_SCHEMA,
        model: "haiku",
        phase: "Plan",
      });
      if (verify && verify.corrected) {
        log(
          `Corrected ${target}: planner set "Dev Ready" but it still has an unresolved dependency — forced to "Blocked".`,
        );
        results.push({
          ticketId: target,
          phase: "plan",
          outcome: "corrected-to-blocked",
          summary:
            "Planner marked this Dev Ready but it still has an unresolved dependency; forced to Blocked so Execute never receives it.",
        });
      }
    }
    continue;
  }

  // Choose: the State call above already gathered unblockedTodo/todoByPriority/
  // noRalph, so no second data-gathering round trip is needed here — just
  // the deterministic pick and (if one was found) the mutation to move it.
  phase("Choose");
  const unblocked = new Set(state.unblockedTodo);
  const noRalph = new Set(state.noRalph);
  const target =
    state.todoByPriority.find(
      (id) => unblocked.has(id) && !noRalph.has(id) && !humanOnly.has(id),
    ) ?? null;

  if (!target) {
    stopReason = "drained";
    const blockedOnHuman = state.todoByPriority.filter(
      (id) => unblocked.has(id) && !noRalph.has(id) && humanOnly.has(id),
    );
    log(
      blockedOnHuman.length > 0
        ? `Backlog drained for agent work. ${blockedOnHuman.length} ready ticket(s) are waiting on a human: ${blockedOnHuman.join(", ")}.`
        : "Backlog drained: no To Do ticket has all dependencies Done, is unlabeled no-ralph, and is not @human-assigned.",
    );
    break;
  }

  const moved = await agent(moveToNeedsPlanPrompt(target), {
    schema: MOVE_SCHEMA,
    model: "haiku",
    phase: "Choose",
  });
  if (!moved || !moved.success) {
    stopReason = "choose-error";
    log(`Failed to move ${target} to Needs Plan; stopping.`);
    break;
  }

  log(`Iteration ${i + 1}: choose -> ${target}`);
  results.push({
    ticketId: target,
    phase: "choose",
    outcome: "queued-for-planning",
    summary: `Deterministically picked: highest-priority To Do ticket whose dependencies are all Done, not labeled no-ralph and not @human-assigned, out of ${state.todoByPriority.length} To Do candidate(s).`,
  });
}

const table = [
  "| Ticket | Phase | Outcome | Summary |",
  "|---|---|---|---|",
  ...results.map(
    (r) => `| ${r.ticketId} | ${r.phase} | ${r.outcome} | ${r.summary} |`,
  ),
].join("\n");

return { stopReason, iterations: results.length, results, table };
