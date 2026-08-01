---
name: performance-analyzer
description: Use when analyzing performance issues, creating benchmarks, or optimizing code
---

# Performance Analysis

## Overview

**Never optimize without profiling data.** Code review cannot tell you where bottlenecks are—only measurement can.

## The Iron Law

```
NO OPTIMIZATION WITHOUT PROFILING DATA
```

**Before suggesting ANY optimization:**
1. Profile to identify bottlenecks
2. Create benchmarks for current state
3. Only then suggest changes
4. Benchmark again to verify improvement

**No exceptions:**
- Not for "obvious" improvements
- Not for "best practices"
- Not for code that "looks slow"
- Not when user is confident they know the problem

## Red Flags - STOP and Profile First

These thoughts mean STOP—you're about to violate measurement discipline:

| Thought | Reality |
|---------|---------|
| "I can identify optimization opportunities" | Code review ≠ profiling. Measure first. |
| "This is obviously slow" | Obvious to you ≠ actual bottleneck. Profile. |
| "Multiple passes are inefficient" | Maybe. Measure to confirm. |
| "Lazy evaluation would be faster" | Maybe not. Benchmark both. |
| "The user knows it's an N+1 problem" | Users guess wrong. Verify with profiling. |
| "I would profile if I had access" | You do. Insist on profiling before proceeding. |
| "Let me explain how to profile" | Don't explain. Insist on actual profiling. |

**All of these mean: Refuse to suggest optimizations. Profile first.**

## Profiling Tool Selection

| What to Measure | Tool Type | Description |
|-----------------|-----------|-------------|
| **Function call frequency** | Call profiler | Counts how many times functions called |
| **Time per function** | Time profiler | Measures where time is spent |
| **Detailed time breakdown** | Flame graph | Visual time breakdown |
| **Memory allocations** | Memory profiler | Tracks allocations and sizes |
| **Comparison before/after** | Benchmark tool | Statistical comparison |

**Wrong tool = wrong conclusions:**
- Time profilers measure TIME, not memory
- Call profilers count CALLS, not time
- Use memory profilers for memory issues

For language-specific profiling tools:
- **Elixir**: See `references/elixir.md` - cprof, eprof, fprof, tprof
- **Rust**: See `references/rust.md` - perf, flamegraph, valgrind

## Benchmark Pattern

```pseudocode
# Statistical benchmark template
benchmark({
    "current implementation": lambda:
        CurrentModule.function(input),

    "proposed optimization": lambda:
        OptimizedModule.function(input)
},
    warmup=2,      # Seconds for warmup
    time=5,        # Seconds for measurement
    memory_time=2  # Seconds for memory measurement
)

# Output:
# Name                  ips        average  deviation         median
# current             500 K     2.00 μs    ±10.00%     1.98 μs
# optimized         1000 K     1.00 μs     ±8.00%     0.99 μs
# Comparison:
# optimized is 2.0x faster
```

**CRITICAL:** Benchmark BEFORE changing code to establish baseline.

For language-specific benchmark frameworks:
- **Elixir**: See `references/elixir.md` - Benchee
- **Rust**: See `references/rust.md` - criterion, divan

## The Correct Workflow

```
User reports slowness or asks for optimization
            ↓
    Has profiling data?
        ↓ NO           ↓ YES
   REFUSE to give   Has baseline benchmark?
   advice               ↓ NO          ↓ YES
        ↓          Create baseline   Suggest optimization
   Profile to find     benchmark      based on data
   bottleneck              ↓              ↓
        ↓              Suggest optimization based on data
        →                      ↓
                         Benchmark optimization
                               ↓
                         Compare results
```

## Common Performance Patterns (After Profiling Shows These)

Only suggest these if profiling data confirms the problem:

### High Call Frequency
- **Symptom:** Profiler shows function called 100,000+ times
- **Fix:** Reduce calls (caching, memoization) or optimize hot function

### Multiple Data Passes
- **Symptom:** Profiler shows significant time in iteration chains
- **Fix:** Lazy evaluation for large data, single-pass with reduce, or parallel processing

### Memory Pressure
- **Symptom:** Memory profiler shows large allocations
- **Fix:** Lazy/streaming processing, more efficient data structures, or chunking

### I/O Bound Operations
- **Symptom:** Profiler shows time in database/network calls
- **Fix:** Parallel processing, connection pooling, batching

### Database N+1 Queries
- **Symptom:** Database logs show 1 + N queries for associations
- **Fix:** Eager loading/joins, or custom batch queries

## Complexity Analysis for O(n²)+ Code

When profiling reveals slow code AND analysis shows O(n²) or worse complexity:

**Create benchmark with increasing input sizes:**

```pseudocode
benchmark({
    "current O(n²)": lambda n:
        CurrentImplementation.function(range(n))
},
    inputs={
        "100 items": 100,
        "1,000 items": 1000,
        "10,000 items": 10_000
    }
)
```

If time increases quadratically with input size, algorithmic improvement is needed.

## Refusing Without Profiling Data

When user asks for optimization advice without profiling:

**DON'T say:**
- "I would profile..."
- "You might want to profile..."
- "To properly verify, I recommend profiling..."

**DO say:**
- "I cannot suggest optimizations without profiling data. Let's profile first."
- "Profile first to identify the actual bottleneck."
- "Create a benchmark showing current performance before changing anything."

**Make profiling MANDATORY, not optional.**

## Validating "Improvements"

User claims code is faster after changes:

**REFUSE to confirm without benchmarks:**
1. "Show me the before and after benchmarks"
2. If no baseline exists, cannot verify improvement
3. "Should be faster" is not the same as "measured to be faster"

## Common Mistakes

| Mistake | Fix |
|---------|-----|
| Optimizing based on code review | Profile first, optimize what's actually slow |
| Using wrong profiling tool | Match tool to what you're measuring (time/memory/calls) |
| No baseline benchmark | Measure current state before changing anything |
| Assuming optimization worked | Benchmark after changes to verify |
| Profiling in dev instead of prod-like data | Use realistic data volumes and conditions |
| Optimizing O(1) code instead of O(n²) | Profile to find where time is actually spent |

## Real-World Example

**Wrong:**
```
User: "This module looks slow"
Agent: "Use lazy evaluation instead of eager loading, and parallelize"
```

**Right:**
```
User: "This module looks slow"
Agent: "Let's profile it first to see where time is actually spent.

Run the time profiler:
  <language-specific profiling command>

Then create a baseline benchmark:
  <language-specific benchmark command>

Only after seeing profiling data can we identify the actual bottleneck."
```

## Language-Specific Reference

For language-specific profiling tools, benchmark frameworks, and optimization patterns:
- **Elixir**: See `references/elixir.md` - mix profile, Benchee, ETS optimization
- **Rust**: See `references/rust.md` - perf, criterion, flamegraph, allocation tracking

## Integration with Other Skills

- **Before optimizing:** Use THIS skill to profile and benchmark
- **When O(n²)+ detected:** Use algorithms skill to find better algorithms
- **After optimization:** Use verification-before-completion to confirm improvements
- **In code review:** Check that optimizations have benchmark data
