---
name: benchmark
description: Use when creating benchmarks, measuring performance, comparing implementations, or user invokes /benchmark. Provides step-by-step workflow for benchmark creation, data generation, and results interpretation.
---

# Benchmark Skill

Create and run benchmarks with representative data for performance analysis. This is a workflow skill - for profiling methodology and when to optimize, see the **performance-analyzer** skill.

## When to Use

Invoke with `/benchmark` when:
- Creating benchmarks for a function or module
- Comparing performance of alternative implementations
- Validating optimization improvements with data
- Measuring algorithm complexity empirically
- Before and after optimization verification

Skip this skill when:
- Performance not critical for the feature
- Premature optimization (no evidence of slowness)
- Simple O(1) operations on small data
- Need profiling methodology (use `/performance-analyzer`)

## Usage

```bash
/benchmark MyApp.OrderProcessor.process_batch/1
/benchmark lib/my_app/search.ex
/benchmark "compare Enum vs Stream for filtering"
```

Expects a benchmark target (function, module, or comparison description) as the argument.

## Tools Used

This skill typically uses:
- **Task** - Spawn performance-analyzer agent for complexity analysis
- **Bash** - Run language-specific benchmark commands
- **Read** - Load target code and existing benchmarks
- **Write** - Create benchmark files
- **Glob/Grep** - Find related code and tests


## Related Skills

- **performance-analyzer** - Load for profiling methodology, tool selection, when to optimize
- **language-detection** - Detect project language for correct benchmark framework

---

## Language Detection

Detect project language to use appropriate benchmarking tools.

**Load**: the `language-detection` skill for canonical detection logic.

**Benchmarking tools by language**:

| Language | Tool | Command | Directory |
|----------|------|---------|-----------|
| Elixir | Benchee | `mix run bench/*.exs` | `bench/` |
| Rust | Criterion | `cargo bench` | `benches/` |
| Python | pytest-benchmark | `pytest --benchmark-only` | `benchmarks/` |
| TypeScript | Benchmark.js | `npm run bench` | `benchmarks/` |
| Go | testing.B | `go test -bench=.` | `*_test.go` |

---

## Workflow

### Step 1: Identify Benchmark Target

Parse the target from user input:

```markdown
Analyzing benchmark target: MyApp.OrderProcessor.process_batch/1

Finding target:
- Module: MyApp.OrderProcessor
- Function: process_batch/1
- Location: lib/my_app/order_processor.ex:42
```

If comparing alternatives:

```markdown
Benchmark request: "compare Enum vs Stream for filtering"

Identifying comparison:
- Approach 1: Enum-based filtering (eager evaluation)
- Approach 2: Stream-based filtering (lazy evaluation)
- Context: Large dataset processing
- Creating benchmark to compare both approaches
```

### Step 2: Analyze Complexity

Launch **performance-analyzer** agent to analyze code complexity:

```markdown
Launching performance-analyzer agent for complexity analysis...

Agent will:
1. Read implementation
2. Identify nested iterations
3. Calculate algorithmic complexity
4. Estimate real-world impact with project data sizes
5. Recommend benchmark data sizes

Waiting for analysis...
```

**Example complexity output**:

```markdown
## Complexity Analysis

**Algorithm**: Nested iterations with list comprehension
**Complexity**: O(n x m x k) where:
- n = number of orders
- m = line items per order
- k = inventory size

**Real-world impact** (based on project data):
- Typical: 100 orders, 5 items each, 1000 inventory = 500,000 operations
- Peak: 1000 orders, 10 items each, 5000 inventory = 50,000,000 operations

**Recommended data sizes**:
- Small: 10 items (baseline)
- Typical: 100-1000 items (production average)
- Peak: 10,000+ items (maximum expected load)
```

### Step 3: Generate Representative Data

Create test data that reflects realistic scenarios:

**Data size guidelines**:
- **Small**: 10-100 items (unit test scale, quick iterations)
- **Typical**: 1K-10K items (average production load)
- **Peak**: 100K+ items (maximum expected load, stress testing)

**Data characteristics**:
- Realistic distribution (not uniform random)
- Edge cases included (empty, single item, duplicates)
- Match production patterns from project-learnings.md

### Step 4: Create Benchmark File

Create benchmark in appropriate directory for the language.

**Language-specific templates**:
- **Elixir (Benchee)**: See `references/elixir.md`
- **Rust (Criterion)**: See `references/rust.md`

**Benchmark file structure**:
1. Header with creation date and complexity note
2. Module/function under test
3. Data generation functions
4. Multiple input sizes
5. Comparison scenarios (if applicable)

### Step 5: Run Benchmark

Execute the benchmark using language-appropriate command:

```bash
# Elixir (Benchee)
mix run bench/order_processor_benchmark.exs

# Rust (Criterion)
cargo bench --bench order_processor

# Python (pytest-benchmark)
pytest benchmarks/test_order_processor.py --benchmark-only

# Go
go test -bench=. -benchmem ./...
```

### Step 6: Analyze and Report Results

Present benchmark results in standardized format:

```markdown
## Benchmark Results: order_processor.process_batch/1

### Performance Summary

**Small dataset** (10 orders, 100 inventory):
| Implementation | IPS | Average | vs Baseline |
|----------------|-----|---------|-------------|
| Optimized | 15.2 K | 65.8 us | Baseline |
| Original | 12.4 K | 80.6 us | 1.22x slower |

**Typical dataset** (100 orders, 1K inventory):
| Implementation | IPS | Average | vs Baseline |
|----------------|-----|---------|-------------|
| Optimized | 1.85 K | 540 us | Baseline |
| Original | 245 | 4.08 ms | 7.55x slower |

**Peak dataset** (1K orders, 5K inventory):
| Implementation | IPS | Average | vs Baseline |
|----------------|-----|---------|-------------|
| Optimized | 185 | 5.4 ms | Baseline |
| Original | 2.8 | 357 ms | 66x slower |

### Memory Impact

- Original: 45.2 MB allocated
- Optimized: 12.8 MB allocated
- **Reduction**: 71% less memory

### Key Findings

1. **Complexity validated**: Performance degrades exponentially with data size
2. **Critical at scale**: 66x slower at peak load
3. **Memory efficient**: 71% reduction in allocations

### Recommendation

**Action**: Implement optimized O(n x m) version
**Impact**: Peak load 357ms -> 5.4ms (66x faster)
**Risk**: Low - same logic, just preprocessing
```

---

## Benchmark File Organization

Organize benchmarks by module:

**Elixir**:
```
bench/
├── results/           # Generated HTML/CSV reports
├── support.exs        # Shared fixtures (optional)
├── order_processor_benchmark.exs
├── product_search_benchmark.exs
└── README.md
```

**Rust**:
```
benches/
├── order_processor.rs
├── product_search.rs
└── criterion.toml     # Optional configuration
```

**Python**:
```
benchmarks/
├── conftest.py        # Shared fixtures
├── test_order_processor.py
├── test_product_search.py
└── README.md
```

---

## Configuration

### Benchmark Defaults

**Timing**:
- warmup: 2 seconds (JIT compilation, cache warming)
- time: 5-10 seconds (accurate measurements)
- memory_time: 2-5 seconds (memory profiling)

**Output formats**:
- Console (always) - immediate feedback
- HTML report (recommended) - detailed analysis
- JSON/CSV (optional) - CI/CD integration

---

## Error Handling

### Benchmark Target Not Found

```
Error: Module or function does not exist

Check:
- Module/function name spelling
- Function exists and is exported/public
- Code has been compiled

Fix varies by language:
- Elixir: Run `mix compile`
- Rust: Check pub visibility
- Python: Check PYTHONPATH, __init__.py files
```

### Benchmark Failed to Run

```
Error: Dependencies or fixtures not available

Fix:
- Elixir: Use Code.require_file for test support
- Rust: Check dev-dependencies in Cargo.toml
- Python: Ensure conftest.py has fixtures
```

### Insufficient Data Range

```
Warning: Only testing small data size

Recommendation: Add larger data sizes to reveal complexity:
- Small: 10-100 items
- Typical: 1K-10K items
- Peak: 100K items

Large data sizes reveal O(n^2) and O(n log n) differences.
```

---

## Best Practices

1. **Representative data**: Use realistic data sizes from production
2. **Multiple sizes**: Test small, typical, and peak loads
3. **Compare alternatives**: Always benchmark before/after or A vs B
4. **Memory matters**: Track memory usage, not just speed
5. **Warmup**: Allow JIT compilation before measurements
6. **Document context**: Explain why benchmark was created
7. **Version control**: Commit benchmark files
8. **Re-run after changes**: Validate optimizations work

---

## Integration

**Triggered by**:
- User request (`/benchmark`)
- `/feature` (architect detects O(n^2)+)
- `/review` (performance concerns)

**Triggers**:
- Performance-analyzer agent for complexity analysis
- Results inform optimization decisions
- Updates to project-learnings.md

**Related skills**:
- `/cognitive-audit` - If optimization makes code complex
- `/algorithm-research` - For advanced optimization techniques
- `/review` - Validate benchmark-driven changes

---

## Success Criteria

Benchmark succeeds when:
- [ ] Representative data sizes tested (small/typical/peak)
- [ ] Multiple scenarios covered
- [ ] Clear performance comparison
- [ ] Memory usage measured
- [ ] Actionable recommendations provided
- [ ] Results saved for future reference
- [ ] Complexity assumptions validated
