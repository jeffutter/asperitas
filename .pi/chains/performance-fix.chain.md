---
name: performance-fix
description: Profile a performance problem, research modern algorithmic alternatives, then implement the fix with before/after benchmarks.
---

## performance-analyzer
output: performance-report.md
progress: true

Analyze the performance issue: {task}

Profile the code, establish a concrete baseline (latency, throughput, memory), identify the bottleneck, and determine algorithmic complexity with real data sizes. Note whether the issue is algorithmic, architectural, or implementation-level.

## algorithms-researcher
reads: performance-report.md
output: algorithm-recommendation.md

Research better algorithms for the bottleneck in: {previous}

Original problem: {task}

Find modern alternatives with academic citations. Compare time complexity, space complexity, accuracy tradeoffs, and implementation effort. Provide a concrete recommendation with a library suggestion or pseudocode, and specify the data-size threshold where switching is worth it.

## developer
reads: performance-report.md, algorithm-recommendation.md
progress: true

Implement the performance fix for: {task}

Create benchmarks first to capture the current baseline, then implement the recommended algorithm, then run benchmarks to confirm the improvement. Write regression tests to prevent future performance degradation.
