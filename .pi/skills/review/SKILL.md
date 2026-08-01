---
name: review
description: Use when conducting comprehensive code reviews, checking code quality against production standards, analyzing files for issues and project convention violations
---

# Review Skill

Comprehensive code review against production standards with confidence-based reporting.

## When to Use

Invoke with `/review` when:
- Conducting a comprehensive code review (multiple files or concerns)
- Reviewing specific files or directories for logical errors and patterns
- Validating code against project-specific conventions
- Analyzing code for improvement opportunities before major releases

Use for knowledge (Claude loads contextually) when:
- Assessing code quality during implementation
- Determining what to check in a code review

Skip this skill when:
- Quality gate before commit (use `/precommit` instead - that handles compile/format/lint)
- Reviewing a GitHub PR (use `/pr-review`)
- Analyzing cognitive complexity specifically (use `/cognitive-audit`)

## Usage

```bash
/review                          # Review entire project
/review lib/my_app/accounts.ex   # Review specific file
/review lib/my_app/accounts/     # Review directory
```

## Tools Used

This skill typically uses:
- **Task** - Spawn reviewer agent for analysis
- **Glob** - Find files to review
- **Read** - Load files and project standards
- **Grep** - Search for patterns


## Confidence-Based Reporting

Only report issues with **>=80% confidence** to reduce noise.

### Confidence Calibration

```
90-100% (Critical):
- Objective failures (missing tests, no typespecs)
- Security vulnerabilities
- Data loss risks
- Project convention violations (documented in project-learnings.md)

80-89% (Important):
- Logic bugs with clear evidence
- Performance issues with measurements
- Missing error handling
- Pattern violations

<80% (Not reported):
- Subjective style preferences
- Speculative improvements
- Uncertain issues
- Micro-optimizations without proof
```

Lower-confidence observations can be included in an "Additional Notes" section.

### How to Rate Confidence

Assess each issue against objective evidence:

**90-100% (Critical) Evidence**:
- Code doesn't compile or has type errors (objective)
- Test file exists but function lacks test coverage (objective)
- Project-learnings.md explicitly prohibits this pattern (objective)
- Identical issue found in 3+ files (pattern violation)

**80-89% (Important) Evidence**:
- Logic analysis shows edge case handling is missing
- Pattern used elsewhere in codebase differs from this usage
- Function exceeds complexity thresholds (>50 lines, nested control flow)
- Missing typespec on public function

**<80% (Not Reported)**:
- "This could be more efficient" without profiling data
- Style preference absent from project conventions
- Subjective improvement suggestions

## Workflow

### Step 1: Launch Reviewer Agent

The reviewer agent executes a comprehensive analysis:

1. **Load project standards** - AGENTS.md, CLAUDE.md, project-learnings.md
2. **Run precommit baseline** - Verify code compiles and passes format/linting
   (If precommit fails, review stops and suggests running `/precommit` first)
3. **Analyze code patterns** - Type annotations, error handling, project conventions
4. **Detect SPIKE code** - Identify code ready for migration
5. **Report findings** - Only high-confidence issues (>=80%)

### Step 2: Analysis Process

```bash
# Load context
Read AGENTS.md
Read CLAUDE.md
Read .claude/project-learnings.md

# Check dependencies
Run 

# Run precommit (language-aware)
# Elixir: mix compile -> format -> credo -> test
# Rust: cargo check -> fmt -> clippy -> test
# See production-quality skill for details

# Analyze files
Read [target files]
Check against standards
Rate confidence for each issue
Filter issues <80% confidence
```

### Step 3: Present Findings

**No issues found:**

```markdown
Code Review Complete - No Issues Found

Reviewed: lib/my_app/accounts.ex (245 lines)

Analysis:
- Typespecs: All public functions covered (8/8)
- Error handling: All cases handled explicitly
- Tests: Comprehensive coverage with edge cases (28 tests)
- Patterns: Consistent with project conventions
- Precommit: All checks pass

Project compliance:
- Follows project-learnings.md conventions
- No SPIKE markers found

Code is production-ready!
```

**Issues found:**

```markdown
Code Review Findings

Reviewed: lib/my_app/products.ex (180 lines)

## Critical Issues (Confidence: 90-100%)

[Critical] Missing typespec (Confidence: 95%): create_product/1 at line 42
All public functions require typespecs for production code.
Add: @spec create_product(map()) :: {:ok, Product.t()} | {:error, Ecto.Changeset.t()}

[Critical] Missing tests (Confidence: 100%): delete_product/1 at line 78
New public function has no corresponding tests.
Create test/my_app/products_test.exs with tests for success, error, and edge cases.

## Important Issues (Confidence: 80-89%)

[Important] Missing error handling (Confidence: 85%): fetch_external_data/0 at line 102
Function doesn't handle HTTP failures or timeouts.
Wrap Req calls with error handling and timeout configuration.

[Important] Project convention violation (Confidence: 90%): list_products/1 at line 23
Project convention requires passing 'current_scope' as first argument.
See .claude/project-learnings.md "Domain Conventions" section.
Change: list_products(filters, scope) -> list_products(scope, filters)

[Important] High complexity (Confidence: 85%): process_batch/1 at line 120
Function is 85 lines with nested loops.
Consider: Extract helper functions, use Enum.reduce, or run /benchmark

## Recommendations

- Typespecs: 7/8 functions covered (87.5%)
- Tests: Missing tests for 2 functions
- Precommit: 2 credo warnings (lines 45, 103)

## Next Steps

1. Add missing typespecs (2 functions)
2. Create tests for untested functions
3. Fix error handling for external call
4. Address project convention violation
5. Optional: Refactor complex function or run /benchmark
```

**SPIKE code found:**

```markdown
Code Review Findings

Reviewed: lib/my_app_web/live/dashboard_live.ex (320 lines)

## SPIKE Code Detected

[Info] SPIKE code ready for migration (Confidence: 80%): lib/my_app_web/live/dashboard_live.ex
Code has been stable for 2 weeks with clear patterns.

Quality gaps:
- Missing typespecs (12 functions)
- Minimal error handling (only happy path)
- Tests: 2 smoke tests (need 15+ comprehensive tests)
- No complexity analysis

Migration readiness: Medium
- Design validated with users
- Patterns clear and consistent
- Performance acceptable for 100 users

Next step: /spike-migrate lib/my_app_web/live/dashboard_live.ex

## Other Findings

[Important] LiveView stream IDs inconsistent (Confidence: 85%): lines 45-67
Project pattern uses "prefix-id" format (see project-learnings.md).
Update stream IDs to match: "dashboard-#{id}"
```

## Review Scope Options

### Entire Project

```bash
/review
```

Reviews all files in `lib/` and `lib/*_web/`:

```
Reviewing: 45 files
- Contexts: 12 files
- Schemas: 18 files
- LiveViews: 8 files
- Controllers: 7 files

[Progress tracking with TodoWrite]

Completed: 45/45 files
Issues found: 12 (8 critical, 4 important)
SPIKE code: 2 files ready for migration
```

### Specific File

```bash
/review lib/my_app/accounts.ex
```

Deep review of single file:

```
Reviewing: lib/my_app/accounts.ex (245 lines)

Functions analyzed: 14
- Public: 8
- Private: 6

Checking:
- Typespecs (8/8 public functions)
- Error handling (all paths)
- Tests (28 tests, 100% coverage)
- Project patterns
- Complexity (<50 lines per function)
```

### Directory

```bash
/review lib/my_app/accounts/
```

Review all files in directory:

```
Reviewing: lib/my_app/accounts/ (8 files)

Files:
- accounts.ex (245 lines)
- user.ex (120 lines)
- user_token.ex (80 lines)
- email_verification.ex (95 lines)
[...]

[Aggregate findings from all files]
```

## Configuration

### Review Strictness

Based on `.claude/elixir-production.local.md`:

**Enterprise mode** (strict_mode: true):
- All confidence levels reported (even 70%+)
- Extensive checks
- Strict compliance with project-learnings.md

**Standard mode** (default):
- Confidence >=80% only
- Focused on critical issues
- Practical recommendations

**Lenient mode** (strict_mode: false):
- Confidence >=90% only
- Critical issues only
- Minimal suggestions

## Integration with Project Knowledge

After review, suggest updates to `.claude/project-learnings.md` if:

- Recurring issues found (3+ times)
- New project-specific patterns discovered
- Common mistakes to document
- Performance insights

```markdown
Suggest updating project-learnings.md:

Found recurring pattern: Missing timeout on Req calls
Seen in 4 files: accounts.ex, products.ex, orders.ex, analytics.ex

Add to "Common Gotchas" section:
"All Req HTTP calls must have explicit timeout (default 5000ms)"

Example:
Req.get!(url, receive_timeout: 5000)

Use: /learn "All Req calls need explicit timeout"
```

## Error Handling

### File Not Found

```
Review Failed

Error: File not found: lib/my_app/missing.ex

Check:
- File path is correct
- File exists in repository
- Typo in path
```

### Not in Supported Project

```
No Supported Project Found

Current directory: /Users/username/Documents
No mix.exs, Cargo.toml, package.json, or pyproject.toml found.

Navigate to a supported project root and try again.
```

### Review Agent Failure

```
Review Failed

Error: Reviewer agent timeout

This can happen with very large files (>1000 lines).
Try reviewing specific functions or smaller sections.

Fallback: Use /precommit for basic quality checks
```

## Best Practices

1. **Review before commit** - Catch issues early
2. **Review after implementation** - Validate quality
3. **Review SPIKE code regularly** - Identify migration opportunities
4. **Address critical issues first** - Security and correctness
5. **Use feedback to update project-learnings.md** - Build team knowledge

## Success Criteria

Review succeeds when:
- Only high-confidence issues reported (>=80%)
- Each issue has clear, actionable fix
- Severity correctly categorized (Critical vs Important)
- SPIKE code identified and migration path suggested
- Project patterns compliance verified

## Related Skills

- **production-quality** - Standards and precommit workflows (knowledge)
- `/precommit` - Basic quality gate (workflow)
- `/spike-migrate` - Upgrade SPIKE code (workflow)
- `/cognitive-audit` - Deep complexity analysis (workflow)
- `/pr-review` - Review GitHub PRs (workflow)
