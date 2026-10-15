#!/usr/bin/env python3
"""Interval statistics for podtest knob records in an asperitas-rig artifact.

Usage: knob-interval.py ARTIFACT

Uses the poll time t= inside the body (not the record t_ms), and only records
after the first sequence gap, where pre-attach drops end. Deltas quantise to 1 ms.
"""
import re
import statistics
import sys

BODY = re.compile(r"^I ([0-9a-f]{8}) \d+ \[podtest\] t=(\d+) r1=\d+ r2=\d+")


def main(path):
    lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
    sep = lines.index("--")
    first_gap = next((l for l in lines[:sep] if l.startswith("first_gap=")), None)
    recs = []
    for l in lines[sep + 1:]:
        m = BODY.match(l)
        if m:
            recs.append((int(m.group(1), 16), int(m.group(2))))
    start = 0
    if first_gap and "->" in first_gap:
        after = int(first_gap.split("->")[1], 16)
        start = next(i for i, (s, _) in enumerate(recs) if s >= after)
    recs = recs[start:]
    d = [b[1] - a[1] for a, b in zip(recs, recs[1:])]
    q = statistics.quantiles(d, n=20)
    print(f"first_gap line: {first_gap}")
    print(f"knob records used: {len(recs)}  deltas: {len(d)}")
    print(f"median={statistics.median(d)} mean={statistics.mean(d):.3f} "
          f"p5={q[0]} p95={q[-1]} min={min(d)} max={max(d)} stdev={statistics.stdev(d):.3f}")
    near = [i for i, x in enumerate(d) if x <= 2]
    runs, cur = [], []
    for i in near:
        if cur and i == cur[-1] + 1:
            cur.append(i)
        else:
            if len(cur) >= 2:
                runs.append(cur)
            cur = [i]
    if len(cur) >= 2:
        runs.append(cur)
    stalls = [i for i, x in enumerate(d) if x > 20]
    print(f"deltas<=2ms: {len(near)}  runs>=2 consecutive near-zero: {len(runs)}  deltas>20ms: {len(stalls)}")
    for r in runs:
        print("  burst at t=", recs[r[0]][1], "len", len(r))
    for i in stalls:
        print("  stall at t=", recs[i][1], "delta", d[i])
    hist = {}
    for x in d:
        hist[x] = hist.get(x, 0) + 1
    print("histogram:", dict(sorted(hist.items())))


main(sys.argv[1])
