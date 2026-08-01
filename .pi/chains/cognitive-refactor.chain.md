---
name: cognitive-refactor
description: Audit cognitive complexity using Ousterhout's principles, design cleaner architecture, then implement the refactoring safely.
---

## cognitive-scientist
output: complexity-audit.md
progress: true

Audit the cognitive complexity of: {task}

Apply Ousterhout's principles:
- Identify shallow modules (interface nearly as complex as implementation)
- Find information leakage (same knowledge in multiple places)
- Detect temporal coupling and pass-through arguments
- Measure working memory load (parameter counts, variable lifespans, nesting depth)
- Score onboarding difficulty (1–10)

Prioritize refactoring recommendations by impact and effort.

## architect
reads: complexity-audit.md
output: refactor-plan.md

Design a refactoring plan based on this audit.

Target: {task}

Audit findings: {previous}

Design deeper modules, eliminate leakage, and pull complexity downward. For each recommended change, provide:
- Before/after interface comparison
- Which callers are affected
- Whether behavior changes (it shouldn't)
- Phased approach to keep the system working throughout

## developer
reads: complexity-audit.md, refactor-plan.md
progress: true

Implement the refactoring for: {task}

Write characterization tests first — capture existing behavior as tests before touching any code. Then refactor incrementally following the plan, keeping all tests green at each step. Never change behavior and structure in the same commit.
