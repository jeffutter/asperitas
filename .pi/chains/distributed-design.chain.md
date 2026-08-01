---
name: distributed-design
description: Evaluate distributed system requirements, design the architecture with correct CAP tradeoffs, then implement with proper consensus and partition handling.
---

## distributed-systems-expert
output: distributed-analysis.md
progress: true

Analyze distributed system requirements for: {task}

Evaluate:
- Consistency vs availability requirements (CP vs AP)
- Recommended consensus strategy with rationale (Raft/:ra, CRDTs, LWW)
- Correct quorum calculation for the cluster size
- Failure modes: split-brain prevention, network partition behavior
- Clock safety: identify any System.system_time() usage that needs monotonic time
- Clustering strategy based on expected node count

## architect
reads: distributed-analysis.md
output: architecture.md
progress: true

Design the distributed system architecture for: {task}

Build on this analysis: {previous}

Include:
- Module structure and state machine design
- Explicit error handling for distributed failures (nodedown, partition, timeout)
- How state is replicated and conflicts resolved
- Monitoring and health check hooks
- Phased implementation plan that can be tested incrementally

## developer
reads: distributed-analysis.md, architecture.md
progress: true

Implement the distributed component for: {task}

Write tests first, including partition simulation scenarios. Implement with:
- Correct quorum arithmetic (div(n, 2) + 1, not div(n, 2))
- Monotonic time for all interval/lease logic
- Explicit merge strategy for partition recovery
- Health check endpoints

Run the test suite including any simulated partition tests.
