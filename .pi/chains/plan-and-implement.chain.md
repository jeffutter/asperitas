---
name: plan-and-implement
description: Design a feature with the architect, implement it with TDD, then review for production quality. Good for most feature work.
---

## architect
output: architecture.md
progress: true

Design the architecture for: {task}

Provide module structure, data models, public API signatures, complexity analysis with real-world data sizes, test specifications, and a phased implementation plan.

## developer
reads: architecture.md
progress: true

Implement the feature described in the architecture plan.

Original task: {task}

Follow strict TDD — write comprehensive tests exploring the entire result space first, run to red, implement green, then refactor. Use the phased approach from the plan.

## reviewer
reads: architecture.md

Review the implementation for production quality.

Original task: {task}

Check:
- Type annotations on all public functions
- Error handling covers all failure cases
- Tests cover success variants, error cases, and edge cases
- Code follows project patterns from project-learnings.md
- No SPIKE code left untracked
