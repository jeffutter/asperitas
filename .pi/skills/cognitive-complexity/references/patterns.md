# Cognitive Clarity Refactoring Patterns

Catalog of refactoring patterns to reduce cognitive complexity.

## Pattern 1: Deep Module Extraction

**Problem**: Shallow module with complex interface pushing complexity to callers

**Solution**: Create deep module that pulls complexity downward

### Example: User Validation

**Before** (shallow):
```python
# Callers must know validation rules
def create_user(attrs):
    if not has_required_fields(attrs):
        return Error("missing_fields")

    if not valid_email_format(attrs["email"]):
        return Error("invalid_email")

    if not password_strong_enough(attrs["password"]):
        return Error("weak_password")

    return db.insert(User, attrs)
```

**After** (deep):
```python
# Accounts context hides all validation
class Accounts:
    @staticmethod
    def create_user(attrs):
        # Validation handled internally
        validated = UserSchema.validate(attrs)
        if validated.error:
            return validated

        return db.insert(User, validated.data)

class UserSchema:
    @staticmethod
    def validate(attrs):
        # All validation logic encapsulated
        if not has_required_fields(attrs):
            return Error("missing_fields")

        if not is_valid_email(attrs.get("email")):
            return Error("invalid_email")

        if not is_strong_password(attrs.get("password")):
            return Error("weak_password")

        return Ok(attrs)
```

**Benefits**:
- Validation in one place
- Callers just call `create_user()`
- Easy to add new validation rules

## Pattern 2: Context Object

**Problem**: Many parameters passed through multiple layers

**Solution**: Group related parameters into context object

### Example: Request Pipeline

**Before** (pass-through hell):
```python
def handle_request(conn, current_user, org, feature_flags, config, locale):
    return authenticate(conn, current_user, org, feature_flags, config, locale)

def authenticate(conn, current_user, org, feature_flags, config, locale):
    return authorize(conn, current_user, org, feature_flags, config, locale)

def authorize(conn, current_user, org, feature_flags, config, locale):
    return process(conn, current_user, org, feature_flags, config, locale)

def process(conn, current_user, org, feature_flags, config, locale):
    # Finally uses some of these
    pass
```

**After** (context object):
```python
@dataclass
class RequestContext:
    conn: Connection
    current_user: User | None
    org: Organization | None
    feature_flags: dict
    config: dict
    locale: str

    @classmethod
    def from_conn(cls, conn: Connection) -> "RequestContext":
        return cls(
            conn=conn,
            current_user=conn.assigns.get("current_user"),
            org=conn.assigns.get("current_org"),
            feature_flags=get_feature_flags(conn),
            config=get_app_config(),
            locale=get_locale(conn)
        )

def handle_request(ctx: RequestContext):
    return authenticate(ctx)

def authenticate(ctx: RequestContext):
    return authorize(ctx)

def authorize(ctx: RequestContext):
    return process(ctx)

def process(ctx: RequestContext):
    # Pattern match only what you need
    user = ctx.current_user
    flags = ctx.feature_flags
    # ...
```

**Benefits**:
- One parameter instead of 6
- Easy to add new context fields
- Can destructure specific fields where needed

## Pattern 3: Eliminate Temporal Coupling

**Problem**: Functions must be called in specific order

**Solution**: Make dependencies explicit in function signatures

### Example: System Initialization

**Before** (temporal coupling):
```python
def start_system():
    # Must be called in this exact order!
    # (No enforcement, easy to get wrong)
    start_database()
    load_configuration()
    initialize_cache()
    connect_services()
    start_web_server()

# Each function depends on previous, but not obvious
def start_database():
    # ...
    pass

def load_configuration():
    # Assumes database started (!)
    pass
```

**After** (explicit dependencies):
```python
def start_system():
    db = start_database()
    if db.error:
        return db

    config = load_configuration(db)
    if config.error:
        return config

    cache = initialize_cache(config)
    if cache.error:
        return cache

    services = connect_services(config)
    if services.error:
        return services

    server = start_web_server(cache, services)
    return server

def start_database() -> Result[Database, Error]:
    return Ok(Database())

def load_configuration(db: Database) -> Result[Config, Error]:
    # Type shows it needs db
    return Ok(Config())

def initialize_cache(config: Config) -> Result[Cache, Error]:
    # Type shows it needs config
    return Ok(Cache())
```

**Benefits**:
- Order explicit in code
- Can't call in wrong order (types enforce)
- Each function declares what it needs

## Pattern 4: Strategy Pattern (Eliminate Special Cases)

**Problem**: Many special case conditionals accumulating over time

**Solution**: Use polymorphism (protocols) to eliminate branches

### Example: Pricing Logic

**Before** (special cases):
```python
def calculate_discount(user):
    if user.type == "admin":
        base_discount = 100
    elif user.type == "premium":
        base_discount = 20
    elif user.type == "trial" and days_remaining(user) > 7:
        base_discount = 10
    elif user.type == "trial":
        base_discount = 5
    elif user.referral_count > 10:
        base_discount = 15
    elif user.first_purchase:
        base_discount = 10
    else:
        base_discount = 0

    # More special cases for holidays, promotions, etc.
    final_discount = adjust_for_holidays(base_discount, user)
    final_discount = adjust_for_promotions(final_discount, user)
    return final_discount
```

**After** (strategy pattern):
```python
class DiscountStrategy(Protocol):
    """Calculate discount percentage for user."""
    def calculate(self) -> int: ...

class AdminDiscount:
    def calculate(self) -> int:
        return 100

class PremiumDiscount:
    def calculate(self) -> int:
        return 20

class TrialDiscount:
    def __init__(self, days_remaining: int):
        self.days_remaining = days_remaining

    def calculate(self) -> int:
        if self.days_remaining > 7:
            return 10
        return 5

class FreeDiscount:
    def __init__(self, referral_count: int, first_purchase: bool):
        self.referral_count = referral_count
        self.first_purchase = first_purchase

    def calculate(self) -> int:
        if self.referral_count > 10:
            return 15
        if self.first_purchase:
            return 10
        return 0

# Main function now simple
def calculate_discount(strategy: DiscountStrategy, user) -> int:
    base = strategy.calculate()
    return adjust_for_holidays(adjust_for_promotions(base, user), user)
```

**Benefits**:
- Adding new user types: just implement protocol
- Each type's logic isolated
- No growing conditional

## Pattern 5: Pipeline for Complex Transformations

**Problem**: Nested function calls hard to read

**Solution**: Use pipe operator/method chaining for left-to-right flow

### Example: Data Processing

**Before** (nested):
```python
def process_users(ids):
    return format_for_api(
        enrich_with_metadata(
            filter_active(
                add_preferences(
                    load_users(ids)
                )
            )
        )
    )
```

**After** (pipeline):
```python
def process_users(ids):
    return (
        load_users(ids)
        .pipe(add_preferences)
        .pipe(filter_active)
        .pipe(enrich_with_metadata)
        .pipe(format_for_api)
    )

# Or with explicit steps:
def process_users(ids):
    users = load_users(ids)
    users = add_preferences(users)
    users = filter_active(users)
    users = enrich_with_metadata(users)
    return format_for_api(users)
```

**Benefits**:
- Read top-to-bottom
- Easy to add/remove steps
- Clear data flow

## Pattern 6: Chunk Complex Function

**Problem**: Function doing too many things at once

**Solution**: Break into smaller, named chunks

### Example: Order Processing

**Before** (high working memory load):
```python
def process_order(order_id):
    order = db.get(Order, order_id)
    user = db.get(User, order.user_id)

    if user.active:
        items = db.query(Item).filter_by(order_id=order_id).all()
        total = sum(item.price for item in items)

        if user.balance >= total:
            new_balance = user.balance - total
            db.update(user, balance=new_balance)

            for item in items:
                inventory = db.get(Inventory, item.product_id)
                new_count = inventory.count - item.quantity
                db.update(inventory, count=new_count)

            db.update(order, status="completed")
            send_confirmation_email(user, order)

            return Ok(order)
        else:
            return Error("insufficient_funds")
    else:
        return Error("user_inactive")
```

**After** (chunked):
```python
def process_order(order_id):
    result = load_order_and_user(order_id)
    if result.error:
        return result
    order, user = result.value

    result = validate_order(order, user)
    if result.error:
        return result

    result = charge_user(order, user)
    if result.error:
        return result

    result = update_inventory(order)
    if result.error:
        return result

    result = mark_complete(order)
    if result.error:
        return result

    notify_user(user, order)
    return Ok(order)

def load_order_and_user(order_id):
    order = db.get(Order, order_id)
    if order is None:
        return Error("order_not_found")

    user = db.get(User, order.user_id)
    if user is None:
        return Error("user_not_found")

    return Ok((order, user))

def validate_order(order, user):
    if not user.active:
        return Error("user_inactive")
    if not has_sufficient_funds(user, order):
        return Error("insufficient_funds")
    return Ok(None)

def charge_user(order, user):
    # One responsibility
    pass

def update_inventory(order):
    # One responsibility
    pass
```

**Benefits**:
- Each function has one job
- Easy to understand each piece
- Easy to test individually

## Pattern 7: Make Invalid States Unrepresentable

**Problem**: Data structure can be in invalid state

**Solution**: Use types/dataclasses that prevent invalid states

### Example: Email Verification

**Before** (can be invalid):
```python
@dataclass
class User:
    email_verified: bool
    verified_at: datetime | None
    verification_token: str | None

# Can have:
# verified=True, verified_at=None (invalid!)
# verified=False, verified_at=<date> (invalid!)
# verified=True, token=<present> (invalid!)
```

**After** (invalid states impossible):
```python
@dataclass
class UnverifiedEmail:
    token: str

@dataclass
class PendingVerification:
    token: str
    requested_at: datetime

@dataclass
class VerifiedEmail:
    verified_at: datetime

EmailStatus = UnverifiedEmail | PendingVerification | VerifiedEmail

@dataclass
class User:
    email_status: EmailStatus
```

**Benefits**:
- Type system enforces valid states
- Can't forget to set related fields
- Transitions explicit

## Pattern 8: Domain-Specific Error Handling

**Problem**: Generic error tuples don't convey domain meaning

**Solution**: Create domain-specific error types

### Example: Payment Processing

**Before** (generic):
```python
def charge_card(user, amount):
    result = gateway.charge(user.card, amount)
    if result.ok:
        return Ok("charged")
    else:
        return Error(result.error)  # What kind of error?

# Caller must know gateway error codes
result = charge_card(user, amount)
if result.ok:
    pass  # success
elif result.error == "insufficient_funds":  # string matching fragile
    pass
elif result.error == "card_expired":  # what other errors exist?
    pass
```

**After** (domain-specific):
```python
class PaymentError(Enum):
    INSUFFICIENT_FUNDS = "insufficient_funds"
    CARD_EXPIRED = "card_expired"
    INVALID_CARD = "invalid_card"
    GATEWAY_TIMEOUT = "gateway_timeout"
    GATEWAY_ERROR = "gateway_error"

def charge_card(user, amount) -> Result[str, PaymentError]:
    result = gateway.charge(user.card, amount)
    if result.ok:
        return Ok(result.value)
    return Error(translate_gateway_error(result.error))

def translate_gateway_error(error: str) -> PaymentError:
    mapping = {
        "insufficient_funds": PaymentError.INSUFFICIENT_FUNDS,
        "expired_card": PaymentError.CARD_EXPIRED,
        "invalid_card": PaymentError.INVALID_CARD,
    }
    return mapping.get(error, PaymentError.GATEWAY_ERROR)

# Caller has clear error types
result = charge_card(user, amount)
match result:
    case Ok(_):
        pass  # success
    case Error(PaymentError.INSUFFICIENT_FUNDS):
        pass  # known error
    case Error(PaymentError.CARD_EXPIRED):
        pass  # known error
    case Error(_):
        pass  # catch-all
```

**Benefits**:
- Errors are domain concepts
- Type system helps with pattern matching
- Clear what errors are possible

## Pattern 9: Extract Configuration

**Problem**: Configuration scattered throughout code

**Solution**: Centralize configuration, access where needed

### Example: Rate Limiting

**Before** (scattered):
```python
def check_rate_limit(user, action):
    limit = 1000 if user.premium else 100  # Magic numbers
    # ...

def check_api_limit(user):
    limit = 10000 if user.premium else 1000  # Different numbers
    # ...
```

**After** (centralized):
```python
class RateLimits:
    @staticmethod
    def get_limit(user, limit_type: str) -> int:
        limits = {
            "api_calls": (10_000, 1_000),     # (premium, free)
            "actions": (1_000, 100),
            "file_uploads": (100, 10),
        }
        premium_limit, free_limit = limits.get(limit_type, (100, 10))
        return premium_limit if user.premium else free_limit

def check_rate_limit(user, action):
    limit = RateLimits.get_limit(user, "actions")
    # ...
```

**Benefits**:
- One place to change limits
- Easy to see all limits
- Can add complex logic without touching callers

## Pattern 10: Flatten Nested Conditionals

**Problem**: Deep nesting hard to follow

**Solution**: Use early returns or Result pattern

### Example: Validation

**Before** (nested):
```python
def process(attrs):
    if has_required_fields(attrs):
        if valid_email(attrs["email"]):
            if strong_password(attrs["password"]):
                if unique_email(attrs["email"]):
                    return insert_user(attrs)
                else:
                    return Error("email_taken")
            else:
                return Error("weak_password")
        else:
            return Error("invalid_email")
    else:
        return Error("missing_fields")
```

**After** (flat with early returns):
```python
def process(attrs):
    if not has_required_fields(attrs):
        return Error("missing_fields")

    if not valid_email(attrs["email"]):
        return Error("invalid_email")

    if not strong_password(attrs["password"]):
        return Error("weak_password")

    if not unique_email(attrs["email"]):
        return Error("email_taken")

    return insert_user(attrs)

# Or with validation functions:
def process(attrs):
    validators = [
        (has_required_fields, "missing_fields"),
        (lambda a: valid_email(a["email"]), "invalid_email"),
        (lambda a: strong_password(a["password"]), "weak_password"),
        (lambda a: unique_email(a["email"]), "email_taken"),
    ]

    for validate, error_msg in validators:
        if not validate(attrs):
            return Error(error_msg)

    return insert_user(attrs)
```

**Benefits**:
- Linear flow, easy to read
- Easy to add/remove validations
- Each validation isolated

## Applying Patterns

1. **Identify high cognitive load code** (use metrics)
2. **Select appropriate pattern** (match problem)
3. **Refactor incrementally** (one pattern at a time)
4. **Test thoroughly** (ensure behavior unchanged)
5. **Measure improvement** (re-run metrics)

## Pattern Selection Guide

| Problem | Pattern |
|---------|---------|
| Shallow module | Deep Module Extraction |
| Many parameters | Context Object |
| Call order dependency | Eliminate Temporal Coupling |
| Growing conditionals | Strategy Pattern |
| Nested calls | Pipeline |
| Large function | Chunk Complex Function |
| Invalid states possible | Make Invalid States Unrepresentable |
| Generic errors | Domain-Specific Error Handling |
| Magic numbers | Extract Configuration |
| Deep nesting | Flatten Conditionals |
