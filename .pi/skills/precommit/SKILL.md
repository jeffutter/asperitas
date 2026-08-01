---
name: precommit
description: Use when running quality checks before commits, validating code passes compilation/formatting/linting/tests, or when user invokes /precommit
---

# Precommit Skill

Executes the production-ready precommit workflow to ensure code quality before commits.

## When to Use

Invoke with `/precommit` when:
- Preparing code for commit
- Validating code quality after completing a feature
- Running the full quality check suite
- Checking if code meets production standards

Skip this skill when:
- Doing rapid prototyping (use `/spike` instead)
- Only need to run tests (use `mix test` or `cargo test` directly)
- Reviewing code for design issues (use `/review` instead)

## Usage

```bash
/precommit          # Run all checks
/precommit --fix    # Run checks and auto-fix formatting issues
```

## Tools Used

This skill typically uses:
- **Bash** - Run language-specific commands (`mix`, `cargo`, `npm`, `pytest`)
- **Read** - Check configuration files and dependencies
- **Edit** - Auto-fix formatting issues (with `--fix` flag)


## What It Does

Runs four quality checks in sequence (language-aware):

1. **Compilation/Type Check** with warnings as errors
2. **Code formatting** verification
3. **Static analysis/Linting**
4. **Test suite** execution

## Implementation

### Step 1: Detect Language and Check Dependencies

First detect project language and verify required tools.

**Language Detection:**

Load the `language-detection` skill for canonical detection logic and tooling commands.

**Check Language-Specific Dependencies:**

**Elixir:**
```bash
# Check if credo is in mix.exs
if ! grep -q ':credo' mix.exs; then
  echo "Missing dependency: credo"
  echo "Add to mix.exs:"
  echo '  {:credo, "~> 1.7", only: [:dev, :test], runtime: false}'
  exit 1
fi
```

**Rust:**
```bash
# Check rustfmt and clippy installed
if ! command -v cargo-fmt &> /dev/null; then
  echo "rustfmt not installed"
  echo "Run: rustup component add rustfmt"
  exit 1
fi
```

**Python:**
```bash
# Check for quality tools
if ! command -v mypy &> /dev/null; then
  echo "mypy not installed"
  echo "Run: pip install mypy"
fi
```

**TypeScript:**
```bash
# Check for eslint
if ! npm list eslint &> /dev/null; then
  echo "eslint not installed"
  echo "Run: npm install --save-dev eslint"
fi
```

### Step 2: Run Checks

Execute checks based on detected language:

**Language-Specific Commands:**

| Step | Elixir | Rust | Python | TypeScript |
|------|--------|------|--------|------------|
| 1. Compile/Check | mix compile --warnings-as-errors | cargo check | mypy src/ | tsc --noEmit |
| 2. Format | mix format --check-formatted | cargo fmt -- --check | ruff format --check . | prettier --check . |
| 3. Lint | mix credo --strict | cargo clippy -- -D warnings | ruff check . | eslint . |
| 4. Test | mix test | cargo test | pytest | npm test |

**Elixir Example:**
```bash
echo "Running precommit checks..."
echo ""

# 1. Compile with warnings as errors
echo "1/4 Compiling..."
if mix compile --warnings-as-errors; then
  echo "Compilation passed"
else
  echo "Compilation failed with warnings"
  exit 1
fi

# 2. Format code
echo "2/4 Formatting..."
if mix format --check-formatted; then
  echo "Formatting correct"
else
  echo "Code needs formatting. Run: mix format"
  exit 1
fi

# 3. Run credo
echo "3/4 Static analysis..."
if mix credo --strict; then
  echo "Credo passed"
else
  echo "Credo found issues"
  exit 1
fi

# 4. Run tests
echo "4/4 Running tests..."
if mix test; then
  echo "All tests passed"
else
  echo "Tests failed"
  exit 1
fi

echo "All precommit checks passed!"
```

**Rust Example:**
```bash
echo "Running precommit checks..."

# 1. Check compilation
echo "1/4 Checking..."
cargo check || exit 1

# 2. Check formatting
echo "2/4 Formatting..."
cargo fmt -- --check || exit 1

# 3. Run clippy
echo "3/4 Linting..."
cargo clippy -- -D warnings || exit 1

# 4. Run tests
echo "4/4 Testing..."
cargo test || exit 1

echo "All precommit checks passed!"
```

### Step 3: Report Results

If all checks pass:

```
Precommit Checks Complete

All checks passed:
- Compilation (0 warnings)
- Formatting (all files formatted)
- Linting (strict mode)
- Tests (X tests, 0 failures)

Ready to commit!
```

If any check fails:

```
Precommit Checks Failed

Results:
- Compilation: passed
- Formatting: 3 files need formatting

Next steps:
1. Run formatter (see language-detection skill for command)
2. Review changes
3. Run /precommit again
```

## Auto-Fix Mode

With `--fix` flag:

```
Precommit with Auto-Fix

1/4 Compilation... passed
2/4 Formatting... Fixed 3 files
3/4 Static analysis... passed
4/4 Tests... passed

Auto-fixed:
- Formatted 3 files
- Run git diff to review changes

All checks passed!
```

## Error Handling

### Missing Dependencies

**Elixir:**
```
Required dependencies not found

Missing:
- credo: Add {:credo, "~> 1.7", only: [:dev, :test], runtime: false}

After adding to mix.exs, run: mix deps.get
```

**Rust:**
```
Clippy not installed

Run: rustup component add clippy
```

### Compilation Errors

**Elixir:**
```
Compilation Failed

Errors found:
  lib/my_app/accounts.ex:42: warning: variable "user" is unused

Fix: Remove unused variables or prefix with underscore: _user
Run: mix compile --warnings-as-errors
```

**Rust:**
```
Compilation Failed

error[E0382]: borrow of moved value: `data`

Fix: Review ownership and borrowing
Run: cargo check
```

### Test Failures

**Elixir:**
```
Tests Failed

Failed tests:
  1) test create_user/1 creates user (MyApp.AccountsTest)
     test/my_app/accounts_test.exs:42

Fix: Review test failure, fix implementation or update tests
Run: mix test test/my_app/accounts_test.exs:42
```

**Rust:**
```
Tests Failed

---- tests::test_create_user stdout ----
thread 'tests::test_create_user' panicked

Fix: Review test failure and fix implementation
Run: cargo test test_create_user
```

## Configuration

Configure behavior per language via environment variables or project config:

**Environment Variables:**
- `PRECOMMIT_SKIP_TESTS=1` - Skip test step
- `PRECOMMIT_FIX=1` - Auto-fix formatting issues

**Language-Specific Configuration:**

**Elixir:** `.credo.exs` for Credo rules
**Rust:** `clippy.toml` for Clippy settings
**Python:** `pyproject.toml` for tool configuration
**TypeScript:** `.eslintrc.js` for ESLint rules

## Integration with Git Hooks

To run automatically before each commit, add to `.git/hooks/pre-commit`:

```bash
#!/bin/bash
set -e

echo "Running precommit checks..."
claude-code /precommit

if [ $? -ne 0 ]; then
  echo ""
  echo "Precommit checks failed. Commit aborted."
  echo "Fix issues and try again, or use --no-verify to skip (not recommended)."
  exit 1
fi
```

Make executable:
```bash
chmod +x .git/hooks/pre-commit
```

## Tips

1. **Run frequently**: Don't wait until commit time
2. **Fix as you go**: Address warnings immediately
3. **Use --fix**: Auto-fix formatting issues
4. **Focus on tests first**: If tests fail, fix before other checks
5. **Batch formatting**: Run formatter on entire codebase periodically

## Success Criteria

Precommit succeeds when:
- All code compiles without warnings
- All files properly formatted
- Linter passes in strict mode
- All tests pass
- No flaky tests (consistent pass/fail)

## Related Skills

- `/review` - Comprehensive code review beyond precommit checks
- `/spike` - Fast prototyping mode (skips some checks)
- `/spike-migrate` - Upgrade SPIKE code to pass precommit
- `production-quality` - Knowledge about quality standards (not a workflow)
