---
name: algorithm-spike
description: Research the best algorithm for a problem with academic citations, then implement it with TDD and benchmarks to validate the choice.
---

## algorithms-researcher
output: algorithm-recommendation.md
progress: true

Research the best algorithm for: {task}

Search recent CS literature (last 5 years preferred). Compare at least 3 candidate approaches:
- Classic baseline
- Modern alternative(s)
- Any approximate/probabilistic option if applicable

For each: time complexity, space complexity, accuracy tradeoffs, implementation complexity, and known production users. Include academic citations. Give a clear recommendation with the data-size threshold where it makes sense.

## developer
reads: algorithm-recommendation.md
progress: true

Implement the recommended algorithm for: {task}

Recommendation: {previous}

Write benchmarks first establishing a baseline with the naive approach, then implement the recommended algorithm with full test coverage (including edge cases specific to probabilistic algorithms if applicable), then run benchmarks to confirm the expected improvement matches the paper's claims.

Document the paper citation and key parameters in a comment above the implementation.
