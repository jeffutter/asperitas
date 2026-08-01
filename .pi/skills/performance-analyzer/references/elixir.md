# Elixir Reference: Performance Analysis

## Overview

Elixir performance analysis uses BEAM VM profilers and Benchee for benchmarking. The BEAM provides multiple profiling tools with different tradeoffs between overhead and detail.

## Benchee

Statistical benchmarking with warmup, memory measurement, and comparison.

### Installation

```elixir
# mix.exs
defp deps do
  [{:benchee, "~> 1.0", only: :dev}]
end
```

### Basic Usage

```elixir
Benchee.run(%{
  "map" => fn -> Enum.map(1..1000, &(&1 * 2)) end,
  "comprehension" => fn -> for x <- 1..1000, do: x * 2 end
})
```

### Configuration Options

```elixir
Benchee.run(%{
  "baseline" => fn input -> baseline(input) end,
  "optimized" => fn input -> optimized(input) end
},
  warmup: 2,           # Warmup time in seconds
  time: 5,             # Measurement time in seconds
  memory_time: 2,      # Memory measurement time
  reduction_time: 2,   # Reduction counting time
  parallel: 1,         # Parallel jobs (1 for single-threaded)
  pre_check: true,     # Run once before benchmarking
  print: [             # Output configuration
    benchmarking: true,
    configuration: true,
    fast_warning: true
  ]
)
```

### Input Variations

```elixir
Benchee.run(%{
  "Enum.map" => fn list -> Enum.map(list, &(&1 * 2)) end,
  "Stream.map" => fn list -> list |> Stream.map(&(&1 * 2)) |> Enum.to_list() end
},
  inputs: %{
    "small (100)" => Enum.to_list(1..100),
    "medium (10,000)" => Enum.to_list(1..10_000),
    "large (1,000,000)" => Enum.to_list(1..1_000_000)
  }
)
```

### Before/After Scenarios

```elixir
Benchee.run(%{
  "with setup" => {
    fn input -> process(input) end,
    before_scenario: fn _input -> expensive_setup() end,
    after_scenario: fn _result -> cleanup() end
  }
})
```

### Formatters

```elixir
Benchee.run(scenarios,
  formatters: [
    Benchee.Formatters.Console,
    {Benchee.Formatters.HTML, file: "output/benchmark.html"},
    {Benchee.Formatters.CSV, file: "output/benchmark.csv"}
  ]
)
```

## BEAM Profilers

### cprof - Call Counting

Counts function calls with minimal overhead. Use to identify hot functions.

```elixir
# Start profiling
:cprof.start()

# Run code
MyModule.function(args)

# Stop and get results
:cprof.pause()
:cprof.analyse()

# Or use mix task
mix profile.cprof -e "MyModule.function(args)"
```

**Output interpretation**:
```
                                                      CNT
Total                                                1234
...
MyApp.Worker.process/1                                456
  MyApp.Worker."-process/1-fun-0-"/1                  456
  Enum.map/2                                          456
```

**When to use**: Finding which functions are called most frequently (hotspots by call count).

### eprof - Time Profiling

Measures total time per function.

```elixir
# Profile specific modules
:eprof.start()
:eprof.profile([self()], fn -> MyModule.function(args) end, [set_on_spawn: true])
:eprof.stop()
:eprof.analyze(:total)

# Or use mix task
mix profile.eprof -e "MyModule.function(args)"
```

**Output interpretation**:
```
FUNCTION                         CALLS      %   TIME  [uS / CALLS]
--------                         -----    ---   ----  [----------]
MyApp.Worker.process/1             100  45.2   1234  [12.34]
Enum.map/2                         100  23.1    634  [6.34]
```

**When to use**: Finding where total time is spent (not call count).

### fprof - Detailed Profiling

Full call graph with detailed timing. High overhead but most detailed.

```elixir
# Profile to file
:fprof.trace([:start, {:procs, :all}])
MyModule.function(args)
:fprof.trace(:stop)
:fprof.profile()
:fprof.analyse([:totals, {:dest, :standard_io}])

# Or use mix task
mix profile.fprof -e "MyModule.function(args)"
```

**Output interpretation**:
```
                                     CNT       ACC       OWN
MyApp.Worker.process/1               100      1234       456
  MyApp.Helper.transform/1           100       778       234
    Enum.map/2                       100       544       544
```

- **CNT**: Call count
- **ACC**: Accumulated time (including callees)
- **OWN**: Own time (excluding callees)

**When to use**: Understanding full call graph and where time accumulates.

### tprof (OTP 27+)

Unified profiler replacing eprof/fprof.

```elixir
# Time profiling
:tprof.profile(fn -> MyModule.function(args) end, type: :time)

# Memory profiling
:tprof.profile(fn -> MyModule.function(args) end, type: :memory)

# Or use mix task
mix profile.tprof -e "MyModule.function(args)" --type memory
```

**When to use**: OTP 27+ projects needing time or memory profiling.

## Process Inspection

### Observer

GUI tool for runtime inspection.

```elixir
# Start Observer
:observer.start()
```

**Key tabs**:
- **Processes**: List all processes, sort by memory/reductions
- **Applications**: Supervision tree visualization
- **System**: Memory, scheduler usage
- **Table Viewer**: ETS/Mnesia inspection

### :recon

Production-safe inspection library.

```elixir
# mix.exs
{:recon, "~> 2.5"}

# Top processes by memory
:recon.proc_count(:memory, 10)

# Top processes by message queue
:recon.proc_count(:message_queue_len, 10)

# Top processes by reductions
:recon.proc_count(:reductions, 10)

# Binary memory usage
:recon.bin_leak(10)

# Scheduler usage
:recon.scheduler_usage(1000)

# Remote node inspection
:recon.rpc_call(:"node@host", :recon, :proc_count, [:memory, 10])
```

## ETS Optimization

### Table Type Selection

```elixir
# set: Fast lookup, unique keys
:ets.new(:cache, [:set, :public, :named_table])

# ordered_set: Ordered by key, range queries
:ets.new(:timeline, [:ordered_set, :public, :named_table])

# bag: Multiple values per key
:ets.new(:tags, [:bag, :public, :named_table])

# duplicate_bag: Allow duplicate key-value pairs
:ets.new(:events, [:duplicate_bag, :public, :named_table])
```

### Concurrency Options

```elixir
# High read concurrency (many readers)
:ets.new(:read_heavy, [:set, :public, {:read_concurrency, true}])

# High write concurrency (many writers)
:ets.new(:write_heavy, [:set, :public, {:write_concurrency, true}])

# Both (read-heavy with occasional writes)
:ets.new(:balanced, [:set, :public,
  {:read_concurrency, true},
  {:write_concurrency, true}
])
```

### Match Specifications

```elixir
# Efficient filtering in ETS
match_spec = [{{:_, :"$1", :"$2"}, [{:>, :"$1", 100}], [:"$2"]}]
:ets.select(:my_table, match_spec)

# Using ex2ms for readable match specs
import Ex2ms

fun = Ex2ms.fun do {key, value, timestamp} when timestamp > 1000 -> {key, value} end
:ets.select(:my_table, fun)
```

## Tool Selection Guide

| Question | Tool |
|----------|------|
| How many times is function called? | cprof |
| Where is time spent (total)? | eprof/tprof |
| Full call graph with timing? | fprof |
| Memory allocations? | tprof --type memory |
| Compare implementations? | Benchee |
| Production process inspection? | :recon |
| GUI inspection? | :observer |

## Common Performance Patterns

### N+1 Query Detection

```elixir
# Log queries in test/dev
config :my_app, MyApp.Repo,
  log: :debug

# Then grep logs for repeated similar queries
```

### Memory Leak Detection

```elixir
# Track binary memory
:recon.bin_leak(5)

# Find large message queues
:recon.proc_count(:message_queue_len, 10)

# Check process count over time
:erlang.system_info(:process_count)
```

### Scheduler Analysis

```elixir
# Check scheduler utilization
:recon.scheduler_usage(1000)  # Sample for 1 second

# Output: [{1, 0.85}, {2, 0.82}, ...]
# Values > 0.9 indicate scheduler saturation
```

## Additional Resources

- [Benchee documentation](https://hexdocs.pm/benchee)
- [recon documentation](https://ferd.github.io/recon/)
- [Erlang Efficiency Guide](https://www.erlang.org/doc/efficiency_guide/users_guide.html)
- [Phoenix Performance Guide](https://hexdocs.pm/phoenix/performance.html)
