---
name: data-validation
description: Use when validating input data, implementing parse-don't-validate patterns, designing validation pipelines, or handling external data safely
---

# Data Validation Patterns

Ensure data correctness at system boundaries through parsing, not just checking.

## When to Use

- Validating API input
- Processing form submissions
- Importing external data (files, third-party APIs)
- Enforcing business rules on data
- Type-safe data transformation
- Preventing invalid state from entering the system

## Core Concepts

### Parse, Don't Validate

The key insight: validation alone leaves you with unvalidated types. Parsing transforms unvalidated data into validated types that **cannot represent invalid state**.

```python
# VALIDATE - check then use raw data (risky)
def process(email: str):
    if not is_valid_email(email):
        raise ValueError("Invalid email")
    send_email(email)  # Still a raw string - could be invalid elsewhere

# PARSE - transform to validated type (safe)
@dataclass(frozen=True)
class Email:
    """An email address that is guaranteed to be valid."""
    value: str

    def __init__(self, raw: str):
        if not EMAIL_REGEX.match(raw):
            raise InvalidEmail(raw)
        # Normalize: lowercase, strip whitespace
        object.__setattr__(self, 'value', raw.lower().strip())

def process(email: Email):  # Type guarantees validity
    send_email(email)
```

**Benefits:**
- Invalid data cannot propagate through the system
- No need to re-validate at every usage site
- Type system documents what's been validated
- Errors caught at the boundary, not deep in business logic

### Validation at Boundaries

Validate external data at entry points; trust internal data:

```
┌─────────────────────────────────────┐
│   System Boundary (validate here)   │
│   - HTTP requests                   │
│   - Form submissions                │
│   - File uploads                    │
│   - External API responses          │
├─────────────────────────────────────┤
│   Application Core (trust types)    │
│   - Business logic uses validated   │
│     types (Email, Money, UserId)    │
│   - No validation needed            │
├─────────────────────────────────────┤
│   Database (final guard)            │
│   - Constraints as safety net       │
│   - NOT primary validation          │
└─────────────────────────────────────┘
```

```python
# API boundary - validate and parse
@app.post("/users")
def create_user(request: Request):
    # Parse raw input into validated types
    match parse_create_user_request(request.json):
        case Ok(validated):
            # Now we have validated types
            user = user_service.create(validated)
            return user.to_json()
        case Err(errors):
            return {"errors": errors}, 422

# Internal code - trust the types
class UserService:
    def create(self, data: ValidatedCreateUser) -> User:
        # No validation needed - types guarantee correctness
        return User(
            email=data.email,  # Email type, not str
            age=data.age,      # PositiveInt type, not int
        )
```

### Validated Types (Newtypes/Smart Constructors)

Create types that can only hold valid values:

```python
from dataclasses import dataclass
from typing import Self
import re

@dataclass(frozen=True)
class Email:
    """Email address - always valid by construction."""
    value: str

    EMAIL_PATTERN = re.compile(r'^[^@]+@[^@]+\.[^@]+$')

    def __new__(cls, raw: str) -> Self:
        normalized = raw.lower().strip()
        if not cls.EMAIL_PATTERN.match(normalized):
            raise ValueError(f"Invalid email: {raw}")
        instance = object.__new__(cls)
        object.__setattr__(instance, 'value', normalized)
        return instance

@dataclass(frozen=True)
class PositiveInt:
    """Integer greater than zero."""
    value: int

    def __new__(cls, raw: int) -> Self:
        if raw <= 0:
            raise ValueError(f"Must be positive: {raw}")
        instance = object.__new__(cls)
        object.__setattr__(instance, 'value', raw)
        return instance

@dataclass(frozen=True)
class Money:
    """Monetary amount with currency."""
    amount: Decimal
    currency: str

    VALID_CURRENCIES = {"USD", "EUR", "GBP"}

    def __new__(cls, amount: Decimal, currency: str) -> Self:
        if currency not in cls.VALID_CURRENCIES:
            raise ValueError(f"Invalid currency: {currency}")
        if amount < 0:
            raise ValueError(f"Amount cannot be negative: {amount}")
        instance = object.__new__(cls)
        object.__setattr__(instance, 'amount', amount)
        object.__setattr__(instance, 'currency', currency)
        return instance
```

### Composable Validation

Build complex validators from simple ones:

```python
from dataclasses import dataclass
from typing import Callable, Generic, TypeVar

T = TypeVar('T')
E = TypeVar('E')

@dataclass
class Validator(Generic[T]):
    """Composable validator that accumulates errors."""
    validate: Callable[[T], list[str]]

    def and_then(self, other: "Validator[T]") -> "Validator[T]":
        """Chain validators, accumulating all errors."""
        def combined(value: T) -> list[str]:
            return self.validate(value) + other.validate(value)
        return Validator(combined)

# Simple validators
def required(field: str) -> Validator[dict]:
    def validate(data: dict) -> list[str]:
        if field not in data or data[field] is None:
            return [f"{field} is required"]
        return []
    return Validator(validate)

def min_length(field: str, length: int) -> Validator[dict]:
    def validate(data: dict) -> list[str]:
        value = data.get(field, "")
        if len(str(value)) < length:
            return [f"{field} must be at least {length} characters"]
        return []
    return Validator(validate)

def matches_pattern(field: str, pattern: re.Pattern, message: str) -> Validator[dict]:
    def validate(data: dict) -> list[str]:
        value = data.get(field, "")
        if not pattern.match(str(value)):
            return [message]
        return []
    return Validator(validate)

# Compose validators
user_validator = (
    required("email")
    .and_then(matches_pattern("email", EMAIL_PATTERN, "Invalid email format"))
    .and_then(required("password"))
    .and_then(min_length("password", 8))
)

# Use
errors = user_validator.validate(data)
if errors:
    return Err(ValidationErrors(errors))
```

### Accumulating vs Fail-Fast

| Strategy | Behavior | Use Case |
|----------|----------|----------|
| Fail-fast | Stop on first error | Internal assertions, single-field focus |
| Accumulating | Collect all errors | Form validation, user feedback |

```python
# Fail-fast - stop on first error
def validate_fail_fast(data: dict) -> Result[ValidatedData, str]:
    if "email" not in data:
        return Err("email is required")
    if not is_valid_email(data["email"]):
        return Err("invalid email format")
    if "password" not in data:
        return Err("password is required")
    # ...

# Accumulating - collect all errors (better for forms)
def validate_accumulating(data: dict) -> Result[ValidatedData, list[str]]:
    errors = []

    if "email" not in data:
        errors.append("email is required")
    elif not is_valid_email(data["email"]):
        errors.append("invalid email format")

    if "password" not in data:
        errors.append("password is required")
    elif len(data["password"]) < 8:
        errors.append("password must be at least 8 characters")

    if errors:
        return Err(errors)

    return Ok(ValidatedData(...))
```

### Validation Levels

Different validation types serve different purposes:

| Level | Purpose | Examples |
|-------|---------|----------|
| Format | Structural correctness | Email format, date format, JSON schema |
| Business | Domain rules | Age >= 18, unique email, credit limit |
| Cross-field | Field relationships | End date > start date, confirm password matches |
| State-dependent | Context rules | Can't cancel completed order |

```python
def validate_order_update(order: Order, update: OrderUpdate) -> Result[ValidatedUpdate, list[str]]:
    errors = []

    # Format validation
    if update.quantity is not None and update.quantity <= 0:
        errors.append("quantity must be positive")

    # Business validation
    if update.discount_percent is not None and update.discount_percent > 50:
        errors.append("discount cannot exceed 50%")

    # State-dependent validation
    if order.status == OrderStatus.SHIPPED:
        if update.quantity is not None:
            errors.append("cannot change quantity of shipped order")
        if update.shipping_address is not None:
            errors.append("cannot change address of shipped order")

    if errors:
        return Err(errors)

    return Ok(ValidatedUpdate(...))
```

## Trade-offs

### Validation vs Parsing

| Approach | Pros | Cons |
|----------|------|------|
| Validation | Simple, quick to implement | Leaves raw types, easy to forget to validate |
| Parsing | Type safety, impossible invalid states | More types to define, initial overhead |

**Recommendation:** Use parsing for core domain concepts (Email, Money, UserId). Use validation for one-off checks.

### Where to Validate

| Location | Pros | Cons |
|----------|------|------|
| Controller/Handler | Early feedback, clear error responses | Duplicated if multiple entry points |
| Service layer | Single validation point, reusable | Later feedback |
| Domain types | Impossible invalid states | Can't represent "in progress" validation |

**Recommendation:** Validate at entry points, parse into domain types.

## Anti-Patterns

### 1. Stringly Typed

```python
# BAD - everything is string, validation scattered
def create_order(
    user_id: str,      # Could be anything
    amount: str,       # Could be "abc"
    currency: str,     # Could be "XYZ"
):
    # Must validate everywhere this is called
    if not user_id.isdigit():
        raise ValueError()
    ...

# GOOD - types enforce validity
def create_order(
    user_id: UserId,   # Already validated
    amount: Money,     # Already validated
):
    # No validation needed
    ...
```

### 2. Validation Everywhere

```python
# BAD - validating the same thing repeatedly
def a(email: str):
    if not is_valid_email(email):
        raise ValueError()
    b(email)

def b(email: str):
    if not is_valid_email(email):  # Why again?
        raise ValueError()
    c(email)

def c(email: str):
    if not is_valid_email(email):  # And again?
        raise ValueError()
    send_email(email)

# GOOD - validate once at boundary, trust afterwards
def api_handler(request):
    email = Email(request["email"])  # Parse once
    process_user(email)

def process_user(email: Email):  # Trust the type
    send_welcome(email)

def send_welcome(email: Email):  # Trust the type
    mailer.send(email.value, "Welcome!")
```

### 3. Silent Coercion

```python
# BAD - silently changes meaning
age = int(data.get("age", 0))  # Missing age becomes 0?
quantity = max(1, int(data.get("quantity", 1)))  # Force minimum?

# GOOD - explicit handling
age_result = parse_age(data.get("age"))
match age_result:
    case Ok(age): ...
    case Err(MissingField()): return error("age is required")
    case Err(InvalidFormat(v)): return error(f"invalid age: {v}")
```

### 4. Database as Validator

```python
# BAD - relying on database to catch errors
def create_user(email: str):
    try:
        db.execute("INSERT INTO users (email) VALUES (?)", email)
    except UniqueConstraintError:
        return {"error": "email exists"}

# GOOD - validate before database
def create_user(email: str):
    if await user_exists(email):
        return Err(EmailExists(email))

    validated_email = Email(email)  # Format validation
    await db.execute("INSERT INTO users (email) VALUES (?)", validated_email.value)
```

### 5. Mixing Validation and Business Logic

```python
# BAD - validation mixed with business logic
def transfer_money(from_id, to_id, amount):
    if not from_id:  # Validation
        return error("from_id required")
    if amount <= 0:  # Validation
        return error("invalid amount")

    from_account = get_account(from_id)
    if from_account.balance < amount:  # Business rule
        return error("insufficient funds")
    # ...

# GOOD - separate validation from business logic
def transfer_money(request: TransferRequest) -> Result[Transfer, Error]:
    # Validation happens in TransferRequest construction
    validated = TransferRequest.parse(request)  # Returns Result

    # Business logic uses validated types
    return execute_transfer(validated)

def execute_transfer(request: TransferRequest) -> Result[Transfer, Error]:
    # Only business logic here
    from_account = get_account(request.from_id)
    if from_account.balance < request.amount:
        return Err(InsufficientFunds())
    ...
```

## Testing Validation

```python
class TestEmailValidation:
    def test_valid_email(self):
        email = Email("user@example.com")
        assert email.value == "user@example.com"

    def test_normalizes_to_lowercase(self):
        email = Email("User@Example.COM")
        assert email.value == "user@example.com"

    def test_strips_whitespace(self):
        email = Email("  user@example.com  ")
        assert email.value == "user@example.com"

    def test_rejects_invalid_format(self):
        with pytest.raises(ValueError, match="Invalid email"):
            Email("not-an-email")

    def test_rejects_empty_string(self):
        with pytest.raises(ValueError):
            Email("")

class TestUserValidation:
    def test_accumulates_all_errors(self):
        result = validate_user({})

        assert isinstance(result, Err)
        assert "email is required" in result.error
        assert "password is required" in result.error

    def test_valid_user_returns_ok(self):
        result = validate_user({
            "email": "user@example.com",
            "password": "securepassword123"
        })

        assert isinstance(result, Ok)
        assert isinstance(result.value.email, Email)
```

## Additional References

- the `data-validation` skill\'s elixir reference - Ecto.Changeset patterns, embedded schemas
- the `data-validation` skill\'s rust reference - serde, validator crate, TryFrom trait
