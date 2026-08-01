---
name: distributed-review
description: Use when reviewing distributed system code, checking for distributed bugs (split-brain, quorum errors, clock drift), or user invokes /distributed-review
---

# Distributed Review Skill

Comprehensive review of distributed system architecture, consensus algorithms, clustering strategy, and potential distributed bugs. Launches the **distributed-systems-expert** agent (Opus) for deep analysis.

## When to Use

Invoke with `/distributed-review` when:
- Reviewing distributed system design before production
- Checking for split-brain, quorum errors, or clock drift bugs
- Evaluating consensus algorithm choice (Raft vs CRDTs)
- Analyzing CAP tradeoffs for a system
- After a distributed incident (netsplit, data inconsistency)
- Quarterly distributed health check

Skip this skill when:
- Learning about distributed concepts (use `distributed-systems` skill)
- General code review without distributed aspects (use `/review`)
- Performance profiling (use `/benchmark` or `performance-analyzer`)

## Usage

```bash
# Review entire distributed system
/distributed-review

# Review specific module
/distributed-review lib/my_app/consensus.ex

# Review feature area
/distributed-review lib/my_app/cluster/

# Review configuration
/distributed-review config/config.exs
```

Expects an optional path to scope the review. Without argument, reviews the entire system.

## Tools Used

This skill typically uses:
- **Task** - Spawn distributed-systems-expert agent for analysis
- **Read** - Load target files and configuration
- **Glob/Grep** - Find distributed patterns in codebase
- **TodoWrite** - Track review findings


## Related Skills

- **distributed-systems** - Load for CAP theorem, consensus algorithms, CRDTs, vector clocks

When deeper knowledge about consensus algorithms, CRDTs, or distributed patterns is needed during review, reference the distributed-systems skill.

## Workflow

### Phase 1: Launch Distributed Systems Expert

```markdown
Launching distributed-systems-expert agent (Opus) for distributed system analysis...

Scope: [target area]

The expert will:
1. Evaluate clustering strategy (full-mesh vs overlay)
2. Analyze consensus algorithm choice (Raft, CRDTs, etc.)
3. Check for distributed bugs (split-brain, clock drift, quorum errors)
4. Assess CAP tradeoffs and consistency model
5. Review network partition handling
6. Provide specific recommendations

Waiting for analysis...
```

### Phase 2: Analysis Process

The distributed-systems-expert agent executes:

```bash
# Load context
Read [target files]
Read config files (clustering configuration)
Grep "Node.list|consensus|CRDT|PubSub" (distributed patterns)
Read .claude/project-learnings.md (existing distributed patterns)

# Analyze architecture
- Clustering: Full-mesh or overlay network?
- Scale: Node count, projected growth
- Consensus: Raft, CRDTs, or ad-hoc?
- Consistency model: CP, AP, or CA?
- State synchronization: PubSub, custom?

# Check for bugs
- Split-brain prevention: Quorum checks present?
- Quorum calculation: div(n, 2) + 1 verified?
- Clock usage: Monotonic time for intervals?
- Race conditions: Optimistic locking or CRDTs?
- Network partition: Merge strategy defined?

# Evaluate decisions
- Consensus choice appropriate for use case?
- Clustering strategy suitable for scale?
- CAP tradeoffs align with requirements?
- Failure modes handled correctly?

# Generate recommendations
- Immediate fixes (critical bugs)
- Strategic improvements (architecture)
- Monitoring and alerting
- Testing approach (partition simulation)
```

### Phase 3: Present Findings

#### No Issues Found

```markdown
Distributed System Review Complete - Well Designed

## Architecture Summary

**Clustering**: [Strategy with discovery mechanism]
- Cluster size: [N nodes]
- Status: Appropriate for scale

**Consensus**: [Algorithm] for [use case]
- Quorum: [X of Y nodes]
- Status: Correct choice

**Partition Handling**: [Strategy]
- Detection: [Mechanism]
- Status: Correctly implemented

## Analysis

[Checkmarks for each verified area]

## Recommendations

**Monitoring enhancements**:
1. [Specific monitoring improvements]

**Testing**:
- [Partition simulation tests]
- [Verification scenarios]

Distributed system is production-ready!
```

#### Issues Found

```markdown
Distributed System Review Findings

Reviewed: [scope] ([N files], distributed system implementation)

## Critical Issues

### [Critical] [Issue Title]
**Location**: `[file:line]`
**Confidence**: [X%]

**Problem**: [Description]

**Current code**:
```[language]
[problematic code]
```

**Impact**: [What goes wrong]

**Fix**:
```[language]
[corrected code]
```

## Important Issues

### [Important] [Issue Title]
**Confidence**: [X%]

**Problem**: [Description]

**Recommendation**: [Solution]

## Architecture Analysis

### Clustering Strategy
**Current**: [Strategy]
**Assessment**: [Evaluation]
**Status**: [Needs attention / OK]

### Consensus Algorithm
**Current**: [Algorithm or "ad-hoc"]
**Assessment**: [Evaluation]
**Status**: [Needs attention / OK]

### CAP Tradeoffs
**Current implementation**: [CA/CP/AP attempt]
**Problem**: [If any]
**Recommended**: [Appropriate choice with rationale]

### Network Partition Behavior
**Current**: [Defined or undefined]
**After partition**: [Behavior]
**Recommended**: [Strategy]

## Monitoring Gaps

Missing critical distributed system monitoring:

1. **[Gap]**: [How to add]
2. **[Gap]**: [How to add]

## Recommendations Priority

### High Priority (This Sprint)
[Numbered list with effort estimates]

### Medium Priority (Next Sprint)
[Numbered list with effort estimates]

### Low Priority
[Numbered list]

## Testing Recommendations

**Partition simulation**:
[How to simulate partitions]

**Test scenarios**:
- [Specific scenarios to test]

**Verification**:
- [What to verify after each scenario]

## Next Steps

1. **Review** findings with team
2. **Prioritize** fixes
3. **Fix** critical bugs immediately
4. **Test** with partition simulation
5. **Monitor** cluster health continuously
```

## Bug Detection Checklists

### Split-Brain Prevention

Check for:
- [ ] Quorum verification before leader election
- [ ] Fencing tokens for leader operations
- [ ] Cluster size awareness in partition decisions
- [ ] Explicit partition detection logic

Red flags:
- Leader election without quorum check
- Operations proceeding when cluster is split
- No fencing mechanism for writes

### Quorum Calculation

Check for:
- [ ] Correct formula: `div(cluster_size, 2) + 1`
- [ ] Odd cluster sizes preferred (3, 5, 7)
- [ ] Dynamic cluster size handled correctly
- [ ] Quorum check before critical operations

Red flags:
- `div(n, 2)` without `+ 1` (off-by-one)
- Even cluster sizes without tie-breaker
- Hardcoded quorum values

### Clock Drift

Check for:
- [ ] Monotonic time for duration calculations
- [ ] System time only for display/logging
- [ ] NTP offset monitoring in production
- [ ] Clock-bound waits if using leases

Red flags:
- `System.system_time()` for intervals/timeouts
- Lease expiry using wall-clock time
- No clock skew monitoring
- Timestamp comparisons across nodes

### Race Conditions

Check for:
- [ ] Optimistic locking or CRDTs for concurrent updates
- [ ] Version tracking on distributed state
- [ ] Retry logic for version conflicts
- [ ] Serialization via consensus for critical operations

Red flags:
- Read-modify-write without version check
- Concurrent decrements without CRDT
- Distributed counters without coordination

### Network Partition Handling

Check for:
- [ ] Explicit merge strategy defined
- [ ] CRDT or application-specific conflict resolution
- [ ] Read-repair for stale replicas
- [ ] Partition detection and logging

Red flags:
- No defined behavior after partition heals
- Implicit last-write-wins (data loss risk)
- No partition event logging

## Monitoring Recommendations

Essential distributed system metrics:

1. **Cluster health**
   - Visible nodes count
   - Has quorum status
   - Missing nodes

2. **Clock skew**
   - NTP offset per node
   - Alert if >100ms

3. **Leader election**
   - Elections per hour
   - Alert if >3 per hour (instability)

4. **Network partition events**
   - Node down events with reason
   - Partition detection timestamps

## Confidence Thresholds

Only report findings with confidence >= 80%:

- **100%**: Definite bug (quorum calculation error)
- **95%**: Very high confidence (split-brain risk without quorum)
- **90%**: High confidence (clock drift bug)
- **85%**: Good confidence (race condition risk)
- **80%**: Threshold (potential issue worth mentioning)
- **<80%**: Do not report (too speculative)

## Success Criteria

Review succeeds when:
- Clustering strategy appropriate for scale
- Consensus algorithm justified or issues identified
- Split-brain prevention verified
- Quorum calculations correct
- Clock drift mitigation in place
- Network partition handling defined
- CAP tradeoffs explicit
- Monitoring recommendations provided
- Findings documented in project-learnings.md
