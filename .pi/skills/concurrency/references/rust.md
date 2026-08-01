# Rust Concurrency

Rust's ownership system prevents data races at compile time. The async ecosystem (primarily tokio) provides efficient async I/O.

## Async Runtime: tokio

tokio is the dominant async runtime for Rust:

```rust
use tokio;

#[tokio::main]
async fn main() {
    let result = fetch_data().await;
    println!("Got: {:?}", result);
}

async fn fetch_data() -> Result<String, Error> {
    let response = reqwest::get("https://api.example.com/data").await?;
    response.text().await.map_err(Into::into)
}
```

### Task Spawning

```rust
use tokio::task;

// Spawn a task - runs concurrently
let handle = task::spawn(async {
    expensive_computation().await
});

// Wait for result
let result = handle.await?;

// Spawn blocking work on dedicated thread pool
let result = task::spawn_blocking(|| {
    blocking_io_operation()
}).await?;
```

### Concurrent Operations

```rust
use tokio::join;

// Run multiple futures concurrently
let (user, orders, reviews) = join!(
    fetch_user(user_id),
    fetch_orders(user_id),
    fetch_reviews(user_id)
);

// Or use futures::future::join_all for dynamic collections
use futures::future::join_all;

let urls = vec!["url1", "url2", "url3"];
let results = join_all(urls.iter().map(|url| fetch_url(url))).await;
```

### Select: Racing Futures

```rust
use tokio::select;

// First future to complete wins
select! {
    result = fetch_primary() => {
        println!("Primary: {:?}", result);
    }
    result = fetch_fallback() => {
        println!("Fallback: {:?}", result);
    }
    _ = tokio::time::sleep(Duration::from_secs(5)) => {
        println!("Timeout!");
    }
}
```

## Channels

Message passing between tasks:

### mpsc (Multiple Producer, Single Consumer)

```rust
use tokio::sync::mpsc;

// Create channel with buffer
let (tx, mut rx) = mpsc::channel::<Message>(100);

// Spawn producer
let tx_clone = tx.clone();
tokio::spawn(async move {
    tx_clone.send(Message::new("hello")).await.unwrap();
});

// Consumer
while let Some(msg) = rx.recv().await {
    process(msg);
}
```

### oneshot (Single Value)

```rust
use tokio::sync::oneshot;

// One-time response channel
let (tx, rx) = oneshot::channel();

tokio::spawn(async move {
    let result = compute().await;
    tx.send(result).unwrap();
});

// Wait for single response
let result = rx.await?;
```

### broadcast (Multiple Consumers)

```rust
use tokio::sync::broadcast;

let (tx, _rx) = broadcast::channel(16);

// Each subscribe() creates a new receiver
let mut rx1 = tx.subscribe();
let mut rx2 = tx.subscribe();

tx.send("message").unwrap();

// Both receivers get the message
let msg1 = rx1.recv().await?;
let msg2 = rx2.recv().await?;
```

### watch (Latest Value)

```rust
use tokio::sync::watch;

// Holds latest value, receivers see most recent
let (tx, rx) = watch::channel("initial");

// Update value
tx.send("updated").unwrap();

// Get current value
let current = *rx.borrow();

// Wait for changes
let mut rx = rx.clone();
while rx.changed().await.is_ok() {
    println!("New value: {}", *rx.borrow());
}
```

## Shared State

### Arc (Atomic Reference Counting)

Share ownership across threads:

```rust
use std::sync::Arc;

let data = Arc::new(vec![1, 2, 3]);

let data_clone = Arc::clone(&data);
tokio::spawn(async move {
    println!("Data: {:?}", data_clone);
});
```

### Mutex (Mutual Exclusion)

Protect mutable shared state:

```rust
use tokio::sync::Mutex;
use std::sync::Arc;

let counter = Arc::new(Mutex::new(0));

let counter_clone = Arc::clone(&counter);
tokio::spawn(async move {
    let mut lock = counter_clone.lock().await;
    *lock += 1;
});
```

**Note:** Use `tokio::sync::Mutex` for async code, `std::sync::Mutex` for sync code.

### RwLock (Read-Write Lock)

For read-heavy workloads:

```rust
use tokio::sync::RwLock;
use std::sync::Arc;

let data = Arc::new(RwLock::new(HashMap::new()));

// Multiple readers can access concurrently
let read_guard = data.read().await;
let value = read_guard.get("key");

// Writers get exclusive access
let mut write_guard = data.write().await;
write_guard.insert("key", "value");
```

### When to Use What

| Primitive | Use Case |
|-----------|----------|
| `Arc<T>` | Share immutable data |
| `Arc<Mutex<T>>` | Share mutable data, contended access |
| `Arc<RwLock<T>>` | Share mutable data, read-heavy |
| Channels | Task communication, avoid shared state |

## Concurrency Patterns

### Worker Pool

```rust
use tokio::sync::mpsc;

async fn worker_pool<T, R>(
    items: Vec<T>,
    worker_fn: impl Fn(T) -> R + Send + Sync + Clone + 'static,
    max_workers: usize,
) -> Vec<R>
where
    T: Send + 'static,
    R: Send + 'static,
{
    let (tx, mut rx) = mpsc::channel(max_workers);

    let worker_fn = Arc::new(worker_fn);

    for item in items {
        let tx = tx.clone();
        let worker_fn = Arc::clone(&worker_fn);

        tokio::spawn(async move {
            let result = worker_fn(item);
            tx.send(result).await.ok();
        });
    }

    drop(tx);  // Close sender so receiver can finish

    let mut results = Vec::new();
    while let Some(result) = rx.recv().await {
        results.push(result);
    }

    results
}
```

### Semaphore for Bounded Concurrency

```rust
use tokio::sync::Semaphore;
use std::sync::Arc;

async fn fetch_all_bounded(urls: Vec<&str>, max_concurrent: usize) {
    let semaphore = Arc::new(Semaphore::new(max_concurrent));

    let tasks: Vec<_> = urls
        .into_iter()
        .map(|url| {
            let permit = Arc::clone(&semaphore);
            tokio::spawn(async move {
                let _permit = permit.acquire().await.unwrap();
                fetch_url(url).await
            })
        })
        .collect();

    for task in tasks {
        task.await.unwrap();
    }
}
```

### Graceful Shutdown

```rust
use tokio::sync::broadcast;
use tokio::signal;

async fn run_with_shutdown() {
    let (shutdown_tx, _) = broadcast::channel(1);

    let worker_shutdown = shutdown_tx.subscribe();
    let worker = tokio::spawn(worker_loop(worker_shutdown));

    // Wait for Ctrl+C
    signal::ctrl_c().await.unwrap();

    // Signal shutdown
    shutdown_tx.send(()).unwrap();

    // Wait for worker to finish
    worker.await.unwrap();
}

async fn worker_loop(mut shutdown: broadcast::Receiver<()>) {
    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                println!("Shutting down worker");
                break;
            }
            _ = do_work() => {}
        }
    }
}
```

### Actor Pattern

```rust
use tokio::sync::mpsc;

struct Actor {
    receiver: mpsc::Receiver<Message>,
    state: ActorState,
}

enum Message {
    GetValue { respond_to: oneshot::Sender<i32> },
    SetValue { value: i32 },
}

impl Actor {
    fn new(receiver: mpsc::Receiver<Message>) -> Self {
        Self {
            receiver,
            state: ActorState::default(),
        }
    }

    async fn run(mut self) {
        while let Some(msg) = self.receiver.recv().await {
            self.handle_message(msg);
        }
    }

    fn handle_message(&mut self, msg: Message) {
        match msg {
            Message::GetValue { respond_to } => {
                respond_to.send(self.state.value).ok();
            }
            Message::SetValue { value } => {
                self.state.value = value;
            }
        }
    }
}

#[derive(Clone)]
struct ActorHandle {
    sender: mpsc::Sender<Message>,
}

impl ActorHandle {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel(100);
        let actor = Actor::new(receiver);
        tokio::spawn(actor.run());
        Self { sender }
    }

    async fn get_value(&self) -> i32 {
        let (tx, rx) = oneshot::channel();
        self.sender.send(Message::GetValue { respond_to: tx }).await.ok();
        rx.await.unwrap()
    }

    async fn set_value(&self, value: i32) {
        self.sender.send(Message::SetValue { value }).await.ok();
    }
}
```

## Thread Safety Traits

### Send and Sync

```rust
// Send: Can be transferred to another thread
// Sync: Can be shared between threads (via &T)

// Arc<T> is Send + Sync if T is Send + Sync
// Mutex<T> is Send + Sync if T is Send
// Rc<T> is neither Send nor Sync
```

### Common Patterns

```rust
// Shared read-only data
let data: Arc<Vec<i32>> = Arc::new(vec![1, 2, 3]);

// Shared mutable data
let data: Arc<Mutex<Vec<i32>>> = Arc::new(Mutex::new(vec![]));

// Shared mutable data, read-heavy
let data: Arc<RwLock<HashMap<K, V>>> = Arc::new(RwLock::new(HashMap::new()));
```

## CPU-Bound Work: rayon

For data parallelism, use rayon:

```rust
use rayon::prelude::*;

// Parallel iteration
let sum: i32 = (0..1000)
    .into_par_iter()
    .map(|i| expensive_computation(i))
    .sum();

// Parallel map
let results: Vec<_> = items
    .par_iter()
    .map(|item| process(item))
    .collect();
```

**Note:** Don't mix rayon with tokio tasks without care. Use `spawn_blocking` to run rayon work from async context.

## Best Practices

### 1. Prefer Channels Over Shared State

```rust
// Good: Message passing
let (tx, rx) = mpsc::channel(100);

// Avoid when possible: Shared state
let data = Arc::new(Mutex::new(value));
```

### 2. Use the Right Mutex

```rust
// In async code
use tokio::sync::Mutex;

// In sync code or when lock is held briefly
use std::sync::Mutex;
```

### 3. Avoid Holding Locks Across Await Points

```rust
// BAD: Lock held across await
let mut guard = data.lock().await;
expensive_async_operation().await;  // Lock still held!
*guard += 1;

// GOOD: Release lock before await
let value = {
    let guard = data.lock().await;
    guard.clone()
};
expensive_async_operation().await;
let mut guard = data.lock().await;
*guard = value + 1;
```

### 4. Handle Cancellation

```rust
// Tasks can be cancelled - clean up properly
tokio::select! {
    result = operation() => {
        // Completed
    }
    _ = cancellation_token.cancelled() => {
        // Cleanup before exit
    }
}
```

### 5. Set Timeouts

```rust
use tokio::time::timeout;

match timeout(Duration::from_secs(5), operation()).await {
    Ok(result) => handle_result(result),
    Err(_) => handle_timeout(),
}
```

### 6. Bound Channel Sizes

```rust
// Bounded channel provides backpressure
let (tx, rx) = mpsc::channel(100);  // Buffer of 100

// Unbounded channel can cause memory issues
let (tx, rx) = mpsc::unbounded_channel();  // Use carefully
```
