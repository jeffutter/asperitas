#!/usr/bin/env python3
"""Categorize tool calls for an agent run — find what consumed the context budget.

Usage:
    python3 tool-breakdown.py [run-index]   # default: last run in latest session
    python3 tool-breakdown.py 3             # developer run

Shows call counts and estimated token output per category, sorted by token cost.
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from _lib import (
    add_common_args,
    agent_name,
    common_setup,
    estimate_tokens,
    find_runs,
    load_jsonl,
    messages,
    tool_result_text,
)


def categorize_bash(cmd: str) -> str:
    cmd = cmd.strip()
    if "pytest" in cmd:
        return "bash:pytest"
    if re.search(r"python[23]?\s+-c", cmd):
        return "bash:python-c"
    if "mypy" in cmd:
        return "bash:mypy"
    if "ruff" in cmd:
        return "bash:ruff"
    if "backlog" in cmd:
        return "bash:backlog"
    if "uv add" in cmd or "pip install" in cmd:
        return "bash:package-install"
    if re.search(r"\b(ls|find|cat|head|tail|wc)\b", cmd):
        return "bash:filesystem"
    if "git" in cmd:
        return "bash:git"
    return "bash:other"


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
        "--show-examples",
        "-e",
        action="store_true",
        help="Show one example command per category",
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

    # Map from tool_call_id -> category (so we can match with tool result)
    call_id_to_cat: dict[str, str] = {}
    call_examples: dict[str, str] = {}

    # Count calls per category
    call_counts: dict[str, int] = defaultdict(int)
    result_tokens: dict[str, int] = defaultdict(int)
    result_counts: dict[str, int] = defaultdict(int)

    msgs = messages(lines)

    for ev in msgs:
        msg = ev["message"]
        role = msg.get("role", "")

        if role == "assistant":
            for c in msg.get("content") or []:
                if not isinstance(c, dict):
                    continue
                if c.get("type") not in ("tool_use", "toolCall"):
                    continue
                tool_name = c.get("name", "?")
                call_id = c.get("id", "")
                args_val = c.get("arguments", c.get("input", {}))

                if tool_name == "bash":
                    cat = categorize_bash(args_val.get("command", ""))
                    example = args_val.get("command", "")[:100]
                else:
                    cat = tool_name
                    example = str(args_val)[:80]

                call_id_to_cat[call_id] = cat
                call_counts[cat] += 1
                if cat not in call_examples:
                    call_examples[cat] = example

        elif role == "toolResult":
            tool_name = msg.get("toolName", "?")
            call_id = msg.get("toolCallId", "")
            text = tool_result_text(msg)
            tok = estimate_tokens(text)

            # Look up category from the originating call
            cat = call_id_to_cat.get(call_id, tool_name)
            result_tokens[cat] += tok
            result_counts[cat] += 1

    print(f"Tool breakdown: {name}\n")
    print(
        f"  {'Category':<24} {'Calls':>5}  {'Results':>7}  {'~Tokens out':>11}  {'Avg tok/result':>14}"
    )
    print(f"  {'-' * 24}  {'-' * 5}  {'-' * 7}  {'-' * 11}  {'-' * 14}")

    # Sort by token output descending
    all_cats = sorted(
        set(list(call_counts.keys()) + list(result_tokens.keys())),
        key=lambda c: result_tokens.get(c, 0),
        reverse=True,
    )
    total_tok = sum(result_tokens.values())
    for cat in all_cats:
        calls = call_counts.get(cat, 0)
        results = result_counts.get(cat, 0)
        tok = result_tokens.get(cat, 0)
        avg = tok // results if results else 0
        pct = f"({100 * tok // total_tok}%)" if total_tok else ""
        print(f"  {cat:<24} {calls:>5}  {results:>7}  {tok:>8,} {pct:<4}  {avg:>14,}")

    print(f"\n  Total estimated output tokens: {total_tok:,}")

    if args.show_examples:
        print("\nExamples:")
        for cat in all_cats:
            if cat in call_examples:
                print(f"  [{cat}] {call_examples[cat]}")


if __name__ == "__main__":
    main()
