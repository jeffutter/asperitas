#!/usr/bin/env python3
"""Show the largest tool result outputs for an agent run — find token spikes.

Usage:
    python3 biggest-results.py [run-index]    # default: last run
    python3 biggest-results.py 3 --top 20
    python3 biggest-results.py 3 --category bash:pytest
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from _lib import (
    add_common_args,
    agent_name,
    common_setup,
    estimate_tokens,
    find_runs,
    fmt_ts,
    load_jsonl,
    messages,
    tool_result_text,
)


def categorize(tool: str, _result_text: str, call_cmd: str | None) -> str:
    if tool == "bash":
        if call_cmd:
            if "pytest" in call_cmd:
                return "bash:pytest"
            if re.search(r"python[23]?\s+-c", call_cmd):
                return "bash:python-c"
            if "mypy" in call_cmd:
                return "bash:mypy"
            if "ruff" in call_cmd:
                return "bash:ruff"
        return "bash:other"
    return tool


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    add_common_args(parser)
    parser.add_argument("run_index", nargs="?", type=int, default=None)
    parser.add_argument(
        "--top", "-n", type=int, default=10, help="Show top N results (default: 10)"
    )
    parser.add_argument(
        "--category",
        "-c",
        default=None,
        help="Filter to a specific category (e.g. bash:pytest)",
    )
    args = parser.parse_args()

    _project, sess = common_setup(args)
    if not sess:
        print("No session found.", file=sys.stderr)
        sys.exit(1)

    runs = find_runs(sess)
    if not runs:
        print("No runs found.", file=sys.stderr)
        sys.exit(1)

    idx = args.run_index if args.run_index is not None else len(runs) - 1
    _group_id, _run_name, run_path = runs[idx]
    lines = load_jsonl(run_path)
    name = agent_name(lines)

    # Map call_id -> command for bash calls
    call_id_to_cmd: dict[str, str] = {}
    results = []

    for ev in messages(lines):
        msg = ev["message"]
        role = msg.get("role", "")
        ts = fmt_ts(ev.get("timestamp", ""))

        if role == "assistant":
            for c in msg.get("content") or []:
                if isinstance(c, dict) and c.get("type") in ("tool_use", "toolCall"):
                    cid = c.get("id", "")
                    cmd = c.get("arguments", c.get("input", {})).get("command", "")
                    if cid and cmd:
                        call_id_to_cmd[cid] = cmd

        elif role == "toolResult":
            tool = msg.get("toolName", "?")
            cid = msg.get("toolCallId", "")
            is_err = msg.get("isError", False)
            text = tool_result_text(msg)
            tok = estimate_tokens(text)
            cmd = call_id_to_cmd.get(cid)
            cat = categorize(tool, text, cmd)
            results.append(
                {
                    "ts": ts,
                    "tool": tool,
                    "cat": cat,
                    "tokens": tok,
                    "is_err": is_err,
                    "cmd": (cmd or "")[:120],
                    "preview": text[:200],
                }
            )

    if args.category:
        results = [r for r in results if r["cat"] == args.category]

    results.sort(key=lambda r: r["tokens"], reverse=True)
    top = results[: args.top]

    print(f"Biggest tool results: {name}\n")
    print(f"  {'#':>3}  {'~Tokens':>7}  {'Time':>8}  {'Category':<20}  Preview")
    print(f"  {'─' * 3}  {'─' * 7}  {'─' * 8}  {'─' * 20}  {'─' * 40}")

    for i, r in enumerate(top, 1):
        err_marker = " ⚠" if r["is_err"] else ""
        preview = r["preview"].replace("\n", " ")[:60]
        print(
            f"  {i:>3}  {r['tokens']:>7,}  {r['ts']:>8}  {r['cat']:<20}  {preview}{err_marker}"
        )

    total = sum(r["tokens"] for r in results)
    shown = sum(r["tokens"] for r in top)
    pct = 100 * shown // total if total else 0
    print(f"\n  Showing top {len(top)}: {shown:,} tokens ({pct}% of {total:,} total)")

    if top and top[0]["cmd"]:
        print(f"\nTop result command:\n  {top[0]['cmd']}")


if __name__ == "__main__":
    main()
