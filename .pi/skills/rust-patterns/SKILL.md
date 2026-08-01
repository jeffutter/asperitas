---
name: rust-patterns
description: Rust-specific language patterns including ownership, lifetimes, traits, and common idioms
---

# Rust Patterns

This skill covers patterns specific to the Rust language. For universal concepts, see the cross-references below.

## Cross-References

For cross-cutting concerns implemented in Rust, see these universal skills:

- **Error handling**: the `error-handling` skill and the `rust-patterns` skill\'s error-handling reference
- **Concurrency**: the `concurrency` skill and the `rust-patterns` skill\'s concurrency reference
- **Domain-Driven Design**: the `domain-driven-design` skill
- **Data validation**: the `data-validation` skill

## 1. Ownership Patterns

Rust's ownership system determines how to structure data and function signatures.

### Move, Borrow, Clone - When to Use Each

```rust
// Move: transfer ownership when caller doesn't need value anymore
fn consume_string(s: String) {
    println!("{}", s);
    // s is dropped here
}

// Borrow: read access without ownership transfer
fn read_string(s: &String) {
    println!("{}", s);
}

// Borrow mutably: modify without ownership transfer
fn modify_string(s: &mut String) {
    s.push_str(" modified");
}

// Clone: when you need independent copies
fn process(data: &ExpensiveData) -> ExpensiveData {
    let mut copy = data.clone();
    copy.transform();
    copy
}
```

**Decision Guide:**

| Need | Pattern |
|------|---------|
| Read-only access | `&T` |
| Modify in place | `&mut T` |
| Caller done with value | Take by value (move) |
| Both need the value | `Clone` (or `Arc` for shared ownership) |
| Performance-critical small types | `Copy` |

### Function Signature Patterns

```rust
// Take ownership when storing or transforming
impl Collection {
    fn add(&mut self, item: String) {
        self.items.push(item);  // Needs ownership to store
    }
}

// Borrow when just reading
impl Collection {
    fn contains(&self, item: &str) -> bool {
        self.items.iter().any(|i| i == item)
    }
}

// Return owned when creating new data
fn create_greeting(name: &str) -> String {
    format!("Hello, {}!", name)
}

// Return reference when data already exists
fn first(&self) -> Option<&T> {
    self.items.first()
}
```

### Cow<T> for Clone-on-Write

`Cow` (Clone on Write) delays cloning until mutation is needed:

```rust
use std::borrow::Cow;

fn process_text(input: &str) -> Cow<str> {
    if input.contains("bad") {
        // Only allocate when modification needed
        Cow::Owned(input.replace("bad", "good"))
    } else {
        // Return borrowed reference
        Cow::Borrowed(input)
    }
}

// Use when most inputs don't need modification
fn normalize_path(path: &str) -> Cow<str> {
    if path.starts_with("./") {
        Cow::Owned(path[2..].to_string())
    } else {
        Cow::Borrowed(path)
    }
}
```

### Interior Mutability

For mutation through shared references:

```rust
use std::cell::{Cell, RefCell};

// Cell<T> for Copy types - cheap, no runtime checking
struct Counter {
    count: Cell<u32>,
}

impl Counter {
    fn increment(&self) {  // Note: &self not &mut self
        self.count.set(self.count.get() + 1);
    }
}

// RefCell<T> for non-Copy types - runtime borrow checking
struct Cache {
    data: RefCell<HashMap<String, String>>,
}

impl Cache {
    fn get_or_insert(&self, key: &str, value: String) -> String {
        let mut data = self.data.borrow_mut();
        data.entry(key.to_string())
            .or_insert(value)
            .clone()
    }
}
```

**Warning:** `RefCell` panics on borrow violations at runtime. Use sparingly.

## 2. Lifetime Annotations

Explicit lifetimes communicate reference relationships.

### When Lifetimes Are Needed

```rust
// Multiple input references - must specify output lifetime
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}

// Input reference doesn't relate to output - no annotation needed
fn first_char(s: &str) -> char {
    s.chars().next().unwrap()
}

// Struct holding references
struct Parser<'a> {
    input: &'a str,
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, position: 0 }
    }

    fn remaining(&self) -> &'a str {
        &self.input[self.position..]
    }
}
```

### Lifetime Bounds

```rust
// T must live at least as long as 'a
struct Wrapper<'a, T: 'a> {
    value: &'a T,
}

// T must be 'static (owned or static reference)
fn spawn_task<T: Send + 'static>(task: T) {
    std::thread::spawn(move || {
        // task is moved into thread
    });
}

// Multiple lifetimes
fn split<'a, 'b>(s: &'a str, delimiter: &'b str) -> impl Iterator<Item = &'a str> {
    s.split(delimiter)
}
```

### Common Lifetime Patterns

```rust
// Self-referential won't compile - use indices or Pin
struct Bad<'a> {
    data: String,
    slice: &'a str,  // Can't reference data
}

// Instead, use indices
struct Good {
    data: String,
    start: usize,
    end: usize,
}

impl Good {
    fn slice(&self) -> &str {
        &self.data[self.start..self.end]
    }
}
```

## 3. Trait Objects vs Generics

Choose based on performance needs and design constraints.

### Generics (Static Dispatch)

```rust
// Monomorphized at compile time - zero runtime cost
fn process<T: Display>(item: T) {
    println!("{}", item);
}

// Each concrete type gets its own function
process(42);       // process::<i32>
process("hello");  // process::<&str>

// Impl Trait for cleaner signatures
fn create_iter() -> impl Iterator<Item = i32> {
    (1..10).filter(|n| n % 2 == 0)
}
```

**Advantages:** Zero-cost, compiler optimizations, no heap allocation.
**Disadvantages:** Larger binary (code duplication), can't have heterogeneous collections.

### Trait Objects (Dynamic Dispatch)

```rust
// Runtime dispatch via vtable
fn process_any(item: &dyn Display) {
    println!("{}", item);
}

// Heterogeneous collection
let items: Vec<Box<dyn Display>> = vec![
    Box::new(42),
    Box::new("hello"),
    Box::new(3.14),
];

for item in &items {
    println!("{}", item);
}
```

**Advantages:** Smaller binary, heterogeneous collections, runtime flexibility.
**Disadvantages:** Virtual call overhead, heap allocation, limited by object safety.

### Object Safety

A trait is object-safe if:
- No `Self` in return position (except through `Box<Self>`)
- No generic methods
- No associated functions (only methods with `&self`)

```rust
// Object-safe
trait Draw {
    fn draw(&self);
}

// NOT object-safe
trait Clone {
    fn clone(&self) -> Self;  // Self in return position
}

trait Generic {
    fn process<T>(&self, value: T);  // Generic method
}
```

## 4. Enum-Based State Machines

Rust enums model state machines with compile-time guarantees.

### Basic State Machine

```rust
enum ConnectionState {
    Disconnected,
    Connecting { attempt: u32, started_at: Instant },
    Connected { socket: TcpStream },
    Failed { error: String, retries: u32 },
}

struct Connection {
    state: ConnectionState,
}

impl Connection {
    fn connect(&mut self, addr: &str) -> io::Result<()> {
        match &self.state {
            ConnectionState::Disconnected => {
                self.state = ConnectionState::Connecting {
                    attempt: 1,
                    started_at: Instant::now(),
                };
                self.try_connect(addr)
            }
            ConnectionState::Failed { retries, .. } if *retries < 3 => {
                self.state = ConnectionState::Connecting {
                    attempt: retries + 1,
                    started_at: Instant::now(),
                };
                self.try_connect(addr)
            }
            _ => Err(io::Error::new(
                io::ErrorKind::Other,
                "invalid state for connect",
            )),
        }
    }

    fn send(&mut self, data: &[u8]) -> io::Result<()> {
        match &mut self.state {
            ConnectionState::Connected { socket } => {
                socket.write_all(data)
            }
            _ => Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "not connected",
            )),
        }
    }
}
```

### Exhaustive State Handling

The compiler ensures all states are handled:

```rust
fn state_name(state: &ConnectionState) -> &'static str {
    match state {
        ConnectionState::Disconnected => "disconnected",
        ConnectionState::Connecting { .. } => "connecting",
        ConnectionState::Connected { .. } => "connected",
        ConnectionState::Failed { .. } => "failed",
        // Compiler error if new variant added without handling
    }
}
```

## 5. Builder Pattern

Build complex objects incrementally with a fluent API.

### Basic Builder

```rust
#[derive(Debug)]
pub struct Config {
    host: String,
    port: u16,
    timeout: Duration,
    retries: u32,
}

#[derive(Default)]
pub struct ConfigBuilder {
    host: Option<String>,
    port: Option<u16>,
    timeout: Option<Duration>,
    retries: Option<u32>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn retries(mut self, retries: u32) -> Self {
        self.retries = Some(retries);
        self
    }

    pub fn build(self) -> Result<Config, &'static str> {
        Ok(Config {
            host: self.host.ok_or("host is required")?,
            port: self.port.unwrap_or(8080),
            timeout: self.timeout.unwrap_or(Duration::from_secs(30)),
            retries: self.retries.unwrap_or(3),
        })
    }
}

// Usage
let config = ConfigBuilder::new()
    .host("localhost")
    .port(3000)
    .timeout(Duration::from_secs(60))
    .build()?;
```

### Consuming vs Non-Consuming Builder

```rust
// Consuming (takes self) - shown above
// Cannot reuse builder after each method call
// More common, enables move optimizations

// Non-consuming (takes &mut self) - allows reuse
impl ConfigBuilder {
    pub fn host(&mut self, host: impl Into<String>) -> &mut Self {
        self.host = Some(host.into());
        self
    }
    // ...
}

// Usage with non-consuming
let mut builder = ConfigBuilder::new();
builder.host("localhost").port(3000);
let config1 = builder.clone().build()?;
builder.port(4000);
let config2 = builder.build()?;
```

## 6. Newtype Pattern

Wrap types for type safety with zero runtime cost.

### Basic Newtypes

```rust
// Prevent accidental mixing of IDs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OrderId(u64);

// Functions accept only the correct type
fn get_user(id: UserId) -> Option<User> { /* ... */ }
fn get_order(id: OrderId) -> Option<Order> { /* ... */ }

// Compile error: can't mix them up
// get_user(OrderId(123));  // Error!

impl UserId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn into_inner(self) -> u64 {
        self.0
    }
}
```

### Newtypes for Validation

```rust
#[derive(Debug, Clone)]
pub struct Email(String);

impl Email {
    pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        if value.contains('@') && value.contains('.') {
            Ok(Self(value))
        } else {
            Err("invalid email format")
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// Email is always valid once constructed
fn send_email(to: &Email, body: &str) {
    // No need to validate - Email type guarantees validity
}
```

### Implementing Traits on Newtypes

```rust
use std::fmt;

#[derive(Clone)]
pub struct Meters(f64);

impl fmt::Display for Meters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}m", self.0)
    }
}

impl std::ops::Add for Meters {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Meters(self.0 + other.0)
    }
}

// Deref for transparent access (use sparingly)
impl std::ops::Deref for Meters {
    type Target = f64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
```

## 7. Typestate Pattern

Encode states in the type system for compile-time validation.

### Basic Typestate

```rust
use std::marker::PhantomData;

// State markers (zero-sized types)
pub struct Draft;
pub struct Published;
pub struct Archived;

pub struct Article<State> {
    title: String,
    content: String,
    _state: PhantomData<State>,
}

impl Article<Draft> {
    pub fn new(title: String) -> Self {
        Article {
            title,
            content: String::new(),
            _state: PhantomData,
        }
    }

    pub fn set_content(&mut self, content: String) {
        self.content = content;
    }

    // Only drafts can be published
    pub fn publish(self) -> Article<Published> {
        Article {
            title: self.title,
            content: self.content,
            _state: PhantomData,
        }
    }
}

impl Article<Published> {
    // Only published articles can be archived
    pub fn archive(self) -> Article<Archived> {
        Article {
            title: self.title,
            content: self.content,
            _state: PhantomData,
        }
    }

    // Can return to draft
    pub fn unpublish(self) -> Article<Draft> {
        Article {
            title: self.title,
            content: self.content,
            _state: PhantomData,
        }
    }
}

// Usage
let draft = Article::<Draft>::new("My Post".to_string());
let published = draft.publish();
// draft.publish();  // Error: draft was moved
// published.set_content(...);  // Error: no set_content on Published
let archived = published.archive();
```

### Typestate for Protocol Enforcement

```rust
pub struct Unvalidated;
pub struct Validated;

pub struct Form<State> {
    email: String,
    password: String,
    _state: PhantomData<State>,
}

impl Form<Unvalidated> {
    pub fn new(email: String, password: String) -> Self {
        Form {
            email,
            password,
            _state: PhantomData,
        }
    }

    pub fn validate(self) -> Result<Form<Validated>, ValidationError> {
        if !self.email.contains('@') {
            return Err(ValidationError::InvalidEmail);
        }
        if self.password.len() < 8 {
            return Err(ValidationError::PasswordTooShort);
        }
        Ok(Form {
            email: self.email,
            password: self.password,
            _state: PhantomData,
        })
    }
}

impl Form<Validated> {
    // Only validated forms can be submitted
    pub fn submit(self) -> Result<UserId, SubmitError> {
        // Safe to use - validation guaranteed by type system
        create_user(&self.email, &self.password)
    }
}

// API requires validated form - impossible to call with unvalidated
fn register(form: Form<Validated>) -> UserId {
    form.submit().unwrap()
}
```

## Additional References

- the `rust-patterns` skill\'s error-handling reference
- the `rust-patterns` skill\'s concurrency reference
- the `rust-patterns` skill\'s algorithms reference
- the rust LANGUAGE.md reference
- the rust tooling.md reference
