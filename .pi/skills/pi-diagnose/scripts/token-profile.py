#!/usr/bin/env python3
"""Show token usage progression for an agent run.

Usage:
    python3 token-profile.py [run-index]   # default: last run in latest session
    python3 token-profile.py 3             # developer run
    python3 token-profile.py --session 20-14

Prints per-turn token counts, compaction events, and where the budget went.
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
    duration_min,
    find_runs,
    fmt_ts,
    load_jsonl,
)


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
    dur = duration_min(lines)

    print(f"Token profile: {name}  ({dur:.1f} min)\n")

    # Track per-turn reported usage
    turn = 0
    peak_reported = 0
    compactions = 0
    empty_turns = 0

    for ev in lines:
        t = ev.get("type", "")
        ts = fmt_ts(ev.get("timestamp", ""))

        if t == "compaction":
            compactions += 1
            tb = ev.get("tokensBefore", 0)
            summary_len = len(ev.get("summary", ""))
            print(
                f"  [{ts}] ── COMPACTION #{compactions}  tokensBefore={tb:,}  summary={summary_len} chars ──"
            )
            continue

        if t != "message":
            continue

        msg = ev["message"]
        role = msg.get("role", "")

        if role == "assistant":
            usage = msg.get("usage", {})
            inp = usage.get("input", 0)
            out = usage.get("output", 0)
            cache_r = usage.get("cacheRead", 0)
            cache_w = usage.get("cacheWrite", 0)
            total = inp + out + cache_r + cache_w
            stop = msg.get("stopReason", "")
            turn += 1

            if total == 0:
                empty_turns += 1
                print(
                    f"  [{ts}] turn {turn:3d}  usage=0 (model didn't report)  stopReason={stop}"
                )
            else:
                peak_reported = max(peak_reported, total)
                bar_width = min(40, total // 1000)
                bar = "█" * bar_width
                print(
                    f"  [{ts}] turn {turn:3d}  in={inp:>6,}  out={out:>5,}  total={total:>7,}  {stop}  {bar}"
                )

    print("\nSummary:")
    print(f"  Turns: {turn}  (empty/unreported: {empty_turns})")
    print(f"  Peak reported total tokens: {peak_reported:,}")
    print(f"  Compactions: {compactions}")
    if peak_reported == 0:
        print(
            "\n  ⚠  Model reported 0 tokens every turn — proactive compaction threshold"
        )
        print("     cannot fire. Only overflow-error recovery will trigger compaction.")


if __name__ == "__main__":
    main()
