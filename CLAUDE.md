# Asperitas

A musically-interactive audio effect for acoustic and clean electric instruments, built
in Rust for the Electro-Smith Daisy Seed3 in a Daisy Pod.

## Where things live

Crates (root workspace, `crates/`):

- `asperitas-dsp` - hardware-independent DSP.
- `asperitas-stimulus` - rig measurement stimuli, replay effect stage and its wire codes; depends on dsp.
- `asperitas-cli` - offline WAV `process`/`generate` and goldens.
- `asperitas-wire` - `no_std` wire protocol shared by firmware and host.
- `asperitas-replay` - replay staging core shared by firmware and host.
- `asperitas-logging` - device logging runtime, USB console, panic handling, LEDs.
- `asperitas-pod` - Pod control drivers.
- `asperitas-measure` - host loopback analysis.
- `asperitas-rig` - host capture engine (see the rig doc below).
- `asperitas-host` - host tooling for the wire protocol.

`firmware/` is a separate workspace (`src/bin`: main, blinky, ledtest, panictest, podtest, rig).

Other docs: `backlog/docs/doc-001 - Asperitas-Project-Plan.md` (the plan),
[idea.md](idea.md) (original research handoff), [audio/README.md](audio/README.md) (corpus, goldens,
captures layout), [docs/development.md](docs/development.md) (gates, nix, lefthook, CI),
[docs/investigations/seed3-schematic-search-log.md](docs/investigations/seed3-schematic-search-log.md),
[docs/investigations/](docs/investigations/) (bench evidence behind the reference docs),
[docs/upstream/](docs/upstream/) (upstream bug reports and write-ups).

## How to check your work

- `bash scripts/gates.sh commit` or `push` runs the gate tier; no argument runs the CI set,
  `--list` prints every gate.
- Host crates: `cargo test`, `cargo clippy`, `cargo fmt` at the repo root.
- Firmware has no host test harness: `cd firmware && make check|clippy|build BINARY=... FEATURES=...`.
  Correctness rests on the cross-build, lints and the rig.
- Captures: `captures/TASK-x/` holds the rig evidence of each hardware verification ticket (layout and
  size policy in [audio/README.md](audio/README.md)). Format and flags are in
  [docs/reference/asperitas-rig.md](docs/reference/asperitas-rig.md).

## Hardware & ecosystem reference — READ BEFORE TOUCHING FIRMWARE

These notes record hardware and ecosystem facts that are non-obvious, easy to get wrong,
or expensive to rediscover. Consult them before writing or debugging any device-facing
code.

- **[docs/reference/daisy-seed3.md](docs/reference/daisy-seed3.md)** — Seed3 hardware.
  What differs from earlier Seeds (only the codec and USB-C), the exact SAI
  configuration, and why **the TAC5242 codec is hardware-strapped rather than
  I²C-configured**. Also records that RTT keeps no loss ledger, so a drop count from the
  console does not describe an RTT capture, and that libDaisy (C++) has no
  Seed3 support at all, so "prove it in C++ first" is not an available fallback.
- **[docs/reference/flashing.md](docs/reference/flashing.md)** — Flashing runbook. Two routes
  and two log channels: **DFU over the onboard USB-C** or **an ST-Link probe on the SWD pads**,
  logged over the **framed USB console** or **`defmt`/RTT** through that probe.
  The probe is the way into a board that never brings USB up (verified on the bench, with
  probe-rs built from main for `--connect-under-reset`). Never name a stock `STM32H7...` chip
  to probe-rs: it wedges the debug port until the USB-C is replugged. Evidence lives in
  [docs/investigations/](docs/investigations/).
- **[docs/reference/daisy-pod.md](docs/reference/daisy-pod.md)** — Pod control pin map
  (knobs, encoder, buttons, RGB LEDs), libDaisy's defaults, and the fact that **Pod
  audio I/O is line level, not hi-Z instrument level** — a gain-staging trap that reads
  as a DSP bug. Also: **plugging both output jacks (headphone out and line out) mixes the
  channels**, so a one-channel signal comes out of both sides; use one output at a time for
  left/right checks.
- **[docs/reference/protocol.md](docs/reference/protocol.md)** - index from each wire grammar
  (console frame, `BOOT`/`STATUS`, rig records, dump, excerpts, host control, capture geometry) to the
  rustdoc module that owns it. The grammar is in rustdoc, not in the Seed3 page.
- **[docs/reference/asperitas-rig.md](docs/reference/asperitas-rig.md)** — host capture runner
  for the framed USB console: flags, exit codes (0 pass, 1 failed verdict, 2 error), the
  artifact format, and the convention of committing artifacts with hardware verification
  tickets.
- **[docs/reference/rust-daisy-stack.md](docs/reference/rust-daisy-stack.md)** — crate
  landscape and, critically, the status of **daisy-embassy PR #80**, which supplied
  Seed3 support and merged on 2026-08-01, so the `seed3` feature is on `master` and no
  commit SHA pin is needed. Time-sensitive; re-check the doc before relying on it.

## Ticket assignment convention — @agent vs @human

This is a physical-hardware project. Some work genuinely cannot be done by an agent: it
needs a board plugged in, ears, instruments, or a decision that is the owner's to make.

**Every ticket carries an `assignee` of `@agent` or `@human`.**

- **`@agent`** — an agent may pick this up and carry it to Done unattended.
- **`@human`** — an agent must **not** pick this up on its own, and an unattended run must
  **never** mark it Done. Its acceptance criteria are prefixed `HUMAN:` and cannot be
  satisfied by reading code or watching a build succeed. An agent working with the owner
  in a live session may tick the criteria and close the ticket, but only after the owner
  has walked through every criterion with it and agreed each one is met. Record that the
  close was done with the owner, so the history shows a person signed off.

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
If a criterion says `HUMAN:`, no amount of agent work alone satisfies it; the owner's
sign-off does.

When creating new tickets, apply the same rule. Anything requiring the device, ears,
instruments, or an outward-facing action (creating a repo, posting upstream) is `@human`
or gets split. Pushing `main` to origin is the exception: the owner allows agents to push it,
and CI runs are readable with `gh`, so a push-and-check-CI step is `@agent` work. Force-pushes
and other branches or remotes still need asking.

<!-- BACKLOG.MD MCP GUIDELINES START -->
<!-- backlog.md-instructions-version: 1.48.0 -->

## Code comments

Comments describe the current state of the code and why it is the way it is. They carry no ticket
numbers, no dates, no measurement history and no narration of earlier versions ("was X until
TASK-n", "measured on ..."): that belongs in the ticket notes, the commit message or a doc. Do not
cite line numbers in other files, they go stale; name the function or module instead.

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
