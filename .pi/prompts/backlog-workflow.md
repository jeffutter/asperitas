Run the full backlog workflow for task $ARGUMENTS.

You are the orchestrator. Execute the steps below in order, calling each one with the Agent tool and `run_in_background: false`. Every call blocks until the agent finishes before you start the next step.

**On agent error**: if any Agent call returns an explicit error (not just short or truncated output), tell the user which step failed with a one-line summary and stop. Do not attempt to recover or do the step yourself.

**You are the orchestrator — never the implementer.** You may not read source files, edit code, run tests, or fix bugs directly. All implementation work (including fixing review issues) must go through a subagent. Your only direct tool calls are: `mcp`, `bash` (grep/awk only — no file editing, no running tests), `git status --short`, and spawning agents.

**Output truncation is normal** for long-running steps like developer and researcher — their output may be cut off. Always verify step completion via grep against the task file, not by reading the agent's output text.

---

## Preamble

Run these steps before spawning any agents:

1. Update task status:
   - mcp backlog_task_edit — id: "$ARGUMENTS", status: "Needs Plan"

2. Read the full task once:
   - mcp backlog_task_view — id: "$ARGUMENTS"

   From the output, extract and record:
   - **TASK_FILE**: the path shown on the "File:" line — use this path for grep checks in the developer loop
   - **AC list**: every acceptance criterion — its number (the integer after `#`) and exact full text (e.g., `#1 Install mt3-infer…`)
   - **HAS_RESEARCH**: true if the notes section contains a `## Research Brief` heading with content beneath it
   - **HAS_PLAN**: true if the plan section is non-empty (any content under a plan heading or between plan markers)

Do not call backlog_task_view again in the orchestrator; subagents read it themselves.

---

## Step 1 — Researcher (skip if HAS_RESEARCH is true)

If HAS_RESEARCH is true (from preamble), proceed directly to Step 2.

Otherwise call the Agent tool with:
- `subagent_type`: `researcher`
- `description`: `Research $ARGUMENTS`
- `prompt`: the text between the `<prompt>` tags below, passed verbatim

<prompt>
Mark the task In Progress:
- backlog_task_edit — id: "$ARGUMENTS", status: "In Progress"

Read the task in full:
- backlog_task_view — id: "$ARGUMENTS"

Research the task. Identify the key unknowns — libraries to evaluate, APIs to understand, implementation patterns to compare. Produce a concise brief covering: recommended approaches with rationale, tradeoffs between options, gotchas, and exact API signatures for any external libraries the developer will call.

SCOPE — you are ONLY researching, not building. Do not implement any acceptance criterion, do not create or edit source/test/config files, do not run builds, tests, installs, or code generators, and do not check off acceptance criteria or set the task to Done. The task's implementation plan and acceptance criteria are context to research, NOT a checklist to execute. Document API signatures from the docs — do not write code that calls them. Your single deliverable is the Research Brief written into the task notes (below); leave the status as In Progress for the architect and developer that follow.

When done, write the brief directly into the task file — do NOT use backlog_task_edit notesAppend (it fails on large payloads). The task file path was shown by backlog_task_view on the "File:" line. First use your `write` tool to save your full brief text to `/tmp/research-brief.md`, then splice it into the task's notes section with this Node snippet (Node ships in the Nix dev shell; Python does not):

```bash
nix develop -c node -e '
const fs = require("fs");
const taskFile = process.argv[1];
const block = "## Research Brief\n\n" + fs.readFileSync("/tmp/research-brief.md", "utf8") + "\n";
const END = "<!-- SECTION:NOTES:END -->";
let content = fs.readFileSync(taskFile, "utf8");
content = content.includes(END)
  ? content.replace(END, block + END)
  : content + "\n## Notes\n\n<!-- SECTION:NOTES:BEGIN -->\n" + block + END + "\n";
fs.writeFileSync(taskFile, content);
' "<path from the File: line>"
```
</prompt>

---

## Step 2 — Architect (always runs)

The architect always runs so the plan is fresh: if the task already has a plan, the architect validates and refines it against the research brief and current codebase; otherwise it drafts one. This is the chance to revise the plan if anything around the task changed since it was first written.

**If HAS_PLAN is true (from preamble)** — call the Agent tool to REFINE the existing plan with:
- `subagent_type`: `architect`
- `description`: `Refine plan $ARGUMENTS`
- `prompt`: the text between the `<refine-prompt>` tags below, passed verbatim

<refine-prompt>
Validate and refine the existing implementation plan for backlog task $ARGUMENTS.

Read the task in full — it already has an implementation plan, and the Research Brief is in the notes section:
- backlog_task_view — id: "$ARGUMENTS"

The research brief already covers what's in the codebase — do not re-read source files the researcher summarised. Trust the brief.

The existing plan was written earlier and may be stale. Bring it up to date — do NOT replace it wholesale:
- Preserve the original author's intent, structure, and explicit step-by-step level of detail. Keep it executable by someone who follows directions literally.
- Confirm every acceptance criterion is covered by a step; add steps for any that are not.
- Correct anything that no longer matches reality: wrong or vague library API calls, file paths, commands, or assumptions the research brief or current codebase contradicts. Replace hand-waved "find the right API" steps with the exact calls from the brief.
- Note any new risks or prerequisites that have emerged since the plan was written.
- If the plan is already correct and complete, make only minimal edits and say so.

Record the refined plan and update status:
- backlog_task_edit — id: "$ARGUMENTS", planSet: "<full refined plan text>"
- backlog_task_edit — id: "$ARGUMENTS", status: "To Do"

Output the full refined plan text, then briefly list what you changed and why (or "no substantive changes" if it was already sound).
</refine-prompt>

**Otherwise (no existing plan)** — call the Agent tool to DRAFT a new plan with:
- `subagent_type`: `architect`
- `description`: `Plan $ARGUMENTS`
- `prompt`: the text between the `<prompt>` tags below, passed verbatim

<prompt>
Plan backlog task $ARGUMENTS.

Read the task in full — the Research Brief is in the notes section:
- backlog_task_view — id: "$ARGUMENTS"

The research brief in the task notes already covers what's in the codebase — do not re-read source files the researcher summarised. Trust the brief.

Draft a concrete implementation plan covering:
- Key files to create or modify (exact paths)
- TDD order (which tests to write first)
- How each acceptance criterion will be met
- Exact library API calls the developer should use (copy from the research brief)
- Any risks or prerequisites

Record the plan and update status:
- backlog_task_edit — id: "$ARGUMENTS", planSet: "<full plan text>"
- backlog_task_edit — id: "$ARGUMENTS", status: "To Do"

Output the full plan text.
</prompt>

**⛔ APPROVAL GATE** — Present the plan to the user and wait for explicit approval before continuing. Do not start Step 3 until the user says to proceed.

---

## Step 3 — Developer (per-AC loop, only after user approval)

Loop through each AC from the list you extracted in the preamble, in order. For each AC #N with text `<ac text>`:

**1. Check if already done:**
```bash
grep -q "^- \[x\] #N " "$TASK_FILE" && echo DONE || echo TODO
```
(Replace `N` with the actual integer.) If DONE, skip to the next AC.

**2. Spawn developer for this AC:**

Call the Agent tool with:
- `subagent_type`: `developer`
- `description`: `Implement $ARGUMENTS AC #N`
- `maxOutput`: `{ lines: 20000 }`
- `prompt`: the text between the `<prompt>` tags below, substituting the actual AC number and text

<prompt>
Implement acceptance criterion #N for backlog task $ARGUMENTS.

The AC you must implement:
  #N <ac text>

Read the task and plan:
- backlog_task_view — id: "$ARGUMENTS"

Mark it in progress:
- backlog_task_edit — id: "$ARGUMENTS", status: "In Progress", assignee: ["developer"]

Work on ONLY this AC. Every command runs in the Nix dev shell — prefix with `nix develop -c` (or rely on direnv). Choose the command set matching the files this AC touches:

- Rust core (`crates/**`, `*.rs`):
  - single test: `nix develop -c cargo test -p gql-core <test_name>`
  - format + lint: `nix develop -c cargo fmt --check` and `nix develop -c cargo clippy --all-targets -- -D warnings`
- Web shell (`web/**`, `*.ts`/`*.tsx`):
  - single test: `nix develop -c pnpm -C web test run <test_file>`
  - format + lint + typecheck: `nix develop -c pnpm -C web prettier --check <file>`, `nix develop -c pnpm -C web lint`, `nix develop -c pnpm -C web tsc --noEmit`

1. Read the specific source files the plan names for this AC — **at most 3 files total**. Do not grep, glob, or read additional files beyond what the plan names.
2. Write a test for this AC only.
3. Run just that test (never the full suite) with the "single test" command above.
4. Implement the minimum code to make it pass.
5. Run the test again to confirm green.
6. Run the format, lint, and (web only) typecheck commands above on the files you changed.
   If lint or typecheck errors persist after one fix attempt, stop and report: "STUCK: lint/typecheck errors — <summary>". Do not iterate in a loop.
7. Check off this AC: backlog_task_edit — id: "$ARGUMENTS", acceptanceCriteriaCheck: [N]

Stop after completing this AC. Do not proceed to other ACs.
</prompt>

**3. Verify completion:**
```bash
grep -q "^- \[x\] #N " "$TASK_FILE" && echo DONE || echo RETRY
```

If RETRY, spawn the developer once more with the continuation prompt below. If it still fails after 2 total attempts, report exactly which AC is stuck and stop.

<prompt>
Retry acceptance criterion #N for backlog task $ARGUMENTS.

The AC you must implement:
  #N <ac text>

Read the current task state:
- backlog_task_view — id: "$ARGUMENTS"

Do not rewrite tests or code that already exist on disk for this AC. Focus on what is missing or broken. Fix it, confirm the AC's test passes, then run the format/lint commands (plus `tsc --noEmit` for web) on the files you changed, then check off the AC:
- backlog_task_edit — id: "$ARGUMENTS", acceptanceCriteriaCheck: [N]
</prompt>

**After the loop**, confirm no unchecked ACs remain:
```bash
grep -c "^- \[ \]" "$TASK_FILE"
```
Should print `0`. If not, report the remaining unchecked ACs and stop.

---

## Step 4 — Reviewer

Call the Agent tool with:
- `subagent_type`: `reviewer`
- `description`: `Review $ARGUMENTS`
- `prompt`: the text between the `<prompt>` tags below, passed verbatim

<prompt>
Review the implementation for task $ARGUMENTS.

Read the current task state:
- backlog_task_view — id: "$ARGUMENTS"

Run the full quality gate (use a 300-second timeout — the test suites can take several minutes):
```bash
nix develop -c cargo test -p gql-core
nix develop -c cargo fmt --check
nix develop -c cargo clippy --all-targets -- -D warnings
nix develop -c pnpm -C web test run
nix develop -c pnpm -C web tsc --noEmit
nix develop -c pnpm -C web lint
```

Review for production quality: errors are returned as values across the WASM boundary (no `panic!`/`unwrap()`/`expect()` outside tests), error handling covers all failure cases, the JS↔Rust boundary returns our own DTOs (never apollo-federation internals), tests cover success/error/edge cases, all acceptance criteria checked, all Definition of Done items checked, code follows project conventions (see AGENTS.md).

If everything passes:
  1. Check off every Definition of Done item (1-based index):
     backlog_task_edit — id: "$ARGUMENTS", definitionOfDoneCheck: [<index>]
  2. Write a final summary and close the task:
     backlog_task_edit — id: "$ARGUMENTS", status: "Done", finalSummary: "<one paragraph>"

If issues remain, append a note and leave the status as In Progress:
  backlog_task_edit — id: "$ARGUMENTS", notesAppend: ["Review issues: <list>"]

Your last output line must be exactly one of:
  REVIEW RESULT: PASS
  REVIEW RESULT: FAIL — <one-line summary>
</prompt>

Check the reviewer's last output line:
- If `REVIEW RESULT: PASS` — proceed to Step 5.
- If `REVIEW RESULT: FAIL` — spawn a developer to fix the reported issues (do NOT fix them yourself).

Call the Agent tool with:
- `subagent_type`: `developer`
- `description`: `Fix review issues $ARGUMENTS`
- `maxOutput`: `{ lines: 20000 }`
- `prompt`: the text between the `<prompt>` tags below, passed verbatim

<prompt>
Fix the review issues for task $ARGUMENTS.

Read the current task state to find the review issues in the notes:
- backlog_task_view — id: "$ARGUMENTS"

Fix each issue. For each fix: edit the file, run the relevant quality tool to confirm it passes, then move on.

When all issues are resolved run the full suite:
```bash
nix develop -c cargo test -p gql-core
nix develop -c cargo fmt --check
nix develop -c cargo clippy --all-targets -- -D warnings
nix develop -c pnpm -C web test run
nix develop -c pnpm -C web tsc --noEmit
nix develop -c pnpm -C web lint
```

All must pass before you finish.
</prompt>

After the fix developer returns, re-run the reviewer (Step 4) up to **2 total reviewer attempts**. If still failing after 2, report the remaining issues to the user and stop.

---

## Step 5 — Hooks

Run pre-commit hooks and fix any failures before the commit step.

Call the Agent tool with:
- `subagent_type`: `developer`
- `description`: `Pre-commit hooks $ARGUMENTS`
- `maxOutput`: `{ lines: 5000 }`
- `prompt`: the text between the `<prompt>` tags below, passed verbatim

<prompt>
Run pre-commit hooks for task $ARGUMENTS and fix any failures.

```bash
lefthook run pre-commit 2>&1; echo "EXIT: $?"
```

Use a 600-second timeout on that command — hooks can take several minutes.

If all hooks pass (EXIT: 0), output "HOOKS: PASS" and stop.

If any hook fails:
1. Read the failure output carefully
2. Fix each reported issue — edit the file, run the specific failing tool to confirm the fix
3. Re-run `lefthook run pre-commit 2>&1; echo "EXIT: $?"` (timeout: 600)
4. Repeat until all hooks pass

Do not stage or commit anything.

Your last output line must be exactly one of:
  HOOKS: PASS
  HOOKS: FAIL — <one-line summary>
</prompt>

Check the agent's last output line:
- If `HOOKS: PASS` — proceed to Step 6.
- If `HOOKS: FAIL` — spawn the agent once more. If still failing after 2 total attempts, report the hook failures to the user and stop.

---

## Step 6 — Committer (only if hooks passed)

Call the Agent tool with:
- `subagent_type`: `committer`
- `description`: `Commit $ARGUMENTS`
- `prompt`: the text between the `<prompt>` tags below, passed verbatim

<prompt>
Commit all changes for task $ARGUMENTS.

Check what changed:
```bash
git status
git diff --stat
```

Stage all modified files explicitly — no `git add .` or `git add -A`:
```bash
git add <each specific file that was created or modified>
git add backlog/tasks/
```

Commit with `--no-verify` (hooks were already validated in the prior step — re-running them inside git commit will cause a hang):
```bash
git commit --no-verify -m "$ARGUMENTS: <short description of what was implemented>"
```
</prompt>
