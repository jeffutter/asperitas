---
name: domain-driven-design
description: Use when organizing code by business domain, designing bounded contexts, creating aggregate roots, or establishing clear module boundaries
---

# Domain-Driven Design Patterns

Organize code around business domains, not technical layers.

## When to Use

- Starting new projects or major features
- Refactoring monolithic code
- Designing module/package boundaries
- Establishing team ownership boundaries
- Reducing coupling between subsystems
- Code is organized by technical layer (models/, services/, controllers/) instead of business capability

## Core Concepts

### The Problem with Technical Layers

```
# Anti-pattern: Technical layering
src/
  models/
    user.py
    order.py
    product.py
  services/
    user_service.py
    order_service.py
    product_service.py
  repositories/
    user_repository.py
    order_repository.py
    product_repository.py
```

**Problems:**
- Related code is scattered across directories
- Changes to a feature touch many folders
- Unclear what capabilities the system has
- Hard to reason about feature scope

### Bounded Contexts

A bounded context is a logical boundary where terms have specific meanings and code has clear ownership:

```
# Good: Organize by business capability
src/
  accounts/           # Bounded context: User management
    __init__.py       # Public API
    user.py           # Internal entity
    authentication.py # Internal service
    repository.py     # Internal data access

  orders/             # Bounded context: Order processing
    __init__.py       # Public API
    order.py
    fulfillment.py
    repository.py

  products/           # Bounded context: Product catalog
    __init__.py
    product.py
    inventory.py
```

**Each context:**
- Owns its data and business rules
- Exposes a public API (`__init__.py`)
- Hides implementation details
- Can be developed independently

### Public API Module

The context's `__init__.py` (or equivalent) defines what other contexts can use:

```python
# accounts/__init__.py - Public API
from .user import User  # Export only what's needed externally
from .authentication import AuthResult

def register_user(email: str, password: str) -> Result[User, RegistrationError]:
    """Register a new user account."""
    ...

def authenticate(email: str, password: str) -> Result[AuthResult, AuthError]:
    """Authenticate user credentials."""
    ...

def get_user(user_id: UserId) -> Result[User, NotFoundError]:
    """Fetch user by ID."""
    ...

# Private functions - NOT exported
def _hash_password(password: str) -> str:
    ...

def _send_verification_email(user: User) -> None:
    ...
```

**Guidelines:**
- Export business operations, not CRUD
- Return domain types, not internal schemas
- Hide implementation details (private functions, internal entities)

### Aggregates and Aggregate Roots

An aggregate is a cluster of entities treated as a single unit. The aggregate root is the entry point:

```python
class Order:  # Aggregate root
    """Order is the aggregate root - all access goes through it."""

    def __init__(self, id: OrderId, customer_id: CustomerId):
        self.id = id
        self.customer_id = customer_id
        self._items: list[OrderItem] = []  # Internal, managed by Order
        self._status = OrderStatus.DRAFT

    def add_item(self, product_id: ProductId, quantity: int, price: Money):
        """Business logic lives in the aggregate."""
        if self._status != OrderStatus.DRAFT:
            raise InvalidOperationError("Cannot modify submitted order")

        if quantity <= 0:
            raise ValidationError("Quantity must be positive")

        # Invariant: no duplicate products
        existing = self._find_item(product_id)
        if existing:
            existing.quantity += quantity
        else:
            self._items.append(OrderItem(product_id, quantity, price))

    def submit(self) -> None:
        """Transition to submitted state."""
        if not self._items:
            raise ValidationError("Cannot submit empty order")
        self._status = OrderStatus.SUBMITTED

    @property
    def total(self) -> Money:
        return sum(item.subtotal for item in self._items)
```

**Aggregate rules:**
- External code only interacts with the root
- The root enforces invariants
- Entities inside are only accessible through the root
- One aggregate = one transaction boundary

### Entities vs Value Objects

| Aspect | Entity | Value Object |
|--------|--------|--------------|
| Identity | Has unique ID | No identity |
| Equality | Equal by ID | Equal by value |
| Mutability | Can change over time | Immutable |
| Example | User, Order, Product | Money, Address, Email |

```python
# Entity - identity matters
class User:
    def __init__(self, id: UserId, email: Email):
        self.id = id
        self.email = email

    def __eq__(self, other):
        return isinstance(other, User) and self.id == other.id

# Value Object - value matters, immutable
@dataclass(frozen=True)
class Money:
    amount: Decimal
    currency: str

    def __add__(self, other: "Money") -> "Money":
        if self.currency != other.currency:
            raise ValueError("Currency mismatch")
        return Money(self.amount + other.amount, self.currency)

@dataclass(frozen=True)
class Address:
    street: str
    city: str
    postal_code: str
    country: str
```

### Context Communication

Contexts communicate through their public APIs:

```python
# orders/fulfillment.py
from accounts import get_user  # Use public API
from products import get_product, reserve_inventory

def create_order(user_id: UserId, items: list[OrderItemRequest]) -> Result[Order, OrderError]:
    # Fetch from other contexts via their public APIs
    user_result = get_user(user_id)
    if isinstance(user_result, Err):
        return Err(OrderError.UserNotFound(user_id))

    user = user_result.value

    # Validate products exist and have inventory
    for item in items:
        product_result = get_product(item.product_id)
        if isinstance(product_result, Err):
            return Err(OrderError.ProductNotFound(item.product_id))

        reserve_result = reserve_inventory(item.product_id, item.quantity)
        if isinstance(reserve_result, Err):
            return Err(OrderError.InsufficientInventory(item.product_id))

    # Create order in our context
    order = Order(generate_id(), user.id)
    for item in items:
        order.add_item(item.product_id, item.quantity, item.price)

    return Ok(order)
```

**Never cross context boundaries at the database level:**

```python
# BAD - crossing context boundaries with joins
def get_user_orders(user_id: int):
    return db.query("""
        SELECT o.*, u.name
        FROM orders o
        JOIN users u ON o.user_id = u.id
        WHERE u.id = ?
    """, user_id)

# GOOD - use public APIs
def get_user_orders(user_id: UserId):
    user = accounts.get_user(user_id)
    orders = orders.list_for_user(user_id)
    return UserOrdersDTO(user=user, orders=orders)
```

### Domain Events

Decouple contexts with events:

```python
# accounts/events.py
@dataclass(frozen=True)
class UserRegistered:
    user_id: UserId
    email: str
    registered_at: datetime

# accounts/__init__.py
def register_user(email: str, password: str) -> Result[User, RegistrationError]:
    ...
    # Publish event for other contexts
    event_bus.publish(UserRegistered(
        user_id=user.id,
        email=user.email,
        registered_at=datetime.utcnow()
    ))
    return Ok(user)

# notifications/handlers.py
@event_handler(UserRegistered)
def send_welcome_email(event: UserRegistered):
    send_email(event.email, "Welcome!", ...)
```

## Trade-offs

### When DDD Helps

- Multiple teams working on same codebase
- Complex business logic
- Long-lived projects
- Need for clear ownership boundaries

### When DDD May Be Overkill

- Simple CRUD applications
- Small teams (1-3 developers)
- Short-lived prototypes
- Well-understood, stable domains

### Bounded Context Size

| Size | Characteristics | Risk |
|------|-----------------|------|
| Too small | Many contexts, high coordination overhead | Over-engineering |
| Too large | Monolith in disguise, unclear boundaries | Under-engineering |
| Right-sized | Clear business capability, independent deployment | - |

**Heuristic:** A context should map to a business capability that a single team can own.

## Anti-Patterns

### 1. God Context

```python
# BAD - everything in one context
class Core:
    def create_user(self): ...
    def create_order(self): ...
    def create_product(self): ...
    def process_payment(self): ...
    def send_notification(self): ...
    # 500+ methods...
```

**Fix:** Split by business capability.

### 2. Technical Layering

```
# BAD
models/
services/
repositories/
controllers/

# GOOD
accounts/
orders/
products/
payments/
```

### 3. Anemic Domain Model

```python
# BAD - data with no behavior
@dataclass
class Order:
    id: OrderId
    items: list[OrderItem]
    status: str

# Business logic scattered in services
class OrderService:
    def add_item(self, order: Order, item: OrderItem): ...
    def submit(self, order: Order): ...
    def cancel(self, order: Order): ...
```

**Fix:** Put behavior in domain objects:

```python
# GOOD - rich domain model
class Order:
    def add_item(self, item: OrderItem): ...
    def submit(self): ...
    def cancel(self): ...
```

### 4. Leaky Abstractions

```python
# BAD - exposing internal schema
def get_user(id: int) -> UserRow:  # Database row leaked
    return db.query("SELECT * FROM users WHERE id = ?", id)

# GOOD - return domain object
def get_user(id: UserId) -> Result[User, NotFoundError]:
    row = db.query("SELECT * FROM users WHERE id = ?", id)
    if not row:
        return Err(NotFoundError("User", id))
    return Ok(User.from_row(row))
```

### 5. Cross-Context Database Queries

```python
# BAD - joining across context tables
SELECT o.*, p.name, u.email
FROM orders o
JOIN products p ON o.product_id = p.id
JOIN users u ON o.user_id = u.id

# GOOD - each context queries its own data
# Then aggregate in application code
```

### 6. Shared Database Tables

```python
# BAD - multiple contexts write to same table
# accounts context
db.execute("UPDATE users SET name = ? WHERE id = ?", name, id)

# orders context
db.execute("UPDATE users SET order_count = ? WHERE id = ?", count, id)

# GOOD - each context owns its tables
# accounts owns users table
# orders has order_statistics table with user_id foreign key
```

## Testing Bounded Contexts

```python
# Test context public API
class TestAccounts:
    def test_register_user_success(self):
        result = accounts.register_user("test@example.com", "password123")
        assert isinstance(result, Ok)
        assert result.value.email == "test@example.com"

    def test_register_duplicate_email(self):
        accounts.register_user("test@example.com", "password123")
        result = accounts.register_user("test@example.com", "password456")
        assert isinstance(result, Err)
        assert isinstance(result.error, EmailAlreadyExists)

    def test_authenticate_valid_credentials(self):
        accounts.register_user("test@example.com", "password123")
        result = accounts.authenticate("test@example.com", "password123")
        assert isinstance(result, Ok)

# Test cross-context integration
class TestOrderCreation:
    def test_create_order_with_valid_user_and_products(self):
        user = accounts.register_user("test@example.com", "pass").value
        product = products.create_product("Widget", Money(10, "USD")).value

        result = orders.create_order(user.id, [
            OrderItemRequest(product.id, quantity=2)
        ])

        assert isinstance(result, Ok)
        assert result.value.total == Money(20, "USD")
```

## Refactoring to DDD

### Step 1: Identify Bounded Contexts

Look for:
- Groups of related entities
- Teams or ownership boundaries
- Distinct business capabilities
- Natural transaction boundaries

### Step 2: Define Public APIs

For each context:
- List operations other code needs
- Define input/output types
- Hide everything else

### Step 3: Move Code

```python
# Before: scattered
models/user.py
services/user_service.py
repositories/user_repository.py

# After: cohesive
accounts/
  __init__.py      # register_user, authenticate, get_user
  user.py          # User entity
  authentication.py # Auth logic
  repository.py    # Data access
```

### Step 4: Fix Cross-Context Dependencies

Replace direct database access with public API calls.

## Additional References

- the `domain-driven-design` skill\'s elixir reference - Phoenix contexts, public API patterns
- the `domain-driven-design` skill\'s rust reference - Module visibility, workspace organization
