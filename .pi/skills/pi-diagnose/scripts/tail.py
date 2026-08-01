#!/usr/bin/env python3
"""Show the last N messages for a specific agent run.

Usage:
    python3 tail.py [run-index]          # default: last run in latest session
    python3 tail.py 3                    # run-3 (developer) in latest session
    python3 tail.py 3 --lines 20
    python3 tail.py 3 --session 20-14   # partial session timestamp match

run-index is 0-based (scout=0, researcher=1, architect=2, developer=3, ...).
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from _lib import (
    add_common_args,
    agent_name,
    common_setup,
    find_runs,
    fmt_ts,
    load_jsonl,
    messages,
    tool_result_text,
)


def print_message(ev: dict) -> None:
    msg = ev["message"]
    role = msg.get("role", "")
    ts = fmt_ts(ev.get("timestamp", ""))
    is_err = msg.get("isError", False)

    if role == "assistant":
        for c in msg.get("content") or []:
            if not isinstance(c, dict):
                continue
            t = c.get("type", "")
            if t == "text" and c.get("text", "").strip():
                print(f"[{ts} ASST] {c['text'][:400]}")
            elif t in ("tool_use", "toolCall"):
                name = c.get("name", "?")
                args = c.get("arguments", c.get("input", {}))
                if name == "bash":
                    val = args.get("command", str(args))[:200]
                elif name in ("read", "write", "edit"):
                    val = args.get("path", str(args)[:80])
                else:
                    val = str(args)[:150]
                print(f"[{ts} CALL:{name}] {val}")

    elif role == "toolResult":
        tool = msg.get("toolName", "?")
        text = tool_result_text(msg)
        if text.strip():
            label = f"{tool} ERR" if is_err else tool
            print(f"[{ts} {label}] {text[:300]}")

    elif role == "user":
        for c in msg.get("content") or []:
            if isinstance(c, dict) and c.get("type") == "text":
                t = c["text"].strip()
                if t:
                    print(f"[{ts} USER] {t[:300]}")

    elif role == "thinking":
        pass  # skip

    else:
        print(f"[{ts} {role}]")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    add_common_args(parser)
    parser.add_argument(
        "run_index",
        nargs="?",
        type=int,
        default=None,
        help="0-based run index (default: last run)",
    )
    parser.add_argument(
        "--lines",
        "-n",
        type=int,
        default=15,
        help="Number of messages to show (default: 15)",
    )
    parser.add_argument("--all", "-a", action="store_true", help="Show all messages")
    args = parser.parse_args()

    _project, sess = common_setup(args)
    if not sess:
        print("No session found.", file=sys.stderr)
        sys.exit(1)

    runs = find_runs(sess)
    if not runs:
        print(f"No runs found in {sess}", file=sys.stderr)
        sys.exit(1)

    idx = args.run_index if args.run_index is not None else len(runs) - 1
    if idx >= len(runs):
        print(f"Run index {idx} out of range (0-{len(runs) - 1})", file=sys.stderr)
        sys.exit(1)

    _group_id, _run_name, run_path = runs[idx]
    lines = load_jsonl(run_path)
    name = agent_name(lines)
    msgs = messages(lines)

    # Show all runs for context
    print(f"Session: {sess.name[:23]}")
    for i, (_gid, _rn, rp) in enumerate(runs):
        rlines = load_jsonl(rp)
        rname = agent_name(rlines)
        rmsgs = messages(rlines)
        rasst = sum(1 for m in rmsgs if m["message"].get("role") == "assistant")
        marker = "→" if i == idx else " "
        print(f"  {marker} run-{i} [{rname}] ({rasst} asst turns)")

    total = len(msgs)
    n = total if args.all else min(args.lines, total)
    print(f"\n── {name} — last {n} of {total} messages ──\n")

    for ev in msgs[-n:]:
        print_message(ev)
        print()


if __name__ == "__main__":
    main()
