# Rust Reference: Production Quality

## Overview

Rust's production quality workflow uses cargo subcommands with deny-by-default patterns. The compiler's strict type system catches many issues at compile time that other languages find at runtime.

## Compilation: cargo check / cargo build

### Difference Between check and build

```bash
# Fast type checking without generating binary
cargo check

# Full compilation
cargo build

# Release build with optimizations
cargo build --release

# Check all targets (lib, bins, tests, examples)
cargo check --all-targets
```

### Feature Flags

```toml
# Cargo.toml
[features]
default = ["json"]
json = ["serde_json"]
full = ["json", "metrics", "tracing"]

[dependencies]
serde_json = { version = "1.0", optional = true }
```

```bash
# Build with specific features
cargo build --features "json metrics"

# Build with all features
cargo build --all-features

# Build without default features
cargo build --no-default-features
```

### Common Compilation Issues

```rust
// Warning: unused variable
fn process(data: &str) {  // data is unused
    println!("processing");
}

// Fix: prefix with underscore
fn process(_data: &str) {
    println!("processing");
}

// Warning: unused import
use std::collections::HashMap;  // never used

// Fix: remove or use
use std::collections::HashMap;
let map: HashMap<String, i32> = HashMap::new();

// Warning: unreachable pattern
match value {
    Some(_) => {}
    None => {}
    _ => {}  // unreachable after None
}
```

## Formatting: cargo fmt

### rustfmt.toml Configuration

```toml
# rustfmt.toml
max_width = 100
hard_tabs = false
tab_spaces = 4
newline_style = "Auto"
use_small_heuristics = "Default"
reorder_imports = true
reorder_modules = true
remove_nested_parens = true
edition = "2021"
merge_derives = true
use_field_init_shorthand = true
force_explicit_abi = true
imports_granularity = "Crate"
group_imports = "StdExternalCrate"
```

### Common Options

```toml
# Single imports per line
imports_granularity = "Item"

# Group by crate
imports_granularity = "Crate"

# Module-level grouping
imports_granularity = "Module"

# Import grouping (std, external, crate)
group_imports = "StdExternalCrate"
```

### CI Check

```bash
# Check if formatted (non-destructive)
cargo fmt -- --check

# Format all code
cargo fmt
```

## Linting: cargo clippy

### Default vs Pedantic Lints

```bash
# Default lints
cargo clippy

# With all warnings as errors (for CI)
cargo clippy -- -D warnings

# Pedantic lints (stricter)
cargo clippy -- -W clippy::pedantic

# Nursery lints (experimental)
cargo clippy -- -W clippy::nursery
```

### Configuration (.cargo/config.toml)

```toml
[target.'cfg(all())']
rustflags = [
    "-D", "warnings",
    "-W", "clippy::pedantic",
    "-A", "clippy::module_name_repetitions",
    "-A", "clippy::must_use_candidate",
]
```

### Project-Level Configuration (Cargo.toml)

```toml
[lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"

[lints.clippy]
all = "warn"
pedantic = "warn"
nursery = "warn"
cargo = "warn"
```

### Common Clippy Fixes

```rust
// Issue: Manual implementation of Option::map
let result = match opt {
    Some(x) => Some(x + 1),
    None => None,
};

// Fix: Use map
let result = opt.map(|x| x + 1);

// Issue: Unnecessary clone
let s = String::from("hello");
let len = s.clone().len();

// Fix: Borrow instead
let len = s.len();

// Issue: Using collect then iter
let v: Vec<i32> = vec![1, 2, 3];
let doubled: Vec<i32> = v.iter().map(|x| x * 2).collect::<Vec<_>>().iter().sum();

// Fix: Chain directly
let doubled: i32 = v.iter().map(|x| x * 2).sum();

// Issue: Explicit return in closure
let f = |x| { return x + 1; };

// Fix: Implicit return
let f = |x| x + 1;
```

### Allowing/Denying Specific Lints

```rust
// Module level
#![allow(clippy::too_many_arguments)]
#![deny(unsafe_code)]

// Function level
#[allow(clippy::needless_pass_by_value)]
fn process(data: String) {
    // ...
}

// Statement level
#[allow(clippy::cast_possible_truncation)]
let small: u8 = large_number as u8;
```

## Testing: cargo test

### Unit Tests

```rust
// In src/lib.rs or src/module.rs
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(2, 3), 5);
    }

    #[test]
    fn test_add_negative() {
        assert_eq!(add(-1, 1), 0);
    }

    #[test]
    #[should_panic(expected = "overflow")]
    fn test_overflow() {
        let _ = add(i32::MAX, 1);  // Should panic
    }

    #[test]
    #[ignore]  // Skip by default, run with --ignored
    fn expensive_test() {
        // Long-running test
    }
}
```

### Integration Tests

```rust
// tests/integration_test.rs
use mylib::add;

#[test]
fn test_from_outside() {
    assert_eq!(add(10, 20), 30);
}

// Shared test utilities in tests/common/mod.rs
mod common;

#[test]
fn test_with_setup() {
    let data = common::setup();
    // ...
}
```

### Doc Tests

```rust
/// Adds two numbers together.
///
/// # Examples
///
/// ```
/// use mylib::add;
/// assert_eq!(add(2, 3), 5);
/// ```
///
/// # Panics
///
/// Panics if the result overflows.
///
/// ```should_panic
/// use mylib::add;
/// let _ = add(i32::MAX, 1);
/// ```
pub fn add(a: i32, b: i32) -> i32 {
    a.checked_add(b).expect("overflow")
}
```

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run tests in specific module
cargo test module_name::

# Run ignored tests
cargo test -- --ignored

# Run with output
cargo test -- --nocapture

# Run doc tests only
cargo test --doc

# Run integration tests only
cargo test --test integration_test
```

### Mocking with mockall

```rust
use mockall::{automock, predicate::*};

#[automock]
trait Database {
    fn get(&self, id: u32) -> Option<String>;
    fn save(&mut self, id: u32, value: String) -> Result<(), String>;
}

#[test]
fn test_with_mock() {
    let mut mock = MockDatabase::new();

    mock.expect_get()
        .with(eq(42))
        .times(1)
        .returning(|_| Some("value".to_string()));

    mock.expect_save()
        .with(eq(1), eq("new".to_string()))
        .times(1)
        .returning(|_, _| Ok(()));

    // Use mock in test
    assert_eq!(mock.get(42), Some("value".to_string()));
}
```

## Security: cargo audit

```bash
# Install
cargo install cargo-audit

# Run audit
cargo audit

# Fix vulnerabilities (if possible)
cargo audit fix

# JSON output for CI
cargo audit --json
```

### CI Integration

```yaml
- name: Security audit
  run: |
    cargo install cargo-audit
    cargo audit
```

## Documentation: cargo doc

### Rustdoc Conventions

```rust
//! # My Crate
//!
//! `my_crate` provides utilities for processing data.
//!
//! ## Example
//!
//! ```
//! use my_crate::process;
//! let result = process("input");
//! ```

/// Processes the input string.
///
/// # Arguments
///
/// * `input` - The string to process
///
/// # Returns
///
/// The processed result
///
/// # Errors
///
/// Returns `Err` if the input is empty
///
/// # Examples
///
/// ```
/// use my_crate::process;
/// assert!(process("hello").is_ok());
/// ```
pub fn process(input: &str) -> Result<String, Error> {
    // ...
}
```

### Building Documentation

```bash
# Build docs
cargo doc

# Build and open in browser
cargo doc --open

# Include private items
cargo doc --document-private-items

# Build without dependencies
cargo doc --no-deps
```

## Precommit Workflow Script

```bash
#!/usr/bin/env bash
set -euo pipefail

echo "Running precommit checks..."

echo "1. Checking compilation..."
cargo check --all-targets

echo "2. Checking formatting..."
cargo fmt -- --check

echo "3. Running Clippy..."
cargo clippy -- -D warnings

echo "4. Running tests..."
cargo test

echo "5. Security audit..."
cargo audit

echo "All checks passed!"
```

## CI Configuration (GitHub Actions)

```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Cache cargo
        uses: actions/cache@v3
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            target/
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Check
        run: cargo check --all-targets

      - name: Format
        run: cargo fmt -- --check

      - name: Clippy
        run: cargo clippy -- -D warnings

      - name: Test
        run: cargo test

      - name: Security audit
        run: |
          cargo install cargo-audit
          cargo audit

  # Optional: test on multiple Rust versions
  test-matrix:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        rust: [stable, beta, nightly]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.rust }}
      - run: cargo test
```

## Tool Summary

| Tool | Purpose | CI Flag | Notes |
|------|---------|---------|-------|
| cargo check | Fast type checking | --all-targets | No binary output |
| cargo build | Full compilation | --release | With optimizations |
| cargo fmt | Formatting | -- --check | rustfmt.toml config |
| cargo clippy | Linting | -- -D warnings | Configurable lints |
| cargo test | Testing | | Unit, integration, doc tests |
| cargo audit | Security | | Advisory database |
| cargo doc | Documentation | --no-deps | Rustdoc |
