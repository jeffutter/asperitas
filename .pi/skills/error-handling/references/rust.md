# Rust Error Handling

Rust enforces explicit error handling at compile time through the type system.

## Result<T, E> and Option<T>

Rust's standard library provides two fundamental types for handling absence and failure:

### Result<T, E>

For operations that can fail:

```rust
enum Result<T, E> {
    Ok(T),   // Success with value of type T
    Err(E),  // Failure with error of type E
}

fn divide(a: f64, b: f64) -> Result<f64, String> {
    if b == 0.0 {
        Err("Division by zero".to_string())
    } else {
        Ok(a / b)
    }
}

// Pattern matching to handle
match divide(10.0, 2.0) {
    Ok(result) => println!("Result: {}", result),
    Err(e) => println!("Error: {}", e),
}
```

### Option<T>

For values that may or may not exist:

```rust
enum Option<T> {
    Some(T),  // Value present
    None,     // Value absent
}

fn find_user(id: u64) -> Option<User> {
    users.get(&id).cloned()  // Returns Option<User>
}

// Pattern matching
match find_user(42) {
    Some(user) => println!("Found: {}", user.name),
    None => println!("User not found"),
}
```

### Option vs Result

| Use Case | Type |
|----------|------|
| Value may be absent (no error info needed) | `Option<T>` |
| Operation can fail (error info needed) | `Result<T, E>` |
| Converting absence to error | `option.ok_or(error)` |
| Discarding error info | `result.ok()` → `Option<T>` |

## The ? Operator

The `?` operator propagates errors automatically:

```rust
fn read_config() -> Result<Config, ConfigError> {
    let file = File::open("config.toml")?;  // Returns early if Err
    let contents = read_to_string(file)?;   // Returns early if Err
    let config = parse_config(&contents)?;  // Returns early if Err
    Ok(config)
}

// Equivalent to:
fn read_config_verbose() -> Result<Config, ConfigError> {
    let file = match File::open("config.toml") {
        Ok(f) => f,
        Err(e) => return Err(e.into()),
    };
    // ... and so on
}
```

### Requirements for ?

- Function must return `Result` or `Option`
- Error types must be compatible (implement `From` trait)

### Using ? with Option

```rust
fn get_user_email(id: u64) -> Option<String> {
    let user = users.get(&id)?;      // Returns None if not found
    let profile = user.profile()?;    // Returns None if no profile
    Some(profile.email.clone())
}
```

## Combinators

### map and map_err

Transform success or error values:

```rust
let result: Result<i32, &str> = Ok(5);

// Transform success value
let doubled = result.map(|x| x * 2);  // Ok(10)

// Transform error value
let result: Result<i32, &str> = Err("error");
let detailed = result.map_err(|e| format!("Failed: {}", e));
```

### and_then (flatMap)

Chain operations that return Result:

```rust
fn parse_and_double(s: &str) -> Result<i32, ParseIntError> {
    s.parse::<i32>()
        .and_then(|n| Ok(n * 2))
}

// More complex chaining
fn process_user(id: u64) -> Result<Report, Error> {
    get_user(id)
        .and_then(|user| get_orders(&user))
        .and_then(|orders| generate_report(orders))
}
```

### unwrap_or and unwrap_or_else

Provide defaults:

```rust
// Static default
let value = result.unwrap_or(0);

// Computed default (lazy)
let value = result.unwrap_or_else(|e| {
    log::warn!("Using default due to: {}", e);
    compute_default()
});

// unwrap_or_default for types implementing Default
let value: String = result.unwrap_or_default();
```

### ok_or and ok_or_else

Convert Option to Result:

```rust
fn find_user(id: u64) -> Option<User> { ... }

fn get_user(id: u64) -> Result<User, Error> {
    find_user(id).ok_or(Error::NotFound(id))
}

// Lazy error construction
fn get_user(id: u64) -> Result<User, Error> {
    find_user(id).ok_or_else(|| Error::NotFound(id))
}
```

## Custom Error Types

### Using thiserror (Library Code)

For libraries, use `thiserror` to derive `Error` implementations:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Query failed: {query}")]
    QueryFailed {
        query: String,
        #[source]
        source: sqlx::Error,
    },

    #[error("Record not found: {resource} with id {id}")]
    NotFound {
        resource: &'static str,
        id: i64,
    },

    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),
}
```

### Using anyhow (Application Code)

For applications, use `anyhow` for ergonomic error handling:

```rust
use anyhow::{Context, Result, bail, ensure};

fn process_config(path: &str) -> Result<Config> {
    let contents = std::fs::read_to_string(path)
        .context("Failed to read config file")?;

    let config: Config = toml::from_str(&contents)
        .context("Failed to parse config")?;

    // Validation with bail!
    if config.workers == 0 {
        bail!("Workers must be at least 1");
    }

    // Validation with ensure!
    ensure!(config.port > 0, "Port must be positive");

    Ok(config)
}
```

### When to Use Which

| Crate | Use Case | Features |
|-------|----------|----------|
| `thiserror` | Library code | Derive Error, structured types, #[source] |
| `anyhow` | Application code | Context, bail!, ensure!, downcasting |
| Both | Hybrid | Define library errors with thiserror, wrap with anyhow in main |

## Error Conversion with From

Implement `From` to enable automatic conversion with `?`:

```rust
#[derive(Error, Debug)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

// Now ? automatically converts these error types
fn load_data() -> Result<Data, AppError> {
    let file = File::open("data.json")?;        // io::Error -> AppError
    let data: Data = serde_json::from_reader(file)?;  // serde_json::Error -> AppError
    Ok(data)
}
```

### Manual From Implementation

```rust
impl From<std::io::Error> for MyError {
    fn from(err: std::io::Error) -> Self {
        MyError::Io {
            message: err.to_string(),
            kind: err.kind(),
        }
    }
}
```

## Error Context

Adding context helps debugging:

```rust
use anyhow::Context;

fn process_user(id: u64) -> Result<()> {
    let user = get_user(id)
        .with_context(|| format!("Failed to get user {}", id))?;

    let orders = get_orders(&user)
        .with_context(|| format!("Failed to get orders for user {}", user.name))?;

    process_orders(orders)
        .context("Failed to process orders")?;

    Ok(())
}
```

Error output shows the chain:

```
Error: Failed to process orders

Caused by:
    0: Failed to get orders for user Alice
    1: Database connection timeout
```

## Recoverable vs Unrecoverable Errors

### Unrecoverable: panic!

For bugs and programming errors:

```rust
// Program bug - should never happen
fn get_element(vec: &[i32], index: usize) -> i32 {
    vec.get(index).expect("Index out of bounds - this is a bug")
}

// Assertion failure
assert!(user.is_valid(), "Invalid user state");

// Unreachable code
match status {
    Status::Active => ...,
    Status::Inactive => ...,
    _ => unreachable!("Unknown status"),
}
```

### Recoverable: Result

For expected failures:

```rust
fn read_file(path: &str) -> Result<String, io::Error> {
    std::fs::read_to_string(path)
}

// Caller decides how to handle
match read_file("config.toml") {
    Ok(contents) => parse(contents),
    Err(e) if e.kind() == ErrorKind::NotFound => use_defaults(),
    Err(e) => return Err(e.into()),
}
```

## Pattern: Define Errors Out of Existence

Design APIs to make errors impossible:

```rust
// BAD: Can fail at runtime
struct User {
    email: String,  // Might be invalid
}

impl User {
    fn send_email(&self) -> Result<(), EmailError> {
        // Must validate email every time
        if !is_valid_email(&self.email) {
            return Err(EmailError::InvalidEmail);
        }
        // ...
    }
}

// GOOD: Invalid state unrepresentable
struct Email(String);  // Private field

impl Email {
    pub fn new(s: &str) -> Result<Self, InvalidEmailError> {
        if is_valid_email(s) {
            Ok(Email(s.to_string()))
        } else {
            Err(InvalidEmailError(s.to_string()))
        }
    }
}

struct User {
    email: Email,  // Always valid by construction
}

impl User {
    fn send_email(&self) {
        // No validation needed - Email is always valid
        send(&self.email.0);
    }
}
```

## Error Handling Best Practices

### 1. Use Specific Error Types

```rust
// BAD: Generic error
fn process() -> Result<(), Box<dyn Error>> { ... }

// GOOD: Specific error type
fn process() -> Result<(), ProcessError> { ... }
```

### 2. Add Context at Boundaries

```rust
fn handle_request(req: Request) -> Response {
    match process_request(req) {
        Ok(data) => Response::ok(data),
        Err(e) => {
            // Log full error chain internally
            log::error!("Request failed: {:?}", e);
            // Return sanitized error to client
            Response::error(e.user_message())
        }
    }
}
```

### 3. Don't Panic in Libraries

```rust
// BAD: Library panics
pub fn parse(s: &str) -> Config {
    serde_json::from_str(s).unwrap()  // Panics on invalid input
}

// GOOD: Library returns Result
pub fn parse(s: &str) -> Result<Config, ParseError> {
    serde_json::from_str(s).map_err(ParseError::from)
}
```

### 4. Use expect Over unwrap

```rust
// BAD: No context on panic
let file = File::open(path).unwrap();

// GOOD: Explains why this should never fail
let file = File::open(path)
    .expect("Config file must exist - checked at startup");
```

### 5. Exhaustive Error Matching

```rust
match result {
    Ok(value) => use_value(value),
    Err(Error::NotFound) => handle_not_found(),
    Err(Error::Unauthorized) => handle_unauthorized(),
    Err(Error::Network(e)) => retry_or_fail(e),
    // Compiler ensures all variants handled
}
```
