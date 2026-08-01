# Rust Algorithm Libraries and Implementations

This reference covers Rust-specific algorithm libraries, implementation patterns, and ecosystem recommendations.

## Library Ecosystem

### Recommended Libraries by Category

| Category | Recommended | Alternative | Notes |
|----------|-------------|-------------|-------|
| Hash (non-crypto) | `xxhash-rust` | `highway`, `ahash` | xxHash3 fastest for general use |
| Hash (crypto) | `blake3` | `sha2` | BLAKE3 faster than SHA-256 |
| HyperLogLog | `hyperloglog-rs` | `probabilistic-collections` | Good precision controls |
| Bloom Filters | `bloom` | `probabilistic-collections` | Simple, well-tested |
| Cuckoo Filters | `cuckoofilter` | `scalable_cuckoo_filter` | Supports deletion |
| Count-Min Sketch | `streaming-algorithms` | `datasketches-rs` | Part of larger crate |
| Geospatial | `geo` | `rstar` (R-tree) | Full geometry support |
| Graph algorithms | `petgraph` | `graphlib` | De facto standard |
| Sorting | Built-in + `pdqsort` | `rayon` for parallel | `slice::sort` uses pdqsort |

### Library Evaluation Scorecard Template

```markdown
## [Library Name] (https://crates.io/crates/...)

**Maturity**: [Experimental/Beta/Stable/Production]
**Last Updated**: [Date]
**Downloads**: [crates.io downloads/month]
**Maintainership**: [Active/Maintained/Stale]

**API Quality**:
- Documentation: [docs.rs coverage]
- Examples: [Quality and completeness]
- Testing: [Test coverage, fuzzing]

**Performance**:
- Benchmarks: [Criterion benchmarks available?]
- Production use: [Known users]
- SIMD: [Uses SIMD acceleration?]

**Rust Integration**:
- Idiomatic: [Follows Rust conventions]
- no_std: [Supports embedded?]
- Safety: [Safe API? Any unsafe?]

**Recommendation**: [Use/Consider/Avoid]
```

## Implementation Patterns

### Zero-Copy Pattern

Minimize allocations with borrowing and slices:

```rust
use std::collections::HashMap;

pub struct TopK<'a> {
    counters: HashMap<&'a str, u64>,
    k: usize,
}

impl<'a> TopK<'a> {
    pub fn new(k: usize) -> Self {
        Self {
            counters: HashMap::with_capacity(k),
            k,
        }
    }

    pub fn track(&mut self, event: &'a str) {
        *self.counters.entry(event).or_insert(0) += 1;

        if self.counters.len() > self.k {
            // Space-Saving: remove minimum
            if let Some((&min_key, _)) = self.counters.iter()
                .min_by_key(|(_, &count)| count)
            {
                self.counters.remove(min_key);
            }
        }
    }

    pub fn top(&self) -> Vec<(&'a str, u64)> {
        let mut items: Vec<_> = self.counters.iter()
            .map(|(&k, &v)| (k, v))
            .collect();
        items.sort_by(|a, b| b.1.cmp(&a.1));
        items.truncate(self.k);
        items
    }
}
```

### Thread-Safe Pattern

For concurrent access, use appropriate synchronization:

```rust
use std::sync::Arc;
use parking_lot::RwLock;
use dashmap::DashMap;

// Option 1: DashMap for high-concurrency
pub struct ConcurrentCounter {
    counters: DashMap<String, u64>,
}

impl ConcurrentCounter {
    pub fn new() -> Self {
        Self { counters: DashMap::new() }
    }

    pub fn increment(&self, key: &str) {
        self.counters
            .entry(key.to_string())
            .and_modify(|c| *c += 1)
            .or_insert(1);
    }

    pub fn get(&self, key: &str) -> u64 {
        self.counters.get(key).map(|v| *v).unwrap_or(0)
    }
}

// Option 2: RwLock for read-heavy workloads
pub struct ReadHeavyCounter {
    counters: Arc<RwLock<HashMap<String, u64>>>,
}

impl ReadHeavyCounter {
    pub fn increment(&self, key: &str) {
        let mut counters = self.counters.write();
        *counters.entry(key.to_string()).or_insert(0) += 1;
    }

    pub fn get(&self, key: &str) -> u64 {
        let counters = self.counters.read();
        counters.get(key).copied().unwrap_or(0)
    }
}
```

### SIMD-Accelerated Pattern

For maximum performance on supported operations:

```rust
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

// Example: SIMD-accelerated byte comparison
#[cfg(target_arch = "x86_64")]
pub fn count_matches_simd(haystack: &[u8], needle: u8) -> usize {
    if !is_x86_feature_detected!("avx2") {
        return haystack.iter().filter(|&&b| b == needle).count();
    }

    unsafe { count_matches_avx2(haystack, needle) }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn count_matches_avx2(haystack: &[u8], needle: u8) -> usize {
    let needle_vec = _mm256_set1_epi8(needle as i8);
    let mut count = 0;

    for chunk in haystack.chunks(32) {
        if chunk.len() == 32 {
            let data = _mm256_loadu_si256(chunk.as_ptr() as *const __m256i);
            let cmp = _mm256_cmpeq_epi8(data, needle_vec);
            let mask = _mm256_movemask_epi8(cmp) as u32;
            count += mask.count_ones() as usize;
        } else {
            count += chunk.iter().filter(|&&b| b == needle).count();
        }
    }
    count
}
```

### Async Pattern

For I/O-bound algorithm applications:

```rust
use tokio::sync::RwLock;
use std::sync::Arc;

pub struct AsyncHyperLogLog {
    inner: Arc<RwLock<hyperloglog::HyperLogLog>>,
}

impl AsyncHyperLogLog {
    pub fn new(precision: u8) -> Self {
        Self {
            inner: Arc::new(RwLock::new(hyperloglog::HyperLogLog::new(precision))),
        }
    }

    pub async fn add(&self, value: &str) {
        let mut hll = self.inner.write().await;
        hll.add(value);
    }

    pub async fn count(&self) -> u64 {
        let hll = self.inner.read().await;
        hll.count()
    }

    pub async fn merge(&self, other: &Self) {
        let mut this = self.inner.write().await;
        let other = other.inner.read().await;
        this.merge(&*other);
    }
}
```

## Rust-Specific Considerations

### Memory Layout

Optimize struct layout for cache efficiency:

```rust
// Bad: padding wastes memory
struct BadLayout {
    a: u8,      // 1 byte + 7 padding
    b: u64,     // 8 bytes
    c: u8,      // 1 byte + 7 padding
}  // Total: 24 bytes

// Good: pack efficiently
#[repr(C)]
struct GoodLayout {
    b: u64,     // 8 bytes
    a: u8,      // 1 byte
    c: u8,      // 1 byte + 6 padding
}  // Total: 16 bytes

// Or use repr(packed) for exact control (careful with alignment!)
```

### Avoiding Allocations

```rust
// Reuse buffers
pub struct StreamProcessor {
    buffer: Vec<u8>,  // Reused across calls
}

impl StreamProcessor {
    pub fn process(&mut self, input: &[u8]) -> &[u8] {
        self.buffer.clear();
        self.buffer.extend_from_slice(input);
        // Process in-place...
        &self.buffer
    }
}

// Use SmallVec for small, variable-size data
use smallvec::SmallVec;

fn process_items(items: impl Iterator<Item = u32>) -> SmallVec<[u32; 8]> {
    // Stack-allocated if <=8 items, heap otherwise
    items.collect()
}
```

### Const Generics

Use const generics for compile-time size optimization:

```rust
pub struct FixedBloomFilter<const N: usize> {
    bits: [u64; N],
    hash_count: u8,
}

impl<const N: usize> FixedBloomFilter<N> {
    pub const fn new(hash_count: u8) -> Self {
        Self {
            bits: [0; N],
            hash_count,
        }
    }

    pub fn insert(&mut self, item: &[u8]) {
        for i in 0..self.hash_count {
            let hash = self.hash(item, i);
            let idx = hash % (N * 64);
            self.bits[idx / 64] |= 1 << (idx % 64);
        }
    }
}

// Usage: size known at compile time
let filter: FixedBloomFilter<1024> = FixedBloomFilter::new(3);
```

## When No Library Exists

### Option 1: Pure Rust Implementation

**Pros**: No dependencies, full control, no unsafe
**Cons**: Development time, potential for bugs

**When to choose**:
- Algorithm is well-understood
- Need custom optimizations
- Want to avoid dependency

### Option 2: C/C++ Bindings (bindgen)

**Pros**: Access to mature implementations
**Cons**: Unsafe code, build complexity

```rust
// build.rs
fn main() {
    cc::Build::new()
        .file("src/xxhash.c")
        .compile("xxhash");

    println!("cargo:rerun-if-changed=src/xxhash.c");
}

// src/lib.rs
extern "C" {
    fn XXH3_64bits(data: *const u8, len: usize) -> u64;
}

pub fn xxhash3(data: &[u8]) -> u64 {
    unsafe { XXH3_64bits(data.as_ptr(), data.len()) }
}
```

### Option 3: WASM Module

**Pros**: Sandboxed, portable
**Cons**: Performance overhead, limited interop

## Example: HyperLogLog Integration

Complete example of integrating HyperLogLog in a Rust application:

```rust
use hyperloglog::HyperLogLog;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct UniqueVisitorTracker {
    hll: Arc<Mutex<HyperLogLog>>,
}

impl UniqueVisitorTracker {
    pub fn new() -> Self {
        // 14-bit precision = 16KB memory, ~0.8% error
        Self {
            hll: Arc::new(Mutex::new(HyperLogLog::new(14))),
        }
    }

    pub async fn track(&self, visitor_id: &str) {
        let mut hll = self.hll.lock().await;
        hll.add(visitor_id);
    }

    pub async fn count(&self) -> u64 {
        let hll = self.hll.lock().await;
        hll.count()
    }

    pub fn clone_for_worker(&self) -> Self {
        Self {
            hll: Arc::clone(&self.hll),
        }
    }
}

// Axum integration example
use axum::{Router, routing::post, Extension, Json};

async fn track_visitor(
    Extension(tracker): Extension<Arc<UniqueVisitorTracker>>,
    Json(payload): Json<VisitorPayload>,
) -> &'static str {
    tracker.track(&payload.visitor_id).await;
    "tracked"
}

async fn get_count(
    Extension(tracker): Extension<Arc<UniqueVisitorTracker>>,
) -> Json<CountResponse> {
    Json(CountResponse {
        unique_visitors: tracker.count().await,
    })
}

fn app(tracker: Arc<UniqueVisitorTracker>) -> Router {
    Router::new()
        .route("/track", post(track_visitor))
        .route("/count", axum::routing::get(get_count))
        .layer(Extension(tracker))
}
```

## Performance Benchmarking

Use Criterion for algorithm comparison:

```rust
// benches/algorithms.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::collections::HashSet;
use hyperloglog::HyperLogLog;

fn benchmark_counting(c: &mut Criterion) {
    let mut group = c.benchmark_group("unique_counting");

    group.bench_function("HashSet (exact)", |b| {
        b.iter(|| {
            let mut set = HashSet::new();
            for i in 0..100_000u64 {
                set.insert(i);
            }
            black_box(set.len())
        })
    });

    group.bench_function("HyperLogLog (approximate)", |b| {
        b.iter(|| {
            let mut hll = HyperLogLog::new(14);
            for i in 0..100_000u64 {
                hll.add(&i.to_le_bytes());
            }
            black_box(hll.count())
        })
    });

    group.finish();
}

criterion_group!(benches, benchmark_counting);
criterion_main!(benches);
```

```toml
# Cargo.toml
[dev-dependencies]
criterion = "0.5"

[[bench]]
name = "algorithms"
harness = false
```

## References

- **Error handling**: the `rust-patterns` skill\'s error-handling reference
- **Concurrency**: the `rust-patterns` skill\'s concurrency reference
- **Performance profiling**: the `performance-analyzer` skill
