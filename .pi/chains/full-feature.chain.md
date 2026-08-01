---
name: full-feature
description: Complete feature pipeline — architect designs, test-designer specifies full test coverage, developer implements TDD, reviewer validates. Best for critical or complex features.
---

## architect
output: architecture.md
progress: true

Design the architecture for: {task}

Include module structure, data models, public API, complexity analysis anchored to real data sizes, and a phased implementation plan. Note any O(n²)+ operations that need benchmarks.

## test-designer
reads: architecture.md
output: test-plan.md

Design a comprehensive test strategy for: {task}

Architecture: {previous}

Explore the entire result space:
- All success variants
- All error variants (each error type separately)
- Edge cases: empty collections, nil/null, boundary values, very large inputs
- Property-based tests for invariants
- Integration tests for multi-module interactions

Assign criticality ratings (1–10) and provide pseudo-code test specifications for criticality 8+.

## developer
reads: architecture.md, test-plan.md
progress: true

Implement using strict TDD.

Task: {task}

Start from the test plan — implement highest-criticality tests first, run to red, then implement green, then refactor. Track each TDD cycle. Do not write implementation code before the corresponding tests pass their red phase.

## reviewer
reads: architecture.md, test-plan.md

Review the implementation for production quality.

Task: {task}

Verify tests match the test plan, type annotations are complete, error handling is explicit, and the code follows project conventions. Flag any gaps between test plan and actual tests.
