#!/usr/bin/env python3
"""Diagnose pi chain/subagent session failures.

Usage:
    python3 diagnose.py [--sessions N] [--project-dir DIR]

Finds the N most recent pi sessions for the current project (or DIR),
parses their JSONL files, and prints a structured failure report.
"""

from __future__ import annotations

import re
from pathlib import Path

from _lib import (
    find_orchestrator_jsonl,
    find_runs,
    load_jsonl,
)
from _lib import (
    find_sessions as _find_sessions,
)
from _lib import (
    session_dir as _session_dir_fn,
)


def _slug(path: Path) -> str:
    return "--" + str(path).replace("/", "-").lstrip("-") + "--"


def _session_dir(project: Path) -> Path:
    return _session_dir_fn(project)


def _load_jsonl(path: Path) -> list[dict]:
    return load_jsonl(path)


def _extract_text(content: object) -> str:
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "\n".join(
            item["text"]
            for item in content
            if isinstance(item, dict) and "text" in item
        )
    return str(content)


# ── Per-session analysis ──────────────────────────────────────────────────────


def analyse_orchestrator(lines: list[dict]) -> dict:
    """Extract key signals from the main orchestrator session JSONL."""
    result = {
        "intercom_received": [],  # (from_name, text_snippet)
        "intercom_failed": [],  # sessions that couldn't be reached
        "chain_results": [],  # list of agent result dicts
        "file_not_found": [],  # files the orchestrator failed to read
        "permission_denials": [],  # bash commands that were denied
        "loop_detected": False,  # orchestrator repeating itself
        "watchdog_interrupt": False,  # orchestrator interrupted after watchdog signal
        "orchestrator_texts": [],  # all assistant text snippets (for loop detection)
    }

    for ev in lines:
        t = ev.get("type", "")

        if t == "custom_message":
            ct = ev.get("customType", "")
            if ct == "intercom_message":
                content = ev.get("content", "")
                # Extract sender name from content
                m = re.search(r"From (subagent-\S+)", content)
                sender = m.group(1) if m else "unknown"
                result["intercom_received"].append((sender, content[:200]))
                if "needs attention" in content and "subagent-control" in content:
                    result["watchdog_fired"] = True

            elif ct == "subagent-slash-result":
                details = ev.get("details", {}).get("result", {}).get("details", {})
                for r in details.get("results", []):
                    result["chain_results"].append(
                        {
                            "agent": r.get("agent"),
                            "exit_code": r.get("exitCode"),
                            "turns": r.get("usage", {}).get("turns", 0),
                            "error": r.get("error", ""),
                            "status": r.get("progress", {}).get("status", ""),
                        }
                    )

        elif t == "message":
            msg = ev.get("message", {})
            role = msg.get("role", "")
            content = msg.get("content", [])

            if role == "assistant":
                for c in content if isinstance(content, list) else []:
                    if not isinstance(c, dict):
                        continue
                    if c.get("type") == "text":
                        text = c.get("text", "").strip()
                        if text:
                            result["orchestrator_texts"].append(text[:300])
                    elif c.get("type") in ("tool_use", "toolCall"):
                        if c.get("name") == "subagent":
                            args = c.get("arguments", c.get("input", {}))
                            if args.get("action") == "interrupt":
                                result["watchdog_interrupt"] = True

            elif role == "toolResult":
                for c in content if isinstance(content, list) else []:
                    text = c.get("text", "") if isinstance(c, dict) else ""
                    if "FILE_NOT_FOUND" in text or "ENOENT" in text:
                        result["file_not_found"].append(text[:200])
                    if "denied external directory" in text.lower():
                        result["permission_denials"].append(text[:300])
                    if "was not delivered: Session not found" in text:
                        m = re.search(r'"([^"]+)" was not delivered', text)
                        target = m.group(1) if m else "unknown"
                        result["intercom_failed"].append(target)

    # Detect loops: look for repeated orchestrator text prefixes
    seen: dict[str, int] = {}
    for txt in result["orchestrator_texts"]:
        key = txt[:80]
        seen[key] = seen.get(key, 0) + 1
    if any(v >= 3 for v in seen.values()):
        result["loop_detected"] = True

    return result


def analyse_subagent_run(run_path: Path) -> dict:
    """Parse a single subagent run session."""
    lines = _load_jsonl(run_path)
    result = {
        "name": "unknown",
        "permission_denials": [],
        "intercom_failed": [],
        "file_not_found": [],
        "tool_errors": [],
        "contact_supervisor_progress": False,
        "final_output": "",
        "turns": len(
            [
                ev
                for ev in lines
                if ev.get("type") == "message"
                and ev.get("message", {}).get("role") == "assistant"
            ]
        ),
    }

    for ev in lines:
        if ev.get("type") == "session_info":
            result["name"] = ev.get("name", "unknown")
            continue
        if ev.get("type") != "message":
            continue
        msg = ev.get("message", {})
        role = msg.get("role", "")

        if role == "assistant":
            for c in msg.get("content") or []:
                if not isinstance(c, dict):
                    continue
                ct = c.get("type", "")
                if ct == "text":
                    text = c.get("text", "").strip()
                    if text:
                        result["final_output"] = text[
                            :400
                        ]  # keep overwriting → last wins
                elif ct in ("tool_use", "toolCall"):
                    if c.get("name") == "contact_supervisor":
                        # pi uses "arguments"; Anthropic API uses "input"
                        inp = c.get("arguments", c.get("input", {}))
                        if inp.get("reason") == "progress_update":
                            result["contact_supervisor_progress"] = True

        elif role == "toolResult":
            tool = msg.get("toolName", "?")
            is_err = msg.get("isError", False)
            for c in msg.get("content") or []:
                text = c.get("text", "") if isinstance(c, dict) else ""
                if "denied external directory" in text.lower():
                    result["permission_denials"].append(text[:300])
                if "was not delivered: Session not found" in text:
                    m = re.search(r'"([^"]+)" was not delivered', text)
                    target = m.group(1) if m else "unknown"
                    result["intercom_failed"].append(target)
                if "FILE_NOT_FOUND" in text or "ENOENT" in text:
                    result["file_not_found"].append(text[:200])
                if is_err and text.strip() and "denied" not in text.lower():
                    result["tool_errors"].append(f"[{tool}] {text[:200]}")

    return result


# ── Session discovery ─────────────────────────────────────────────────────────


def find_sessions(project: Path, n: int) -> list[Path]:
    """Return the N most recent session directories (with subagent runs) for project."""
    return _find_sessions(project, n)


# ── Known failure signatures ──────────────────────────────────────────────────

FAILURE_SIGNATURES = {
    "permission_tmp_shallow": {
        "check": lambda _o, runs: any(
            "pi-subagents" in d or "chain-runs" in d
            for r in runs
            for d in r.get("permission_denials", [])
        ),
        "label": "PERMISSION: /tmp glob too shallow",
        "detail": (
            "Bash writes to /tmp/pi-subagents-uid-*/chain-runs/... denied. "
            "Fix: change /tmp/* to /tmp/** in "
            "~/.pi/agent/extensions/pi-permission-system/config.json"
        ),
    },
    "intercom_session_not_found": {
        "check": lambda o, runs: bool(
            o.get("intercom_failed") or any(r.get("intercom_failed") for r in runs)
        ),
        "label": "INTERCOM: supervisor session not found",
        "detail": (
            "A subagent tried to send a progress update but the orchestrator "
            "session had already closed. The chain stalls — later agents never start. "
            "Fix: remove 'progress: true' from chain steps so intercom isn't sent, "
            "OR ensure the orchestrator stays alive for the full chain duration."
        ),
    },
    "orchestrator_loop": {
        "check": lambda o, _runs: o.get("loop_detected", False),
        "label": "LOOP: orchestrator spinning on same action",
        "detail": (
            "The orchestrator received a mid-chain intercom and started doing the "
            "workflow itself, in parallel with the actual chain. Got stuck. "
            "Fix: remove 'progress: true' from researcher/architect/developer steps "
            "so no intercom fires during chain execution."
        ),
    },
    "orchestrator_file_not_found": {
        "check": lambda o, _runs: bool(o.get("file_not_found")),
        "label": "FILE: orchestrator read chain artifact from wrong path",
        "detail": (
            "After receiving an intercom, the orchestrator tried to read a chain "
            "artifact (e.g. research.md) from the project CWD instead of /tmp/. "
            "It concluded the file was missing and tried to recreate it. "
            "Fix: remove 'progress: true' so the orchestrator doesn't react to mid-chain updates."
        ),
    },
    "researcher_terminated": {
        "check": lambda o, _runs: any(
            r.get("error") == "terminated" and "researcher" in r.get("agent", "")
            for r in o.get("chain_results", [])
        ),
        "label": "TERMINATED: researcher killed by inactivity watchdog",
        "detail": (
            "Researcher was killed after 180s of no intercom activity, even though "
            "it was actively doing web research. "
            "Fix: remove 'progress: true' — without it the watchdog uses different "
            "signals and doesn't false-positive-kill long-running research agents."
        ),
    },
    "zero_turn_results": {
        "check": lambda o, _runs: any(
            r.get("turns", 0) == 0 and r.get("exit_code") == 0
            for r in o.get("chain_results", [])
        ),
        "label": "RESULTS: chain reported exitCode=0 with 0 turns (phantom success)",
        "detail": (
            "Chain result shows exitCode=0 but 0 turns — the result was populated "
            "with defaults, likely because the chain was cancelled mid-run or the "
            "result is from an earlier chain invocation that was superseded."
        ),
    },
    "watchdog_interrupt": {
        "check": lambda o, _runs: o.get("watchdog_interrupt", False),
        "label": "WATCHDOG: orchestrator interrupted chain after inactivity signal",
        "detail": (
            "subagent-control sent a 'needs attention' watchdog signal after 180s "
            "of intercom silence, and the orchestrator responded by interrupting "
            "the chain. The agent was likely still working (TDD runs and web "
            "research take 5-10 min silently). "
            "Fix: AGENTS.md now instructs the orchestrator to nudge instead of "
            "interrupt. Developer step also sends a progress_update after each "
            "TDD cycle to keep the watchdog quiet."
        ),
    },
    "contact_supervisor_progress": {
        "check": lambda _o, runs: any(
            r.get("contact_supervisor_progress") for r in runs
        ),
        "label": "INTERCOM: contact_supervisor fired for progress_update mid-chain",
        "detail": (
            "A subagent called contact_supervisor(reason='progress_update') during "
            "chain execution, triggering the orchestrator to take over. "
            "Two causes: (1) chain step missing 'progress: false' — the builtin "
            "researcher has defaultProgress: true which fires when step has no override; "
            "(2) intercom bridge appends permissive contact_supervisor instructions. "
            "Fix: add 'progress: false' to all mid-chain steps AND create "
            ".pi/agents/researcher.md with defaultProgress: false and the "
            "'Intercom orchestration channel:' marker pre-empting bridge instructions."
        ),
    },
    "plain_flag_error": {
        "check": lambda _o, runs: any(
            "unknown option '--plain'" in e
            for r in runs
            for e in r.get("tool_errors", [])
        ),
        "label": "CLI: backlog doc view --plain not supported",
        "detail": (
            "Subagent ran 'backlog doc view <id> --plain' which fails — "
            "only 'backlog task view' accepts --plain, not doc view. "
            "Fix: drop --plain from any 'backlog doc view' calls in chain prompts."
        ),
    },
}


# ── Formatting ────────────────────────────────────────────────────────────────


def _header(text: str, char: str = "─") -> str:
    return f"\n{text}\n{char * len(text)}"


def report(project: Path, n: int) -> None:
    sessions = find_sessions(project, n)
    if not sessions:
        print(f"No pi sessions found for {project}")
        print(f"Expected session dir: {_session_dir(project)}")
        return

    all_findings: list[tuple[str, str, str]] = []  # (session_ts, label, detail)

    for session_dir in sessions:
        ts = session_dir.name[:23]  # e.g. 2026-05-17T14-13-19-882Z
        print(_header(f"Session: {ts}", "═"))

        orch_jsonl = find_orchestrator_jsonl(session_dir)
        orch = analyse_orchestrator(_load_jsonl(orch_jsonl)) if orch_jsonl else {}

        runs_raw = find_runs(session_dir)
        runs: list[dict] = []
        for _group_id, _run_name, run_path in runs_raw:
            r = analyse_subagent_run(run_path)
            runs.append(r)
            status = (
                "✓" if not (r["permission_denials"] or r["intercom_failed"]) else "✗"
            )
            print(f"  {status} {r['name']} ({r['turns']} assistant turns)")

        if orch.get("chain_results"):
            print("\n  Chain outcomes:")
            for cr in orch["chain_results"]:
                turns = cr["turns"]
                ec = cr["exit_code"]
                err = f" [{cr['error']}]" if cr.get("error") else ""
                status_icon = (
                    "✓" if ec == 0 and turns > 0 else ("?" if turns == 0 else "✗")
                )
                print(
                    f"    {status_icon} {cr['agent']:12s}  exit={ec}  turns={turns}{err}"
                )

        # Detect failures
        session_findings = []
        for sig in FAILURE_SIGNATURES.values():
            if sig["check"](orch, runs):
                session_findings.append((sig["label"], sig["detail"]))
                all_findings.append((ts, sig["label"], sig["detail"]))

        if session_findings:
            print("\n  Failures detected:")
            for label, _detail in session_findings:
                print(f"    ⚠  {label}")
        else:
            print("\n  No known failure patterns detected.")

    # Summary
    print(_header("Diagnosis Summary", "═"))
    if not all_findings:
        print("No known failures across all sessions examined.")
        return

    seen_labels: set[str] = set()
    for _ts, label, detail in all_findings:
        if label in seen_labels:
            continue
        seen_labels.add(label)
        print(f"\n⚠  {label}")
        # Wrap detail at 80 chars
        words = detail.split()
        line = "   "
        for word in words:
            if len(line) + len(word) + 1 > 80:
                print(line)
                line = "   " + word
            else:
                line += " " + word
        if line.strip():
            print(line)


# ── Entry point ───────────────────────────────────────────────────────────────

if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--sessions",
        "-n",
        type=int,
        default=3,
        help="Number of most recent sessions to examine (default: 3)",
    )
    parser.add_argument(
        "--project-dir",
        "-d",
        type=str,
        default=None,
        help="Project directory (default: current working directory)",
    )
    args = parser.parse_args()

    project = Path(args.project_dir).resolve() if args.project_dir else Path.cwd()
    report(project, args.sessions)
