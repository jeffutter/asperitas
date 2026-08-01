# Elixir Reference: Algorithms

## Overview

Elixir algorithm implementations leverage functional patterns with immutable data structures. For performance-critical code, NIFs (Native Implemented Functions) via Rustler provide access to native code while maintaining Elixir's safety guarantees.

## Hash Functions

### exhash (xxHash)

Fast non-cryptographic hashing for hash tables, deduplication, and checksums.

```elixir
# mix.exs
defp deps do
  [{:exhash, "~> 0.2"}]
end

# Usage
:exhash.xxh3_64("data to hash")
# => 17241709254077376921

:exhash.xxh3_128("data to hash")
# => 329877766635069010451249428498779834881

# Streaming API for large data
state = :exhash.xxh3_64_init()
state = :exhash.xxh3_64_update(state, "chunk 1")
state = :exhash.xxh3_64_update(state, "chunk 2")
:exhash.xxh3_64_final(state)
```

**Performance**: ~31 GB/s on modern hardware (vs ~0.5 GB/s for MD5)

**Use Cases**:
- Hash tables and maps
- Data deduplication
- Non-cryptographic checksums
- Bloom filter hashing

### :b3 (BLAKE3)

Fast cryptographic hashing suitable for content addressing and integrity verification.

```elixir
# mix.exs
defp deps do
  [{:b3, "~> 0.2"}]
end

# Basic hashing
B3.hash("data to hash")
# => <<...32 bytes...>>

# Keyed hashing (MAC)
key = :crypto.strong_rand_bytes(32)
B3.keyed_hash(key, "message")

# Streaming for large files
state = B3.new()
state = B3.update(state, chunk1)
state = B3.update(state, chunk2)
B3.finalize(state)
```

**Performance**: ~2.5 GB/s (vs ~0.4 GB/s for SHA-256)

**Use Cases**:
- Content-addressed storage
- File integrity verification
- Message authentication codes

## Probabilistic Data Structures

### HyperLogLog

Cardinality estimation with configurable accuracy.

```elixir
# mix.exs
defp deps do
  [{:hyperloglog, "~> 1.0"}]
end

# Create with precision (higher = more accurate, more memory)
hll = HyperLogLog.new(14)  # 14-bit = 16,384 registers, ~0.8% error

# Add elements
hll = HyperLogLog.add(hll, "user_1")
hll = HyperLogLog.add(hll, "user_2")
hll = HyperLogLog.add(hll, "user_1")  # Duplicate, won't increase count

# Get estimated count
HyperLogLog.count(hll)  # ~2

# Merge from multiple nodes
hll_node1 = HyperLogLog.add(HyperLogLog.new(14), "user_1")
hll_node2 = HyperLogLog.add(HyperLogLog.new(14), "user_2")
merged = HyperLogLog.merge(hll_node1, hll_node2)
```

**Memory vs Accuracy**:

| Precision | Registers | Memory | Error Rate |
|-----------|-----------|--------|------------|
| 10 | 1,024 | 1 KB | ~3.2% |
| 12 | 4,096 | 4 KB | ~1.6% |
| 14 | 16,384 | 16 KB | ~0.8% |
| 16 | 65,536 | 64 KB | ~0.4% |

### Bloom Filters (bloomex)

Probabilistic set membership testing.

```elixir
# mix.exs
defp deps do
  [{:bloomex, "~> 1.0"}]
end

# Create with capacity and false positive rate
bloom = Bloomex.scalable(10_000, 0.01)  # 10k items, 1% FPR

# Add elements
bloom = Bloomex.add(bloom, "item_1")
bloom = Bloomex.add(bloom, "item_2")

# Check membership
Bloomex.member?(bloom, "item_1")  # true
Bloomex.member?(bloom, "item_3")  # false (or rare false positive)
```

**Note**: Bloom filters do not support deletion. Use Cuckoo filters if deletion is needed.

### Cuckoo Filters

Set membership with deletion support.

```elixir
# mix.exs
defp deps do
  [{:cuckoo_filter, "~> 0.1"}]
end

# Create filter
filter = CuckooFilter.new(capacity: 1_000_000, fpr: 0.01)

# Add and check
filter = CuckooFilter.add(filter, "cached_key")
CuckooFilter.contains?(filter, "cached_key")  # true

# Delete (not possible with Bloom filters)
filter = CuckooFilter.delete(filter, "cached_key")
CuckooFilter.contains?(filter, "cached_key")  # false
```

### Count-Min Sketch

Frequency estimation for streams.

```elixir
# Custom implementation (no stable library)
defmodule CountMinSketch do
  defstruct [:width, :depth, :table, :hash_seeds]

  def new(epsilon, delta) do
    width = ceil(:math.e() / epsilon)
    depth = ceil(:math.log(1 / delta))
    table = :atomics.new(width * depth, signed: false)
    seeds = for _ <- 1..depth, do: :rand.uniform(1_000_000)

    %__MODULE__{width: width, depth: depth, table: table, hash_seeds: seeds}
  end

  def add(%__MODULE__{} = cms, item, count \\ 1) do
    for {seed, row} <- Enum.with_index(cms.hash_seeds) do
      col = hash(item, seed, cms.width)
      index = row * cms.width + col + 1
      :atomics.add(cms.table, index, count)
    end
    cms
  end

  def count(%__MODULE__{} = cms, item) do
    for {seed, row} <- Enum.with_index(cms.hash_seeds) do
      col = hash(item, seed, cms.width)
      index = row * cms.width + col + 1
      :atomics.get(cms.table, index)
    end
    |> Enum.min()
  end

  defp hash(item, seed, width) do
    :erlang.phash2({item, seed}, width)
  end
end
```

## GenServer Patterns for Stateful Algorithms

### HyperLogLog Service

```elixir
defmodule MyApp.Analytics.UniqueVisitors do
  use GenServer

  def start_link(opts) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  def init(_opts) do
    # Reset daily
    schedule_reset()
    {:ok, %{hll: HyperLogLog.new(14), date: Date.utc_today()}}
  end

  def track(user_id) do
    GenServer.cast(__MODULE__, {:track, user_id})
  end

  def count do
    GenServer.call(__MODULE__, :count)
  end

  def handle_cast({:track, user_id}, state) do
    hll = HyperLogLog.add(state.hll, to_string(user_id))
    {:noreply, %{state | hll: hll}}
  end

  def handle_call(:count, _from, state) do
    {:reply, HyperLogLog.count(state.hll), state}
  end

  def handle_info(:reset, _state) do
    schedule_reset()
    {:noreply, %{hll: HyperLogLog.new(14), date: Date.utc_today()}}
  end

  defp schedule_reset do
    # Reset at midnight UTC
    now = DateTime.utc_now()
    tomorrow = Date.add(Date.utc_today(), 1)
    midnight = DateTime.new!(tomorrow, ~T[00:00:00], "Etc/UTC")
    delay = DateTime.diff(midnight, now, :millisecond)
    Process.send_after(self(), :reset, delay)
  end
end
```

### Cache Filter Service

```elixir
defmodule MyApp.Cache.Filter do
  use GenServer

  def start_link(opts) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  def init(_opts) do
    filter = CuckooFilter.new(capacity: 10_000_000, fpr: 0.01)
    {:ok, %{filter: filter}}
  end

  def mark_cached(key), do: GenServer.cast(__MODULE__, {:add, key})
  def invalidate(key), do: GenServer.cast(__MODULE__, {:delete, key})
  def is_cached?(key), do: GenServer.call(__MODULE__, {:contains, key})

  def handle_cast({:add, key}, state) do
    {:noreply, %{state | filter: CuckooFilter.add(state.filter, key)}}
  end

  def handle_cast({:delete, key}, state) do
    {:noreply, %{state | filter: CuckooFilter.delete(state.filter, key)}}
  end

  def handle_call({:contains, key}, _from, state) do
    {:reply, CuckooFilter.contains?(state.filter, key), state}
  end
end
```

## NIF/Rustler for Performance

### When to Use NIFs

- CPU-intensive algorithms (hashing, compression, encryption)
- Algorithms with no good Elixir library
- Performance-critical hot paths
- Leveraging existing Rust/C libraries

### Rustler Setup

```elixir
# mix.exs
defp deps do
  [{:rustler, "~> 0.30"}]
end

# Generate NIF module
mix rustler.new MyApp.Native
```

```rust
// native/myapp_native/src/lib.rs
use rustler::{Encoder, Env, NifResult, Term};

#[rustler::nif]
fn xxhash(data: &[u8]) -> u64 {
    xxhash_rust::xxh3::xxh3_64(data)
}

rustler::init!("Elixir.MyApp.Native", [xxhash]);
```

```elixir
# lib/myapp/native.ex
defmodule MyApp.Native do
  use Rustler, otp_app: :myapp, crate: "myapp_native"

  def xxhash(_data), do: :erlang.nif_error(:nif_not_loaded)
end
```

### Dirty Schedulers

For long-running NIFs that might block the scheduler:

```rust
#[rustler::nif(schedule = "DirtyCpu")]
fn expensive_computation(data: &[u8]) -> Vec<u8> {
    // CPU-intensive work
}

#[rustler::nif(schedule = "DirtyIo")]
fn disk_operation(path: &str) -> Vec<u8> {
    // I/O-bound work
}
```

## Library Recommendations

| Library | Algorithm | Use Case | Notes |
|---------|-----------|----------|-------|
| `exhash` | xxHash3 | Non-crypto hashing | Fastest option |
| `:b3` | BLAKE3 | Crypto hashing | Fast and secure |
| `hyperloglog` | HyperLogLog | Cardinality estimation | Mature |
| `bloomex` | Bloom filter | Set membership | No deletion |
| `cuckoo_filter` | Cuckoo filter | Set membership | With deletion |
| `delta_crdt` | CRDTs | Distributed state | Production-ready |
| `rustler` | NIFs | Native code | Rust bindings |

## Additional Resources

- [xxHash specification](https://github.com/Cyan4973/xxHash)
- [BLAKE3 paper](https://github.com/BLAKE3-team/BLAKE3-specs/blob/master/blake3.pdf)
- [HyperLogLog paper](http://algo.inria.fr/flajolet/Publications/FlFuGaMe07.pdf)
- [Cuckoo Filter paper](https://www.cs.cmu.edu/~dga/papers/cuckoo-conext2014.pdf)
- [Rustler documentation](https://docs.rs/rustler)
