---
name: distributed-systems
description: Use when working with distributed systems, clustering, consensus algorithms, network partitions, CAP tradeoffs, or multi-language distributed patterns
---

# Distributed Systems Patterns

## Overview

Building distributed systems provides powerful capabilities but introduces unique challenges: consensus, network partitions, clock synchronization, and split-brain scenarios. This skill covers production-tested patterns for distributed systems design, consensus algorithms, clustering strategies, and debugging distributed bugs.

## When to Use

Use this skill when:
- Designing distributed system architectures
- Evaluating consensus algorithms (Paxos, Raft, Multi-Raft)
- Planning clustering strategies
- Debugging distributed system issues
- Analyzing CAP theorem tradeoffs
- Handling network partitions
- Implementing conflict resolution (CRDTs, vector clocks)
- Building globally distributed applications
- Working with distributed state or caching

## Core Concepts

### CAP Theorem Tradeoffs

**Consistency + Availability + Partition Tolerance** - Pick 2:

**CA (Consistency + Availability, no Partition Tolerance)**:
- Traditional RDBMS in single datacenter
- Example: PostgreSQL without replication
- Limitation: Network partition breaks system
- Use case: Single-datacenter applications

**CP (Consistency + Partition Tolerance)**:
- Prioritize correctness over availability
- Examples: Raft consensus, etcd, ZooKeeper
- During partition: Minority partition becomes unavailable
- Use case: Financial systems, inventory management, leader election

**AP (Availability + Partition Tolerance)**:
- Prioritize availability over strong consistency
- Examples: CRDTs, Cassandra, DynamoDB
- During partition: All nodes serve requests, reconcile later
- Use case: Social media, content delivery, collaborative editing

**Implementation approaches**:
```pseudocode
# CP approach: Raft consensus
cluster = consensus.start_cluster(
    cluster_id="my_cluster",
    state_machine=MyStateMachine(),
    nodes=[node1, node2, node3]
)

# AP approach: CRDTs
crdt = CRDT.start(type=AWLWWMap)
crdt.put("key", "value")  # Eventually consistent
```

For language-specific implementations:
- **Elixir**: See `references/elixir.md` - `:ra`, Partisan, libcluster, delta_crdt
- **Rust**: See `references/rust.md` - raft-rs, tonic, tokio networking

### Distributed Node Architecture

**Capabilities**:
- Transparent remote process communication
- Location transparency (messaging works across nodes)
- Distributed process registry
- Global name registration
- Automatic network failure detection

**Limitations**:
- **Full-mesh topology**: Every node connects to every other node
- **O(n²) network overhead**: Breaks down at ~50-100 nodes
- **All-or-nothing security**: Either fully trusted or isolated
- **Global namespace conflicts**: Name collisions across nodes
- **Netsplits**: Network partitions create split-brain scenarios

**When full-mesh distributed mode works**:
- Small to medium clusters (< 50 nodes)
- Trusted network environment
- Simple clustering needs

For larger clusters, consider overlay network approaches (see language-specific references).

### Consensus Algorithms

#### Paxos

**Properties**:
- Classic consensus algorithm (Leslie Lamport, 1989)
- Provably correct
- Notoriously difficult to understand and implement
- Requires majority quorum (⌊n/2⌋ + 1)

**Phases**:
1. Prepare: Proposer sends proposal number
2. Promise: Acceptors promise not to accept lower proposals
3. Accept: Proposer sends value
4. Accepted: Acceptors record accepted value

**Recommendation**: Use Raft instead for most use cases due to better understandability.

#### Raft

**Properties**:
- Designed for understandability (Diego Ongaro, 2014)
- Leader-based consensus
- Log replication
- Strong consistency guarantees
- Requires majority quorum

**Key components**:
```pseudocode
# Raft state machine pattern
class MyStateMachine implements RaftMachine:
    state = {}

    def apply(command):
        match command:
            case SetCommand(key, value):
                state[key] = value
                return Ok(value)
            case GetCommand(key):
                return Ok(state.get(key))

# Start Raft cluster
cluster = Raft.start_cluster(
    cluster_id="my_cluster",
    machine=MyStateMachine(),
    nodes=[node1, node2, node3]
)

# Submit command (goes through leader)
result = cluster.submit_command(SetCommand("key", "value"))
```

**When to use Raft**:
- Need strong consistency (CP)
- Leader election
- Replicated state machines
- Configuration management
- Distributed locks

For language-specific implementations:
- **Elixir**: See `references/elixir.md` - `:ra` (RabbitMQ's production Raft)
- **Rust**: See `references/rust.md` - `raft-rs` (TiKV's implementation)

#### Multi-Raft with Leader Leases

**Advanced pattern** used by CockroachDB, TiKV:

**Key optimizations**:
- Multiple Raft groups for sharding
- Leader leases reduce read latency
- Lease-based reads (no quorum check)
- Clock-bound wait (Google Spanner style)

**Concept**:
```pseudocode
# Multiple Raft groups (sharding)
# Group 1: Handles keys "a-m"
# Group 2: Handles keys "n-z"

# Leader lease optimization
class RaftLeader:
    lease_duration = 10_000  # milliseconds

    def read_with_lease(state):
        if lease_valid(state.lease):
            # Fast read: No quorum needed
            return Ok(state.data)
        else:
            # Fallback: Quorum read
            return quorum_read(state)

    def lease_valid(lease):
        return monotonic_time() < lease.expiry
```

**Benefits**:
- 10x faster reads (no quorum)
- Better scalability (sharding)
- Maintains strong consistency

**Tradeoffs**:
- Clock drift sensitivity
- Increased complexity
- Lease management overhead

### Conflict-Free Replicated Data Types (CRDTs)

**For AP systems** - Automatic conflict resolution:

**Key CRDT types**:
```pseudocode
# G-Counter: Grow-only counter
counter = CRDT.new(GCounter)
counter.increment()

# PN-Counter: Increment/decrement counter
counter = CRDT.new(PNCounter)
counter.increment()
counter.decrement()

# AWLWWMap: Add-wins, Last-write-wins map
map = CRDT.new(AWLWWMap)
map.add("key", "value")
map.remove("key")

# OR-Set: Observed-remove set
set = CRDT.new(ORSet)
set.add("element")
set.remove("element")
```

**When to use CRDTs**:
- Eventually consistent systems (AP)
- Offline-first applications
- Collaborative editing
- Shopping carts
- Presence tracking
- Like/favorite counts

For language-specific implementations:
- **Elixir**: See `references/elixir.md` - `delta_crdt`, `lasp`
- **Rust**: See `references/rust.md` - CRDT crates

### Vector Clocks

**For causal consistency** - Track causality between events:

```pseudocode
class VectorClock:
    def new():
        return {}

    def increment(clock, node):
        clock[node] = clock.get(node, 0) + 1
        return clock

    def merge(clock1, clock2):
        result = {}
        for node in union(clock1.keys(), clock2.keys()):
            result[node] = max(clock1.get(node, 0), clock2.get(node, 0))
        return result

    # Returns: concurrent, before, or after
    def compare(clock1, clock2):
        if descends(clock1, clock2):
            return :after
        elif descends(clock2, clock1):
            return :before
        else:
            return :concurrent

    def descends(clock1, clock2):
        return all(
            clock1.get(node, 0) >= count
            for node, count in clock2.items()
        ) and clock1 != clock2

# Usage in distributed system
class DistributedCache:
    def put(key, value, clock):
        new_clock = VectorClock.increment(clock, current_node())
        # Store {value, new_clock}
        return Ok(new_clock)

    def resolve_conflict(values_with_clocks):
        # Keep all concurrent values, discard causally dominated
        return [
            (v, c) for (v, c) in values_with_clocks
            if not any(
                VectorClock.compare(c, other_c) == :before
                for (_, other_c) in values_with_clocks
            )
        ]
```

## Common Distributed Bugs

### Split-Brain Scenario

**Problem**: Network partition creates multiple leaders

**Example**:
```
# Before partition: Single leader
[Node1 (Leader)] <-> [Node2] <-> [Node3]

# After partition: TWO leaders!
[Node1 (Leader)] <-> [Node2]  |  [Node3 (Leader)]
```

**Prevention strategies**:

**1. Quorum-based consensus (CP approach)**:
```pseudocode
# Raft: Requires majority
# 5 nodes: Need 3 for quorum
# After split: 3-node partition has leader, 2-node cannot elect

def has_quorum(cluster_size, reachable_nodes):
    return reachable_nodes >= (cluster_size // 2) + 1
```

**2. Fencing tokens**:
```pseudocode
class LeaderFence:
    # Each leader gets monotonically increasing token
    def write_with_fence(resource, data, fence_token):
        result = Storage.conditional_write(resource, data, fence_token)
        if result == :ok:
            return Ok()
        elif result == :stale_token:
            return Error(:not_leader)

# Storage layer rejects writes with old tokens
```

**3. External coordinator** (etcd, ZooKeeper, Consul):
```pseudocode
class LeaderElection:
    def acquire_leadership(node_id):
        result = Etcd.acquire_lease(node_id, ttl=10)
        if result.ok:
            # Periodically renew lease
            schedule_renew_lease(5_000)
            return Ok(result.lease_id)
        elif result.error == :already_held:
            return Error(:not_leader)
```

**Detection**:
```pseudocode
class SplitBrainDetector:
    def check_cluster_health():
        visible_nodes = get_visible_nodes()
        expected_nodes = config.cluster_nodes

        if len(visible_nodes) < len(expected_nodes) / 2:
            log.error(f"Possible split-brain: {visible_nodes}")
            trigger_alarm(:split_brain)
```

### Clock Drift Issues

**Problem**: Distributed clocks drift apart

**Impact**:
- Incorrect event ordering
- Lease expiry bugs
- Timestamp-based logic fails
- Data inconsistency

**Mitigation**:

**1. Use monotonic time for intervals**:
```pseudocode
# Wrong: System time changes (NTP adjustments)
start_time = system_time()
# ... work ...
elapsed = system_time() - start_time

# Correct: Monotonic time never goes backwards
start_time = monotonic_time()
# ... work ...
elapsed = monotonic_time() - start_time
```

**2. Hybrid Logical Clocks (HLC)**:
```pseudocode
class HLC:
    # Combines physical time + logical counter
    physical_time: int
    logical_counter: int
    node_id: str

    def new(node_id):
        return HLC(
            physical_time=system_time(),
            logical_counter=0,
            node_id=node_id
        )

    def send_event(clock):
        physical = system_time()

        if physical > clock.physical_time:
            return HLC(physical, 0, clock.node_id)
        else:
            return HLC(clock.physical_time, clock.logical_counter + 1, clock.node_id)

    def receive_event(clock, remote_clock):
        physical = system_time()
        max_physical = max(physical, clock.physical_time, remote_clock.physical_time)

        if max_physical == clock.physical_time and max_physical == remote_clock.physical_time:
            logical = max(clock.logical_counter, remote_clock.logical_counter) + 1
        else:
            logical = 0

        return HLC(max_physical, logical, clock.node_id)
```

**3. NTP monitoring**:
```pseudocode
class ClockMonitor:
    def check_ntp_offset():
        output = run_command("ntpq -c rv")
        offset = parse_offset(output)
        if abs(offset) > 100:  # 100ms threshold
            log.error(f"Clock offset too high: {offset}ms")
            trigger_alarm(:clock_skew)
```

### Quorum Calculation Errors

**Problem**: Incorrect quorum math

**Examples**:
```pseudocode
# Wrong: Even split isn't majority
quorum = cluster_size // 2  # 5 nodes -> 2 (not majority!)

# Correct: Need more than half
quorum = (cluster_size // 2) + 1  # 5 nodes -> 3

# Handle edge cases
class Quorum:
    def required_nodes(cluster_size):
        assert cluster_size > 0
        return (cluster_size // 2) + 1

    def has_quorum(cluster_size, available_nodes):
        return available_nodes >= required_nodes(cluster_size)

    # Account for node failures
    def max_failures(cluster_size):
        return (cluster_size - 1) // 2

# Examples:
# 1 node: quorum=1, max_failures=0
# 3 nodes: quorum=2, max_failures=1
# 5 nodes: quorum=3, max_failures=2
# 7 nodes: quorum=4, max_failures=3
```

### Network Partition Handling

**Strategies**:

**1. Last-Write-Wins (LWW)**:
```pseudocode
# Simple but loses data
def resolve(value1, timestamp1, value2, timestamp2):
    if timestamp1 > timestamp2:
        return value1
    else:
        return value2
```

**2. Application-specific merge**:
```pseudocode
class ShoppingCart:
    # Merge carts from both sides of partition
    def merge(cart1, cart2):
        items1 = {item.product_id: item for item in cart1.items}
        items2 = {item.product_id: item for item in cart2.items}

        merged_items = list({**items1, **items2}.values())

        return Cart(items=merged_items)
```

**3. Read-repair**:
```pseudocode
class ReadRepair:
    def get(key):
        # Read from multiple nodes
        results = [
            rpc_call(node, Storage.get, key)
            for node in nodes()
        ]

        # Find latest version
        latest_value, latest_version = find_latest(results)

        # Repair stale replicas
        for node, value, version in results:
            if version < latest_version:
                rpc_cast(node, Storage.put, key, latest_value, latest_version)

        return latest_value
```

### Race Conditions in Distributed State

**Problem**: Concurrent updates from different nodes

**Example**:
```
# Two nodes decrement counter simultaneously
# Node1: read=10, write=9
# Node2: read=10, write=9
# Expected: 8, Actual: 9 (lost update!)
```

**Solution 1: Optimistic locking**:
```pseudocode
class OptimisticLock:
    def update(key, update_fn):
        value, version = Storage.get_with_version(key)
        if value is None:
            return Error(:not_found)

        new_value = update_fn(value)
        result = Storage.put_if_version(key, new_value, version)

        if result == :ok:
            return Ok(new_value)
        elif result == :version_mismatch:
            # Retry
            return update(key, update_fn)
```

**Solution 2: CRDTs**:
```pseudocode
# PN-Counter handles concurrent decrements correctly
counter = CRDT.new(PNCounter)

# Node1 and Node2 can both decrement
counter.decrement()

# CRDTs merge automatically
```

**Solution 3: Serialization via consensus**:
```pseudocode
# All updates go through Raft leader
cluster.submit_command(DecrementCommand("counter"))
```

## Distributed Debugging Patterns

### Distributed Tracing

```pseudocode
# Using telemetry for distributed traces
telemetry.span(
    ["myapp", "distributed_call"],
    {node: target_node, operation: "fetch_data"},
    lambda:
        result = rpc_call(target_node, Module, "function", args)
        (result, {status: "ok"})
)

# Correlate across nodes with trace_id
def distributed_operation(trace_id):
    logger.set_metadata(trace_id=trace_id)
    rpc_call(other_node, RemoteModule, "operation", trace_id)
```

### Investigating Netsplits

```pseudocode
class NetsplitDebug:
    # Detect and log partition events
    def monitor_cluster():
        subscribe_to_node_events(include_reason=True)

    def handle_nodedown(node, reason):
        log.error(f"Node down: {node}, reason: {reason}")
        log_cluster_state()

    def log_cluster_state():
        log.info(f"""
        Cluster state:
        - Current node: {current_node()}
        - Visible nodes: {list_nodes()}
        - Hidden nodes: {list_hidden_nodes()}
        - Connected: {list_connected_nodes()}
        """)
```

## Quick Reference

| Problem | Pattern | See Reference |
|---------|---------|---------------|
| Large clusters (>50 nodes) | Overlay network topology | `references/elixir.md`, `references/rust.md` |
| Strong consistency | Raft consensus | `references/elixir.md` (`:ra`), `references/rust.md` (`raft-rs`) |
| Eventual consistency | CRDTs | `references/elixir.md` (`delta_crdt`), `references/rust.md` |
| Leader election | Consensus or external coordinator | Language references |
| Service discovery | DNS, Consul, K8s | `references/elixir.md` (`libcluster`) |
| Distributed state sync | PubSub | Language references |
| Presence tracking | CRDT-based tracker | Language references |
| Clock synchronization | Hybrid Logical Clocks | Manual implementation (see above) |
| Split-brain prevention | Quorum + fencing | Consensus libraries |
| Causal ordering | Vector clocks | Manual implementation (see above) |

## Production Checklist

Before deploying distributed system:

- [ ] **Consensus**: Choose CP (Raft) or AP (CRDT) based on requirements
- [ ] **Quorum**: Verify correct quorum calculation (`n // 2 + 1`)
- [ ] **Split-brain**: Implement detection and prevention
- [ ] **Clock drift**: Use monotonic time for intervals, NTP monitoring
- [ ] **Network partitions**: Define merge/resolution strategy
- [ ] **Cluster size**: Use overlay networks if >50 nodes
- [ ] **Service discovery**: Configure for environment
- [ ] **Monitoring**: Set up distributed tracing (telemetry, OpenTelemetry)
- [ ] **Failure modes**: Test with Chaos Monkey, network partition simulation
- [ ] **Observability**: Distributed metrics, logging with trace_id

## When to Use Which Approach

### Use Raft (CP) when:
- Financial transactions
- Inventory management
- Configuration management
- Leader election
- Strong consistency required
- Can tolerate reduced availability during partitions

### Use CRDTs (AP) when:
- Social features (likes, follows)
- Collaborative editing
- Shopping carts
- Presence/status
- Offline-first apps
- Availability more important than consistency

### Use Full-Mesh Distributed Mode when:
- Small to medium cluster (<50 nodes)
- Trusted network
- Simple clustering needs
- Process distribution
- Transparent remote messaging

### Use Overlay Networks when:
- Large clusters (>50 nodes)
- Need custom topology
- Multi-datacenter deployment
- Gossip protocols
- Peer-to-peer systems

## Language-Specific Implementations

For language-specific implementations of these patterns:
- **Elixir**: See `references/elixir.md` - `:ra`, Partisan, libcluster, Phoenix.PubSub, delta_crdt
- **Rust**: See `references/rust.md` - raft-rs, tonic, tokio networking, async patterns

## Related Skills

- **production-quality**: Monitoring, observability, error handling
- **algorithms**: Probabilistic data structures for distributed systems
- **cognitive-complexity**: Designing maintainable distributed code

Use the **distributed-systems-expert** agent for deep analysis of distributed architectures, consensus algorithm selection, and distributed bug investigation.
