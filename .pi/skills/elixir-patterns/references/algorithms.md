# Elixir Algorithm Libraries and Implementations

This reference covers Elixir-specific algorithm libraries, implementation patterns, and ecosystem recommendations.

## Library Ecosystem

### Recommended Libraries by Category

| Category | Recommended | Alternative | Notes |
|----------|-------------|-------------|-------|
| Hash (non-crypto) | `exhash` | Roll your own NIF | Mature, well-tested, xxHash support |
| Hash (crypto) | `b3` | Built-in `:crypto` | BLAKE3 faster than SHA-256 |
| HyperLogLog | `hyperloglog` | `hypex` | More complete API |
| Bloom Filters | `bloomex` | `exbloom` | Better documentation |
| Cuckoo Filters | `cuckoo_filter` | Write custom | Beta but usable |
| Count-Min Sketch | Write custom | - | No stable library |
| Geospatial | `geo` + `topo` | `geocalc` | Full geometry support |
| Graph algorithms | `libgraph` | `graph` | More features |

### Library Evaluation Scorecard Template

```markdown
## [Library Name] (https://hex.pm/packages/...)

**Maturity**: [Experimental/Beta/Stable/Production]
**Last Updated**: [Date]
**Stars/Downloads**: [GitHub stars / Hex downloads]
**Maintainership**: [Active/Maintained/Stale]

**API Quality**:
- Documentation: [Coverage and clarity]
- Examples: [Quality and completeness]
- Testing: [Test coverage percentage]

**Performance**:
- Benchmarks: [Available benchmarks]
- Production use: [Known users]
- Bottlenecks: [Known performance issues]

**Elixir Integration**:
- Idiomatic: [Follows Elixir conventions]
- OTP patterns: [GenServer/Supervisor support]
- Types: [Typespec coverage]

**Recommendation**: [Use/Consider/Avoid]
```

## Implementation Patterns

### GenServer Wrapper Pattern

Most algorithm implementations in Elixir should be wrapped in a GenServer for concurrent access:

```elixir
defmodule MyApp.TopK do
  use GenServer

  defstruct counters: %{}, min_heap: [], k: 100

  def start_link(opts \\ []) do
    k = Keyword.get(opts, :k, 100)
    GenServer.start_link(__MODULE__, %__MODULE__{k: k}, name: __MODULE__)
  end

  def track_event(event_type) do
    GenServer.cast(__MODULE__, {:track, event_type})
  end

  def get_top_k do
    GenServer.call(__MODULE__, :top_k)
  end

  @impl true
  def init(state) do
    {:ok, state}
  end

  @impl true
  def handle_cast({:track, event_type}, state) do
    state =
      if Map.has_key?(state.counters, event_type) do
        update_in(state.counters[event_type], &(&1 + 1))
      else
        if map_size(state.counters) < state.k do
          put_in(state.counters[event_type], 1)
        else
          # Space-Saving: Replace minimum counter
          {min_type, min_count} = find_minimum(state)
          state
          |> update_in([:counters], &Map.delete(&1, min_type))
          |> put_in([:counters, event_type], min_count + 1)
        end
      end

    {:noreply, rebuild_heap(state)}
  end

  @impl true
  def handle_call(:top_k, _from, state) do
    top_k =
      state.counters
      |> Enum.sort_by(fn {_type, count} -> count end, :desc)
      |> Enum.take(state.k)

    {:reply, top_k, state}
  end

  defp find_minimum(state) do
    Enum.min_by(state.counters, fn {_type, count} -> count end)
  end

  defp rebuild_heap(state) do
    # Optimize with proper heap data structure for production
    state
  end
end
```

### ETS-Based Pattern (High Throughput)

For higher throughput where GenServer becomes a bottleneck:

```elixir
defmodule MyApp.FastCounter do
  @table_name :fast_counters

  def init do
    :ets.new(@table_name, [:named_table, :public, :set, {:write_concurrency, true}])
  end

  def increment(key) do
    :ets.update_counter(@table_name, key, {2, 1}, {key, 0})
  end

  def get(key) do
    case :ets.lookup(@table_name, key) do
      [{^key, count}] -> count
      [] -> 0
    end
  end

  def top_k(k) do
    @table_name
    |> :ets.tab2list()
    |> Enum.sort_by(fn {_key, count} -> count end, :desc)
    |> Enum.take(k)
  end
end
```

### NIF Pattern (Rustler)

For CPU-intensive algorithms, use Rustler NIFs:

```elixir
# mix.exs
defp deps do
  [
    {:rustler, "~> 0.30"}
  ]
end

# lib/my_app/native.ex
defmodule MyApp.Native do
  use Rustler, otp_app: :my_app, crate: "my_app_native"

  # NIF stubs - replaced at runtime
  def hyperloglog_new(_precision), do: :erlang.nif_error(:nif_not_loaded)
  def hyperloglog_add(_hll, _value), do: :erlang.nif_error(:nif_not_loaded)
  def hyperloglog_count(_hll), do: :erlang.nif_error(:nif_not_loaded)
  def hyperloglog_merge(_hll1, _hll2), do: :erlang.nif_error(:nif_not_loaded)
end
```

```rust
// native/my_app_native/src/lib.rs
use rustler::{Env, Term, NifResult, ResourceArc};
use hyperloglog::HyperLogLog;

struct HllResource(std::sync::Mutex<HyperLogLog>);

#[rustler::nif]
fn hyperloglog_new(precision: u8) -> ResourceArc<HllResource> {
    ResourceArc::new(HllResource(
        std::sync::Mutex::new(HyperLogLog::new(precision))
    ))
}

#[rustler::nif]
fn hyperloglog_add(hll: ResourceArc<HllResource>, value: &str) -> () {
    hll.0.lock().unwrap().add(value);
}

#[rustler::nif]
fn hyperloglog_count(hll: ResourceArc<HllResource>) -> u64 {
    hll.0.lock().unwrap().count() as u64
}

rustler::init!("Elixir.MyApp.Native", [
    hyperloglog_new,
    hyperloglog_add,
    hyperloglog_count
]);
```

## When No Library Exists

### Option 1: Pure Elixir

**Pros**: No dependencies, easier deployment
**Cons**: Slower for CPU-intensive operations

**When to choose**:
- Algorithm is simple
- Performance isn't critical
- Rapid prototyping

### Option 2: NIF (Rustler)

**Pros**: Native performance, safe (Rustler prevents crashes)
**Cons**: Compilation complexity, requires Rust toolchain

**When to choose**:
- CPU-bound algorithm
- Performance is critical
- Algorithm has mature Rust implementation

### Option 3: Port to External Process

**Pros**: Isolation (crashes don't affect BEAM), can use any language
**Cons**: IPC overhead, more complex deployment

**When to choose**:
- Need to use existing C/C++ implementation
- Memory-intensive operations
- Algorithm stability is uncertain

## Elixir-Specific Considerations

### Immutability

Elixir's immutable data structures affect algorithm design:

```elixir
# Inefficient: rebuilding on each insert
def insert(tree, value) do
  %{tree | nodes: [value | tree.nodes]}  # Creates new struct
end

# More efficient: use persistent data structures
# or ETS for mutable state when needed
```

### Concurrency

Leverage BEAM's concurrency model:

```elixir
# Parallel processing with Task.async_stream
items
|> Task.async_stream(&expensive_computation/1, max_concurrency: System.schedulers_online())
|> Enum.to_list()

# Distributed counting with multiple processes
defmodule DistributedHLL do
  def count_parallel(streams) do
    streams
    |> Task.async_stream(&count_stream/1)
    |> Enum.reduce(HyperLogLog.new(), &HyperLogLog.merge/2)
  end
end
```

### Memory

Be aware of Elixir memory characteristics:

```elixir
# Binary data is efficient (reference counted)
# Small integers are immediate values
# Large data structures should use :persistent_term for read-heavy access

# For large static lookup tables
:persistent_term.put(:my_lookup, build_lookup_table())
```

## Example: HyperLogLog Integration

Complete example of integrating HyperLogLog in an Elixir application:

```elixir
defmodule MyApp.UniqueVisitors do
  use GenServer

  # Using the hyperloglog package

  def start_link(opts \\ []) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  def track(visitor_id) do
    GenServer.cast(__MODULE__, {:track, visitor_id})
  end

  def count do
    GenServer.call(__MODULE__, :count)
  end

  def merge(other_hll) do
    GenServer.call(__MODULE__, {:merge, other_hll})
  end

  @impl true
  def init(_opts) do
    # 14-bit precision = 16KB memory, ~0.8% error
    {:ok, HyperLogLog.new(14)}
  end

  @impl true
  def handle_cast({:track, visitor_id}, hll) do
    {:noreply, HyperLogLog.add(hll, visitor_id)}
  end

  @impl true
  def handle_call(:count, _from, hll) do
    {:reply, HyperLogLog.count(hll), hll}
  end

  @impl true
  def handle_call({:merge, other_hll}, _from, hll) do
    {:reply, :ok, HyperLogLog.merge(hll, other_hll)}
  end
end

# Supervisor integration
defmodule MyApp.Application do
  use Application

  def start(_type, _args) do
    children = [
      MyApp.UniqueVisitors
    ]

    opts = [strategy: :one_for_one, name: MyApp.Supervisor]
    Supervisor.start_link(children, opts)
  end
end
```

## Performance Benchmarking

Use Benchee for algorithm comparison:

```elixir
# benchmark/algorithms_bench.exs
Benchee.run(%{
  "MapSet (exact)" => fn ->
    Enum.reduce(1..100_000, MapSet.new(), &MapSet.put(&2, &1))
    |> MapSet.size()
  end,
  "HyperLogLog (approximate)" => fn ->
    Enum.reduce(1..100_000, HyperLogLog.new(14), &HyperLogLog.add(&2, &1))
    |> HyperLogLog.count()
  end
}, memory_time: 2)
```

## References

- **OTP patterns**: the `elixir-patterns` skill\'s otp reference
- **Testing algorithms**: the elixir testing.md reference
- **Performance profiling**: the `performance-analyzer` skill
