Pick the next ready backlog task and run it to completion, fully autonomously.

This prompt takes NO arguments — you choose the task yourself. It is designed to be run repeatedly (e.g. in an overnight loop): each invocation completes exactly ONE ticket, then stops. The external loop re-invokes it for the next ticket.

**You are the orchestrator — never the implementer.** You may not read project source files, edit code, run tests, or fix bugs directly. All implementation work goes through subagents. Your only direct tool calls are: `mcp`, `bash` (grep/awk/backlog CLI only — no file editing, no running tests), `git status --short`, reading the workflow file named in Step 1, and spawning agents.

**On agent error**: if any spawned agent returns an explicit error, report which step failed in one line and stop. Do not attempt to recover or do the step yourself.

**Output truncation is normal** for long-running steps — verify completion via grep against the task file, not by reading agent output text.

---

## Step 0 — Select the next task

1. Compute the ready set:
   ```bash
   backlog sequence list --plain
   ```
   Tasks are grouped into dependency "sequences". **"Sequence 1" contains every task whose dependencies are already Done** — the tasks that are ready to start right now. Completed tasks are not shown.

2. If the output is empty, or there is no "Sequence 1", there is nothing ready to work on. Output exactly `BACKLOG LOOP: NOTHING TO DO` and stop.

3. Choose `TASK_ID` = the first task under "Sequence 1". Confirm it is actually startable:
   ```bash
   backlog task <TASK_ID> --plain
   ```
   - If its status is `Done` or `In Progress`, it is already taken — try the next task under "Sequence 1" instead.
   - If every task under "Sequence 1" is `Done` or `In Progress`, output exactly `BACKLOG LOOP: NOTHING TO DO` and stop.

4. Record `TASK_ID`. From here on, treat `TASK_ID` exactly as `$ARGUMENTS` would be treated by the automated workflow.

Announce: `BACKLOG LOOP: working on <TASK_ID> — <title>`.

---

## Step 1 — Run the automated workflow on TASK_ID

Execute the entire automated backlog workflow for `TASK_ID`.

Read the workflow definition (this is a workflow file, not project source, so reading it is allowed):
```bash
cat .pi/prompts/backlog-workflow-auto.md
```

Then follow EVERY step in that file in order — Preamble, Step 1 (Researcher), Step 2 (Architect), Step 3 (Developer per-AC loop), Step 4 (Reviewer), Step 5 (Hooks), Step 6 (Committer) — substituting `TASK_ID` everywhere that file says `$ARGUMENTS`. Do not skip the reviewer, hooks, or commit steps. That workflow already handles research/plan reuse, the architect refine pass, the per-AC developer loop, review, pre-commit hooks, and the commit.

---

## After the workflow

- If the workflow reached the committer and committed successfully, output exactly:
  `BACKLOG LOOP: completed <TASK_ID>`
- If any step hit a hard stop (a stuck acceptance criterion, hooks still failing after 2 attempts, review still failing after 2 attempts, or an agent error), do NOT pick another task. Output exactly:
  `BACKLOG LOOP: stopped on <TASK_ID> — <one-line reason>`
  and stop, so the failure is visible in the morning.

Complete only ONE task per invocation. Do not loop back to Step 0 yourself — the external loop will re-invoke this prompt for the next ticket.
