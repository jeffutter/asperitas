# Rust Data Validation

Rust's type system enables "parse, don't validate" patterns through newtypes, `TryFrom`, and validation libraries.

## Serde for Deserialization

serde handles parsing external data at boundaries:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub name: Option<String>,
}

// Deserialize JSON
let request: CreateUserRequest = serde_json::from_str(json_body)?;

// Deserialize with validation (see below)
let validated: ValidatedCreateUser = serde_json::from_str(json_body)?;
```

### Serde Attributes for Validation

```rust
#[derive(Deserialize)]
pub struct Config {
    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(rename = "serverName")]
    pub server_name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional_field: Option<String>,

    // Parse during deserialization
    #[serde(try_from = "String")]
    pub email: Email,
}

fn default_port() -> u16 {
    8080
}
```

### try_from for Parsing During Deserialization

```rust
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

#[derive(Debug, Clone, Serialize)]
pub struct Email(String);

impl TryFrom<String> for Email {
    type Error = ValidationError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let normalized = s.trim().to_lowercase();
        if normalized.contains('@') && normalized.len() >= 5 {
            Ok(Email(normalized))
        } else {
            Err(ValidationError::InvalidEmail(s))
        }
    }
}

impl<'de> Deserialize<'de> for Email {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Email::try_from(s).map_err(serde::de::Error::custom)
    }
}

// Now Email is validated during JSON parsing
#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub email: Email,  // Automatically validated
    pub password: String,
}
```

## validator Crate

The `validator` crate provides derive macros for declarative validation:

```rust
use validator::{Validate, ValidationError};

#[derive(Debug, Validate, Deserialize)]
pub struct CreateUserRequest {
    #[validate(email(message = "Invalid email format"))]
    pub email: String,

    #[validate(length(min = 8, message = "Password must be at least 8 characters"))]
    pub password: String,

    #[validate(length(min = 2, max = 100))]
    pub name: String,

    #[validate(range(min = 0, max = 150))]
    pub age: Option<u8>,

    #[validate(url)]
    pub website: Option<String>,

    #[validate(custom(function = "validate_username"))]
    pub username: String,
}

fn validate_username(username: &str) -> Result<(), ValidationError> {
    if username.chars().all(|c| c.is_alphanumeric() || c == '_') {
        Ok(())
    } else {
        Err(ValidationError::new("invalid_username"))
    }
}

// Usage
fn create_user(request: CreateUserRequest) -> Result<User, AppError> {
    request.validate()?;  // Returns ValidationErrors if invalid

    // Proceed with validated data
    ...
}
```

### Nested Validation

```rust
#[derive(Debug, Validate, Deserialize)]
pub struct Order {
    #[validate]
    pub shipping_address: Address,

    #[validate]
    #[validate(length(min = 1, message = "Order must have at least one item"))]
    pub items: Vec<OrderItem>,
}

#[derive(Debug, Validate, Deserialize)]
pub struct Address {
    #[validate(length(min = 1))]
    pub street: String,

    #[validate(length(min = 1))]
    pub city: String,

    #[validate(length(equal = 5))]
    pub postal_code: String,
}

#[derive(Debug, Validate, Deserialize)]
pub struct OrderItem {
    pub product_id: i64,

    #[validate(range(min = 1))]
    pub quantity: u32,
}
```

### Custom Validation

```rust
#[derive(Debug, Validate, Deserialize)]
#[validate(schema(function = "validate_date_range"))]
pub struct DateRange {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
}

fn validate_date_range(range: &DateRange) -> Result<(), ValidationError> {
    if range.start_date > range.end_date {
        let mut error = ValidationError::new("invalid_range");
        error.message = Some("End date must be after start date".into());
        return Err(error);
    }
    Ok(())
}
```

## Newtype Pattern

Create types that can only hold valid values:

```rust
use std::convert::TryFrom;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email(String);

#[derive(Debug, Error)]
pub enum EmailError {
    #[error("Invalid email format: {0}")]
    InvalidFormat(String),
    #[error("Email cannot be empty")]
    Empty,
}

impl Email {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Email {
    type Error = EmailError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s.is_empty() {
            return Err(EmailError::Empty);
        }

        let normalized = s.trim().to_lowercase();

        if !normalized.contains('@') || normalized.len() < 5 {
            return Err(EmailError::InvalidFormat(s));
        }

        Ok(Email(normalized))
    }
}

impl TryFrom<&str> for Email {
    type Error = EmailError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Email::try_from(s.to_string())
    }
}
```

### Numeric Newtypes

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositiveInt(i32);

impl TryFrom<i32> for PositiveInt {
    type Error = &'static str;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        if value > 0 {
            Ok(PositiveInt(value))
        } else {
            Err("Value must be positive")
        }
    }
}

impl PositiveInt {
    pub fn get(&self) -> i32 {
        self.0
    }
}

// Usage
let quantity = PositiveInt::try_from(5)?;  // Ok
let invalid = PositiveInt::try_from(-1)?;  // Err
```

### Money Type

```rust
use rust_decimal::Decimal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Money {
    amount: Decimal,
    currency: Currency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Currency {
    USD,
    EUR,
    GBP,
}

impl Money {
    pub fn new(amount: Decimal, currency: Currency) -> Result<Self, MoneyError> {
        if amount < Decimal::ZERO {
            return Err(MoneyError::NegativeAmount);
        }
        Ok(Self { amount, currency })
    }

    pub fn add(&self, other: &Money) -> Result<Money, MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch);
        }
        Money::new(self.amount + other.amount, self.currency)
    }

    pub fn amount(&self) -> Decimal {
        self.amount
    }

    pub fn currency(&self) -> Currency {
        self.currency
    }
}
```

## Builder Pattern with Validation

For complex objects with many optional fields:

```rust
#[derive(Debug)]
pub struct User {
    email: Email,
    name: String,
    age: Option<u8>,
    role: Role,
}

#[derive(Default)]
pub struct UserBuilder {
    email: Option<String>,
    name: Option<String>,
    age: Option<u8>,
    role: Option<Role>,
}

impl UserBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn age(mut self, age: u8) -> Self {
        self.age = Some(age);
        self
    }

    pub fn role(mut self, role: Role) -> Self {
        self.role = Some(role);
        self
    }

    pub fn build(self) -> Result<User, ValidationError> {
        let email = self.email
            .ok_or(ValidationError::missing("email"))?;
        let email = Email::try_from(email)
            .map_err(|e| ValidationError::invalid("email", e))?;

        let name = self.name
            .ok_or(ValidationError::missing("name"))?;
        if name.len() < 2 || name.len() > 100 {
            return Err(ValidationError::invalid("name", "must be 2-100 characters"));
        }

        if let Some(age) = self.age {
            if age > 150 {
                return Err(ValidationError::invalid("age", "must be less than 150"));
            }
        }

        Ok(User {
            email,
            name,
            age: self.age,
            role: self.role.unwrap_or(Role::User),
        })
    }
}

// Usage
let user = UserBuilder::new()
    .email("user@example.com")
    .name("Alice")
    .age(30)
    .build()?;
```

## Accumulating Errors

Collect all errors instead of failing on first:

```rust
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct ValidationErrors {
    errors: HashMap<String, Vec<String>>,
}

impl ValidationErrors {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, field: &str, message: &str) {
        self.errors
            .entry(field.to_string())
            .or_default()
            .push(message.to_string());
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn into_result<T>(self, value: T) -> Result<T, Self> {
        if self.is_empty() {
            Ok(value)
        } else {
            Err(self)
        }
    }
}

pub fn validate_create_user(input: &CreateUserInput) -> Result<ValidatedUser, ValidationErrors> {
    let mut errors = ValidationErrors::new();

    // Email validation
    let email = match Email::try_from(input.email.clone()) {
        Ok(e) => Some(e),
        Err(e) => {
            errors.add("email", &e.to_string());
            None
        }
    };

    // Password validation
    if input.password.len() < 8 {
        errors.add("password", "must be at least 8 characters");
    }
    if !input.password.chars().any(|c| c.is_uppercase()) {
        errors.add("password", "must contain an uppercase letter");
    }

    // Name validation
    if input.name.is_empty() {
        errors.add("name", "is required");
    } else if input.name.len() > 100 {
        errors.add("name", "must be at most 100 characters");
    }

    // Return accumulated errors or validated data
    if errors.is_empty() {
        Ok(ValidatedUser {
            email: email.unwrap(),
            password: input.password.clone(),
            name: input.name.clone(),
        })
    } else {
        Err(errors)
    }
}
```

## API Request Validation Pattern

Combine serde, validator, and custom types:

```rust
use axum::{extract::Json, http::StatusCode, response::IntoResponse};
use serde::Deserialize;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateUserRequest {
    #[validate(email)]
    pub email: String,

    #[validate(length(min = 8))]
    pub password: String,

    #[validate(length(min = 2, max = 100))]
    pub name: String,
}

async fn create_user(
    Json(request): Json<CreateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Serde already parsed JSON
    // Now validate
    request.validate()?;

    // Parse into domain types
    let email = Email::try_from(request.email)?;

    // Create user with validated types
    let user = user_service.create(email, &request.password, &request.name).await?;

    Ok((StatusCode::CREATED, Json(user)))
}

// Error handling
impl From<validator::ValidationErrors> for AppError {
    fn from(errors: validator::ValidationErrors) -> Self {
        AppError::Validation(errors)
    }
}
```

## Best Practices

### 1. Validate at Boundaries

```rust
// HTTP handler - the boundary
async fn create_user(Json(input): Json<CreateUserInput>) -> Result<Json<User>, AppError> {
    input.validate()?;
    let validated = ValidatedCreateUser::try_from(input)?;
    let user = service.create(validated).await?;
    Ok(Json(user))
}

// Service - trusts validated types
impl UserService {
    async fn create(&self, input: ValidatedCreateUser) -> Result<User, ServiceError> {
        // No validation needed - types guarantee correctness
        ...
    }
}
```

### 2. Use Newtypes for Domain Concepts

```rust
// Instead of primitive types
fn send_email(to: &str, subject: &str, body: &str)

// Use domain types
fn send_email(to: &Email, subject: &Subject, body: &EmailBody)
```

### 3. Make Invalid States Unrepresentable

```rust
// Bad - can represent invalid state
struct Order {
    status: OrderStatus,
    shipped_at: Option<DateTime>,  // Should only be Some if status == Shipped
}

// Good - states are explicit
enum Order {
    Draft { items: Vec<Item> },
    Submitted { items: Vec<Item>, submitted_at: DateTime },
    Shipped { items: Vec<Item>, submitted_at: DateTime, shipped_at: DateTime },
}
```

### 4. Combine serde + validator + TryFrom

```rust
#[derive(Deserialize, Validate)]
struct RawInput {
    #[validate(email)]
    email: String,
    // ... basic validation via validator
}

struct ValidatedInput {
    email: Email,  // Domain type via TryFrom
}

impl TryFrom<RawInput> for ValidatedInput {
    type Error = ValidationError;

    fn try_from(raw: RawInput) -> Result<Self, Self::Error> {
        raw.validate()?;
        Ok(Self {
            email: Email::try_from(raw.email)?,
        })
    }
}
```
