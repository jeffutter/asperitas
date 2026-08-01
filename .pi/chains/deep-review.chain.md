---
name: deep-review
description: Thorough code review combining production quality standards and cognitive complexity analysis. Good before merging significant changes.
---

## reviewer
output: review-report.md

Review for production quality: {task}

Report only issues with ≥80% confidence, categorized by severity:
- Critical (90–100%): type safety gaps, missing error handling, security issues, data loss risk
- Important (80–89%): test coverage gaps, pattern violations, SPIKE code needing tracking

Check project-learnings.md for project-specific conventions.

## cognitive-scientist
reads: review-report.md

Analyze cognitive complexity: {task}

Quality review findings: {previous}

Apply Ousterhout's principles — module depth, information leakage, temporal coupling, working memory load. Score onboarding difficulty. Integrate findings with the quality review to give a unified picture of what needs the most attention before this code is merged.
