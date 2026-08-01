# Rust Benchmark Templates (Criterion)

Ready-to-use templates for creating benchmarks with Criterion. For profiling methodology and tool selection, see the `performance-analyzer` skill\'s rust reference.

## Setup

Add Criterion to your project:

```toml
# Cargo.toml
[dev-dependencies]
criterion = "0.5"
rand = "0.8"

[[bench]]
name = "my_benchmark"
harness = false
```

Create benchmark directory:

```bash
mkdir -p benches
```

## Template: Single Function Benchmark

Basic benchmark for a single function with multiple input sizes.

```rust
// benches/module_name.rs
//
// Benchmark: my_crate::module_name::function_name
// Created: YYYY-MM-DD
// Complexity: O(n) / O(n^2) / etc.
// Purpose: [Brief description of what we're measuring]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use my_crate::module_name;
use rand::Rng;

fn generate_items(count: usize) -> Vec<Item> {
    let mut rng = rand::thread_rng();
    (0..count)
        .map(|i| Item {
            id: i,
            name: format!("Item {}", i),
            value: rng.gen_range(0..1000),
        })
        .collect()
}

fn benchmark_function(c: &mut Criterion) {
    let mut group = c.benchmark_group("function_name");

    // Input sizes matching production patterns
    for size in [100, 1_000, 10_000].iter() {
        let items = generate_items(*size);

        group.bench_with_input(
            BenchmarkId::new("function_name", size),
            &items,
            |b, items| {
                b.iter(|| module_name::function_name(black_box(items)))
            },
        );
    }

    group.finish();
}

criterion_group!(benches, benchmark_function);
criterion_main!(benches);
```

## Template: Comparison Benchmark

Compare two or more implementations.

```rust
// benches/comparison.rs
//
// Benchmark: Compare Vec vs HashMap for lookup-heavy workloads
// Created: YYYY-MM-DD
// Context: Evaluating data structure choice for order processing

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::collections::{HashMap, HashSet};
use rand::Rng;

#[derive(Clone)]
struct Order {
    id: String,
    line_items: Vec<LineItem>,
}

#[derive(Clone)]
struct LineItem {
    sku: String,
    quantity: u32,
}

fn generate_orders(count: usize, items_per_order: usize) -> Vec<Order> {
    let mut rng = rand::thread_rng();
    (0..count)
        .map(|i| Order {
            id: format!("ORD-{}", i),
            line_items: (0..items_per_order)
                .map(|_| LineItem {
                    sku: format!("SKU-{}", rng.gen_range(0..10_000)),
                    quantity: rng.gen_range(1..10),
                })
                .collect(),
        })
        .collect()
}

fn generate_inventory_skus(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("SKU-{}", i)).collect()
}

// Original O(n x m x k) approach - linear search
fn process_eager(orders: &[Order], inventory_skus: &[String]) -> Vec<Order> {
    orders
        .iter()
        .map(|order| {
            let filtered_items: Vec<_> = order
                .line_items
                .iter()
                .filter(|item| inventory_skus.contains(&item.sku))
                .cloned()
                .collect();
            Order {
                id: order.id.clone(),
                line_items: filtered_items,
            }
        })
        .collect()
}

// Optimized O(n x m) approach - HashSet lookup
fn process_with_set(orders: &[Order], inventory_skus: &[String]) -> Vec<Order> {
    let sku_set: HashSet<_> = inventory_skus.iter().collect();

    orders
        .iter()
        .map(|order| {
            let filtered_items: Vec<_> = order
                .line_items
                .iter()
                .filter(|item| sku_set.contains(&item.sku))
                .cloned()
                .collect();
            Order {
                id: order.id.clone(),
                line_items: filtered_items,
            }
        })
        .collect()
}

fn benchmark_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("Order Processing");

    let inventory_skus = generate_inventory_skus(5_000);

    for (name, order_count, items_per_order) in [
        ("small", 100, 5),
        ("typical", 1_000, 5),
        ("peak", 10_000, 10),
    ] {
        let orders = generate_orders(order_count, items_per_order);

        group.bench_with_input(
            BenchmarkId::new("eager (linear search)", name),
            &(&orders, &inventory_skus),
            |b, (orders, skus)| {
                b.iter(|| process_eager(black_box(orders), black_box(skus)))
            },
        );

        group.bench_with_input(
            BenchmarkId::new("set lookup (HashSet)", name),
            &(&orders, &inventory_skus),
            |b, (orders, skus)| {
                b.iter(|| process_with_set(black_box(orders), black_box(skus)))
            },
        );
    }

    group.finish();
}

criterion_group!(benches, benchmark_comparison);
criterion_main!(benches);
```

## Template: Before/After Optimization

Benchmark current vs optimized implementation with throughput measurement.

```rust
// benches/optimization.rs
//
// Benchmark: String processing optimization
// Created: YYYY-MM-DD
// Before: Multiple allocations per item
// After: Pre-allocated buffer with capacity
// Expected improvement: ~2-5x for large inputs

use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};

// Original implementation - allocates on each iteration
fn process_strings_original(items: &[String]) -> Vec<String> {
    items
        .iter()
        .map(|s| {
            let mut result = String::new();
            result.push_str("PREFIX_");
            result.push_str(s);
            result.push_str("_SUFFIX");
            result
        })
        .collect()
}

// Optimized implementation - pre-allocated capacity
fn process_strings_optimized(items: &[String]) -> Vec<String> {
    items
        .iter()
        .map(|s| {
            let capacity = 7 + s.len() + 7; // PREFIX_ + s + _SUFFIX
            let mut result = String::with_capacity(capacity);
            result.push_str("PREFIX_");
            result.push_str(s);
            result.push_str("_SUFFIX");
            result
        })
        .collect()
}

fn generate_strings(count: usize, avg_len: usize) -> Vec<String> {
    (0..count)
        .map(|i| format!("{:0>width$}", i, width = avg_len))
        .collect()
}

fn benchmark_optimization(c: &mut Criterion) {
    let mut group = c.benchmark_group("String Processing");

    for (name, count, avg_len) in [
        ("small", 100, 10),
        ("typical", 10_000, 50),
        ("peak", 100_000, 100),
    ] {
        let items = generate_strings(count, avg_len);

        // Set throughput for bytes/sec measurement
        let total_bytes: usize = items.iter().map(|s| s.len()).sum();
        group.throughput(Throughput::Bytes(total_bytes as u64));

        group.bench_with_input(
            BenchmarkId::new("original (realloc)", name),
            &items,
            |b, items| {
                b.iter(|| process_strings_original(black_box(items)))
            },
        );

        group.bench_with_input(
            BenchmarkId::new("optimized (pre-alloc)", name),
            &items,
            |b, items| {
                b.iter(|| process_strings_optimized(black_box(items)))
            },
        );
    }

    group.finish();
}

criterion_group!(benches, benchmark_optimization);
criterion_main!(benches);
```

## Template: Async Benchmark

Benchmark async functions with Tokio runtime.

```rust
// benches/async_benchmark.rs
//
// Benchmark: Async HTTP client performance
// Created: YYYY-MM-DD
// Purpose: Compare sequential vs concurrent request processing

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use tokio::runtime::Runtime;

async fn fetch_sequential(urls: &[String]) -> Vec<String> {
    let client = reqwest::Client::new();
    let mut results = Vec::with_capacity(urls.len());

    for url in urls {
        let resp = client.get(url).send().await.unwrap();
        results.push(resp.text().await.unwrap());
    }

    results
}

async fn fetch_concurrent(urls: &[String]) -> Vec<String> {
    let client = reqwest::Client::new();

    let futures: Vec<_> = urls
        .iter()
        .map(|url| {
            let client = client.clone();
            let url = url.clone();
            async move {
                let resp = client.get(&url).send().await.unwrap();
                resp.text().await.unwrap()
            }
        })
        .collect();

    futures::future::join_all(futures).await
}

fn benchmark_async(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("HTTP Fetching");
    group.sample_size(10); // Fewer samples for network operations

    let urls: Vec<String> = (0..10)
        .map(|i| format!("http://localhost:8080/item/{}", i))
        .collect();

    group.bench_with_input(
        BenchmarkId::new("sequential", urls.len()),
        &urls,
        |b, urls| {
            b.to_async(&rt)
                .iter(|| fetch_sequential(urls))
        },
    );

    group.bench_with_input(
        BenchmarkId::new("concurrent", urls.len()),
        &urls,
        |b, urls| {
            b.to_async(&rt)
                .iter(|| fetch_concurrent(urls))
        },
    );

    group.finish();
}

criterion_group!(benches, benchmark_async);
criterion_main!(benches);
```

## Template: Memory-Focused Benchmark

Benchmark with custom allocator tracking.

```rust
// benches/memory_benchmark.rs
//
// Benchmark: Memory allocation patterns
// Created: YYYY-MM-DD
// Purpose: Compare memory usage of different approaches

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

// Counting allocator for memory tracking
struct CountingAllocator;

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATED.fetch_add(layout.size(), Ordering::SeqCst);
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn reset_allocation_counter() {
    ALLOCATED.store(0, Ordering::SeqCst);
}

fn get_allocated_bytes() -> usize {
    ALLOCATED.load(Ordering::SeqCst)
}

// Functions to benchmark
fn create_with_push(count: usize) -> Vec<i32> {
    let mut v = Vec::new();
    for i in 0..count {
        v.push(i as i32);
    }
    v
}

fn create_with_capacity(count: usize) -> Vec<i32> {
    let mut v = Vec::with_capacity(count);
    for i in 0..count {
        v.push(i as i32);
    }
    v
}

fn create_with_collect(count: usize) -> Vec<i32> {
    (0..count as i32).collect()
}

fn benchmark_memory(c: &mut Criterion) {
    let mut group = c.benchmark_group("Vec Creation Memory");

    for size in [100, 1_000, 10_000, 100_000] {
        group.bench_with_input(
            BenchmarkId::new("push (reallocating)", size),
            &size,
            |b, &size| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    for _ in 0..iters {
                        reset_allocation_counter();
                        let _ = create_with_push(size);
                    }
                    start.elapsed()
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("with_capacity", size),
            &size,
            |b, &size| {
                b.iter(|| create_with_capacity(size))
            },
        );

        group.bench_with_input(
            BenchmarkId::new("collect", size),
            &size,
            |b, &size| {
                b.iter(|| create_with_collect(size))
            },
        );
    }

    group.finish();
}

criterion_group!(benches, benchmark_memory);
criterion_main!(benches);
```

## Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench -- my_benchmark

# Run with filter
cargo bench -- "Order Processing"

# Save baseline for comparison
cargo bench -- --save-baseline main

# Compare against baseline
cargo bench -- --baseline main

# Generate HTML report (in target/criterion/)
cargo bench
open target/criterion/report/index.html
```

## Configuration

Optional Criterion configuration file:

```toml
# benches/criterion.toml
[criterion]
# Warm up time in seconds
warm_up_time = 3
# Measurement time in seconds
measurement_time = 5
# Number of samples
sample_size = 100
# Noise threshold for statistical significance
noise_threshold = 0.01
# Confidence level
confidence_level = 0.95
```

## Interpreting Results

**Key metrics**:
- **time**: Time per iteration (lower is better)
- **thrpt**: Throughput in elements/second or bytes/second
- **change**: Percentage change vs baseline (negative is faster)

**Comparison output**:
```
set lookup (HashSet)/typical
                        time:   [1.2345 ms 1.2456 ms 1.2567 ms]
                        thrpt:  [796.78 Kelem/s 802.79 Kelem/s 810.45 Kelem/s]
                 change: [-65.432% -64.123% -62.891%] (p = 0.00 < 0.05)
                 Performance has improved.
```

**Statistical significance**:
- p < 0.05 indicates statistically significant change
- Criterion uses Student's t-test by default

## Best Practices

1. **Use `black_box`** - Prevent compiler from optimizing away benchmark code
2. **Realistic data** - Match production patterns and sizes
3. **Warm up** - Let the system stabilize before measuring
4. **Save baselines** - Track performance over time
5. **Release mode** - Always benchmark with `--release`
6. **Document purpose** - Include creation date, complexity, context
7. **Commit benchmarks** - Version control for reproducibility
8. **Re-run after changes** - Verify optimizations work

## Profile Settings for Accurate Benchmarks

```toml
# Cargo.toml

# Release profile with debug info for profiling
[profile.release]
debug = true          # Keep debug info
lto = true            # Link-time optimization
codegen-units = 1     # Better optimization

# Benchmark profile (inherits from release)
[profile.bench]
debug = true
```

## Additional Resources

For profiling tools (perf, flamegraph, heaptrack):
- See the `performance-analyzer` skill\'s rust reference
