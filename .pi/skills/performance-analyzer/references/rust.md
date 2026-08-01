# Rust Reference: Performance Analysis

## Overview

Rust performance analysis uses criterion for benchmarking and system profilers for detailed analysis. The language's zero-cost abstractions mean profiling reveals actual hotspots rather than runtime overhead.

## Criterion Benchmarking

Statistical benchmarking framework with comparison support.

### Setup

```toml
# Cargo.toml
[dev-dependencies]
criterion = "0.5"

[[bench]]
name = "my_benchmark"
harness = false
```

### Basic Benchmark

```rust
// benches/my_benchmark.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn fibonacci(n: u64) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        n => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

fn benchmark_fibonacci(c: &mut Criterion) {
    c.bench_function("fib 20", |b| {
        b.iter(|| fibonacci(black_box(20)))
    });
}

criterion_group!(benches, benchmark_fibonacci);
criterion_main!(benches);
```

### Benchmark Groups

```rust
fn benchmark_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("String Operations");

    group.bench_function("format!", |b| {
        b.iter(|| format!("{} {}", "hello", "world"))
    });

    group.bench_function("concat", |b| {
        b.iter(|| {
            let mut s = String::from("hello");
            s.push(' ');
            s.push_str("world");
            s
        })
    });

    group.finish();
}
```

### Input Variations

```rust
use criterion::{BenchmarkId, Criterion};

fn benchmark_with_inputs(c: &mut Criterion) {
    let mut group = c.benchmark_group("Sort");

    for size in [100, 1_000, 10_000, 100_000].iter() {
        let data: Vec<i32> = (0..*size).rev().collect();

        group.bench_with_input(
            BenchmarkId::new("std_sort", size),
            &data,
            |b, data| {
                b.iter(|| {
                    let mut d = data.clone();
                    d.sort();
                    d
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("unstable_sort", size),
            &data,
            |b, data| {
                b.iter(|| {
                    let mut d = data.clone();
                    d.sort_unstable();
                    d
                })
            },
        );
    }

    group.finish();
}
```

### Custom Measurement

```rust
use criterion::{Criterion, SamplingMode, Throughput};

fn benchmark_throughput(c: &mut Criterion) {
    let data = vec![0u8; 1024 * 1024];  // 1 MB

    let mut group = c.benchmark_group("Hash throughput");
    group.throughput(Throughput::Bytes(data.len() as u64));
    group.sampling_mode(SamplingMode::Flat);

    group.bench_function("xxhash", |b| {
        b.iter(|| xxhash_rust::xxh3::xxh3_64(&data))
    });

    group.bench_function("blake3", |b| {
        b.iter(|| blake3::hash(&data))
    });

    group.finish();
}
```

### Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench -- fibonacci

# Generate HTML report
cargo bench -- --save-baseline main

# Compare against baseline
cargo bench -- --baseline main
```

## System Profilers

### perf (Linux)

```bash
# Record profile
perf record --call-graph dwarf ./target/release/myapp

# Analyze
perf report

# With cargo
cargo build --release
perf record --call-graph dwarf cargo run --release

# Generate flamegraph data
perf script | stackcollapse-perf.pl | flamegraph.pl > flame.svg
```

### Instruments (macOS)

```bash
# Time Profiler
xcrun xctrace record --template "Time Profiler" --launch -- ./target/release/myapp

# Open in Instruments
open *.trace
```

Or use Instruments.app directly:
1. Open Instruments
2. Select "Time Profiler"
3. Choose target binary
4. Record and analyze

### cargo-flamegraph

```bash
# Install
cargo install flamegraph

# Generate flamegraph
cargo flamegraph --bin myapp

# With specific args
cargo flamegraph -- --arg1 value1

# For tests
cargo flamegraph --test integration_test

# Differential flamegraph
cargo flamegraph --output before.svg
# Make changes
cargo flamegraph --output after.svg
# Compare visually
```

**Cargo.toml** (for better symbols):
```toml
[profile.release]
debug = true
```

## Memory Profiling

### heaptrack

```bash
# Install (Linux)
sudo apt install heaptrack

# Profile
heaptrack ./target/release/myapp

# Analyze
heaptrack_gui heaptrack.myapp.*.gz
```

### DHAT (valgrind)

```bash
valgrind --tool=dhat ./target/release/myapp

# View report
dh_view.html dhat.out.*
```

### cargo-profiler

```bash
# Install
cargo install cargo-profiler

# Callgrind (call counts and time)
cargo profiler callgrind

# Cachegrind (cache analysis)
cargo profiler cachegrind
```

## Allocation Tracking

### GlobalAlloc Trait

```rust
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountingAllocator;

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static DEALLOCATED: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATED.fetch_add(layout.size(), Ordering::SeqCst);
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOCATED.fetch_add(layout.size(), Ordering::SeqCst);
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn main() {
    // Your code
    println!("Allocated: {} bytes", ALLOCATED.load(Ordering::SeqCst));
    println!("Deallocated: {} bytes", DEALLOCATED.load(Ordering::SeqCst));
}
```

### jemalloc

```toml
# Cargo.toml
[dependencies]
tikv-jemallocator = "0.5"
```

```rust
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
```

### Memory Pooling

```rust
use typed_arena::Arena;

fn process_with_arena() {
    let arena = Arena::new();

    // All allocations freed at once when arena drops
    for _ in 0..10000 {
        let item = arena.alloc(MyStruct::new());
        // Use item
    }
    // Everything freed here
}
```

## Async Profiling

### tokio-console

```toml
# Cargo.toml
[dependencies]
console-subscriber = "0.2"
tokio = { version = "1", features = ["full", "tracing"] }
```

```rust
#[tokio::main]
async fn main() {
    console_subscriber::init();

    // Your async code
}
```

```bash
# Install console
cargo install tokio-console

# Run your app, then in another terminal:
tokio-console
```

**Metrics shown**:
- Task spawn/poll/drop counts
- Task busy time
- Wake counts
- Resource (future) details

### tracing with Timing

```rust
use tracing::{instrument, info_span};

#[instrument]
async fn slow_operation() {
    // Automatically times this function
}

async fn manual_timing() {
    let span = info_span!("database_query");
    let _enter = span.enter();
    // Timed region
}
```

## Tool Selection Guide

| Question | Tool |
|----------|------|
| Micro-benchmarks? | criterion |
| CPU hotspots? | perf/Instruments + flamegraph |
| Memory leaks? | heaptrack/valgrind |
| Cache performance? | cachegrind |
| Async task behavior? | tokio-console |
| Allocation patterns? | DHAT, custom allocator |
| Compare implementations? | criterion with baselines |

## Common Patterns

### Profile-Guided Optimization (PGO)

```bash
# Build with instrumentation
RUSTFLAGS="-Cprofile-generate=/tmp/pgo" cargo build --release

# Run representative workload
./target/release/myapp --typical-args

# Merge profile data
llvm-profdata merge -o /tmp/pgo/merged.profdata /tmp/pgo

# Build with profile data
RUSTFLAGS="-Cprofile-use=/tmp/pgo/merged.profdata" cargo build --release
```

### Release Build Settings

```toml
# Cargo.toml
[profile.release]
lto = true           # Link-time optimization
codegen-units = 1    # Better optimization (slower compile)
panic = "abort"      # Smaller binary
strip = true         # Remove symbols (smaller binary)

[profile.release-with-debug]
inherits = "release"
debug = true         # Keep debug info for profiling
```

## Additional Resources

- [Criterion documentation](https://bheisler.github.io/criterion.rs/book/)
- [The Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Flamegraph documentation](https://github.com/flamegraph-rs/flamegraph)
- [tokio-console guide](https://docs.rs/console-subscriber)
- [perf tutorial](https://perf.wiki.kernel.org/index.php/Tutorial)
