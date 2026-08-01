# Elixir Benchmark Templates (Benchee)

Ready-to-use templates for creating benchmarks with Benchee. For profiling methodology and tool selection, see the `performance-analyzer` skill\'s elixir reference.

## Setup

Add Benchee to your project:

```elixir
# mix.exs
defp deps do
  [{:benchee, "~> 1.0", only: :dev}]
end
```

Create benchmark directory:

```bash
mkdir -p bench
```

## Template: Single Function Benchmark

Basic benchmark for a single function with multiple input sizes.

```elixir
# bench/module_name_benchmark.exs
#
# Benchmark: MyApp.ModuleName.function_name/1
# Created: YYYY-MM-DD
# Complexity: O(n) / O(n^2) / etc.
# Purpose: [Brief description of what we're measuring]

# Load application and dependencies
Mix.install([])
Application.ensure_all_started(:my_app)

# Data generators
defmodule BenchData do
  def generate_items(count) do
    Enum.map(1..count, fn i ->
      %{id: i, name: "Item #{i}", value: :rand.uniform(1000)}
    end)
  end
end

# Input sizes matching production patterns
inputs = %{
  "small (100)" => BenchData.generate_items(100),
  "typical (1,000)" => BenchData.generate_items(1_000),
  "peak (10,000)" => BenchData.generate_items(10_000)
}

Benchee.run(
  %{
    "function_name" => fn items -> MyApp.ModuleName.function_name(items) end
  },
  inputs: inputs,
  warmup: 2,
  time: 5,
  memory_time: 2,
  print: [
    benchmarking: true,
    configuration: true,
    fast_warning: true
  ],
  formatters: [
    Benchee.Formatters.Console,
    {Benchee.Formatters.HTML, file: "bench/results/module_name.html"}
  ]
)
```

## Template: Comparison Benchmark

Compare two or more implementations.

```elixir
# bench/comparison_benchmark.exs
#
# Benchmark: Compare Enum vs Stream for large list processing
# Created: YYYY-MM-DD
# Context: Evaluating lazy vs eager evaluation for batch processing

Mix.install([])
Application.ensure_all_started(:my_app)

defmodule BenchData do
  def generate_orders(count) do
    Enum.map(1..count, fn i ->
      %{
        id: "ORD-#{i}",
        line_items: Enum.map(1..5, fn j ->
          %{sku: "SKU-#{:rand.uniform(10_000)}", quantity: :rand.uniform(10)}
        end),
        total: :rand.uniform(10_000) / 100
      }
    end)
  end
end

# Implementations to compare
defmodule Implementations do
  # Original O(n^2) approach
  def process_eager(orders, inventory_skus) do
    orders
    |> Enum.map(fn order ->
      filtered_items = Enum.filter(order.line_items, fn item ->
        Enum.member?(inventory_skus, item.sku)
      end)
      %{order | line_items: filtered_items}
    end)
  end

  # Optimized with set lookup
  def process_with_set(orders, inventory_skus) do
    sku_set = MapSet.new(inventory_skus)

    orders
    |> Enum.map(fn order ->
      filtered_items = Enum.filter(order.line_items, fn item ->
        MapSet.member?(sku_set, item.sku)
      end)
      %{order | line_items: filtered_items}
    end)
  end

  # Stream-based lazy approach
  def process_lazy(orders, inventory_skus) do
    sku_set = MapSet.new(inventory_skus)

    orders
    |> Stream.map(fn order ->
      filtered_items = Enum.filter(order.line_items, fn item ->
        MapSet.member?(sku_set, item.sku)
      end)
      %{order | line_items: filtered_items}
    end)
    |> Enum.to_list()
  end
end

# Generate test data
inventory_skus = Enum.map(1..5_000, fn i -> "SKU-#{i}" end)

inputs = %{
  "small (100 orders)" => {BenchData.generate_orders(100), inventory_skus},
  "typical (1,000 orders)" => {BenchData.generate_orders(1_000), inventory_skus},
  "peak (10,000 orders)" => {BenchData.generate_orders(10_000), inventory_skus}
}

Benchee.run(
  %{
    "eager (Enum.member?)" => fn {orders, skus} ->
      Implementations.process_eager(orders, skus)
    end,
    "set lookup (MapSet)" => fn {orders, skus} ->
      Implementations.process_with_set(orders, skus)
    end,
    "lazy (Stream + MapSet)" => fn {orders, skus} ->
      Implementations.process_lazy(orders, skus)
    end
  },
  inputs: inputs,
  warmup: 2,
  time: 10,
  memory_time: 5,
  formatters: [
    Benchee.Formatters.Console,
    {Benchee.Formatters.HTML, file: "bench/results/comparison.html"}
  ]
)
```

## Template: Before/After Optimization

Benchmark current vs optimized implementation.

```elixir
# bench/optimization_benchmark.exs
#
# Benchmark: Order processor optimization
# Created: YYYY-MM-DD
# Before: O(n x m x k) nested loops
# After: O(n x m) with preprocessed lookup
# Expected improvement: ~10-100x at production scale

Mix.install([])
Application.ensure_all_started(:my_app)

# The actual module implementations
# (Or inline both versions here for standalone benchmark)

defmodule OriginalImplementation do
  def process_batch(orders, inventory) do
    Enum.map(orders, fn order ->
      line_items =
        Enum.filter(order.line_items, fn item ->
          Enum.any?(inventory, fn inv -> inv.sku == item.sku end)
        end)
      %{order | line_items: line_items}
    end)
  end
end

defmodule OptimizedImplementation do
  def process_batch(orders, inventory) do
    # Build lookup set once: O(k)
    inventory_skus = MapSet.new(inventory, & &1.sku)

    # Process with set lookup: O(n x m)
    Enum.map(orders, fn order ->
      line_items =
        Enum.filter(order.line_items, fn item ->
          MapSet.member?(inventory_skus, item.sku)
        end)
      %{order | line_items: line_items}
    end)
  end
end

# Data generators
defmodule BenchData do
  def make_inventory(count) do
    Enum.map(1..count, fn i ->
      %{sku: "SKU-#{i}", quantity: :rand.uniform(100)}
    end)
  end

  def make_orders(count, items_per_order) do
    Enum.map(1..count, fn i ->
      %{
        id: "ORD-#{i}",
        line_items: Enum.map(1..items_per_order, fn _ ->
          %{sku: "SKU-#{:rand.uniform(10_000)}", quantity: :rand.uniform(10)}
        end)
      }
    end)
  end
end

# Production-like scenarios
scenarios = %{
  "small (10 orders, 100 inventory)" => {
    BenchData.make_orders(10, 5),
    BenchData.make_inventory(100)
  },
  "typical (100 orders, 1K inventory)" => {
    BenchData.make_orders(100, 5),
    BenchData.make_inventory(1_000)
  },
  "peak (1K orders, 5K inventory)" => {
    BenchData.make_orders(1_000, 10),
    BenchData.make_inventory(5_000)
  }
}

Benchee.run(
  %{
    "original O(n x m x k)" => fn {orders, inventory} ->
      OriginalImplementation.process_batch(orders, inventory)
    end,
    "optimized O(n x m)" => fn {orders, inventory} ->
      OptimizedImplementation.process_batch(orders, inventory)
    end
  },
  inputs: scenarios,
  warmup: 2,
  time: 10,
  memory_time: 5,
  formatters: [
    Benchee.Formatters.Console,
    {Benchee.Formatters.HTML, file: "bench/results/optimization.html"}
  ]
)
```

## Template: Database Query Benchmark

Benchmark Ecto queries with realistic data.

```elixir
# bench/query_benchmark.exs
#
# Benchmark: User search query performance
# Created: YYYY-MM-DD
# Purpose: Compare query strategies for user search

Mix.install([])
Application.ensure_all_started(:my_app)

alias MyApp.Repo
alias MyApp.Accounts.User

# Ensure database has test data
# (Run setup separately or use existing dev/test data)

defmodule Queries do
  import Ecto.Query

  # N+1 approach (bad)
  def search_with_n_plus_1(term) do
    User
    |> where([u], ilike(u.name, ^"%#{term}%"))
    |> Repo.all()
    |> Enum.map(fn user ->
      # N+1: loads orders separately for each user
      orders = Repo.all(from o in Order, where: o.user_id == ^user.id)
      %{user | orders: orders}
    end)
  end

  # Preload approach (better)
  def search_with_preload(term) do
    User
    |> where([u], ilike(u.name, ^"%#{term}%"))
    |> preload(:orders)
    |> Repo.all()
  end

  # Join approach (best for filtering)
  def search_with_join(term) do
    User
    |> where([u], ilike(u.name, ^"%#{term}%"))
    |> join(:left, [u], o in assoc(u, :orders))
    |> preload([u, o], [orders: o])
    |> Repo.all()
  end
end

# Search terms that match different result counts
inputs = %{
  "few results (10)" => "rare_name",
  "medium results (100)" => "common",
  "many results (1000)" => "a"
}

Benchee.run(
  %{
    "N+1 queries" => fn term -> Queries.search_with_n_plus_1(term) end,
    "preload" => fn term -> Queries.search_with_preload(term) end,
    "join + preload" => fn term -> Queries.search_with_join(term) end
  },
  inputs: inputs,
  warmup: 1,
  time: 5,
  formatters: [Benchee.Formatters.Console]
)
```

## Template: GenServer Performance

Benchmark GenServer call/cast performance.

```elixir
# bench/genserver_benchmark.exs
#
# Benchmark: Cache GenServer performance
# Created: YYYY-MM-DD
# Purpose: Measure call latency under different cache sizes

Mix.install([])
Application.ensure_all_started(:my_app)

# Start the GenServer if not already running
{:ok, _pid} = MyApp.Cache.start_link([])

# Pre-populate cache with different sizes
defmodule CacheSetup do
  def populate(cache_pid, count) do
    Enum.each(1..count, fn i ->
      MyApp.Cache.put(cache_pid, "key_#{i}", %{data: :rand.bytes(1000)})
    end)
  end

  def clear(cache_pid) do
    MyApp.Cache.clear(cache_pid)
  end
end

# Benchmark scenarios
Benchee.run(
  %{
    "cache hit" => fn ->
      MyApp.Cache.get(:cache, "key_500")
    end,
    "cache miss" => fn ->
      MyApp.Cache.get(:cache, "nonexistent_key")
    end,
    "cache put" => fn ->
      key = "bench_#{:rand.uniform(10_000)}"
      MyApp.Cache.put(:cache, key, %{data: "value"})
    end
  },
  before_scenario: fn _input ->
    CacheSetup.clear(:cache)
    CacheSetup.populate(:cache, 10_000)
  end,
  warmup: 2,
  time: 5,
  formatters: [Benchee.Formatters.Console]
)
```

## Running Benchmarks

```bash
# Run single benchmark
mix run bench/module_name_benchmark.exs

# Run all benchmarks
for f in bench/*.exs; do mix run "$f"; done

# With reduced output
mix run bench/module_name_benchmark.exs 2>/dev/null

# View HTML report
open bench/results/module_name.html
```

## Interpreting Results

**Key metrics**:
- **IPS**: Iterations per second (higher is better)
- **Average**: Mean time per operation
- **Deviation**: Consistency (lower is better, <15% is good)
- **Median**: Middle value (less affected by outliers)
- **Memory**: Bytes allocated per call

**Comparison output**:
```
Comparison:
optimized O(n x m)       1.85 K
original O(n x m x k)    0.24 K - 7.55x slower +3.54 ms
```

The baseline is always the fastest implementation.

## Best Practices

1. **Isolate benchmarks** - Run one at a time for accurate results
2. **Warm up** - Let the BEAM JIT compile before measuring
3. **Use realistic data** - Match production patterns and sizes
4. **Document context** - Include creation date, complexity, purpose
5. **Save results** - Use HTML formatter for detailed analysis
6. **Commit benchmarks** - Version control for reproducibility
7. **Re-run after changes** - Verify optimizations actually help

## Additional Resources

For profiling tools (cprof, eprof, fprof, tprof):
- See the `performance-analyzer` skill\'s elixir reference
