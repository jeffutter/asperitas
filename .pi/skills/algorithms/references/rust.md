# Rust Reference: Algorithms

## Overview

Rust excels at high-performance algorithm implementations with zero-cost abstractions, SIMD support, and memory safety. The ecosystem provides well-optimized crates for most common algorithms.

## Hash Functions

### xxhash-rust

Fastest non-cryptographic hash function.

```rust
use xxhash_rust::xxh3::{xxh3_64, xxh3_128, Xxh3};

// Simple hashing
let hash = xxh3_64(b"data to hash");
let hash128 = xxh3_128(b"data to hash");

// Streaming API
let mut hasher = Xxh3::new();
hasher.update(b"chunk 1");
hasher.update(b"chunk 2");
let hash = hasher.digest();

// With seed
let hash = xxh3_64_with_seed(b"data", 42);
```

**Cargo.toml**:
```toml
[dependencies]
xxhash-rust = { version = "0.8", features = ["xxh3"] }
```

### blake3

Fast cryptographic hashing with parallel processing.

```rust
use blake3::Hasher;

// Simple hashing
let hash = blake3::hash(b"data to hash");
println!("{}", hash);  // 64-character hex string

// Streaming
let mut hasher = Hasher::new();
hasher.update(b"chunk 1");
hasher.update(b"chunk 2");
let hash = hasher.finalize();

// Keyed hashing (MAC)
let key = [0u8; 32];  // Your secret key
let hash = blake3::keyed_hash(&key, b"message");

// Key derivation
let context = "MyApp 2024-01-01 session key";
let derived = blake3::derive_key(context, b"key material");

// Parallel hashing for large files
let mut hasher = Hasher::new();
hasher.update_rayon(large_data);  // Requires rayon feature
let hash = hasher.finalize();
```

**Cargo.toml**:
```toml
[dependencies]
blake3 = { version = "1.5", features = ["rayon"] }
```

## Probabilistic Data Structures

### hyperloglog (crate)

```rust
use hyperloglog::HyperLogLog;

// Create with precision (4-18, higher = more accurate)
let mut hll = HyperLogLog::new(0.01);  // ~1% error

// Add elements
hll.insert(&"user_1");
hll.insert(&"user_2");
hll.insert(&"user_1");  // Duplicate

// Get estimated count
let count = hll.len();  // ~2

// Merge HLLs from different nodes
let mut hll1 = HyperLogLog::new(0.01);
let mut hll2 = HyperLogLog::new(0.01);
hll1.insert(&"a");
hll2.insert(&"b");
hll1.merge(&hll2);
```

**Cargo.toml**:
```toml
[dependencies]
hyperloglog = "1.0"
```

### probabilistic-collections

Comprehensive collection of probabilistic data structures.

```rust
use probabilistic_collections::bloom::BloomFilter;
use probabilistic_collections::cuckoo::CuckooFilter;
use probabilistic_collections::count_min_sketch::CountMinSketch;

// Bloom filter
let mut bloom = BloomFilter::new(10_000, 0.01);  // capacity, FPR
bloom.insert(&"item_1");
bloom.contains(&"item_1");  // true

// Cuckoo filter (with deletion)
let mut cuckoo = CuckooFilter::new(10_000);
cuckoo.insert(&"item_1");
cuckoo.contains(&"item_1");  // true
cuckoo.delete(&"item_1");
cuckoo.contains(&"item_1");  // false

// Count-Min Sketch
let mut cms = CountMinSketch::new(0.001, 0.01);  // epsilon, delta
cms.increment(&"event_type");
cms.increment(&"event_type");
let count = cms.estimate(&"event_type");  // ~2
```

**Cargo.toml**:
```toml
[dependencies]
probabilistic-collections = "0.7"
```

## Graph Algorithms: petgraph

### Graph Types

```rust
use petgraph::graph::{DiGraph, UnGraph, NodeIndex};
use petgraph::Direction;

// Directed graph
let mut graph: DiGraph<&str, u32> = DiGraph::new();
let a = graph.add_node("A");
let b = graph.add_node("B");
let c = graph.add_node("C");
graph.add_edge(a, b, 5);
graph.add_edge(b, c, 3);
graph.add_edge(a, c, 10);

// Undirected graph
let mut graph: UnGraph<&str, u32> = UnGraph::new_undirected();
// Same API as DiGraph

// Access nodes and edges
for node in graph.node_indices() {
    println!("Node: {:?}", graph[node]);
}

for edge in graph.edge_indices() {
    let (source, target) = graph.edge_endpoints(edge).unwrap();
    println!("{:?} -> {:?}: {}", graph[source], graph[target], graph[edge]);
}
```

### Graph Algorithms

```rust
use petgraph::algo::{dijkstra, astar, bellman_ford, is_cyclic_directed};
use petgraph::visit::{Bfs, Dfs};

// Dijkstra's shortest path
let costs = dijkstra(&graph, start, None, |e| *e.weight());
let cost_to_target = costs[&target];

// A* with heuristic
let path = astar(
    &graph,
    start,
    |n| n == target,
    |e| *e.weight(),
    |n| heuristic(n, target),
);

// Bellman-Ford (handles negative weights)
let result = bellman_ford(&graph, start);

// Cycle detection
let has_cycle = is_cyclic_directed(&graph);

// BFS traversal
let mut bfs = Bfs::new(&graph, start);
while let Some(node) = bfs.next(&graph) {
    println!("Visited: {:?}", graph[node]);
}

// DFS traversal
let mut dfs = Dfs::new(&graph, start);
while let Some(node) = dfs.next(&graph) {
    println!("Visited: {:?}", graph[node]);
}
```

### DOT Export

```rust
use petgraph::dot::{Dot, Config};

let dot = Dot::with_config(&graph, &[Config::EdgeNoLabel]);
println!("{:?}", dot);  // Graphviz DOT format
```

**Cargo.toml**:
```toml
[dependencies]
petgraph = "0.6"
```

## Parallel Algorithms: rayon

### Data Parallelism

```rust
use rayon::prelude::*;

// Parallel iterator
let sum: i32 = (0..1_000_000)
    .into_par_iter()
    .map(|x| x * 2)
    .sum();

// Parallel map
let results: Vec<_> = data
    .par_iter()
    .map(|item| expensive_computation(item))
    .collect();

// Parallel filter
let filtered: Vec<_> = items
    .par_iter()
    .filter(|item| predicate(item))
    .cloned()
    .collect();

// Parallel fold/reduce
let total = numbers
    .par_iter()
    .fold(|| 0, |acc, &x| acc + x)
    .reduce(|| 0, |a, b| a + b);
```

### Custom Thread Pool

```rust
use rayon::ThreadPoolBuilder;

let pool = ThreadPoolBuilder::new()
    .num_threads(4)
    .build()
    .unwrap();

pool.install(|| {
    // Code here runs in the custom pool
    let result: Vec<_> = data.par_iter().map(|x| x * 2).collect();
});
```

### When to Use rayon vs tokio

| Scenario | Use |
|----------|-----|
| CPU-bound parallel work | rayon |
| I/O-bound async work | tokio |
| Data parallelism | rayon |
| Concurrent I/O | tokio |
| Heavy computation | rayon |
| Network requests | tokio |

**Cargo.toml**:
```toml
[dependencies]
rayon = "1.8"
```

## Numeric Computing: ndarray

### N-Dimensional Arrays

```rust
use ndarray::{Array, Array1, Array2, s};

// 1D array
let a: Array1<f64> = Array::from_vec(vec![1.0, 2.0, 3.0]);

// 2D array (matrix)
let m: Array2<f64> = Array::from_shape_vec(
    (2, 3),
    vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]
).unwrap();

// Zeros/ones
let zeros: Array2<f64> = Array::zeros((3, 3));
let ones: Array2<f64> = Array::ones((3, 3));

// Element-wise operations
let sum = &a + &a;
let product = &a * &a;
let scaled = &a * 2.0;

// Matrix multiplication
let result = m.dot(&other_matrix);

// Slicing
let slice = m.slice(s![0..2, 1..3]);

// Broadcasting
let row = Array::from_vec(vec![1.0, 2.0, 3.0]);
let broadcasted = &m + &row;  // Adds row to each row of m
```

### BLAS Integration

```rust
// Enable BLAS for faster linear algebra
// Cargo.toml:
// ndarray = { version = "0.15", features = ["blas"] }
// blas-src = { version = "0.8", features = ["openblas"] }

use ndarray::linalg::general_mat_mul;

// Fast matrix multiplication via BLAS
let mut c = Array2::<f64>::zeros((m, n));
general_mat_mul(1.0, &a, &b, 0.0, &mut c);
```

**Cargo.toml**:
```toml
[dependencies]
ndarray = "0.15"
```

## Sorting Algorithms

Rust's standard library uses a pattern-defeating quicksort (pdqsort) that adapts to input patterns.

### Standard Sorting

```rust
// Unstable sort (faster, no allocation)
let mut v = vec![3, 1, 4, 1, 5, 9];
v.sort_unstable();

// Stable sort (preserves equal element order)
v.sort();

// Sort by key
v.sort_by_key(|x| x.abs());

// Custom comparator
v.sort_by(|a, b| b.cmp(a));  // Descending

// Parallel sorting with rayon
use rayon::prelude::*;
v.par_sort_unstable();
```

### Partial Sorting

```rust
// Select nth element (partitions around it)
let mut v = vec![3, 1, 4, 1, 5, 9, 2, 6];
v.select_nth_unstable(3);
// v[3] is now the 4th smallest, smaller elements before, larger after

// Top-k elements
let k = 3;
v.select_nth_unstable(k - 1);
let top_k = &v[..k];
```

## Crate Recommendations

| Crate | Category | Notes |
|-------|----------|-------|
| `xxhash-rust` | Hashing | Fastest non-crypto |
| `blake3` | Hashing | Fast crypto, parallel |
| `petgraph` | Graphs | Full algorithm suite |
| `rayon` | Parallelism | Data parallelism |
| `ndarray` | Numerics | numpy-like |
| `probabilistic-collections` | Probabilistic | Bloom, Cuckoo, CMS |
| `hyperloglog` | Cardinality | HyperLogLog |
| `ordered-float` | Sorting | Ord for floats |
| `indexmap` | Collections | Order-preserving HashMap |

## Additional Resources

- [xxhash-rust documentation](https://docs.rs/xxhash-rust)
- [BLAKE3 documentation](https://docs.rs/blake3)
- [petgraph documentation](https://docs.rs/petgraph)
- [rayon documentation](https://docs.rs/rayon)
- [ndarray documentation](https://docs.rs/ndarray)
- [The Rust Performance Book](https://nnethercote.github.io/perf-book/)
