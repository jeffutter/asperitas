---
name: algorithm-research
description: Use when researching state-of-the-art algorithms for a problem, finding academic papers, comparing algorithm approaches, needing paper citations, or user invokes /algorithm-research
---

# Algorithm Research Skill

Research state-of-the-art algorithms with paper citations and implementation guidance. Launches the algorithms-researcher agent to find modern algorithms that outperform classic approaches.

## When to Use

Invoke with `/algorithm-research` when:
- Need to find better algorithm than current approach
- Novel problem without obvious algorithmic solution
- Performance issues seem algorithmic in nature
- Considering approximate algorithms (probabilistic structures)
- Need academic paper citations for decision justification
- Want to understand tradeoffs before implementing

Skip this skill when:
- Just need to optimize existing code (use `/benchmark` or performance-analyzer)
- Problem is code quality, not algorithms (use architect agent)
- Need testing strategy (use test-designer agent)
- Well-known algorithm, just need implementation (use general assistant)
- Need algorithm knowledge without research workflow (load `algorithms` skill instead)

## Usage

```bash
/algorithm-research <problem-description>

# Examples
/algorithm-research unique visitor counting for 100M users
/algorithm-research faster alternative to MD5 for checksums
/algorithm-research top-k tracking in event stream
/algorithm-research approximate nearest neighbor search
```

## Tools Used

This skill typically uses:
- **Task** - Spawn algorithms-researcher agent
- **WebSearch** - Search academic sources
- **Read** - Load files and project context
- **Write** - Create research bibliography


## Language Detection

The agent automatically detects your project language and provides appropriate library recommendations.

**Load**: the `language-detection` skill for detection logic.

**Language-specific algorithm references**:
- Elixir: the `elixir-patterns` skill\'s algorithms reference
- Rust: the `rust-patterns` skill\'s algorithms reference

## Research Workflow

### Phase 1: Problem Clarification

The agent clarifies requirements before research:
- **Data scale**: How many items? What throughput?
- **Accuracy**: Exact or approximate? Acceptable error rate?
- **Latency**: Real-time or batch processing?
- **Memory/compute budget**: Any constraints?
- **Current approach**: What are you using now? Why insufficient?

### Phase 2: Literature Search

The agent searches academic sources:
- **arXiv preprints** (cs.DS, cs.DB, cs.DC)
- **ACM conferences** (SIGMOD, STOC, FOCS)
- **IEEE publications** (ICDE, Big Data)
- **Industry research** (Google, Facebook, Microsoft)

Search focuses on:
- Recent papers (last 5 years preferred)
- Practical implementations with benchmarks
- Production deployments

### Phase 3: Algorithm Comparison

The agent compares candidate algorithms:
- **Complexity analysis**: Time and space bounds
- **Performance benchmarks**: From papers or estimated
- **Accuracy guarantees**: For approximate algorithms
- **Implementation difficulty**: Effort required
- **Production maturity**: Who's using it in practice

Presented in comparison table format.

### Phase 4: Recommendation

The agent provides:
- **Primary recommendation** with rationale
- **Alternative approaches** for different constraints
- **Implementation guidance** (library or custom)
- **Tradeoff analysis** (accuracy, performance, complexity)
- **Next steps** for prototyping and validation

### Phase 5: Documentation

For substantial research, the agent creates:
- **Annotated bibliography** in `.claude/research-refs.md`
- **Full paper citations** with links
- **Implementation notes** and code sketches
- **Further reading** for deep dives

## Output Formats

### Quick Research (Simple Problems)

For well-known algorithm spaces:

```markdown
## Algorithm Recommendation: [Algorithm Name]

**Paper**: "[Title]" ([Authors], [Year])

**Your Problem**: [Problem statement]
**Your Scale**: [Data volume/throughput]

### Why This Algorithm
[2-3 sentences explaining fit]

### Performance
- Time: O(...)
- Space: O(...)
- At your scale: [concrete metrics]
- Improvement over current: [X]x faster/smaller

### Implementation
**Library**: [package name for detected language]
**Complexity**: Low/Medium/High

### Tradeoffs
**Pros**: [2-3 advantages]
**Cons**: [2-3 limitations]

### Alternatives
- **[Algorithm 2]**: [When to use instead]

## References
1. [Paper citation with link]

## Next Steps
1. [Action 1]
2. [Action 2]
```

### Deep Research (Novel/Complex Problems)

For substantial research:

```markdown
# [Research Topic] - Algorithms Bibliography

**Date**: [Date]
**Researcher**: algorithms-researcher agent
**Context**: [Why research needed]

## Executive Summary
[3-5 sentence overview of findings]

**Recommended**: [Algorithm]
**Key Paper**: [Most important paper]
**Implementation**: [Library or approach for detected language]

## Problem Space

### Requirements
- Scale: [Data volume/throughput]
- Accuracy: [Requirements]
- Latency: [Performance needs]
- Memory: [Constraints]

### Current Approach
[What you're doing now and limitations]

## Literature Review

### Survey Papers
[Overview papers to understand space]

### Foundational Papers
[Classic work that established the field]

### Modern Improvements
[Recent advances (last 5 years)]

### Implementation Papers
[Systems papers with production experience]

## Algorithm Comparison

| Algorithm | Year | Time | Space | Accuracy | Complexity | Production |
|-----------|------|------|-------|----------|------------|------------|
| Current | - | O(n) | O(n) | 100% | Low | Your system |
| Option A | 2018 | O(1) | O(k) | 98% | Medium | Company X |
| **Recommended** | **2020** | **O(1)** | **O(k)** | **99%** | **Medium** | **Company Y** |

## Detailed Analysis: [Recommended Algorithm]

### Algorithm Description
[Plain language explanation]

### Theoretical Guarantees
[Complexity bounds, accuracy]

### Practical Performance
[Real-world benchmarks]

### Implementation Considerations
[Challenges, parameters, gotchas]

### Language-Specific Notes
[Implementation considerations for detected language]

## Implementation Recommendations

### For Your Scale

**Best Choice**: [Algorithm]

**Rationale**:
1. [Reason 1]
2. [Reason 2]

**Implementation Path**:

1. **Quick Prototype** (effort: low):
   - [Approach]
   - [Validation]

2. **Production Version** (effort: medium):
   - [Steps]
   - [Testing]

3. **Optimization** (if needed):
   - [Tuning]

### Alternative Approaches
[If requirements change]

## Language Ecosystem

### Available Libraries
[Library assessments for detected language]

### Custom Implementation
[Guidance if needed]

### Native Bindings
[When to use FFI/NIF/native code]

## Further Reading

### Essential Papers
1. [Paper] - [Why essential]

### Advanced Topics
1. [Paper] - [Deep dive]

### Implementation Resources
- [Tutorials]
- [Reference implementations]

## Appendix

### Search Terms Used
[For reproducibility]

### Related Problems
[Pointers to related topics]
```

## Common Research Scenarios

### Scenario 1: Need Modern Alternative

**Problem**: "I'm using MD5 for checksums, is there something faster?"

**Research Flow**:
1. Clarify: Non-cryptographic use case, performance critical
2. Search: Modern hash functions (xxHash, BLAKE3, HighwayHash)
3. Compare: Speed benchmarks, quality, collision resistance
4. Recommend: xxHash3 (60x faster, excellent quality)
5. Implementation: Language-appropriate library (see language reference)

**Output**: Quick research format with clear recommendation

### Scenario 2: Novel Problem

**Problem**: "How do I track top 1000 items in a stream of billions?"

**Research Flow**:
1. Clarify: Scale (billions/day), accuracy (approximate okay), memory budget
2. Decompose: Stream processing + frequency counting + top-k tracking
3. Survey: Space-Saving, Count-Min Sketch, HeavyKeeper
4. Compare: Memory footprint, accuracy guarantees, update speed
5. Recommend: Space-Saving algorithm (O(k) memory, bounded error)
6. Document: Full bibliography in `.claude/research-refs.md`

**Output**: Deep research format with annotated bibliography

### Scenario 3: Implementation Complexity Assessment

**Problem**: "Should I switch from exact counting to HyperLogLog?"

**Research Flow**:
1. Clarify: Current scale (50M unique items), growth trajectory
2. Calculate: Current memory (400 MB) vs HyperLogLog (12 KB)
3. Assess: Implementation effort vs savings (immediate)
4. Tradeoff: 100% accuracy -> 98% accuracy (acceptable for analytics)
5. Recommend: Yes, switch (33,000x memory reduction)

**Output**: Cost-benefit analysis with clear go/no-go decision

### Scenario 4: Performance Optimization

**Problem**: "This sorting is too slow on large lists"

**Research Flow**:
1. Clarify: Data size (>100K items), data characteristics (random, patterns?)
2. Search: Modern sorting (BlockQuicksort, pdqsort, TimSort)
3. Compare: Cache efficiency, pattern detection, worst-case behavior
4. Recommend: pdqsort for mixed data with patterns
5. Coordinate: Hand off to performance-analyzer for benchmarking

**Output**: Recommendation with handoff to performance-analyzer

## Research Quality Standards

The agent ensures:

**Citations Include**:
- Full paper title
- Authors
- Publication venue and year
- Link to paper (arXiv, DOI, conference site)
- Key contribution in one sentence

**Comparisons Include**:
- Multiple candidate algorithms (3-5)
- Quantitative metrics (time, space, accuracy)
- Real-world benchmarks or estimates
- Production deployments (who uses it)

**Recommendations Include**:
- Clear choice based on requirements
- Rationale (why this algorithm fits)
- Tradeoff acknowledgment
- Implementation complexity
- Alternatives for different constraints

**Implementation Guidance Includes**:
- Language-specific library recommendations with maturity assessment
- Custom implementation approach if no library
- Code sketches or examples in detected language
- Testing strategy
- Monitoring considerations

## Agent Integration

### Handoff to Performance Analyzer

When algorithm recommendation needs validation:

```
algorithms-researcher: Recommends BlockQuicksort over standard sort
             |
             v
performance-analyzer: Creates benchmark comparing both
             |
             v
algorithms-researcher: May suggest tuning based on benchmark results
```

### Handoff from Architect

When code review identifies algorithmic issues:

```
architect: Identifies O(n^2) complexity in code
             |
             v
algorithms-researcher: Finds O(n log n) or O(n) alternative
             |
             v
architect: Reviews implementation approach
```

### Handoff to Cognitive Scientist

When algorithm adds complexity:

```
algorithms-researcher: Recommends complex algorithm for big performance gain
             |
             v
cognitive-scientist: Assesses readability impact
             |
             v
algorithms-researcher: May suggest simpler alternative if cognitive load too high
```

## Tips for Effective Research

### Be Specific About Scale

Instead of: "I need fast counting"
Better: "I need to count 100M unique visitors per day"

Specific scale enables:
- Concrete performance estimates
- Appropriate algorithm selection
- Cost-benefit analysis

### Clarify Accuracy Requirements

Instead of: "Approximate is okay"
Better: "Can tolerate +/-2% error for analytics dashboard"

Accuracy bounds enable:
- Probabilistic structure selection
- Parameter tuning
- Tradeoff evaluation

### Describe Current Approach

Instead of: "Need better algorithm"
Better: "Currently using MapSet for unique counting, using 2GB memory"

Context enables:
- Baseline comparison
- Quantified improvements
- Migration path

### State Constraints

Instead of: "Memory-efficient algorithm"
Better: "Must use <100MB memory, CPU not constrained"

Explicit constraints enable:
- Focused search
- Appropriate tradeoffs
- Feasible recommendations

## Success Criteria

Successful research provides:
- Clear algorithm recommendation with rationale
- Academic paper citations with links
- Performance metrics (quantitative)
- Implementation complexity assessment
- Language-specific library recommendations
- Tradeoff analysis
- Alternative approaches
- Next steps for implementation

## Related Skills

- **algorithms** - Load for algorithm knowledge (modern hash functions, probabilistic data structures, etc.)
- **performance-analyzer** - For benchmarking algorithm implementations
- **cognitive-complexity** - For assessing algorithm readability impact

## See Also

- `algorithms-researcher` agent for detailed methodology
- `performance-analyzer` agent for benchmarking algorithms
- `.claude/research-refs.md` for accumulated research notes
