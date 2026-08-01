---
name: pr-review
description: Use when reviewing GitHub pull requests, checking PR quality, analyzing PR changes, or user invokes /pr-review
---

# PR Review Skill

Automated GitHub Pull Request review using `gh` CLI. Checks code against project standards, identifies issues with confidence-based filtering, and triggers cognitive complexity analysis for large changes.

## When to Use

Invoke with `/pr-review` when:
- Reviewing a GitHub pull request for quality
- Checking PR code against project standards
- Analyzing PR changes before merging
- Looking for SPIKE code in PR changes

Skip this skill when:
- Reviewing local code (use `/review` instead)
- Running quality checks before commit (use `/precommit`)
- Analyzing cognitive complexity specifically (use `/cognitive-audit`)

## Prerequisites

**Required**:
- `gh` CLI installed and authenticated
- GitHub repository with pull requests

**Setup**:
```bash
# Install gh CLI
brew install gh  # macOS
# or apt install gh  # Linux

# Authenticate
gh auth login

# Verify access
gh pr list
```

**If gh CLI unavailable**: Review findings will be reported in the terminal rather than posted as PR comments.

## Usage

```bash
# Review specific PR
/pr-review 123

# Review current branch's PR
/pr-review

# Review with PR URL
/pr-review https://github.com/owner/repo/pull/456
```

## Tools Used

This skill typically uses:
- **Task** - Spawn pr-reviewer and cognitive-scientist agents
- **Bash** - Run gh CLI commands
- **Read** - Load files and project standards
- **Grep/Glob** - Search for patterns and files


## Workflow

### Step 1: Fetch PR Data

```bash
# Get PR metadata
gh pr view 123 --json number,title,body,author,files,additions,deletions

# Get PR diff
gh pr diff 123

# List changed files
gh pr view 123 --json files --jq '.files[].path'
```

### Step 2: Detect Large Changes

**Thresholds for cognitive review**:
- More than 500 lines changed, OR
- More than 5 files modified, OR
- Complex architectural changes

Large PRs trigger parallel cognitive-scientist agent for complexity analysis.

### Step 3: Launch Reviewers

**For all PRs** - pr-reviewer agent executes:
1. Load project standards (AGENTS.md, CLAUDE.md, project-learnings.md)
2. Review changed files against standards
3. Check for missing typespecs, tests, error handling
4. Identify SPIKE code (scan for `# SPIKE:` markers)
5. Verify project convention compliance

**For large PRs** - Also launch cognitive-scientist:
- Deep vs shallow modules analysis
- Working memory load assessment
- Onboarding difficulty evaluation
- Complexity indicators detection

### Step 4: Post Review

Post findings as PR comment using:
```bash
gh pr comment 123 --body "$(cat review.md)"
```

## Confidence-Based Reporting

Only report issues with **>=80% confidence** to reduce noise.

### Confidence Calibration

```
90-100% (Critical):
- Missing tests (objective)
- Missing typespecs (objective)
- Security vulnerabilities (clear evidence)
- Project convention violations (documented)

80-89% (Important):
- Logic bugs (clear evidence)
- Performance issues (measurable)
- Missing error handling (likely needed)
- Pattern violations (established pattern)

<80% (Not reported in main findings):
- Subjective style preferences
- Speculative improvements
- Uncertain issues
```

Lower-confidence observations go in "Additional Notes" section.

## Review Checklist

### Code Quality

**Language-Specific Standards**:

**Elixir:**
- [ ] Public functions have typespecs
- [ ] Error handling uses tagged tuples
- [ ] Pattern matching in function heads
- [ ] Proper use of `with` for railway programming
- [ ] No N+1 queries

**Rust:**
- [ ] Public functions documented
- [ ] Error handling uses Result<T, E>
- [ ] Pattern matching exhaustive
- [ ] Proper use of `?` operator

See the language-specific patterns skill for full standards.

**Testing**:
- [ ] New public functions have tests
- [ ] Edge cases covered
- [ ] Error paths tested
- [ ] Integration tests if needed

**Phoenix/LiveView** (Elixir only):
- [ ] Stream IDs match project pattern
- [ ] Forms use `to_form/2`
- [ ] Proper `on_mount` hooks for auth

**Security**:
- [ ] Input validation present
- [ ] SQL injection risks mitigated
- [ ] XSS vulnerabilities checked
- [ ] Authorization checks in place

### Project Compliance

**Against project-learnings.md**:
- [ ] Follows documented conventions
- [ ] Uses established patterns
- [ ] Avoids documented anti-patterns
- [ ] Consistent with domain model

**Against AGENTS.md/CLAUDE.md**:
- [ ] Meets production quality standards
- [ ] DDD principles applied
- [ ] Error handling strategy followed

### SPIKE Code Detection

**Detection markers**:
```
# SPIKE: <reason>
# TODO: Add type annotations
# TODO: Add tests
```

When SPIKE code is found in PR, report:
- Files containing SPIKE markers
- Quality gaps identified
- Migration readiness assessment
- Recommendation to run `/spike-migrate`

## Review Output Format

```markdown
## Code Review Summary

### 📊 Change Statistics
- Files changed: 8
- Lines added: +452
- Lines deleted: -23
- Complexity: Medium

### ✅ Strengths
- [List positive aspects of the PR]

### ⚠️ Issues Found

#### [Critical] Issue title (Confidence: 95%)
**File**: `path/to/file.ex:45`
**Issue**: Description
**Recommendation**: Fix suggestion with code example

#### [Important] Issue title (Confidence: 85%)
**File**: `path/to/file.ex:23`
**Issue**: Description
**Recommendation**: Fix suggestion

### 🧠 Cognitive Complexity Analysis
[For large PRs - output from cognitive-scientist agent]

### 🚀 SPIKE Code
[List any SPIKE code detected, or "None detected"]

### 📝 Next Steps
- [ ] Action items for the developer
- [ ] Run `/precommit` before merging

---
🤖 Generated with Claude Code
```

## Error Handling

### PR not found

```markdown
❌ PR Not Found

Error: PR #999 does not exist in this repository

Check:
- PR number is correct
- You have access to the repository
- PR hasn't been closed/deleted
```

### Not in git repository

```markdown
❌ Not in Git Repository

Current directory is not a git repository.

Navigate to your project root and try again.
```

### gh CLI not authenticated

```markdown
❌ GitHub CLI Not Authenticated

Run: gh auth login

Then try the pr-review command again.
```

### gh CLI unavailable

If gh CLI is not installed, the skill will:
1. Report findings in the terminal instead of posting as PR comment
2. Suggest installing gh CLI for full functionality
3. Still provide the code review analysis

## gh CLI Commands Reference

```bash
# View PR
gh pr view <number>
gh pr view <number> --json title,body,author,files,additions,deletions

# Get diff
gh pr diff <number>

# List changed files
gh pr view <number> --json files --jq '.files[].path'

# Post comment
gh pr comment <number> --body "review text"

# Post review (approve/request changes)
gh pr review <number> --comment --body "review text"
gh pr review <number> --approve --body "LGTM!"
gh pr review <number> --request-changes --body "Please address..."

# Check CI status
gh pr checks <number>

# List PRs
gh pr list --state open --limit 10
```

## Configuration

**Review strictness** (can be configured in project-specific settings):
- **Standard** (default): Confidence ≥80%
- **Strict**: Confidence ≥70%
- **Lenient**: Confidence ≥90%

**Cognitive threshold**:
- **Lines**: 500+ lines triggers cognitive analysis
- **Files**: 5+ files triggers cognitive analysis

## Best Practices

1. **Review early**: Run on draft PRs for quick feedback
2. **Address critical first**: Security and correctness before style
3. **Use cognitive insights**: Large PRs benefit from complexity analysis
4. **Re-review after changes**: Verify fixes with another review
5. **Document patterns**: Add recurring issues to project-learnings.md with `/learn`

## Success Criteria

Review succeeds when:
- ✅ PR data fetched successfully
- ✅ All changed files analyzed
- ✅ Issues reported with ≥80% confidence
- ✅ Cognitive analysis included for large PRs
- ✅ SPIKE code identified
- ✅ Review posted as PR comment (or displayed if gh unavailable)
- ✅ Actionable recommendations provided

## Related Skills

- `/review` - Local code review (before PR)
- `/precommit` - Quality gate before commit
- `/cognitive-audit` - Deep complexity analysis
- `/spike-migrate` - Upgrade SPIKE code found in PR
