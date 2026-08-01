# Rust Error Handling Reference

Comprehensive error handling patterns for Rust applications.

## Result<T, E> and Option<T>

Rust uses `Result` for fallible operations and `Option` for optional values.

### Result Basics

```rust
// Result represents success or failure
enum Result<T, E> {
    Ok(T),
    Err(E),
}

fn parse_number(s: &str) -> Result<i32, std::num::ParseIntError> {
    s.parse()
}

// Pattern matching
match parse_number("42") {
    Ok(n) => println!("Parsed: {}", n),
    Err(e) => eprintln!("Error: {}", e),
}
```

### Option Basics

```rust
// Option represents presence or absence
enum Option<T> {
    Some(T),
    None,
}

fn find_user(id: u64) -> Option<User> {
    users.iter().find(|u| u.id == id).cloned()
}

// Pattern matching
match find_user(123) {
    Some(user) => println!("Found: {}", user.name),
    None => println!("User not found"),
}
```

### Converting Between Result and Option

```rust
// Option to Result
let opt: Option<i32> = Some(42);
let result: Result<i32, &str> = opt.ok_or("value missing");
let result: Result<i32, String> = opt.ok_or_else(|| format!("missing at {}", line));

// Result to Option
let result: Result<i32, &str> = Ok(42);
let opt: Option<i32> = result.ok();    // Discards error
let opt: Option<&str> = result.err();  // Discards success
```

## The ? Operator

The `?` operator propagates errors, returning early on `Err`.

### Basic Usage

```rust
fn read_config(path: &str) -> Result<Config, io::Error> {
    let content = fs::read_to_string(path)?;  // Returns Err if fails
    let config = parse_config(&content)?;      // Returns Err if fails
    Ok(config)
}

// Equivalent without ?
fn read_config_verbose(path: &str) -> Result<Config, io::Error> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return Err(e),
    };
    let config = match parse_config(&content) {
        Ok(c) => c,
        Err(e) => return Err(e),
    };
    Ok(config)
}
```

### ? in main()

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = read_config("config.toml")?;
    run_app(config)?;
    Ok(())
}

// Or with anyhow
fn main() -> anyhow::Result<()> {
    let config = read_config("config.toml")?;
    run_app(config)?;
    Ok(())
}
```

### ? with Option

```rust
fn get_first_word_length(text: Option<&str>) -> Option<usize> {
    let text = text?;                    // Returns None if None
    let first = text.split_whitespace().next()?;
    Some(first.len())
}
```

### Automatic From Conversion

`?` automatically converts errors using the `From` trait:

```rust
#[derive(Debug)]
enum AppError {
    Io(io::Error),
    Parse(serde_json::Error),
}

impl From<io::Error> for AppError {
    fn from(e: io::Error) -> Self {
        AppError::Io(e)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Parse(e)
    }
}

fn load_data(path: &str) -> Result<Data, AppError> {
    let content = fs::read_to_string(path)?;  // io::Error -> AppError
    let data = serde_json::from_str(&content)?;  // serde_json::Error -> AppError
    Ok(data)
}
```

## Combinators

Transform and chain Results and Options without explicit matching.

### map and map_err

```rust
// Transform success value
let result: Result<i32, &str> = Ok(5);
let doubled: Result<i32, &str> = result.map(|n| n * 2);  // Ok(10)

// Transform error value
let result: Result<i32, &str> = Err("failed");
let with_context: Result<i32, String> = result.map_err(|e| format!("Error: {}", e));
```

### and_then (flatMap)

```rust
// Chain fallible operations (same error type required)
fn parse_positive(s: &str) -> Result<u32, &'static str> {
    s.parse::<i32>()
        .map_err(|_| "invalid number")
        .and_then(|n| {
            if n > 0 {
                Ok(n as u32)
            } else {
                Err("must be positive")
            }
        })
}

// Often cleaner with ?
fn parse_positive_with_question(s: &str) -> Result<u32, &'static str> {
    let n: i32 = s.parse().map_err(|_| "invalid number")?;
    if n > 0 {
        Ok(n as u32)
    } else {
        Err("must be positive")
    }
}
```

### unwrap Variants

```rust
let opt: Option<i32> = Some(42);

// Panic if None/Err (use sparingly)
let n = opt.unwrap();
let n = opt.expect("value should exist");

// Provide default
let n = opt.unwrap_or(0);
let n = opt.unwrap_or_else(|| compute_default());
let n = opt.unwrap_or_default();  // Requires Default trait

// Return Option/Result itself
let n = opt.ok_or("missing")?;  // Convert to Result
```

### transpose

```rust
// Option<Result<T, E>> <-> Result<Option<T>, E>
let x: Option<Result<i32, &str>> = Some(Ok(5));
let y: Result<Option<i32>, &str> = x.transpose();  // Ok(Some(5))

let x: Option<Result<i32, &str>> = Some(Err("fail"));
let y: Result<Option<i32>, &str> = x.transpose();  // Err("fail")
```

## Custom Error Types

Define application-specific errors for clarity and type safety.

### Basic Error Enum

```rust
#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    Unauthorized,
    BadRequest(String),
    Internal(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::NotFound(resource) => write!(f, "not found: {}", resource),
            ApiError::Unauthorized => write!(f, "unauthorized"),
            ApiError::BadRequest(msg) => write!(f, "bad request: {}", msg),
            ApiError::Internal(msg) => write!(f, "internal error: {}", msg),
        }
    }
}

impl std::error::Error for ApiError {}
```

### Wrapping Other Errors

```rust
#[derive(Debug)]
pub enum DataError {
    Database(sqlx::Error),
    Serialization(serde_json::Error),
    Validation(String),
}

impl std::error::Error for DataError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DataError::Database(e) => Some(e),
            DataError::Serialization(e) => Some(e),
            DataError::Validation(_) => None,
        }
    }
}

impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataError::Database(e) => write!(f, "database error: {}", e),
            DataError::Serialization(e) => write!(f, "serialization error: {}", e),
            DataError::Validation(msg) => write!(f, "validation error: {}", msg),
        }
    }
}
```

## thiserror for Library Code

The `thiserror` crate reduces error boilerplate.

### Basic Usage

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file not found: {path}")]
    NotFound { path: String },

    #[error("invalid config format")]
    InvalidFormat(#[from] toml::de::Error),

    #[error("missing required field: {0}")]
    MissingField(String),

    #[error("io error")]
    Io(#[from] std::io::Error),
}
```

### Advanced Features

```rust
#[derive(Debug, Error)]
pub enum AppError {
    // #[from] implements From trait
    #[error("database error")]
    Database(#[from] sqlx::Error),

    // #[source] marks the error source without From
    #[error("failed to process request")]
    Processing {
        #[source]
        cause: std::io::Error,
        context: String,
    },

    // Transparent wrapping
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
```

## anyhow for Application Code

The `anyhow` crate simplifies error handling in applications.

### Basic Usage

```rust
use anyhow::{Result, Context};

fn read_config(path: &str) -> Result<Config> {
    let content = fs::read_to_string(path)
        .context("failed to read config file")?;

    let config: Config = toml::from_str(&content)
        .context("failed to parse config")?;

    Ok(config)
}

fn main() -> Result<()> {
    let config = read_config("config.toml")?;
    run_app(config)?;
    Ok(())
}
```

### Adding Context

```rust
use anyhow::{Context, Result};

fn process_file(path: &str) -> Result<Data> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read file: {}", path))?;

    let data = parse(&content)
        .with_context(|| format!("failed to parse {}", path))?;

    Ok(data)
}
```

### bail! and ensure!

```rust
use anyhow::{bail, ensure, Result};

fn validate(value: i32) -> Result<()> {
    // Early return with error
    if value < 0 {
        bail!("value must be non-negative, got {}", value);
    }

    // Assert-like with error
    ensure!(value < 100, "value must be less than 100, got {}", value);

    Ok(())
}
```

### Downcasting

```rust
fn handle_error(err: anyhow::Error) {
    // Try to downcast to specific type
    if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
        match io_err.kind() {
            std::io::ErrorKind::NotFound => {
                println!("File not found");
            }
            _ => println!("IO error: {}", io_err),
        }
    } else {
        println!("Error: {}", err);
    }
}
```

## Error Patterns

### When to Use What

| Situation | Approach |
|-----------|----------|
| Library crate | Custom errors with `thiserror` |
| Application | `anyhow::Result` |
| Public API boundaries | Custom error types |
| Internal functions | `anyhow` or `Result<T, Box<dyn Error>>` |
| FFI boundaries | Error codes |

### Error Context Chain

```rust
use anyhow::{Context, Result};

fn load_user_data(user_id: u64) -> Result<UserData> {
    let path = get_data_path(user_id)
        .context("failed to determine data path")?;

    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    let data = serde_json::from_str(&content)
        .context("failed to parse user data")?;

    Ok(data)
}

// Error chain:
// Error: failed to read /data/users/123.json
//
// Caused by:
//     0: failed to determine data path
//     1: environment variable DATA_DIR not set
```

### Recoverable vs Fatal Errors

```rust
fn process_batch(items: Vec<Item>) -> Result<Vec<Output>> {
    let mut outputs = Vec::new();
    let mut errors = Vec::new();

    for item in items {
        match process_item(&item) {
            Ok(output) => outputs.push(output),
            Err(e) => {
                // Log but continue
                errors.push((item.id, e));
            }
        }
    }

    if !errors.is_empty() {
        tracing::warn!("failed to process {} items", errors.len());
    }

    Ok(outputs)
}
```

### Must-Use Errors

```rust
// Compiler warns if Result is unused
#[must_use]
fn important_operation() -> Result<(), Error> {
    // ...
}

// Explicit ignore when intentional
let _ = important_operation();
```
