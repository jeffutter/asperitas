# Rust Concurrency Reference

Comprehensive concurrency patterns for Rust applications.

## Send and Sync Traits

Rust's type system enforces thread safety through marker traits.

### Understanding Send and Sync

```rust
// Send: safe to transfer ownership between threads
// A type T is Send if it can be moved to another thread

// Sync: safe to share references between threads
// A type T is Sync if &T is Send (multiple threads can have &T)
```

### Automatic Implementation

Most types are automatically Send + Sync:

| Type | Send | Sync | Why |
|------|------|------|-----|
| `i32`, `bool`, etc. | Yes | Yes | No interior mutability |
| `String`, `Vec<T>` | Yes | Yes | Owned, no shared state |
| `Arc<T>` | Yes | Yes | Atomic reference counting |
| `Mutex<T>` | Yes | Yes | Synchronized access |
| `Rc<T>` | No | No | Non-atomic reference counting |
| `RefCell<T>` | Yes | No | Runtime borrow checking not thread-safe |
| `*const T`, `*mut T` | No | No | Raw pointers are unsafe |

### Manual Implementation

```rust
// Marking a type as thread-safe (requires unsafe)
struct MyType {
    // ...internal fields that are actually thread-safe
}

// SAFETY: MyType's internal synchronization makes it safe
unsafe impl Send for MyType {}
unsafe impl Sync for MyType {}

// Opting out of Send/Sync
struct NotThreadSafe {
    data: *mut u8,
    _marker: std::marker::PhantomData<*mut ()>,  // Makes it !Send + !Sync
}
```

## Shared State Patterns

### Arc<Mutex<T>>

The standard pattern for shared mutable state:

```rust
use std::sync::{Arc, Mutex};
use std::thread;

fn main() {
    let counter = Arc::new(Mutex::new(0));
    let mut handles = vec![];

    for _ in 0..10 {
        let counter = Arc::clone(&counter);
        let handle = thread::spawn(move || {
            let mut num = counter.lock().unwrap();
            *num += 1;
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    println!("Result: {}", *counter.lock().unwrap());
}
```

### Lock Poisoning

```rust
use std::sync::{Arc, Mutex};

let data = Arc::new(Mutex::new(vec![1, 2, 3]));

// Handle poisoned lock
match data.lock() {
    Ok(guard) => {
        // Use guard
    }
    Err(poisoned) => {
        // Lock was held by a panicked thread
        // Recover the data (may be in inconsistent state)
        let guard = poisoned.into_inner();
        // Decide what to do...
    }
}

// Or ignore poisoning if data is always valid
let guard = data.lock().unwrap_or_else(|e| e.into_inner());
```

### Deadlock Prevention

```rust
// WRONG: Potential deadlock
fn transfer_bad(from: &Mutex<Account>, to: &Mutex<Account>, amount: u64) {
    let mut from_guard = from.lock().unwrap();
    let mut to_guard = to.lock().unwrap();  // Deadlock if another thread does reverse
    // ...
}

// CORRECT: Consistent lock ordering
fn transfer_good(from: &Mutex<Account>, to: &Mutex<Account>, amount: u64) {
    // Order by memory address for consistent ordering
    let (first, second) = if std::ptr::addr_of!(*from) < std::ptr::addr_of!(*to) {
        (from, to)
    } else {
        (to, from)
    };

    let mut first_guard = first.lock().unwrap();
    let mut second_guard = second.lock().unwrap();
    // ...
}
```

### RwLock for Read-Heavy Workloads

```rust
use std::sync::{Arc, RwLock};

let config = Arc::new(RwLock::new(Config::default()));

// Multiple readers allowed
let reader1 = config.read().unwrap();
let reader2 = config.read().unwrap();  // OK: multiple readers

// Writer blocks all others
let mut writer = config.write().unwrap();  // Waits for readers to finish
writer.update();

// Use when:
// - Reads far outnumber writes
// - Reads are long-running
// Avoid when:
// - Writes are frequent (write starvation)
```

## Async Runtime: Tokio

### Basic Setup

```rust
use tokio::runtime::Runtime;

// Using macro
#[tokio::main]
async fn main() {
    do_async_work().await;
}

// Manual runtime
fn main() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        do_async_work().await;
    });
}

// Current-thread runtime (single-threaded)
#[tokio::main(flavor = "current_thread")]
async fn main() {
    // All tasks run on current thread
}

// Multi-threaded with specific worker count
#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    // Uses 4 worker threads
}
```

### Spawning Tasks

```rust
use tokio::task;

#[tokio::main]
async fn main() {
    // Spawn a task (runs concurrently)
    let handle = task::spawn(async {
        // This runs on any worker thread
        expensive_computation().await
    });

    // Do other work...

    // Wait for result
    let result = handle.await.unwrap();

    // Spawn many tasks
    let handles: Vec<_> = (0..10)
        .map(|i| task::spawn(async move { process(i).await }))
        .collect();

    // Wait for all
    let results: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();
}
```

### spawn_blocking for CPU Work

```rust
use tokio::task;

async fn process_file(path: &str) -> Result<Data, Error> {
    let path = path.to_string();

    // Move CPU-intensive work off async runtime
    let data = task::spawn_blocking(move || {
        let content = std::fs::read_to_string(&path)?;
        parse_large_file(&content)  // CPU-bound
    }).await??;

    Ok(data)
}
```

## Channels

### mpsc: Multiple Producer, Single Consumer

```rust
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    // Bounded channel (backpressure)
    let (tx, mut rx) = mpsc::channel::<Message>(100);

    // Multiple producers
    for i in 0..3 {
        let tx = tx.clone();
        tokio::spawn(async move {
            tx.send(Message::new(i)).await.unwrap();
        });
    }

    // Drop original sender
    drop(tx);

    // Single consumer
    while let Some(msg) = rx.recv().await {
        process(msg).await;
    }
    // recv returns None when all senders dropped
}
```

### oneshot: Single Value

```rust
use tokio::sync::oneshot;

async fn request_response() {
    let (tx, rx) = oneshot::channel();

    // Spawn responder
    tokio::spawn(async move {
        let result = compute().await;
        tx.send(result).unwrap();
    });

    // Wait for response
    let result = rx.await.unwrap();
}
```

### broadcast: Multi-Consumer

```rust
use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    let (tx, _rx) = broadcast::channel::<Event>(100);

    // Multiple receivers
    let mut rx1 = tx.subscribe();
    let mut rx2 = tx.subscribe();

    // Sender
    tokio::spawn(async move {
        tx.send(Event::Update).unwrap();
        tx.send(Event::Shutdown).unwrap();
    });

    // All receivers get all messages
    tokio::spawn(async move {
        while let Ok(event) = rx1.recv().await {
            handle_event(event).await;
        }
    });

    while let Ok(event) = rx2.recv().await {
        handle_event(event).await;
    }
}
```

### watch: Latest Value

```rust
use tokio::sync::watch;

#[tokio::main]
async fn main() {
    let (tx, mut rx) = watch::channel(Config::default());

    // Receiver gets latest value
    tokio::spawn(async move {
        loop {
            rx.changed().await.unwrap();
            let config = rx.borrow().clone();
            apply_config(config);
        }
    });

    // Sender updates value
    tx.send(Config::new()).unwrap();
}
```

## tokio::select!

Race multiple futures, handling the first to complete.

```rust
use tokio::select;
use tokio::time::{sleep, Duration};

async fn with_timeout() -> Option<Data> {
    select! {
        data = fetch_data() => Some(data),
        _ = sleep(Duration::from_secs(5)) => {
            println!("Timeout!");
            None
        }
    }
}

// Multiple branches
async fn handle_events(
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
    mut messages: mpsc::Receiver<Message>,
) {
    loop {
        select! {
            _ = shutdown.recv() => {
                println!("Shutting down");
                break;
            }
            Some(msg) = messages.recv() => {
                process(msg).await;
            }
            else => {
                // All channels closed
                break;
            }
        }
    }
}

// Biased for deterministic behavior
async fn biased_select() {
    select! {
        biased;  // Check branches in order
        _ = high_priority() => {}
        _ = low_priority() => {}
    }
}
```

## Data Parallelism: rayon

### Basic Usage

```rust
use rayon::prelude::*;

fn main() {
    let numbers: Vec<i32> = (0..1_000_000).collect();

    // Parallel iteration
    let sum: i32 = numbers.par_iter().sum();

    // Parallel map
    let doubled: Vec<i32> = numbers.par_iter().map(|n| n * 2).collect();

    // Parallel filter
    let evens: Vec<i32> = numbers.par_iter()
        .filter(|n| *n % 2 == 0)
        .cloned()
        .collect();
}
```

### join for Parallel Work

```rust
use rayon::join;

fn parallel_sum(data: &[i32]) -> i32 {
    if data.len() <= 1000 {
        data.iter().sum()
    } else {
        let mid = data.len() / 2;
        let (left, right) = data.split_at(mid);
        let (sum_left, sum_right) = join(
            || parallel_sum(left),
            || parallel_sum(right),
        );
        sum_left + sum_right
    }
}
```

### Custom Thread Pool

```rust
use rayon::ThreadPoolBuilder;

fn main() {
    let pool = ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();

    pool.install(|| {
        // All rayon operations in this closure use this pool
        let sum: i32 = (0..1000).into_par_iter().sum();
    });
}
```

### When to Use rayon vs tokio

| Use Case | Tool |
|----------|------|
| CPU-bound computation | rayon |
| I/O-bound operations | tokio |
| Data parallelism | rayon |
| Network services | tokio |
| Parallel iteration | rayon |
| Async/await | tokio |

## Atomic Operations

### Basic Atomics

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn increment() -> usize {
    COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn get() -> usize {
    COUNTER.load(Ordering::SeqCst)
}
```

### Memory Ordering

| Ordering | Use Case |
|----------|----------|
| `Relaxed` | Counter that doesn't synchronize other data |
| `Acquire` | Reading data written by another thread |
| `Release` | Writing data to be read by another thread |
| `AcqRel` | Both acquire and release |
| `SeqCst` | When in doubt (strongest guarantee) |

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

// Simple atomic flag pattern
static REQUEST_COUNT: AtomicUsize = AtomicUsize::new(0);

fn count_request() {
    // Relaxed is fine for counters not synchronizing other data
    REQUEST_COUNT.fetch_add(1, Ordering::Relaxed);
}

fn get_count() -> usize {
    REQUEST_COUNT.load(Ordering::Relaxed)
}

// For lazy initialization, prefer OnceLock (see below)
```

## Synchronization Primitives

### Barrier

```rust
use std::sync::{Arc, Barrier};
use std::thread;

fn main() {
    let barrier = Arc::new(Barrier::new(4));
    let mut handles = vec![];

    for i in 0..4 {
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            println!("Thread {} before barrier", i);
            barrier.wait();  // All threads wait here
            println!("Thread {} after barrier", i);
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }
}
```

### Condvar

```rust
use std::sync::{Arc, Mutex, Condvar};

fn producer_consumer() {
    let pair = Arc::new((Mutex::new(Vec::new()), Condvar::new()));
    let pair_clone = Arc::clone(&pair);

    // Producer
    thread::spawn(move || {
        let (lock, cvar) = &*pair_clone;
        let mut queue = lock.lock().unwrap();
        queue.push(1);
        cvar.notify_one();
    });

    // Consumer
    let (lock, cvar) = &*pair;
    let mut queue = lock.lock().unwrap();
    while queue.is_empty() {
        queue = cvar.wait(queue).unwrap();
    }
    let item = queue.pop();
}
```

### OnceLock for Lazy Initialization

```rust
use std::sync::OnceLock;

static CONFIG: OnceLock<Config> = OnceLock::new();

fn get_config() -> &'static Config {
    CONFIG.get_or_init(|| load_config())
}

// With fallible initialization
static DB_POOL: OnceLock<Pool> = OnceLock::new();

fn get_pool() -> Result<&'static Pool, Error> {
    DB_POOL.get_or_try_init(|| create_pool())
}

// LazyLock for simpler cases (Rust 1.80+)
use std::sync::LazyLock;

static REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\d+").unwrap()
});
```
