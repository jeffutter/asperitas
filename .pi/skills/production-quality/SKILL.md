---
name: production-quality
description: Use when preparing for production deployment, conducting code reviews, setting up precommit workflows, adding type annotations, implementing testing strategies, addressing security concerns, or ensuring documentation quality and best practices across any language
---

# Production Quality Skill

This skill provides comprehensive knowledge of production-quality standards, workflows, and best practices for software development. It covers precommit workflows, testing strategies, type annotations, documentation requirements, and security considerations.

## Precommit Workflow

**Every commit must pass these four checks in order:**

### 1. Compilation/Type Check

Verify code compiles with no warnings/errors.

**Language-Specific Tools:**

| Language | Command | What It Checks |
|----------|---------|----------------|
| Elixir | `mix compile --warnings-as-errors` | Compilation, unused variables, ambiguous calls |
| Rust | `cargo check` | Type checking, borrow checker |
| Python | `mypy .` | Type hint validation |
| TypeScript | `tsc --noEmit` | Type checking without emitting |

For detailed language-specific guidance:
- **Elixir**: See `references/elixir.md`
- **Rust**: See `references/rust.md`

### 2. Code Formatting

Ensure consistent code style.

**Language-Specific Tools:**

| Language | Command | Configuration |
|----------|---------|---------------|
| Elixir | `mix format` | `.formatter.exs` |
| Rust | `cargo fmt -- --check` | `rustfmt.toml` |
| Python | `ruff format --check .` | `pyproject.toml` |
| TypeScript | `prettier --check .` | `.prettierrc` |

### 3. Static Analysis/Linting

Check for common issues and style violations.

**Language-Specific Tools:**

| Language | Command | What It Checks |
|----------|---------|----------------|
| Elixir | `mix credo --strict` | Consistency, readability, design |
| Rust | `cargo clippy -- -D warnings` | Idiomatic patterns, common mistakes |
| Python | `ruff check .` or `pylint` | Style, errors, complexity |
| TypeScript | `eslint .` | Style, potential bugs |

### 4. Test Suite

Run full test suite.

**Language-Specific Tools:**

| Language | Command | Notes |
|----------|---------|-------|
| Elixir | `mix test` | ExUnit framework |
| Rust | `cargo test` | Built-in test framework |
| Python | `pytest` | pytest framework |
| TypeScript | `npm test` | Jest/Vitest/etc. |

### Complete Precommit Script Pattern

```bash
#!/usr/bin/env bash
set -euo pipefail

# Detect language and run appropriate checks
if [[ -f "mix.exs" ]]; then
    mix compile --warnings-as-errors && \
    mix format --check-formatted && \
    mix credo --strict && \
    mix test
elif [[ -f "Cargo.toml" ]]; then
    cargo check && \
    cargo fmt -- --check && \
    cargo clippy -- -D warnings && \
    cargo test
elif [[ -f "pyproject.toml" ]]; then
    mypy . && \
    black --check . && \
    ruff check . && \
    pytest
elif [[ -f "package.json" ]]; then
    tsc --noEmit && \
    prettier --check . && \
    eslint . && \
    npm test
fi
```

For language-specific precommit details:
- **Elixir**: See `references/elixir.md`
- **Rust**: See `references/rust.md`

## Type Annotations

**All public functions should have type annotations.**

### Why Type Annotations Matter

- Document function contracts
- Catch errors at compile time
- Enable better tooling support
- Make refactoring safer
- Serve as documentation

### Language-Specific Patterns

**Type annotation syntax varies by language:**

| Language | Syntax | Example |
|----------|--------|---------|
| Elixir | `@spec` | `@spec get_user(integer()) :: {:ok, User.t()} \| {:error, :not_found}` |
| Rust | Function signature | `fn get_user(id: i64) -> Result<User, Error>` |
| Python | Type hints | `def get_user(id: int) -> User \| None:` |
| TypeScript | TypeScript types | `function getUser(id: number): User \| null` |

### Guidelines

**Do**:
- Use concrete types (not `any`, `term`, or `Any`)
- Document complex return types
- Include all error cases in return type
- Use custom types for domain concepts

**Don't**:
- Use overly generic types
- Omit error cases from return types
- Use inconsistent naming

For detailed type annotation patterns:
- **Elixir**: See `references/elixir.md` - `@spec`, `@type` definitions
- **Rust**: See `references/rust.md` - Result types, custom errors

## Testing Strategy

### Test-Driven Development (TDD)

**Process**:
1. **Red**: Write failing test
2. **Green**: Implement minimal code to pass
3. **Refactor**: Improve design while keeping tests passing

### Test Coverage Goals

**Unit Tests** (70% of tests):
- Test individual functions
- Pure logic, no side effects
- Fast execution (<1ms per test)
- Async when possible

**Integration Tests** (25% of tests):
- Test module interactions
- Database operations
- External service mocking
- Medium execution (<100ms per test)

**End-to-End Tests** (5% of tests):
- Test full workflows
- UI interactions
- API endpoints
- Slow execution (can be seconds)

### Testing Best Practices

```pseudocode
# Descriptive test names
test "returns error when email already exists"  # Good
test "test email"  # Bad

# Arrange-Act-Assert pattern
test "updates user successfully":
    # Arrange
    user = create_user(name="Alice")
    attrs = {name: "Alice Updated"}

    # Act
    result = update_user(user, attrs)

    # Assert
    assert result.ok
    assert result.value.name == "Alice Updated"

# One assertion per test (when possible)
test "validates email format":
    result = create_user({email: "invalid"})
    assert result.error
    assert "invalid format" in result.errors.email
```

### Property-Based Testing

Use generators to explore input space:

```pseudocode
property "name formatting is idempotent":
    for all name in string_generator():
        formatted_once = format_name(name)
        formatted_twice = format_name(formatted_once)
        assert formatted_once == formatted_twice

property "list sorting never loses elements":
    for all list in list_of_integers():
        sorted_list = sort(list)
        assert len(list) == len(sorted_list)
        assert all(item in sorted_list for item in list)
```

For language-specific testing patterns:
- **Elixir**: See `references/elixir.md` - ExUnit, StreamData
- **Rust**: See `references/rust.md` - cargo test, proptest

## Documentation Standards

### Module Documentation

```pseudocode
# Module documentation template
"""
Module: Accounts

The Accounts context manages user accounts and authentication.

## Responsibilities
- User registration and management
- Authentication and session management
- Password reset workflows
- Email verification

## Examples

    >>> Accounts.register_user({email: "alice@example.com", password: "secret"})
    Ok(User)

    >>> Accounts.authenticate("alice@example.com", "secret")
    Ok(User)
"""
```

### Function Documentation

```pseudocode
"""
Authenticates a user by email and password.

Returns `Ok(user)` if credentials are valid, or `Error(:unauthorized)`
if the email doesn't exist or password doesn't match.

## Examples

    >>> authenticate("alice@example.com", "correct_password")
    Ok(User(email="alice@example.com"))

    >>> authenticate("alice@example.com", "wrong_password")
    Error(:unauthorized)

    >>> authenticate("nonexistent@example.com", "password")
    Error(:unauthorized)
"""
def authenticate(email: str, password: str) -> Result[User, Error]:
    # Implementation
```

### Code Comments

**Do comment**:
- Why decisions were made
- Non-obvious business rules
- Performance considerations
- Security concerns

**Don't comment**:
- What the code does (should be obvious)
- Redundant information

```pseudocode
# Bad comment - repeats code
# Get user by ID
def get_user(id):
    return db.get(User, id)

# Good comment - explains why
# Use eager loading to prevent N+1 queries.
# Dashboard displays user profile and last 10 orders,
# so preloading is faster than lazy loading (measured: 500ms → 50ms)
def get_user_for_dashboard(id):
    return (
        User.query()
        .where(id=id)
        .preload("profile", "orders.limit(10).order_by(desc)")
        .first()
    )
```

## Security Practices

### Input Validation

Validate all user input:

```pseudocode
# Good - Validate with schema
def create_user(attrs):
    validated = validate(attrs, UserSchema)
    if validated.error:
        return Error(validated.errors)
    return db.insert(User, validated.data)

# Bad - Trust input
def create_user(attrs):
    return db.insert(User, attrs)  # Unvalidated!
```

### SQL Injection Prevention

Use parameterized queries:

```pseudocode
# Good - Parameterized query
def find_by_email(email):
    return db.query("SELECT * FROM users WHERE email = ?", [email])

# Bad - String interpolation
def find_by_email(email):
    return db.query(f"SELECT * FROM users WHERE email = '{email}'")  # SQL injection!
```

### XSS Prevention

Escape output by default:

```pseudocode
# Good - Auto-escaped by framework
render("p", user_input)  # Escaped automatically

# Dangerous - Raw HTML
render_raw(user_input)  # Only if you trust the source!
```

### Authentication & Authorization

```pseudocode
# Scope routes by authentication
routes:
    authenticated_routes:
        - /dashboard -> DashboardController
        - /settings -> SettingsController

# Check permissions in business logic
def delete_post(user, post):
    if user.id == post.author_id or user.is_admin:
        return db.delete(post)
    else:
        return Error(:unauthorized)
```

### Secrets Management

```pseudocode
# Good - Use environment variables
config = {
    api_key: env("SENDGRID_API_KEY")
}

# Bad - Hardcoded secrets
config = {
    api_key: "SG.abc123..."  # Never commit!
}
```

## Performance Guidelines

### Database Optimization

```pseudocode
# Good - Preload associations to avoid N+1
products = db.query(Product).preload("category", "vendor").all()

# Bad - N+1 query problem
products = db.query(Product).all()  # 1 query
for p in products:
    print(p.category)  # N more queries

# Good - Use indexes for frequent queries
create_index("products", ["user_id"])
create_index("products", ["category_id", "status"])

# Good - Select only needed fields
db.query(User).select("id", "name", "email").all()
```

### Caching Strategies

```pseudocode
# In-memory cache
cache = Cache.new(read_concurrency=True)
cache.get(key)
cache.put(key, value, ttl=3600)
```

## Complexity Analysis

Before implementing algorithms, analyze complexity:

```pseudocode
"""
Matches products with similar users.

Complexity: O(n × m) where n = products, m = users
Real-world: 10,000 products × 1,000 active users = 10M operations
Estimated: ~100ms with in-memory processing

Considered O(n + m) hash-map approach but memory overhead (80MB)
not justified for current scale.
"""
def find_recommendations(user_id):
    # Implementation
```

**When to benchmark**:
- O(n²) or higher complexity
- Core business logic with performance requirements
- Uncertain about approach tradeoffs

## Language-Specific References

For detailed language-specific guidance:
- **Python**: See `references/python.md` - mypy/ruff toolchain, dataclass patterns, singletons, CLI design
- **Elixir**: See `references/elixir.md` - mix tooling, ExUnit, typespecs, Credo
- **Rust**: See `references/rust.md` - cargo tooling, testing, clippy, error handling

## Success Metrics

Production-quality code achieves:
- ✅ Zero precommit failures
- ✅ 100% type annotation coverage for public APIs
- ✅ Comprehensive test coverage (critical paths: 100%)
- ✅ Clear documentation with examples
- ✅ Security best practices applied
- ✅ Performance characteristics understood
