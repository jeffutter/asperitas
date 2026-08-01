# Rust Domain-Driven Design

Rust's module system and visibility controls provide strong support for bounded contexts and information hiding.

## Module Visibility

Rust's visibility system enforces context boundaries at compile time:

```rust
// By default, items are private
struct InternalState { ... }  // Private to module

// pub makes it public to the crate
pub struct User { ... }

// pub(crate) - visible within crate only
pub(crate) fn internal_helper() { ... }

// pub(super) - visible to parent module
pub(super) struct ParentVisible { ... }

// pub(in path) - visible to specific module
pub(in crate::accounts) fn accounts_only() { ... }
```

## Bounded Context Structure

### Single Crate Organization

```
src/
  lib.rs                    # Re-exports public APIs
  accounts/
    mod.rs                  # Public API for accounts context
    user.rs                 # User entity
    authentication.rs       # Auth logic
    repository.rs           # Data access
  orders/
    mod.rs                  # Public API for orders context
    order.rs
    fulfillment.rs
  products/
    mod.rs
    product.rs
    inventory.rs
```

### Workspace Organization

For larger projects, use a Cargo workspace:

```
my_project/
  Cargo.toml                # Workspace manifest
  accounts/
    Cargo.toml
    src/lib.rs              # Context public API
  orders/
    Cargo.toml
    src/lib.rs
  products/
    Cargo.toml
    src/lib.rs
  api/                      # Application layer
    Cargo.toml
    src/main.rs
```

Workspace Cargo.toml:

```toml
[workspace]
members = ["accounts", "orders", "products", "api"]
```

## Public API Pattern

### Module as Context Boundary

```rust
// accounts/mod.rs - Public API

mod user;           // Private - internal implementation
mod authentication; // Private
mod repository;     // Private

// Re-export only what external code needs
pub use user::User;
pub use user::UserId;

// Error types
#[derive(Debug, thiserror::Error)]
pub enum AccountsError {
    #[error("User not found: {0}")]
    NotFound(UserId),

    #[error("Email already registered: {0}")]
    EmailExists(String),

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

// Public functions - business operations
pub async fn register_user(
    db: &PgPool,
    email: &str,
    password: &str,
) -> Result<User, AccountsError> {
    // Implementation uses private modules
    let hashed = authentication::hash_password(password)?;
    repository::insert_user(db, email, &hashed).await
}

pub async fn authenticate(
    db: &PgPool,
    email: &str,
    password: &str,
) -> Result<User, AccountsError> {
    let user = repository::find_by_email(db, email)
        .await?
        .ok_or(AccountsError::InvalidCredentials)?;

    if !authentication::verify_password(password, &user.password_hash)? {
        return Err(AccountsError::InvalidCredentials);
    }

    Ok(user)
}

pub async fn get_user(db: &PgPool, id: UserId) -> Result<User, AccountsError> {
    repository::find_by_id(db, id)
        .await?
        .ok_or(AccountsError::NotFound(id))
}
```

### Private Implementation

```rust
// accounts/user.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub i64);

#[derive(Debug, Clone)]
pub struct User {
    pub id: UserId,
    pub email: String,
    pub(super) password_hash: String,  // Only visible within accounts module
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// accounts/authentication.rs - Not exported

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use argon2::password_hash::SaltString;
use rand_core::OsRng;

pub(super) fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    Ok(argon2.hash_password(password.as_bytes(), &salt)?.to_string())
}

pub(super) fn verify_password(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed_hash = PasswordHash::new(hash)?;
    Ok(Argon2::default().verify_password(password.as_bytes(), &parsed_hash).is_ok())
}

// accounts/repository.rs - Not exported

use super::{User, UserId};
use sqlx::PgPool;

pub(super) async fn insert_user(
    db: &PgPool,
    email: &str,
    password_hash: &str,
) -> Result<User, sqlx::Error> {
    sqlx::query_as!(
        User,
        r#"INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING *"#,
        email,
        password_hash
    )
    .fetch_one(db)
    .await
}

pub(super) async fn find_by_id(db: &PgPool, id: UserId) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(User, "SELECT * FROM users WHERE id = $1", id.0)
        .fetch_optional(db)
        .await
}

pub(super) async fn find_by_email(db: &PgPool, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(User, "SELECT * FROM users WHERE email = $1", email)
        .fetch_optional(db)
        .await
}
```

## Cross-Context Communication

### Via Public APIs

```rust
// orders/mod.rs

use accounts::{AccountsError, UserId, User};
use products::{ProductId, Product, ProductsError};

#[derive(Debug, thiserror::Error)]
pub enum OrdersError {
    #[error("User error: {0}")]
    User(#[from] AccountsError),

    #[error("Product error: {0}")]
    Product(#[from] ProductsError),

    #[error("Insufficient inventory for product {0}")]
    InsufficientInventory(ProductId),
}

pub async fn create_order(
    db: &PgPool,
    user_id: UserId,
    items: Vec<OrderItemRequest>,
) -> Result<Order, OrdersError> {
    // Use other contexts via their public APIs
    let _user = accounts::get_user(db, user_id).await?;

    for item in &items {
        let product = products::get_product(db, item.product_id).await?;

        if !products::check_inventory(db, item.product_id, item.quantity).await? {
            return Err(OrdersError::InsufficientInventory(item.product_id));
        }
    }

    // Create order in our context
    let order = repository::create_order(db, user_id, &items).await?;

    // Reserve inventory
    for item in &items {
        products::reserve_inventory(db, item.product_id, item.quantity).await?;
    }

    Ok(order)
}
```

## Traits for Context Interfaces

Define traits to decouple contexts:

```rust
// Define interface in shared module or consuming context
pub trait UserRepository {
    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, Error>;
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, Error>;
}

// Implement in accounts context
impl UserRepository for AccountsService {
    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, Error> {
        repository::find_by_id(&self.db, id).await
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, Error> {
        repository::find_by_email(&self.db, email).await
    }
}

// Other contexts depend on trait, not concrete implementation
pub struct OrderService<U: UserRepository> {
    user_repo: U,
    db: PgPool,
}
```

## Value Objects with Newtype Pattern

```rust
// Strongly-typed IDs prevent mixing up different ID types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrderId(pub i64);

// Compiler prevents: find_order(user_id) when OrderId is expected

// Value objects
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email(String);

impl Email {
    pub fn new(s: &str) -> Result<Self, ValidationError> {
        if s.contains('@') && s.len() >= 5 {
            Ok(Email(s.to_lowercase()))
        } else {
            Err(ValidationError::InvalidEmail(s.to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Money {
    amount: Decimal,
    currency: Currency,
}

impl Money {
    pub fn new(amount: Decimal, currency: Currency) -> Self {
        Self { amount, currency }
    }

    pub fn add(&self, other: &Money) -> Result<Money, CurrencyMismatch> {
        if self.currency != other.currency {
            return Err(CurrencyMismatch);
        }
        Ok(Money::new(self.amount + other.amount, self.currency))
    }
}
```

## Aggregates

```rust
pub struct Order {
    id: OrderId,
    user_id: UserId,
    items: Vec<OrderItem>,
    status: OrderStatus,
    created_at: DateTime<Utc>,
}

impl Order {
    // Factory method - the only way to create an Order
    pub fn new(id: OrderId, user_id: UserId) -> Self {
        Self {
            id,
            user_id,
            items: Vec::new(),
            status: OrderStatus::Draft,
            created_at: Utc::now(),
        }
    }

    // Aggregate root controls modifications
    pub fn add_item(
        &mut self,
        product_id: ProductId,
        quantity: u32,
        price: Money,
    ) -> Result<(), OrderError> {
        if self.status != OrderStatus::Draft {
            return Err(OrderError::CannotModify);
        }

        if quantity == 0 {
            return Err(OrderError::InvalidQuantity);
        }

        // Invariant: no duplicate products
        if let Some(item) = self.items.iter_mut().find(|i| i.product_id == product_id) {
            item.quantity += quantity;
        } else {
            self.items.push(OrderItem {
                product_id,
                quantity,
                price,
            });
        }

        Ok(())
    }

    pub fn submit(&mut self) -> Result<(), OrderError> {
        if self.items.is_empty() {
            return Err(OrderError::EmptyOrder);
        }

        if self.status != OrderStatus::Draft {
            return Err(OrderError::InvalidStateTransition);
        }

        self.status = OrderStatus::Submitted;
        Ok(())
    }

    pub fn total(&self) -> Money {
        self.items
            .iter()
            .map(|item| item.price.multiply(item.quantity))
            .fold(Money::zero(Currency::USD), |acc, m| acc.add(&m).unwrap())
    }
}
```

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_user() {
        let db = setup_test_db().await;

        let result = register_user(&db, "test@example.com", "password123").await;

        assert!(result.is_ok());
        let user = result.unwrap();
        assert_eq!(user.email, "test@example.com");
    }

    #[tokio::test]
    async fn test_authenticate_valid_credentials() {
        let db = setup_test_db().await;
        register_user(&db, "test@example.com", "password123").await.unwrap();

        let result = authenticate(&db, "test@example.com", "password123").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_authenticate_invalid_password() {
        let db = setup_test_db().await;
        register_user(&db, "test@example.com", "password123").await.unwrap();

        let result = authenticate(&db, "test@example.com", "wrongpassword").await;

        assert!(matches!(result, Err(AccountsError::InvalidCredentials)));
    }
}
```

## Best Practices

### 1. Use Visibility to Enforce Boundaries

```rust
// Public API
pub fn register_user(...) -> Result<User, Error>

// Internal implementation
pub(crate) fn validate_email(...)
fn hash_password(...)  // Private to module
```

### 2. Re-export Only What's Needed

```rust
// mod.rs
pub use user::User;
pub use user::UserId;
// Don't export: user::PasswordHash, internal types
```

### 3. Separate Error Types Per Context

```rust
// Each context has its own error type
pub enum AccountsError { ... }
pub enum OrdersError { ... }
pub enum ProductsError { ... }

// Cross-context errors use From
impl From<AccountsError> for OrdersError { ... }
```

### 4. Use Workspaces for Large Projects

Workspaces enforce separation at the package level, making it impossible to accidentally access private items.
