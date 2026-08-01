---
name: pi-diagnose
description: Diagnose pi chain and subagent failures. Use when a chain stalled, looped, or produced unexpected results.
---

# pi-diagnose

Diagnoses failures in pi chain runs and subagent sessions for this project.

## When to Use

Invoke with `/pi-diagnose` when:
- A chain run stalled, looped, or returned unexpected results
- Subagents appear to not be passing data to each other
- The orchestrator seems to be redoing work that subagents already did
- A chain reported success but nothing changed in the codebase
- You see repeated intercom messages or "session not found" errors

## Scripts

All scripts live in `.pi/skills/pi-diagnose/scripts/`. Run from the project root
with `-d .` or omit (defaults to cwd). Use `--session <partial-ts>` to target
a specific session instead of the most recent.

| Script | What it answers |
|--------|----------------|
| `diagnose.py --sessions N` | Known failure patterns across N sessions |
| `tail.py [run-index] -n 20` | Last N messages for a specific agent |
| `token-profile.py [run-index]` | Per-turn token growth, compaction events |
| `tool-breakdown.py [run-index]` | Tool call counts + token cost by category |
| `biggest-results.py [run-index]` | Largest tool outputs (token spikes) |
| `orch-log.py [--no-text]` | Orchestrator timeline: intercoms, chain results, actions |

**Typical diagnosis flow:**
```bash
S=.pi/skills/pi-diagnose/scripts
python3 $S/diagnose.py                   # 1. Any known patterns?
python3 $S/orch-log.py --no-text         # 2. What did the orchestrator do?
python3 $S/tail.py 3                     # 3. How did the developer end?
python3 $S/token-profile.py 3            # 4. When did context fill up?
python3 $S/tool-breakdown.py 3           # 5. What consumed the budget?
python3 $S/biggest-results.py 3          # 6. Any single spikes?
```

Run indexes: scout=0, researcher=1, architect=2, developer=3 (typical chain order).

## Session File Layout

pi stores sessions under `~/.pi/agent/sessions/<project-slug>/`:

```
~/.pi/agent/sessions/
  --home-user-src-myproject--/
    2026-05-17T14-13-19-882Z_<uuid>/       ← subagent run dirs
      <run-id>/
        run-0/session.jsonl                ← scout agent
        run-1/session.jsonl                ← researcher agent
        run-2/session.jsonl                ← architect agent
    2026-05-17T14-13-19-882Z_<uuid>.jsonl  ← orchestrator session (same name as dir)
```

Each `.jsonl` file is newline-delimited JSON. Each line is one event
(`session`, `message`, `custom_message`, etc.).

### Reading a session manually

```python
import json, pathlib
lines = [json.loads(l) for l in pathlib.Path("session.jsonl").read_text().splitlines() if l]
# Filter to assistant messages:
for l in lines:
    if l.get("type") == "message" and l["message"].get("role") == "assistant":
        for c in l["message"].get("content", []):
            if c.get("type") == "text":
                print(c["text"][:300])
```

Key event types:
| type | what it is |
|------|-----------|
| `message` | LLM turn — check `message.role` (`user`, `assistant`, `toolResult`) |
| `custom_message` `customType=intercom_message` | Intercom notification received by orchestrator |
| `custom_message` `customType=subagent-slash-result` | Final chain result with per-agent exit codes and turn counts |
| `session_info` | Agent name for this session (e.g. `subagent-researcher-abc-2`) |

## Known Failure Patterns

### 1. LOOP — Orchestrator spinning on same action

**Signature:** Orchestrator assistant messages repeat the same prefix 3+ times.
The intercom from a subagent (e.g. "Starting TASK-4, marking In Progress")
triggers the orchestrator to start doing the workflow itself, in parallel with
the actual chain.

**Cause:** `progress: true` on a chain step. The subagent sends a mid-run
intercom; the orchestrator receives it, treats it as a request for help, and
begins executing steps (scout → task.md → research...) on its own. With the
chain also running, two workers compete. The orchestrator loops because it
can't successfully track state.

**Fix:** Set `progress: false` explicitly on researcher, architect, and
developer steps in `.pi/chains/backlog-workflow.chain.md`. Removing
`progress: true` is not enough — the builtin researcher agent has
`defaultProgress: true` in its frontmatter which takes effect when no
step-level override is present. `progress: false` beats that default.

```diff
  ## researcher
  reads: task.md
  output: research.md
+ progress: false
```

Also ensure `.pi/agents/researcher.md` exists as a project override with
`defaultProgress: false` and the intercom bridge pre-emption (see below).

---

### 2. TERMINATED — Researcher killed by inactivity watchdog

**Signature:** Chain result shows `researcher exitCode=1 error=terminated`.
The subagent session for the researcher has many turns (20+) and did produce
output files.

**Cause:** The `pi-subagents` watchdog checks for intercom activity. If no
intercom has been sent for 180 seconds, it marks the agent as needing
attention and eventually kills it. A researcher doing long web searches won't
send intercom during that time — it's busy — but the watchdog sees silence
and terminates it.

**Fix:** Set `progress: false` explicitly on the step AND ensure the project
researcher agent has `defaultProgress: false`. Without both, the watchdog
fires. Also see the "Intercom bridge pre-emption" section below.

### Intercom bridge pre-emption

**Why it matters:** Even with `progress: false`, the builtin researcher's
system prompt says to use `contact_supervisor` for "meaningful progress" and
the intercom bridge appends its own instructions on top. The bridge only
skips appending if the system prompt already contains the marker string
`"Intercom orchestration channel:"`.

**Fix:** Create `.pi/agents/researcher.md` as a project override. The last
line of its system prompt should begin with that marker and give stricter
instructions:

```
Intercom orchestration channel: Do NOT use contact_supervisor for progress
updates or completion handoffs. Only use it with reason "need_decision" when
genuinely blocked. Complete your research and return normally.
```

This pre-empts the bridge's default (permissive) instruction. The tool is
still available for genuine blockers, but won't fire for routine updates.

---

### 3. INTERCOM: Supervisor session not found

**Signature:** Subagent session ends with:
```
[intercom] Message to "subagent-chat-<id>" was not delivered: Session not found
```

**Cause:** The orchestrator session closed (user cancelled, or the orchestrator
concluded the chain had failed) before the subagent finished and tried to
report. The chain stalls — subsequent agents never get spawned.

**Fix:** Remove `progress: true` from mid-chain steps. Without mid-run
intercoms the orchestrator doesn't take early action and stays alive for the
chain's full duration.

---

### 4. PERMISSION: /tmp glob too shallow

**Signature:** Subagent bash tool result contains:
```
User denied external directory access for bash command 'cat > /tmp/pi-subagents-uid-1000/chain-runs/...
```

**Cause:** `~/.pi/agent/extensions/pi-permission-system/config.json` has
`"/tmp/*": "allow"` but `*` only matches one level deep. The chain writes to
`/tmp/pi-subagents-uid-1000/chain-runs/<id>/task.md` which is 4 levels deep —
it falls through to `"*": "ask"` and gets denied.

**Fix:** Already applied — config now uses `/tmp/**`.

If it recurs, verify the config:
```bash
cat ~/.pi/agent/extensions/pi-permission-system/config.json | grep tmp
# Should show: "/tmp/**": "allow"
```

---

### 5. FILE: Orchestrator read chain artifact from wrong path

**Signature:** Orchestrator session has `FILE_NOT_FOUND` or `ENOENT` tool
result immediately after receiving an intercom, then writes a file to the
project root (e.g. `research.md`, `plan.md`).

**Cause:** Chain inter-agent files live in
`/tmp/pi-subagents-uid-1000/chain-runs/<id>/`. When the orchestrator receives
a "I wrote research.md" intercom, it tries to read `research.md` from the
project CWD, finds nothing, and writes its own version there. This file is an
artifact and should be deleted.

**Fix:** Remove `progress: true` from mid-chain steps. Also check for and
delete any stale artifacts in the project root:
```bash
ls *.md | grep -E "research|plan|architecture|task"
```

---

### 6. RESULTS: exitCode=0 with 0 turns (phantom success)

**Signature:** Chain result block shows `exitCode=0` and `turns=0` for every
agent, even agents that clearly ran (their session files exist with content).

**Cause:** This result block is populated before the chain runs or when the
chain is superseded by a retry. `turns=0` means the result is synthetic — it
was never populated with real data. Ignore this result block; look at the
actual subagent session files (run-0, run-1, ...) for ground truth.

**Diagnosis:** Cross-reference with actual session files:
```bash
python3 .pi/skills/pi-diagnose/scripts/diagnose.py --sessions 1
# The "✓/✗ subagent-<name>" lines reflect actual turn counts from session files
```

---

### 7. CLI: backlog doc view --plain not supported

**Signature:** Subagent tool error: `error: unknown option '--plain'`

**Cause:** `backlog doc view <id> --plain` fails — only `backlog task view`
accepts `--plain`. Chain prompts that suggest `backlog doc view doc-1 --plain`
will cause an error.

**Fix:** Drop `--plain` from any `backlog doc view` calls. Use:
```bash
backlog doc view doc-1    # correct — no --plain
backlog task view TASK-4 --plain  # correct — task view supports --plain
```

## What the Script Reports

```
Session: 2026-05-17T14-13-19-882Z
═══════════════════════════════════
  ✓ subagent-scout-11de273d-1 (3 assistant turns)
  ✓ subagent-researcher-11de273d-2 (8 assistant turns)
  ✗ subagent-architect-11de273d-3 (19 assistant turns)

  Chain outcomes:
    ? scout         exit=0  turns=0    ← phantom result (turns=0)
    ✗ researcher    exit=1  turns=22 [terminated]

  Failures detected:
    ⚠  LOOP: orchestrator spinning on same action
    ⚠  TERMINATED: researcher killed by inactivity watchdog

Diagnosis Summary
═════════════════
⚠  LOOP: ...
   Fix: remove 'progress: true' from researcher/architect/developer steps
```

**Row icons:**
- `✓` subagent completed with no detected failures
- `✗` subagent had permission denials or intercom failures
- `?` chain result entry has turns=0 (phantom/superseded)

## Inspecting Raw Sessions Manually

When the script doesn't have a matching pattern, inspect manually:

```bash
# List recent sessions
ls -t ~/.pi/agent/sessions/--home-jeffutter-src-practice-journal--/ | head -6

# Get agent names for each run
SESSION=2026-05-17T14-13-19-882Z_019e3648-c78a-704e-8813-2b4237944696
for run in ~/.pi/agent/sessions/--home-jeffutter-src-practice-journal--/$SESSION/*/*/session.jsonl; do
  python3 -c "
import json, sys
lines = [json.loads(l) for l in open('$run') if l.strip()]
name = next((l.get('name') for l in lines if l.get('type') == 'session_info'), 'unknown')
msgs = [l for l in lines if l.get('type') == 'message' and l['message'].get('role') == 'toolResult']
errors = [c.get('text','') for m in msgs for c in m['message'].get('content',[]) if m['message'].get('isError')]
print(f'{name}: {len(lines)} events, {len(errors)} tool errors')
for e in errors[:3]: print(f'  ERR: {e[:120]}')
"
done

# See what the orchestrator actually did
SESSION_JSONL=~/.pi/agent/sessions/--home-jeffutter-src-practice-journal--/$SESSION.jsonl
python3 -c "
import json
for l in [json.loads(x) for x in open('$SESSION_JSONL') if x.strip()]:
    if l.get('type') == 'message' and l['message'].get('role') == 'assistant':
        for c in l['message'].get('content', []):
            if c.get('type') == 'text' and c['text'].strip():
                print(c['text'][:300])
                print('---')
"
```

## Related

- `.pi/chains/backlog-workflow.chain.md` — the chain being diagnosed
- `~/.pi/agent/extensions/pi-permission-system/config.json` — bash permission rules
- `pi-subagents` npm package — the chain runner
