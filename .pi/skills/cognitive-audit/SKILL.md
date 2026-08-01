---
name: cognitive-audit
description: Use when analyzing code cognitive load, onboarding difficulty, or user invokes /cognitive-audit for complexity analysis and refactoring recommendations
---

# Cognitive Audit Skill

Workflow for analyzing code cognitive load, onboarding difficulty, and providing strategic refactoring recommendations.

## When to Use

Invoke with `/cognitive-audit` when:
- Analyzing cognitive complexity of a module or codebase
- Assessing onboarding difficulty for new developers
- Identifying high-burden areas that need refactoring
- Generating strategic refactoring recommendations with ROI
- Creating onboarding guides for complex areas

Skip this skill when:
- Learning about Ousterhout principles (use cognitive-complexity skill instead)
- Simple code review (use /review instead)
- Tactical quick fixes (not worth deep analysis)

## Usage

```bash
# Analyze entire codebase
/cognitive-audit

# Analyze specific module
/cognitive-audit lib/my_app/order_processor.ex

# Analyze directory
/cognitive-audit lib/my_app/payments/

# Analyze multiple modules
/cognitive-audit lib/my_app/{orders,payments,inventory}
```

## Tools Used

This skill typically uses:
- **Task** - Launch cognitive-scientist agent (Opus) for deep analysis
- **Glob/Grep** - Find files to analyze
- **Read** - Load target files and existing reports
- **Edit** - Update project-learnings.md with cognitive patterns
- **TodoWrite** - Track analysis phases


## Related Skills

Load these skills when needed during analysis:
- **cognitive-complexity** - Ousterhout principles, complexity metrics, refactoring patterns (load for applying principles during analysis)

## Workflow

### Phase 1: Scope and Context

1. **Identify scope**: Parse argument to determine files/directories to analyze
2. **Load existing context**:
   - Read `.claude/project-learnings.md` (if exists)
   - Read `.claude/cognitive-audit-report.md` (previous audit, if exists)
3. **Present scope to user**: Confirm analysis target before proceeding

### Phase 2: Launch Analysis

Use the **Task** tool to spawn a **cognitive-scientist agent** (Opus model) with this prompt:

```markdown
Analyze cognitive complexity for: [target files/directories]

Apply Ousterhout's principles (load cognitive-complexity skill for reference):
1. Deep vs shallow modules - Calculate depth ratio (power / complexity)
2. Information leakage - Temporal coupling, pass-through, exposed internals
3. Complexity direction - Push-up vs pull-down analysis
4. Strategic vs tactical code - Identify accumulating complexity
5. Errors that could be defined out - Type-level prevention opportunities

Measure cognitive load:
- Working memory load (parameters, lifespans, concerns, nesting)
- Semantic ambiguity (generic names, inconsistent naming)
- Temporal coupling (hidden dependencies)
- Pass-through complexity (arguments through layers)

Assess onboarding:
- Context requirements (modules to understand first)
- Domain knowledge needed
- Historical decisions documented (or missing)
- State machine complexity
- Overall difficulty score (1-10)

Generate recommendations:
- Prioritized refactoring list with ROI
- Specific code changes suggested
- Estimated impact on onboarding time
```

### Phase 3: Generate Reports

Create or update these files:

**`.claude/cognitive-audit-report.md`** - Full audit findings:
```markdown
# Cognitive Complexity Audit Report

Generated: [timestamp]
Scope: [files analyzed]

## Executive Summary

- **Overall complexity**: [score]/10
- **Onboarding difficulty**: [score]/10
- **Estimated comprehension time**: [hours] for new developer

## Primary Issues

### 1. [Issue Category] (Severity)
- **Module**: [name]
- **Problem**: [description]
- **Impact**: [who/what affected]
- **Recommendation**: [specific fix]

[... additional issues ...]

## Ousterhout Analysis

### Deep vs Shallow Modules
[Table of modules with depth ratios]

### Information Leakage
[List of leakage points]

### Strategic vs Tactical Code
[Assessment of code investment patterns]

## Refactoring Recommendations

### High Priority (This Sprint)
[Prioritized list with ROI]

### Medium Priority (Next Sprint)
[Secondary improvements]

## Metrics

| Metric | Current | Target | Status |
|--------|---------|--------|--------|
| Shallow modules | X | 0 | [status] |
| High working memory functions | X | <5 | [status] |
| Temporal coupling instances | X | 0 | [status] |
| Onboarding difficulty | X/10 | 4/10 | [status] |

## Next Steps

1. [Actionable item]
2. [Actionable item]
```

**`.claude/onboarding-guide-[module].md`** (for complex areas):
```markdown
# Onboarding Guide: [Module Name]

## Prerequisites
- Understanding of [concept 1]
- Familiarity with [concept 2]

## Key Concepts
[Core concepts to understand]

## Code Flow
[How requests/data flow through this module]

## Common Gotchas
[Things that trip up new developers]

## Related Modules
[What to read next]
```

**Update `.claude/project-learnings.md`** with cognitive patterns section.

### Phase 4: Present Findings

Present summary to user:

```markdown
# Cognitive Complexity Analysis Complete

## Executive Summary

**Scope**: [files analyzed]
**Overall complexity**: [score]/10
**Onboarding difficulty**: [score]/10

## Top Issues

1. **[Issue 1]** - [impact]
2. **[Issue 2]** - [impact]
3. **[Issue 3]** - [impact]

## Recommended Actions

### High Priority
[Top 3 refactoring recommendations with ROI]

## Reports Generated

- `.claude/cognitive-audit-report.md` - Full analysis
- `.claude/onboarding-guide-[module].md` - [if created]
- `.claude/project-learnings.md` - Updated with patterns

## Next Steps

1. Review full report
2. Prioritize refactoring with team
3. Schedule strategic work
4. Re-run audit after refactoring

**Ready to proceed with any refactoring?**
```

## Understanding the Output

### Module Depth Ratio

```
Depth = Implementation Power / Interface Complexity

> 2.0  = Deep module (good!)
1.0-2.0 = Balanced (acceptable)
< 1.0  = Shallow module (refactor needed)
```

### Working Memory Load

```
Score = Parameters + Lifespan + Concerns + Nesting

0-5   = Low load (easy to understand)
6-10  = Moderate load (requires focus)
11-15 = High load (difficult)
16+   = Very high load (refactor!)
```

### Onboarding Difficulty

```
1-3  = Easy (new dev productive in days)
4-6  = Moderate (new dev productive in 1-2 weeks)
7-8  = Hard (new dev productive in 3-4 weeks)
9-10 = Very hard (new dev productive in 1-2 months)
```

## When to Run

**Recommended triggers**:
- Before onboarding new developers
- After major feature completion
- Quarterly complexity checks
- Before refactoring sprints
- Large PRs (>500 lines, >5 files)

**Warning signs requiring audit**:
- New developers struggling (>4 weeks to productivity)
- Frequent bugs in same areas
- Fear of changing code
- "Don't touch that module" comments
- Long PR review times

## Acting on Findings

### High Priority (Address Immediately)
- Critical shallow modules (depth < 0.8)
- Production bugs from temporal coupling
- Working memory load > 15
- Security or data integrity risks

### Medium Priority (This Quarter)
- Moderate shallow modules (depth 0.8-1.0)
- High working memory load (11-15)
- Strategic refactoring opportunities
- Onboarding difficulty > 7

### Low Priority (When Convenient)
- Minor naming inconsistencies
- Configuration extraction
- Documentation improvements

## Success Criteria

Audit succeeds when:
- Ousterhout principles applied systematically
- Metrics beyond cyclomatic complexity measured
- Onboarding difficulty quantified (1-10 score)
- Strategic refactoring recommendations with ROI
- Comprehensive reports generated
- Actionable next steps provided
- Explicitly avoids Clean Code dogma (arbitrary line limits, excessive fragmentation)
