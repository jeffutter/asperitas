---
name: algorithms
description: Use when researching algorithms, data structures, optimization, performance, or need modern alternatives to classic algorithms
---

# Modern Algorithms and Data Structures

## Overview

This skill provides comprehensive guidance on modern algorithms and data structures from recent computer science research, with focus on practical implementations. Modern algorithms often provide significant performance improvements over classic approaches through better cache utilization, parallelization, and probabilistic techniques.

## When to Use

Use this skill when:
- Researching algorithmic approaches for a problem
- Need modern alternatives to classic algorithms
- Optimizing performance-critical code
- Evaluating data structure choices
- Assessing implementation complexity vs performance gains
- Working with large-scale data requiring approximate solutions
- Implementing distributed system algorithms

## Core Concepts

### Modern Hash Functions

Modern hash functions significantly outperform classic algorithms like MD5 and SHA1 for non-cryptographic use cases.

#### xxHash3 (2020)

**Overview**: Extremely fast non-cryptographic hash function optimized for modern CPUs with SIMD instructions.

**Performance**:
- 31.5 GB/s on modern hardware (vs 0.5 GB/s for MD5)
- Excellent hash quality with low collision rates
- Consistent across 32-bit and 64-bit platforms

**Use Cases**:
- Hash tables and hash maps
- Data deduplication
- Checksum verification (non-security)
- Bloom filters and other probabilistic structures

**Paper**: "xxHash: Fast Hash Algorithm" (Collet, 2020)

#### BLAKE3 (2020)

**Overview**: Cryptographic hash function that's significantly faster than SHA-2/SHA-3 while maintaining security.

**Performance**:
- 2.5 GB/s (vs 0.4 GB/s for SHA-256)
- Parallelizable and optimized for SIMD
- Suitable for cryptographic applications

**Use Cases**:
- Content-addressed storage
- File integrity verification
- Digital signatures
- Password hashing alternatives (use with key derivation)

**Paper**: "BLAKE3: One Function, Fast Everywhere" (O'Connor et al., 2020)

For language-specific implementations:
- **Elixir**: See `references/elixir.md` - exhash, b3 libraries
- **Rust**: See `references/rust.md` - xxhash-rust, blake3 crates

### Probabilistic Data Structures

Probabilistic structures trade perfect accuracy for dramatic space savings and speed improvements. Essential for large-scale systems.

#### HyperLogLog (2007)

**Overview**: Estimates cardinality (unique count) with configurable accuracy using minimal memory.

**Space Complexity**:
- Standard: 1.5 KB for billions of elements
- vs Exact: 8 bytes per unique element (8 GB for 1 billion elements)

**Accuracy**:
- Typical error: ±2% with 1.5 KB
- Error decreases with more memory (configurable)

**Use Cases**:
- Unique visitor counting
- Distinct value estimation in databases
- Network traffic analysis
- Real-time analytics dashboards

**Performance Comparison**:
```
Counting 10M unique items:
- Exact (HashSet): 800 MB memory, 100% accurate
- HyperLogLog: 12 KB memory, 98% accurate
```

**Paper**: "HyperLogLog: the analysis of a near-optimal cardinality estimation algorithm" (Flajolet et al., 2007)

**Implementation pattern**:
```pseudocode
class UniqueVisitorTracker:
    # 16-bit precision = ~0.8% error, 64 KB memory
    hll = HyperLogLog.new(precision=16)

    def track_visitor(user_id):
        hll.add(str(user_id))

    def get_count():
        return hll.count()
```

#### Cuckoo Filters (2014)

**Overview**: Space-efficient probabilistic data structure for set membership testing. Improves on Bloom filters by supporting deletion.

**Advantages over Bloom Filters**:
- Supports deletion (Bloom filters cannot delete)
- Better space efficiency for low false positive rates
- Better cache locality

**Space Complexity**: ~1.5 bytes per item for 3% false positive rate

**Use Cases**:
- Cache invalidation (track what's cached)
- Spam filtering with updating rules
- Distributed system deduplication
- Rate limiting with key expiration

**Performance Comparison**:
```
Storing 1M items with 2% false positive rate:
- Exact (HashSet): 80 MB, 100% accurate, supports deletion
- Bloom Filter: 1.8 MB, 2% FP rate, no deletion
- Cuckoo Filter: 1.5 MB, 2% FP rate, supports deletion
```

**Paper**: "Cuckoo Filter: Practically Better Than Bloom" (Fan et al., 2014)

**Implementation pattern**:
```pseudocode
class CacheFilter:
    filter = CuckooFilter.new(capacity=10_000_000, fpr=0.01)

    def mark_cached(key):
        filter.add(key)

    def is_cached(key):
        return filter.contains(key)

    def invalidate(key):
        filter.delete(key)  # Unlike Bloom filters!
```

#### Count-Min Sketch (2005)

**Overview**: Probabilistic data structure for frequency estimation in streams. Answers "how many times has X appeared?" with bounded error.

**Space Complexity**: Configurable based on error bounds (typically few KB for millions of items)

**Accuracy**: Overestimates by at most ε×N with probability 1-δ
- ε (epsilon): error rate (e.g., 0.01 = 1% error)
- δ (delta): failure probability (e.g., 0.001 = 0.1% chance of exceeding error)

**Use Cases**:
- Real-time analytics (top K items)
- Network traffic monitoring
- Detecting heavy hitters in streams
- Frequency-based rate limiting

**Performance Comparison**:
```
Tracking 100M events with 1% error:
- Exact (HashMap): 1.6 GB memory
- Count-Min Sketch: 40 KB memory
```

**Paper**: "An Improved Data Stream Summary: The Count-Min Sketch and its Applications" (Cormode & Muthukrishnan, 2005)

**Implementation pattern**:
```pseudocode
class FrequencyTracker:
    # 0.1% error, 99% confidence
    cms = CountMinSketch.new(epsilon=0.001, delta=0.01)

    def track_event(event_type):
        cms.add(event_type)

    def get_frequency(event_type):
        return cms.count(event_type)
```

For language-specific implementations:
- **Elixir**: See `references/elixir.md` - hyperloglog, cuckoo_filter libraries
- **Rust**: See `references/rust.md` - probabilistic crates

### Cache-Efficient Algorithms

Modern CPUs have multiple cache levels (L1, L2, L3). Cache-oblivious algorithms automatically adapt to cache hierarchy without tuning.

#### Cache-Oblivious Algorithms

**Overview**: Algorithms designed to work efficiently across all cache levels without knowing cache parameters.

**Key Principle**: Divide-and-conquer with base cases small enough to fit in cache.

**Example - Matrix Multiplication**:
```pseudocode
class CacheObliviousMatrix:
    # Base case threshold (tune to L1 cache size)
    BASE_CASE = 64

    def multiply(a, b):
        n = len(a)

        if n <= BASE_CASE:
            return naive_multiply(a, b)
        else:
            # Divide matrices into quadrants
            (a11, a12, a21, a22) = subdivide(a)
            (b11, b12, b21, b22) = subdivide(b)

            # Recursively multiply submatrices
            c11 = add(multiply(a11, b11), multiply(a12, b21))
            c12 = add(multiply(a11, b12), multiply(a12, b22))
            c21 = add(multiply(a21, b11), multiply(a22, b21))
            c22 = add(multiply(a21, b12), multiply(a22, b22))

            return combine(c11, c12, c21, c22)
```

**Performance Impact**:
- 2-3× faster than naive approach for large matrices
- Scales better as data exceeds cache size

**Paper**: "Cache-Oblivious Algorithms" (Frigo et al., 1999)

#### B+ Trees for Disk/Cache Efficiency

**Overview**: Self-balancing tree optimized for systems with cache/disk hierarchy. Used in databases.

**Key Properties**:
- All data in leaf nodes
- Internal nodes only store keys for navigation
- Leaf nodes linked for range queries
- High fan-out (many children per node) reduces height

**Use Cases**:
- Database indexes
- File systems
- On-disk data structures
- Large sorted collections

**Performance Comparison**:
```
1M items, random access:
- HashMap: O(1) average, poor cache locality
- Binary tree: O(log n), poor cache locality
- B+ tree: O(log n), excellent cache locality, 3-5× faster
```

### Modern Sorting Algorithms

#### BlockQuicksort (2016)

**Overview**: Variant of quicksort with better cache behavior through block-wise partitioning.

**Key Innovation**: Process elements in blocks that fit in CPU cache, reducing cache misses.

**Performance**:
- 1.5-2× faster than standard quicksort on modern hardware
- Better worst-case behavior with median-of-medians pivot selection

**Paper**: "BlockQuicksort: How Branch Mispredictions don't affect Quicksort" (Edelkamp & Weiß, 2016)

**Implementation pattern**:
```pseudocode
class BlockQuicksort:
    BLOCK_SIZE = 128  # Typical L1 cache can hold ~512 elements

    def sort(list):
        if len(list) <= BLOCK_SIZE:
            return standard_sort(list)  # Use native sort for small lists

        pivot = select_pivot(list)

        # Partition in blocks for cache efficiency
        (left, equal, right) = partition_blockwise(list, pivot)

        # Recursively sort partitions (can parallelize)
        sorted_left = sort(left)
        sorted_right = sort(right)

        return sorted_left + equal + sorted_right
```

#### Pattern-Defeating Quicksort (pdqsort, 2021)

**Overview**: Hybrid sorting algorithm that detects patterns (sorted, reverse-sorted, equal elements) and adapts strategy.

**Key Features**:
- Switches to heapsort when recursion depth excessive
- Uses insertion sort for small partitions
- Detects and handles common patterns efficiently

**Performance**:
- O(n log n) worst-case (vs O(n²) for standard quicksort)
- Near-optimal for partially sorted data
- Used in Rust's standard library

**Paper**: "Pattern-defeating Quicksort" (Orson Peters, 2021)

### When to Use Modern vs Classic Algorithms

| Use Case | Modern Algorithm | Classic Alternative | When to Switch |
|----------|------------------|---------------------|----------------|
| **Hashing (non-crypto)** | xxHash3 | MD5, SHA1 | Always - 60× faster |
| **Hashing (crypto)** | BLAKE3 | SHA-256 | When performance critical |
| **Unique Counting** | HyperLogLog | HashSet/exact count | >100K unique items |
| **Membership Testing** | Cuckoo Filter | Bloom Filter | Need deletion support |
| **Frequency Estimation** | Count-Min Sketch | HashMap counter | Millions of items |
| **Sorting Large Data** | BlockQuicksort | Standard quicksort | >10K items on modern CPU |
| **Sorting with Patterns** | pdqsort | Quicksort | Data has patterns |
| **Matrix Multiplication** | Cache-oblivious | Naive O(n³) | Large matrices (>1000×1000) |
| **Database Indexes** | B+ Tree | Binary tree | Persistent storage |

## Quick Reference

### Hash Function Selection

```pseudocode
# Non-cryptographic (fast checksums, hash tables)
hash64(data)   # 64-bit hash
hash128(data)  # 128-bit hash

# Cryptographic (integrity, signatures)
blake3_hash(sensitive_data)

# Message authentication
hmac_sha256(key, message)
```

### Probabilistic Structure Selection

```pseudocode
# Cardinality estimation (unique counts)
hll = HyperLogLog.new(precision=14)
hll.add(item)
count = hll.count()

# Membership testing (with deletion)
filter = CuckooFilter.new(capacity=1_000_000)
filter.add(item)
filter.contains(item)
filter.delete(item)

# Frequency estimation
cms = CountMinSketch.new(epsilon=0.001, delta=0.01)
cms.add(event)
count = cms.count(event)
```

## Common Mistakes

### Using MD5/SHA1 for Non-Security Tasks

**Problem**: MD5 and SHA1 are slow for non-cryptographic hashing.

```pseudocode
# SLOW - cryptographic hash for non-security use
md5_hash(data)  # ~0.5 GB/s

# FAST - modern non-crypto hash
xxhash3_64(data)  # ~31 GB/s
```

**When to fix**: Hashing for hash tables, checksums, deduplication (not security).

### Exact Counting When Estimation Suffices

**Problem**: Using exact counts wastes memory for analytics.

```pseudocode
# MEMORY INTENSIVE - exact count
unique_visitors = HashSet()  # 8 bytes per visitor
for event in events:
    unique_visitors.add(event.user_id)
# 800 MB for 100M unique visitors

# MEMORY EFFICIENT - approximate count
hll = HyperLogLog.new(precision=14)  # 16 KB total
for event in events:
    hll.add(event.user_id)
# 16 KB with ±2% accuracy
```

**When to fix**: Analytics, dashboards, monitoring (where ±2% error acceptable).

### Using Bloom Filters When Need Deletion

**Problem**: Bloom filters don't support deletion; leads to false positives growing over time.

```pseudocode
# WRONG - can't delete from Bloom filter
bloom = BloomFilter.new(1_000_000)
bloom.add("cached_item")
# Can't delete when cache invalidated!

# CORRECT - Cuckoo filter supports deletion
filter = CuckooFilter.new(capacity=1_000_000)
filter.add("cached_item")
filter.delete("cached_item")  # Works!
```

**When to fix**: Cache tracking, rate limiting with expiration, any scenario requiring deletion.

### Naive Sorting on Modern Hardware

**Problem**: Standard quicksort doesn't exploit CPU cache effectively.

```pseudocode
# CACHE INEFFICIENT - many cache misses
standard_sort(large_list)

# CACHE EFFICIENT - block-wise processing
block_quicksort(large_list)  # 1.5-2× faster
```

**When to fix**: Sorting >10K items on modern CPUs (post-2010).

## When to Research Further

Consider using the `algorithms-researcher` agent when:

1. **Novel Problems**: No established solution exists
2. **Scale Beyond Standard**: Handling billions of items, petabytes of data
3. **Extreme Performance Needs**: Current solutions too slow even after optimization
4. **Cutting-Edge Requirements**: Need latest research (published in last 2 years)
5. **Domain-Specific**: Specialized algorithms (graph, geometric, streaming)

**Example Questions for Researcher**:
- "What's the fastest known algorithm for approximate nearest neighbor search?"
- "Are there better alternatives to HyperLogLog for cardinality published recently?"
- "What's the state-of-the-art for real-time top-K tracking in streams?"

## Library Recommendations

### Production-Ready Libraries by Algorithm Type

| Algorithm Type | Description | Maturity |
|----------------|-------------|----------|
| xxHash3/xxHash64 | Modern non-crypto hash | Stable |
| BLAKE3 | Fast cryptographic hash | Stable |
| HyperLogLog | Cardinality estimation | Stable |
| Bloom Filters | Standard membership testing | Stable |
| Cuckoo Filters | Membership with deletion | Beta |
| Count-Min Sketch | Frequency estimation | Write custom |
| B+ Trees | Persistent sorted data | Beta |

### When to Write Custom Implementations

Consider custom implementation when:
- Need specific tuning for your use case
- No stable library available (e.g., Count-Min Sketch)
- Integrating with existing C/Rust code
- Performance critical and library overhead matters

For language-specific library recommendations:
- **Elixir**: See `references/elixir.md` - NIFs, :atomics, ETS
- **Rust**: See `references/rust.md` - crates ecosystem

## Language-Specific Implementations

For language-specific implementations of these algorithms:
- **Elixir**: See `references/elixir.md` - Elixir libraries, NIFs, production patterns
- **Rust**: See `references/rust.md` - Rust crates, zero-copy patterns, SIMD

## References

- **Modern Hash Functions**: "xxHash: Fast Hash Algorithm" (Collet, 2020)
- **Cryptographic Hashing**: "BLAKE3: One Function, Fast Everywhere" (O'Connor et al., 2020)
- **HyperLogLog**: "HyperLogLog: the analysis of a near-optimal cardinality estimation algorithm" (Flajolet et al., 2007)
- **Cuckoo Filters**: "Cuckoo Filter: Practically Better Than Bloom" (Fan et al., 2014)
- **Count-Min Sketch**: "An Improved Data Stream Summary: The Count-Min Sketch and its Applications" (Cormode & Muthukrishnan, 2005)
- **Cache-Oblivious Algorithms**: "Cache-Oblivious Algorithms" (Frigo et al., 1999)
- **BlockQuicksort**: "BlockQuicksort: How Branch Mispredictions don't affect Quicksort" (Edelkamp & Weiß, 2016)
- **Pattern-Defeating Quicksort**: "Pattern-defeating Quicksort" (Orson Peters, 2021)

## Related Skills

- `performance-analyzer` - For profiling and benchmarking optimizations
- `distributed-systems` - For consensus and replication algorithms
- `production-quality` - For testing and documentation
