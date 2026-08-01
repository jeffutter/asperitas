# Asperitas

A musically-interactive audio effect for acoustic and clean electric instruments, built
in Rust for the Electro-Smith Daisy Seed3 in a Daisy Pod.

## Hardware & ecosystem reference — READ BEFORE TOUCHING FIRMWARE

These notes record hardware and ecosystem facts that are non-obvious, easy to get wrong,
or expensive to rediscover. Consult them before writing or debugging any device-facing
code.

- **[docs/reference/daisy-seed3.md](docs/reference/daisy-seed3.md)** — Seed3 hardware.
  What differs from earlier Seeds (only the codec and USB-C), the exact SAI
  configuration, DFU flashing without a debug probe, and why **the TAC5242 codec is
  hardware-strapped rather than I²C-configured**. Also records that libDaisy (C++) has
  no Seed3 support at all, so "prove it in C++ first" is not an available fallback.
- **[docs/reference/daisy-pod.md](docs/reference/daisy-pod.md)** — Pod control pin map
  (knobs, encoder, buttons, RGB LEDs), libDaisy's defaults, and the fact that **Pod
  audio I/O is line level, not hi-Z instrument level** — a gain-staging trap that reads
  as a DSP bug.
- **[docs/reference/rust-daisy-stack.md](docs/reference/rust-daisy-stack.md)** — crate
  landscape and, critically, the status of **daisy-embassy PR #80**, which supplies
  Seed3 support and is currently unmerged. Pin to its commit SHA. Time-sensitive;
  re-check before relying on it.

## Ticket assignment convention — @agent vs @human

This is a physical-hardware project. Some work genuinely cannot be done by an agent: it
needs a board plugged in, ears, instruments, or a decision that is the owner's to make.

**Every ticket carries an `assignee` of `@agent` or `@human`.**

- **`@agent`** — an agent may pick this up and carry it to Done unattended.
- **`@human`** — an agent must **not** pick this up, and must **never** mark it Done.
  Its acceptance criteria are prefixed `HUMAN:` and cannot be satisfied by reading code
  or watching a build succeed.

Work that is part agent and part human is **split into subtasks** (`TASK-005.01`,
`TASK-005.02`, …) rather than assigned to one or the other. A parent ticket is an
umbrella whose only acceptance criterion is that its subtasks are done.

**A parent inherits the strictest assignee among its children.** If any child is
`@human`, the parent is `@human` too — it cannot be closed until that child is, so an
agent picking it up can only spin. This is not hypothetical: TASK-004 was left `@agent`
with a `@human` child, reached the execute stage, and the executor correctly reported
that a person had to flash the board — but with no work left to do it produced no
commit, tripped the "claimed success but no commit landed" guard, and was re-selected
until the failure-streak guard halted the whole run.

**Never leave an abandoned task in `backlog/archive/`.** Archiving preserves both the
ID and the status, so an archived stub sharing an ID with a real task will shadow it
when dependencies are resolved, and every dependent ticket silently looks blocked
forever. `backlog doctor` does not catch this — it only scans active and completed
tasks. Delete throwaways outright.

The failure mode this exists to prevent: an agent marking "audio passthrough works"
complete because it compiled, having never heard a sound. **Compiling is not evidence.**
If a criterion says `HUMAN:`, no amount of agent work satisfies it.

When creating new tickets, apply the same rule. Anything requiring the device, ears,
instruments, or an outward-facing action (creating a repo, posting upstream) is `@human`
or gets split.

<!-- BACKLOG.MD MCP GUIDELINES START -->
<!-- backlog.md-instructions-version: 1.48.0 -->

<CRITICAL_INSTRUCTION>

## BACKLOG WORKFLOW INSTRUCTIONS

This project uses Backlog.md MCP for all task and project management activities.

**CRITICAL GUIDANCE**

- If your client supports MCP resources, read `backlog://workflow/overview` to understand when and how to use Backlog for this project.
- If your client only supports tools or the above request fails, call `backlog.get_backlog_instructions()` to load the tool-oriented overview. Use the `instruction` selector when you need `task-creation`, `task-execution`, or `task-finalization`.

- **First time working here?** Read the overview resource IMMEDIATELY to learn the workflow
- **Already familiar?** You should have the overview cached ("## Backlog.md Overview (MCP)")
- **When to read it**: BEFORE creating tasks, or when you're unsure whether to track work

These guides cover:
- Decision framework for when to create tasks
- Search-first workflow to avoid duplicates
- Links to detailed guides for task creation, execution, and finalization
- MCP tools reference

You MUST read the overview resource to understand the complete workflow. The information is NOT summarized here.

</CRITICAL_INSTRUCTION>

<!-- BACKLOG.MD MCP GUIDELINES END -->
