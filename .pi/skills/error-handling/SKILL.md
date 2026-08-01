---
name: error-handling
description: Use when implementing error handling strategies, choosing between exceptions and result types, designing error propagation patterns, or implementing railway-oriented programming
---

# Error Handling Patterns

Production-quality error handling that makes errors explicit, composable, and recoverable.

## When to Use

- Designing error handling for new features
- Choosing between exceptions and result types
- Implementing error propagation strategies
- Creating domain-specific error types
- Handling errors at system boundaries (APIs, file I/O, databases)

## Core Concepts

### Result Types vs Exceptions

Two fundamental approaches exist for handling operations that can fail:

| Aspect | Exceptions | Result Types |
|--------|-----------|--------------|
| Visibility | Hidden in stack | Explicit in signature |
| Control flow | Jumps up call stack | Local, sequential |
| Performance | Slower on throw | Consistent cost |
| Composition | Try/catch nesting | map/flatMap chains |
| Best for | Unexpected failures | Expected outcomes |

**Guideline**: Use exceptions for truly unexpected situations (programming bugs, corrupted state). Use result types for expected failure cases (user not found, validation failed, network timeout).

### Result Type Pattern

Model operations that can fail using a discriminated union:

```python
from dataclasses import dataclass
from typing import Generic, TypeVar, Callable

T = TypeVar('T')  # Success value type
E = TypeVar('E')  # Error type
U = TypeVar('U')  # Transformed value type

@dataclass(frozen=True)
class Ok(Generic[T]):
    """Represents a successful operation result."""
    value: T

@dataclass(frozen=True)
class Err(Generic[E]):
    """Represents a failed operation result."""
    error: E

# Result is either Ok or Err
Result = Ok[T] | Err[E]
```

**Usage:**

```python
def divide(a: float, b: float) -> Result[float, str]:
    if b == 0:
        return Err("Division by zero")
    return Ok(a / b)

def parse_int(s: str) -> Result[int, str]:
    try:
        return Ok(int(s))
    except ValueError:
        return Err(f"Invalid integer: {s}")

# Pattern matching for handling
match divide(10, 2):
    case Ok(value):
        print(f"Result: {value}")
    case Err(error):
        print(f"Error: {error}")
```

### Railway-Oriented Programming

Chain operations that can fail so that the first error short-circuits the entire pipeline:

```python
def bind(result: Result[T, E], fn: Callable[[T], Result[U, E]]) -> Result[U, E]:
    """Chain operations: if result is Ok, apply fn; if Err, pass through."""
    match result:
        case Ok(value):
            return fn(value)
        case Err(e):
            return Err(e)

def map_result(result: Result[T, E], fn: Callable[[T], U]) -> Result[U, E]:
    """Transform success value without changing error type."""
    match result:
        case Ok(value):
            return Ok(fn(value))
        case Err(e):
            return Err(e)
```

**Example pipeline:**

```python
def validate_email(email: str) -> Result[str, str]:
    if "@" not in email:
        return Err("Invalid email format")
    return Ok(email.lower().strip())

def validate_age(age: int) -> Result[int, str]:
    if age < 0 or age > 150:
        return Err("Age must be between 0 and 150")
    return Ok(age)

def create_user(email: str, age: int) -> Result[User, str]:
    # Chain validations - first failure stops the chain
    email_result = validate_email(email)
    if isinstance(email_result, Err):
        return email_result

    age_result = validate_age(age)
    if isinstance(age_result, Err):
        return age_result

    return Ok(User(email=email_result.value, age=age_result.value))
```

**Benefits:**
- Explicit error handling at each step
- Early exit on first failure
- Easy to add/remove/reorder steps
- Composable and testable

### Error Boundaries

Establish clear layers where errors are handled vs propagated:

```
┌─────────────────────────────────────┐
│   API Layer (boundary)              │
│   - Catch all errors                │
│   - Convert to HTTP responses       │
│   - Log for observability           │
├─────────────────────────────────────┤
│   Service Layer                     │
│   - Propagate domain errors         │
│   - May wrap lower-level errors     │
├─────────────────────────────────────┤
│   Data Layer (boundary)             │
│   - Convert DB errors to domain     │
│   - No raw exceptions escape        │
└─────────────────────────────────────┘
```

**At boundaries, convert errors:**

```python
# Data layer boundary
def get_user(user_id: int) -> Result[User, DomainError]:
    try:
        row = db.query("SELECT * FROM users WHERE id = ?", user_id)
        if row is None:
            return Err(NotFoundError("User", user_id))
        return Ok(User.from_row(row))
    except DatabaseError as e:
        logger.error("Database error", error=e)
        return Err(ServiceUnavailableError("Database temporarily unavailable"))

# API layer boundary
def handle_get_user(request: Request) -> Response:
    match get_user(request.user_id):
        case Ok(user):
            return Response(status=200, body=user.to_json())
        case Err(NotFoundError()):
            return Response(status=404, body={"error": "User not found"})
        case Err(ServiceUnavailableError(msg)):
            return Response(status=503, body={"error": msg})
```

### Domain-Specific Error Types

Create error types that capture domain meaning:

```python
from dataclasses import dataclass
from typing import Any

@dataclass(frozen=True)
class ValidationError:
    field: str
    message: str

@dataclass(frozen=True)
class NotFoundError:
    resource: str
    identifier: Any

@dataclass(frozen=True)
class ConflictError:
    message: str
    existing_value: Any

@dataclass(frozen=True)
class AuthorizationError:
    action: str
    resource: str

# Union type for all domain errors
DomainError = ValidationError | NotFoundError | ConflictError | AuthorizationError
```

**Benefits:**
- Pattern matching ensures exhaustive handling
- Error types document what can go wrong
- Carrying context aids debugging

### Accumulating Errors

Sometimes you want all errors, not just the first:

```python
def validate_user_data(data: dict) -> Result[ValidatedUser, list[ValidationError]]:
    errors: list[ValidationError] = []

    if not data.get("email"):
        errors.append(ValidationError("email", "Email is required"))
    elif "@" not in data["email"]:
        errors.append(ValidationError("email", "Invalid email format"))

    if not data.get("name"):
        errors.append(ValidationError("name", "Name is required"))
    elif len(data["name"]) < 2:
        errors.append(ValidationError("name", "Name too short"))

    age = data.get("age")
    if age is not None and (age < 0 or age > 150):
        errors.append(ValidationError("age", "Age must be between 0 and 150"))

    if errors:
        return Err(errors)

    return Ok(ValidatedUser(
        email=data["email"],
        name=data["name"],
        age=data.get("age")
    ))
```

### Error Context and Wrapping

Preserve original error while adding context:

```python
@dataclass(frozen=True)
class WrappedError:
    message: str
    cause: Exception | None = None

def process_file(path: str) -> Result[Data, WrappedError]:
    try:
        with open(path) as f:
            content = f.read()
    except IOError as e:
        return Err(WrappedError(f"Failed to read file: {path}", cause=e))

    try:
        data = parse_data(content)
    except ParseError as e:
        return Err(WrappedError(f"Failed to parse file: {path}", cause=e))

    return Ok(data)
```

**In Python, use exception chaining:**

```python
try:
    do_something()
except SomeError as e:
    raise HigherLevelError("Operation failed") from e
```

## Trade-offs

### When to Use Result Types

- Expected failure cases (not found, invalid input, timeout)
- Operations where caller must handle failure
- API boundaries where errors need conversion
- Composing multiple fallible operations

### When to Use Exceptions

- Programming bugs (assertion failures, index out of bounds)
- Truly exceptional conditions (out of memory)
- Library code where control flow varies by caller
- When result type would clutter simple operations

### Hybrid Approach

Many codebases use both:

```python
def get_user_or_raise(user_id: int) -> User:
    """Use when user must exist (programming error if not)."""
    match get_user(user_id):
        case Ok(user):
            return user
        case Err(NotFoundError()):
            raise AssertionError(f"Expected user {user_id} to exist")
        case Err(e):
            raise RuntimeError(f"Unexpected error: {e}")

def try_get_user(user_id: int) -> Result[User, DomainError]:
    """Use when user may or may not exist (expected case)."""
    return get_user(user_id)
```

## Anti-Patterns

### 1. Swallowing Errors

```python
# BAD - error information lost
try:
    risky_operation()
except Exception:
    pass  # Silent failure

# GOOD - handle or propagate with context
try:
    risky_operation()
except SpecificError as e:
    logger.error("Operation failed", error=str(e), operation="risky")
    return Err(OperationFailed(cause=e))
```

### 2. Using Exceptions for Control Flow

```python
# BAD - exceptions for expected cases
def get_config_value(key: str) -> str:
    try:
        return config[key]
    except KeyError:
        return "default"

# GOOD - use proper conditionals or result types
def get_config_value(key: str) -> str:
    return config.get(key, "default")

# Or with result type for explicit handling
def get_config_value(key: str) -> Result[str, str]:
    if key in config:
        return Ok(config[key])
    return Err(f"Config key not found: {key}")
```

### 3. Catching Too Broadly

```python
# BAD - catches everything including bugs
try:
    process_data(data)
except Exception as e:
    return {"error": "Something went wrong"}

# GOOD - catch specific exceptions
try:
    process_data(data)
except ValidationError as e:
    return {"error": f"Invalid data: {e}"}
except NetworkError as e:
    return {"error": "Service temporarily unavailable"}
# Let other exceptions bubble up
```

### 4. Losing Error Context

```python
# BAD - original error lost
except SomeError:
    raise ValueError("Something went wrong")

# GOOD - preserve context
except SomeError as e:
    raise ValueError("User creation failed") from e
```

### 5. Stringly-Typed Errors

```python
# BAD - errors are just strings
def create_user(data: dict) -> Result[User, str]:
    if not data.get("email"):
        return Err("missing_email")  # Hard to match reliably
    ...

# GOOD - typed errors
def create_user(data: dict) -> Result[User, UserCreationError]:
    if not data.get("email"):
        return Err(MissingFieldError("email"))  # Type-safe, pattern matchable
    ...
```

### 6. Inconsistent Error Handling

```python
# BAD - inconsistent patterns in same codebase
def func_a():
    return None  # Failure indicated by None

def func_b():
    raise Exception("failed")  # Failure indicated by exception

def func_c():
    return {"error": "failed"}  # Failure indicated by dict key

# GOOD - consistent pattern throughout
def func_a() -> Result[T, Error]: ...
def func_b() -> Result[T, Error]: ...
def func_c() -> Result[T, Error]: ...
```

## Testing Error Handling

```python
def test_returns_error_for_invalid_input():
    result = validate_email("not-an-email")
    assert isinstance(result, Err)
    assert isinstance(result.error, ValidationError)
    assert result.error.field == "email"

def test_returns_ok_for_valid_input():
    result = validate_email("user@example.com")
    assert isinstance(result, Ok)
    assert result.value == "user@example.com"

def test_accumulates_all_errors():
    result = validate_user_data({})
    assert isinstance(result, Err)
    assert len(result.error) == 2  # Missing email and name
```

## Additional References

- the `error-handling` skill\'s elixir reference - Tagged tuples, with expressions, "let it crash"
- the `error-handling` skill\'s rust reference - Result<T,E>, ? operator, thiserror, anyhow
