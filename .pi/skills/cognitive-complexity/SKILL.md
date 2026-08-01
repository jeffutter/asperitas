---
name: cognitive-complexity
description: Use when analyzing cognitive load, code complexity, onboarding difficulty, readability concerns, maintainability issues, or applying Ousterhout principles for deep modules, reducing complexity, and strategic programming approaches
---

# Cognitive Complexity Skill

This skill provides comprehensive knowledge of cognitive complexity analysis based on John Ousterhout's "A Philosophy of Software Design", focusing on deep modules, reducing complexity through better abstractions, and strategic programming approaches that minimize cognitive load.

## Core Philosophy: Ousterhout's Principles

### 1. Complexity is Incremental

**Key insight**: Complexity doesn't come from single bad decisions, but from accumulation of many small decisions. Each "just this once" special case, each quick fix, each tactical hack adds a small amount of complexity. Over time, these accumulate into systems that are hard to understand and modify.

**Implications**:
- You can't completely avoid complexity
- Focus on reducing complexity growth rate
- Strategic investments pay off over time
- Small improvements compound

**Example - Special cases accumulating**:
```python
# Each special case adds complexity
def calculate_discount(user):
    if user.type == "admin":
        return 100  # Special case 1
    elif user.type == "premium":
        return 20  # Special case 2
    elif user.trial_expired:
        return 0  # Special case 3
    elif user.referral_count > 10:
        return 15  # Special case 4
    # More special cases accumulate over time...
    else:
        return 10

# Better: Use polymorphism to eliminate special cases
class DiscountPolicy(Protocol):
    def calculate_discount(self) -> int: ...

class AdminUser:
    def calculate_discount(self) -> int:
        return 100

class PremiumUser:
    def calculate_discount(self) -> int:
        return 20

# Adding new types doesn't increase complexity
```

### 2. Deep Modules

**Core concept**: Best modules have **simple interfaces** but **powerful implementations**. Think of module depth as:

```
Module Depth = Implementation Power / Interface Complexity
```

**Deep modules** (Good):
- Simple to use
- Hide complex implementation
- Provide powerful functionality
- Minimal knowledge required by caller

**Shallow modules** (Bad):
- Complex to use
- Expose implementation details
- Provide minimal functionality
- Caller must understand internals

**Example**:
```python
# Shallow module - complex interface, minimal power
class ShallowCache:
    def put(self, cache, key, value, ttl, on_expire, compression_level, serializer):
        # Caller must know:
        # - TTL semantics
        # - Expiration callback format
        # - Compression options
        # - Serialization format
        # Just wraps a simple dict operation!
        pass

# Deep module - simple interface, powerful implementation
class DeepCache:
    """Put value in cache with sensible defaults."""

    def put(self, key, value):
        # Handles internally:
        # - Automatic TTL (configured globally)
        # - Expiration cleanup
        # - Automatic compression for large values
        # - Serialization based on value type
        # - Memory management
        # - Thread safety
        # Complex implementation, but caller just calls put()
        pass

    # Advanced options available but not required
    def put_with_options(self, key, value, **opts):
        pass
```

**Measuring depth**:
- **Lines of code**: More implementation = more power
- **Features provided**: What can caller accomplish?
- **Required knowledge**: How much must caller know?
- **Special cases**: How many edge cases handled?

**Red flags** (shallow modules):
- Many parameters required
- Caller must read implementation
- Just delegates to other functions
- Exposes internal data structures
- Many "you must also..." requirements

### 3. Information Leakage

**Key insight**: When implementation details escape through the abstraction boundary, it creates **information leakage**. Changes to implementation now affect all callers.

**Types of leakage**:

**Temporal coupling** - Must call functions in specific order:
```python
# Information leakage - caller must know order
def setup_system():
    start_database()  # Must be first
    load_config()     # Must be second
    start_cache()     # Must be third

# No leakage - order hidden
def setup_system():
    db = start_database()
    if db.error:
        return db

    config = load_config(db)
    if config.error:
        return config

    cache = start_cache(config)
    return cache
```

**Pass-through variables** - Same variable through many layers:
```python
# Leakage - config passed through 5 layers unchanged
def handle_request(conn, config):
    return process_request(conn, config)

def process_request(conn, config):
    return validate_request(conn, config)

def validate_request(conn, config):
    return check_rate_limit(conn, config)

def check_rate_limit(conn, config):
    # Finally uses config.rate_limit here
    pass

# Better - get config where needed
def check_rate_limit(conn):
    config = get_app_config().rate_limit
    # Use config
```

**Exposed data structures** - Internal representation visible:
```python
# Leakage - returns internal dict
def get_cache():
    return _internal_cache_dict  # Caller must know dict API

# No leakage - hide internals behind module API
def get(key):
    value = _internal_cache.get(key)
    if value is not None:
        return Ok(value)
    return Error("not_found")
```

### 4. Pull Complexity Downward

**Key insight**: It's better to add complexity to an implementation to simplify the interface than to push complexity to all callers.

**Why?** Because:
- Implementation has full context
- Complexity in one place, not duplicated
- Easier to change (one location)
- Callers stay simple

**Example**:
```python
# Pushing complexity up (bad)
def get_user(id):
    return db.get(User, id)  # Returns None or user
    # Caller must:
    # - Check for None
    # - Verify user is active
    # - Verify user is verified
    # - Preload associations

# Pulling complexity down (good)
def get_active_user(id):
    user = db.get(User, id)

    if user is None:
        return Error("not_found")

    if not user.active:
        return Error("inactive")

    if not user.verified_at:
        return Error("not_verified")

    # Preload commonly needed associations
    user = db.preload(user, ["profile", "settings"])
    return Ok(user)
```

**When to pull down**:
- Check happens at multiple call sites
- Transformation needed by all callers
- Error handling duplicated
- Associations always needed

### 5. Strategic vs Tactical Programming

**Tactical** (working code, accumulates complexity):
- "Just get it working"
- Copy-paste with modifications
- Special cases as encountered
- Minimal documentation
- Quick fixes
- Short-term thinking

**Strategic** (invests in future simplicity):
- "Make it right"
- Create abstractions
- Handle cases systematically
- Comprehensive documentation
- Proper solutions
- Long-term thinking

**Example**:
```python
# Tactical - quick fix that accumulates complexity
def notify_user(user):
    if user.type == "admin":
        send_admin_email(user)
    elif user.type == "premium":
        send_premium_email(user)
    elif user.type == "free":
        send_free_email(user)
    elif user.type == "trial":
        if user.trial_days_remaining > 7:
            send_free_email(user)
        else:
            send_trial_ending_email(user)
    # Each new user type = more special cases here

# Strategic - invest in abstraction
class NotificationStrategy(Protocol):
    """Send notification to user based on their type."""
    def notify(self, user) -> None: ...

class AdminNotifier:
    def notify(self, user):
        send_email(user, template="admin_welcome")

class PremiumNotifier:
    def notify(self, user):
        send_email(user, template="premium_welcome")

# Adding new types: implement protocol, no special cases
```

**When to invest strategically**:
- Feature will be extended
- Similar patterns appearing
- Code will be maintained long-term
- Team is growing

**When tactical is okay**:
- Prototyping (mark as `# SPIKE`)
- One-off scripts
- Time-critical fixes (refactor later)

### 6. Define Errors Out of Existence

**Key insight**: Best error handling is preventing errors from happening through better design.

**Techniques**:

**Use types to prevent errors**:
```python
# Error-prone
def divide(a, b):
    if b == 0:
        return Error("division_by_zero")
    return Ok(a / b)

# Define error out with type
@dataclass
class NonZero:
    value: int

    def __post_init__(self):
        if self.value == 0:
            raise ValueError("zero_not_allowed")

def divide(a: int, b: NonZero) -> float:
    # Type system prevents zero, no runtime check needed
    return a / b.value
```

**Make invalid states unrepresentable**:
```python
# Error-prone - can be in invalid state
@dataclass
class User:
    email_verified: bool
    email_verified_at: datetime | None
    # Can have verified=False but verified_at set!

# Better - states mutually exclusive
@dataclass
class UnverifiedEmail:
    pass

@dataclass
class VerifiedEmail:
    verified_at: datetime

EmailStatus = UnverifiedEmail | VerifiedEmail

@dataclass
class User:
    email_status: EmailStatus
    # Either unverified, or verified with timestamp
    # Invalid state impossible
```

**Use domain semantics**:
```python
# Error checking
def calculate_percentage(part, whole):
    if whole == 0:
        return Error("division_by_zero")
    return Ok((part / whole) * 100)

# Error defined out - 0/0 means "not applicable" in this domain
def calculate_percentage(part, whole):
    if whole == 0:
        return 0  # Sensible domain interpretation
    return (part / whole) * 100
```

### 7. Comments Should Explain "Why"

**Key insight**: Code shows **what** it does. Comments should explain **why** it does it that way.

**Good comments**:
- Design decisions and tradeoffs
- Non-obvious optimizations
- Business rules and domain knowledge
- Historical context
- Warnings about gotchas

**Bad comments**:
- Repeating code
- Obvious information
- Outdated information
- Commented-out code

**Example**:
```python
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
        .filter_by(id=id)
        .options(
            joinedload(User.profile),
            joinedload(User.orders).limit(10).order_by(Order.created_at.desc())
        )
        .first()
    )

# Good comment - historical context
# We hash passwords in the model (not in database trigger) because:
# 1. Allows different hash algorithms per user (migration scenario)
# 2. Enables testing with mock passwords
# 3. Works with any database (not Postgres-specific)
# See ADR-042 for full decision context
def hash_password(password):
    # ...
```

## Cognitive Complexity Metrics

### Beyond Cyclomatic Complexity

Cyclomatic complexity (counting branches) is insufficient. True cognitive burden includes:

### 1. Working Memory Load

**Parameters** (Miller's Law: 7±2 items):
```python
# High working memory load - 8 parameters
def create_order(user_id, items, payment_method, shipping_addr,
                 billing_addr, discount_code, gift_message, gift_wrap):
    # Developer must hold 8 concepts in mind
    pass

# Lower load - group related concepts
@dataclass
class OrderDetails:
    items: list
    payment_method: str
    shipping_addr: Address
    billing_addr: Address
    discount_code: str | None
    gift_message: str | None
    gift_wrap: bool

def create_order(user_id, order_details: OrderDetails):
    # 2 concepts instead of 8
    pass
```

**Variable lifespan**:
```python
# High cognitive load - variable used 50 lines after definition
def process_batch(items):
    user = get_current_user()  # Line 1
    # ... 50 lines of other logic ...
    notify_user(user)  # Line 51 - must remember user from line 1

# Lower load - shorter lifespan
def process_batch(items):
    results = process_items(items)
    user = get_current_user()
    notify_user(user, results)
```

### 2. Semantic Ambiguity

**Generic names** require reading implementation:
- `process`, `handle`, `manage`, `do_something`
- `data`, `info`, `value`, `result`
- `helper`, `util`, `misc`, `common`

**Clear names** explain purpose:
- `convert_user_to_json`, `validate_email_format`
- `calculate_discount_percentage`, `send_welcome_email`

### 3. Inconsistent Abstractions

**Same concept, different names**:
```python
fetch_user(id)      # HTTP fetch?
get_user(id)        # Database get?
load_user(id)       # Load from where?
retrieve_user(id)   # Same as get?
```

**Better - consistent naming**:
```python
# Pattern: get_<noun> for database reads
get_user(id)
get_order(id)
get_product(id)
```

## Refactoring for Cognitive Clarity

### Pattern 1: Extract Deep Module

**Before**:
```python
# Callers duplicate logic
def handler1(request):
    user = get_user(request.user_id)
    if user and user.active and user.verified:
        # do something
        pass

def handler2(request):
    user = get_user(request.user_id)
    if user and user.active and user.verified:
        # do something else
        pass
```

**After**:
```python
# Deep module hides complexity
class Auth:
    @staticmethod
    def get_active_verified_user(id):
        user = get_user(id)
        if user is None:
            return Error("not_found")
        if not user.active:
            return Error("inactive")
        if not user.verified:
            return Error("not_verified")
        return Ok(user)

def handler1(request):
    result = Auth.get_active_verified_user(request.user_id)
    if result.ok:
        # do something
        pass
```

### Pattern 2: Context Object

**Before**:
```python
# Pass-through hell
def handle_request(conn, current_user, feature_flags, config, locale):
    return process_request(conn, current_user, feature_flags, config, locale)

def process_request(conn, current_user, feature_flags, config, locale):
    return validate_request(conn, current_user, feature_flags, config, locale)
```

**After**:
```python
@dataclass
class RequestContext:
    conn: Connection
    current_user: User | None
    feature_flags: dict
    config: dict
    locale: str

def handle_request(context: RequestContext):
    return process_request(context)

def process_request(context: RequestContext):
    return validate_request(context)
```

### Pattern 3: Eliminate Temporal Coupling

**Before**:
```python
# Must call in order (hidden dependency)
setup_database()
load_config()
start_cache()
```

**After**:
```python
# Order explicit
def initialize():
    db = setup_database()
    if db.error:
        return db

    config = load_config(db)
    if config.error:
        return config

    cache = start_cache(config)
    return cache
```

### Pattern 4: Strategy Pattern (Eliminate Special Cases)

**Before**:
```python
# Special cases accumulating
def calculate_price(item, user):
    base = item.price

    if user.type == "admin":
        discount = base * 1.0  # Free
    elif user.type == "premium":
        discount = base * 0.8
    elif user.type == "trial" and user.trial_days > 7:
        discount = base * 0.9
    elif user.type == "trial":
        discount = base * 0.95
    else:
        discount = base

    return discount
```

**After**:
```python
# Strategy pattern - add types without changing this code
class PricingStrategy(Protocol):
    def calculate_discount(self, base_price: float) -> float: ...

def calculate_price(item, strategy: PricingStrategy):
    return strategy.calculate_discount(item.price)
```

## What to Explicitly Avoid

### Clean Code Dogma

**Arbitrary function length rules**:
- "Functions should be 5 lines"
- "Functions should fit on one screen"
- **Ousterhout**: Function length is fine if it has clear abstraction

**Excessive fragmentation**:
- Creating tiny functions for every 3-line block
- **Ousterhout**: Increases shallow modules, makes code harder to follow

**Class/module extraction for single use**:
- Creating abstraction with no reuse
- **Ousterhout**: Adds complexity without benefit

**Rules over understanding**:
- Applying rules mechanically
- **Ousterhout**: Understand principles, apply contextually

### Good Rules of Thumb (Not Dogma)

**Do consider**:
- Deep modules over shallow
- Pull complexity down
- Hide implementation details
- Make common case simple
- Document design decisions

**Don't blindly follow**:
- Arbitrary line count limits
- Always extract if code repeats
- Never have long functions
- Always use smallest classes

## Additional References

For deeper exploration:
- the `cognitive-complexity` skill\'s metrics reference - Detailed cognitive metrics
- the `cognitive-complexity` skill\'s patterns reference - Refactoring patterns catalog
- the `cognitive-complexity` skill\'s onboarding reference - Onboarding difficulty assessment

## Success Metrics

Code with low cognitive complexity:
- New developer productive quickly (<1 week)
- Changes isolated (touching few files)
- Bugs easy to locate
- Mental model matches code structure
- Modifications rarely break unrelated features

Code with high cognitive complexity:
- New developers struggle (>3 weeks)
- Changes ripple through many files
- Bugs hard to isolate
- Surprises when reading code
- Fear of changing anything

Focus on depth over shallowness, simplicity over cleverness, strategy over tactics.
