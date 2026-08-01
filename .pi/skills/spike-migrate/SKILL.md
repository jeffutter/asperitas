---
name: spike-migrate
description: Use when upgrading SPIKE code to production quality, migrating prototypes, converting experimental code to production-ready implementations, or when user invokes /spike-migrate
---

# Spike Migration

Systematically upgrade SPIKE prototype code to production quality with full TDD workflow and quality gates.

## When to Use

Invoke with `/spike-migrate` when:
- SPIKE approach validated and working
- Ready for production deployment
- Other features depend on SPIKE code
- SPIKE code stable for 1+ week
- Users requesting the feature in production

Skip this skill when:
- Still exploring alternatives
- Unclear if feature needed
- Major design changes expected
- Performance problems not yet addressed
- SPIKE code is broken or failing tests

## Usage

```bash
/spike-migrate lib/my_app/user_preferences.ex
/spike-migrate lib/my_app_web/live/preferences_live.ex
/spike-migrate  # Migrate all SPIKE code in project
```

Expects an optional file path. Without arguments, scans for all SPIKE markers.

## Tools Used

This skill typically uses:
- **Task** - Spawn developer agent for TDD implementation
- **Read** - Analyze existing SPIKE code
- **Write/Edit** - Upgrade code to production quality
- **Bash** - Run language-specific commands (mix, cargo, npm, pytest)
- **Glob/Grep** - Find SPIKE markers and related files
- **TodoWrite** - Track migration progress


---

## Workflow

### Step 1: Identify SPIKE Code

If file path provided, analyze that file. Otherwise, scan for all SPIKE markers:

```bash
# Find all SPIKE files
grep -r "# SPIKE:" lib/ --files-with-matches
```

Present inventory:

```markdown
## SPIKE Code Inventory

Found SPIKE markers in 5 files:

**Core Implementation**:
- lib/my_app/user_preferences.ex (85 lines, 6 functions)
- lib/my_app/accounts.ex (3 SPIKE functions added)

**UI Layer**:
- lib/my_app_web/live/preferences_live.ex (120 lines)

**Tests**:
- test/my_app/user_preferences_test.exs (2 smoke tests)
- test/my_app_web/live/preferences_live_test.exs (1 smoke test)

**From spike-debt.md**:
- Feature: Email Preferences
- Created: 2025-01-16 (7 days ago)
- Status: Validated (approach works)
- Estimated migration: 8 hours
```

### Step 2: Analyze Quality Gaps

For each SPIKE file, analyze what's missing:

```markdown
## Quality Gap Analysis

### lib/my_app/user_preferences.ex

**Current State** (SPIKE):
- 6 functions implemented
- Basic functionality works
- 2 smoke tests
- 0/6 functions have typespecs
- No input validation
- Only happy path error handling
- No edge case handling
- No module documentation
- No complexity analysis

**Required for Production**:
1. Typespecs (6 functions) - 1 hour
2. Comprehensive tests (13 additional tests) - 4 hours
   - Success cases: 3 tests
   - Error cases: 7 tests
   - Edge cases: 3 tests
   - Criticality: 8-10
3. Input validation - 1 hour
4. Error handling for all paths - 2 hours
5. Module documentation - 30 mins
6. Function documentation - 30 mins

**Total effort**: ~9 hours
```

### Step 3: Create Migration Plan

Generate detailed todo list with **TodoWrite**:

```markdown
Creating migration plan with TodoWrite...

Migration: Email Preferences SPIKE -> Production

Phase 1: Test Infrastructure
- [pending] Create comprehensive test plan for user_preferences
- [pending] Create integration test plan for preferences_live

Phase 2: Core Module (user_preferences.ex)
- [pending] Add typespecs to all 6 public functions
- [pending] Implement comprehensive tests (13 tests)
- [pending] Add input validation
- [pending] Add error handling for all paths
- [pending] Add module documentation
- [pending] Remove # SPIKE: markers

Phase 3: LiveView (preferences_live.ex)
- [pending] Add error handling and user feedback
- [pending] Implement loading states
- [pending] Create integration tests (5 tests)
- [pending] Add accessibility improvements
- [pending] Remove # SPIKE: markers

Phase 4: Quality Gates
- [pending] Run precommit checks
- [pending] Run full test suite
- [pending] Manual QA of UI
- [pending] Update spike-debt.md (archive as migrated)

Phase 5: Documentation
- [pending] Update project-learnings.md
- [pending] Add usage examples
- [pending] Document patterns used
```

### Step 4: Launch TDD Migration Agent

Launch **developer** agent (Sonnet) with migration requirements:

```markdown
Launching developer (Sonnet) for SPIKE migration...

Migration context:
- SPIKE code validated and working
- Converting to production quality
- Files identified with quality gaps
- Test plan created

Migration approach:
1. **Tests First** (TDD)
   - Start with comprehensive test suite
   - Follow Red -> Green -> Refactor
   - Target criticality 8-10 for all tests

2. **Upgrade Implementation**
   - Add typespecs to all public functions
   - Implement proper error handling
   - Add input validation
   - Extract complex logic if needed

3. **Documentation**
   - Module docs explaining purpose
   - Function docs with examples
   - Document patterns used

4. **Remove SPIKE Markers**
   - Delete all # SPIKE: comments
   - Code now production quality

5. **Quality Gates**
   - Run /precommit (must pass)
   - All tests passing
   - Credo strict mode (Elixir) / Clippy (Rust)
   - Ready for review

Agent will track progress with TodoWrite.
```

### Step 5: TDD Migration Process

Agent follows strict TDD workflow:

**Phase 1: Create Comprehensive Tests**

Tests by language:
- **Elixir**: ExUnit with `@tag criticality: 9`
- **Rust**: `#[test]` with doc comments for criticality
- **Python**: pytest with markers `@pytest.mark.criticality(9)`

See the language testing guide for framework-specific patterns.

**Phase 2: Add Type Annotations**

Type annotations by language:
- **Elixir**: `@spec function_name(arg_type()) :: return_type()`
- **Rust**: `fn function_name(arg: ArgType) -> ReturnType`
- **Python**: `def function_name(arg: ArgType) -> ReturnType:`

See the language-specific patterns skill for type annotation patterns.

**Phase 3: Implement Error Handling**

Use language-appropriate error handling patterns:
- **Elixir**: Tagged tuples `{:ok, value}` / `{:error, reason}`
- **Rust**: `Result<T, E>` with `?` operator
- **Python**: Exceptions or Result types

See `skills/error-handling/references/{lang}.md` for patterns.

**Phase 4: Remove SPIKE Markers**

```bash
# Remove all # SPIKE: comments from migrated files
# Agent does this after all tests pass
```

### Step 6: Quality Validation

Run precommit checks (language-aware):

```markdown
## Quality Validation

Running /precommit on migrated code...

1. Compilation/Check: No warnings
2. Formatting: All files formatted
3. Lint: Strict mode passed
4. Tests: All tests passing

All quality gates passed!
```

### Step 7: Update Documentation

Agent updates tracking:

```markdown
## Documentation Updates

Updated `.claude/spike-debt.md`:
- Moved "Email Preferences" from Active to Migrated
- Archived with migration completion date
- Documented effort (actual vs estimated)

Updated `.claude/project-learnings.md`:
- Added pattern: JSONB preference storage
- Added gotcha: Need indexes on JSONB for search
- Added validation pattern for JSONB fields
```

### Step 8: Present Migration Results

Comprehensive summary:

```markdown
# SPIKE Migration Complete: Email Preferences

## What Was Migrated

**SPIKE code** (created 2025-01-16):
- 5 files, 85 lines core + 120 lines UI
- 2 smoke tests
- Basic functionality working

**Production code** (migrated 2025-01-23):
- 5 files fully upgraded
- 28 comprehensive tests (criticality 8-10)
- Full typespecs (6 functions)
- Complete error handling
- Documentation added

## Quality Improvements

| Metric | SPIKE | Production | Improvement |
|--------|-------|------------|-------------|
| Tests | 2 smoke | 28 comprehensive | +1300% |
| Typespecs | 0/6 (0%) | 6/6 (100%) | +100% |
| Error cases handled | 1/8 | 8/8 | +700% |
| Documentation | 0 lines | 45 lines | Added |
| Criticality rating | N/A | 8-10 | Production-ready |
| Precommit status | Warnings | All pass | Ready |

## Files Upgraded

**Core Logic**:
- lib/my_app/user_preferences.ex
  - Added typespecs (6 functions)
  - Added comprehensive tests (15 tests)
  - Implemented validation
  - Full error handling
  - Module documentation
  - Removed # SPIKE: markers

## Test Coverage

**Success Cases** (3 tests, criticality 9-10):
- Get preferences for existing user
- Update preferences with valid data
- Create default preferences for new user

**Error Cases** (8 tests, criticality 9-10):
- Invalid user ID returns :not_found
- Invalid preference keys rejected
- Database errors handled gracefully
- Concurrent updates handled

**Edge Cases** (5 tests, criticality 7-8):
- Empty preferences map
- Unicode in preference strings
- Null values in optional fields

## Precommit Status

All checks passed:
- Compilation: 0 warnings
- Formatting: All files formatted
- Lint: Strict mode, 0 issues
- Tests: 28/28 passing (100%)

Ready for production deployment!

## Technical Debt Closed

Updated `.claude/spike-debt.md`:
- Moved "Email Preferences" to Migrated section
- Archived with completion metrics

## Next Steps

1. Code review (optional, but recommended)
2. Deploy to staging
3. QA testing
4. Deploy to production
5. Monitor performance
```

---

## Configuration

### Migration Strictness

Based on project needs:

**Standard migration** (default):
- Comprehensive tests (criticality 8-10)
- Full typespecs
- Complete error handling
- Basic documentation

**Enterprise migration**:
- Extensive tests (criticality 9-10)
- Property-based tests for complex logic
- Comprehensive documentation
- Performance benchmarks required
- Security review

**Rapid migration**:
- Core tests only (criticality 9-10)
- Typespecs on public API only
- Essential error handling
- Minimal documentation

---

## Error Handling

### No SPIKE Code Found

```
No SPIKE Code Found

Searched for # SPIKE: markers in:
- lib/my_app/user_preferences.ex

No SPIKE markers detected.

This appears to be production code already.

If this is SPIKE code, ensure it's marked:
# SPIKE: [reason]

Or use /review to assess code quality.
```

### SPIKE Not Ready for Migration

```
SPIKE Not Ready for Migration

Analysis of lib/my_app/experimental.ex:

Issues blocking migration:
1. Core functionality broken (3/5 tests failing)
2. Performance issues detected (O(n^3) complexity)
3. Design unclear (multiple approaches mixed)

Recommendation:
- Fix failing tests first
- Resolve performance issues (see /benchmark)
- Clarify design approach

Continue experimenting with /spike before migration.
```

### Migration Failed - Tests Not Passing

```
Migration Failed

Migration incomplete: 5/28 tests still failing

Last status:
- Phase 1: Tests created
- Phase 2: Typespecs added
- Phase 3: Error handling incomplete
  * 5 edge cases not handled
  * Database error tests failing

Action: Developer fixing error handling...

Note: Migration will not complete until all tests pass.
TDD workflow enforced.
```

---

## Best Practices

1. **Validate before migrating**: Ensure SPIKE approach works
2. **Comprehensive tests first**: Write all tests before changing code
3. **Preserve working code**: Don't break what works
4. **Document learnings**: Capture insights in project-learnings.md
5. **Run precommit**: Must pass all quality gates
6. **Update tracking**: Archive SPIKE in spike-debt.md
7. **Benchmark if needed**: Validate performance assumptions

---

## Migration Checklist

For each SPIKE file:

**Before Migration**:
- [ ] SPIKE code validated and working
- [ ] Approach confirmed with stakeholders
- [ ] Performance acceptable
- [ ] No major design changes expected

**During Migration**:
- [ ] Create comprehensive test plan
- [ ] Write tests first (TDD)
- [ ] Add typespecs to all public functions
- [ ] Implement error handling for all paths
- [ ] Add input validation
- [ ] Add documentation (module + functions)
- [ ] Remove # SPIKE: markers
- [ ] Run precommit (must pass)

**After Migration**:
- [ ] All tests passing (100%)
- [ ] Precommit checks pass
- [ ] Update spike-debt.md (archive)
- [ ] Update project-learnings.md (patterns)
- [ ] Code review (optional)
- [ ] Ready for production

---

## Success Criteria

Migration succeeds when:
- All SPIKE markers removed
- Comprehensive test suite (criticality 8-10)
- All public functions have typespecs
- Complete error handling
- Documentation added
- Precommit checks pass (100%)
- No functionality broken
- spike-debt.md updated
- project-learnings.md updated
- Production-ready code

---

## Related Skills

- **spike** - Create SPIKE prototypes (prerequisite workflow)
- **feature** - Full production implementation (alternative to SPIKE workflow)
- **precommit** - Validate quality gates
- **review** - Code review of migrated code
- **benchmark** - Performance validation (via performance-analyzer)
- **learn** - Document patterns discovered
