#!/usr/bin/env python3
"""Show the orchestrator's full timeline: intercoms, chain results, and actions taken.

Usage:
    python3 orch-log.py                   # latest session
    python3 orch-log.py --session 20-14   # partial timestamp match
    python3 orch-log.py --no-text         # intercoms and chain results only
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from _lib import (
    add_common_args,
    common_setup,
    find_orchestrator_jsonl,
    fmt_ts,
    load_jsonl,
    tool_result_text,
)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    add_common_args(parser)
    parser.add_argument(
        "--no-text",
        action="store_true",
        help="Suppress orchestrator assistant text, show structure only",
    )
    args = parser.parse_args()

    _project, sess = common_setup(args)
    if not sess:
        print("No session found.", file=sys.stderr)
        sys.exit(1)

    orch_jsonl = find_orchestrator_jsonl(sess)
    if not orch_jsonl:
        print(f"No orchestrator JSONL found for {sess.name}", file=sys.stderr)
        sys.exit(1)

    lines = load_jsonl(orch_jsonl)
    print(f"Orchestrator log: {sess.name[:23]}\n")

    orch_turn = 0

    for ev in lines:
        t = ev.get("type", "")
        ts = fmt_ts(ev.get("timestamp", ""))

        if t == "custom_message":
            ct = ev.get("customType", "")

            if ct == "intercom_message":
                content = ev.get("content", "")
                # Extract sender
                sender = "?"
                for line in content.splitlines():
                    if line.startswith("**📨 From "):
                        sender = line.replace("**📨 From ", "").split("**")[0].strip()
                        break
                # Extract signal type
                signal = ""
                for line in content.splitlines():
                    if "needs attention" in line.lower():
                        signal = " [WATCHDOG]"
                    elif "needs a supervisor decision" in line.lower():
                        signal = " [NEED_DECISION]"
                    elif "progress update" in line.lower():
                        signal = " [PROGRESS_UPDATE]"
                    elif "subagent results" in line.lower():
                        signal = " [CHAIN_RESULT_NOTIFY]"
                # First meaningful content line
                body_lines = [
                    ln
                    for ln in content.splitlines()
                    if ln.strip()
                    and not ln.startswith("**")
                    and not ln.startswith("Run:")
                ]
                body = body_lines[0][:120] if body_lines else ""
                print(f"[{ts}] INTERCOM from {sender}{signal}")
                if body:
                    print(f"         {body}")
                print()

            elif ct == "subagent-slash-result":
                details = ev.get("details", {}).get("result", {}).get("details", {})
                results = details.get("results", [])
                if results:
                    content_text = (
                        ev.get("details", {}).get("result", {}).get("content", [{}])
                    )
                    first_text = (
                        content_text[0].get("text", "")[:80] if content_text else ""
                    )
                    print(f"[{ts}] CHAIN RESULT: {first_text!r}")
                    for r in results:
                        turns = r["usage"]["turns"]
                        ec = r["exitCode"]
                        err = f" [{r['error']}]" if r.get("error") else ""
                        icon = (
                            "✓"
                            if ec == 0 and turns > 0
                            else ("?" if turns == 0 else "✗")
                        )
                        print(
                            f"         {icon} {r['agent']:12} exit={ec} turns={turns}{err}"
                        )
                    print()

        elif t == "message":
            msg = ev.get("message", {})
            role = msg.get("role", "")

            if role == "assistant":
                orch_turn += 1
                # Show tool calls
                for c in msg.get("content") or []:
                    if not isinstance(c, dict):
                        continue
                    if c.get("type") in ("tool_use", "toolCall"):
                        name = c.get("name", "?")
                        a = c.get("arguments", c.get("input", {}))
                        if name == "intercom":
                            action = a.get("action", "?")
                            to = a.get("to", "")
                            msg_text = a.get("message", "")[:80]
                            print(
                                f"[{ts}] ORCH turn {orch_turn}: intercom({action}, to={to}) {msg_text!r}"
                            )
                        elif name == "subagent":
                            action = a.get("action", "?")
                            run_id = a.get("id", a.get("runId", ""))
                            print(
                                f"[{ts}] ORCH turn {orch_turn}: subagent(action={action}, id={run_id})"
                            )
                        else:
                            val = (a.get("command") or a.get("path") or str(a))[:100]
                            print(f"[{ts}] ORCH turn {orch_turn}: {name}({val})")

                if not args.no_text:
                    for c in msg.get("content") or []:
                        if isinstance(c, dict) and c.get("type") == "text":
                            text = c.get("text", "").strip()
                            if text:
                                print(f"         → {text[:200]}")
                print()

            elif role == "toolResult":
                tool = msg.get("toolName", "?")
                is_err = msg.get("isError", False)
                text = tool_result_text(msg)
                if text.strip():
                    label = f"{tool} ERR" if is_err else tool
                    print(f"[{ts}]   result [{label}]: {text[:150]}")
                    print()


if __name__ == "__main__":
    main()
