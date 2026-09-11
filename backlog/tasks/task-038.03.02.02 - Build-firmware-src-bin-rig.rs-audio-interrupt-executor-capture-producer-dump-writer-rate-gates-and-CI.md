---
id: TASK-038.03.02.02
title: >-
  Build firmware/src/bin/rig.rs: audio interrupt executor, capture producer,
  dump writer, rate gates and CI coverage
status: Blocked
assignee:
  - '@agent'
created_date: '2026-09-11 15:53'
updated_date: '2026-09-11 15:53'
labels:
  - task
  - planned
dependencies:
  - TASK-038.03.02.01
modified_files:
  - firmware/src/bin/rig.rs
  - firmware/Cargo.toml
  - .github/workflows/ci.yml
  - crates/asperitas-logging/src/spin_budget.rs
  - docs/reference/daisy-seed3.md
parent_task_id: TASK-038.03.02
priority: high
type: task
ordinal: 83700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The firmware half of TASK-038.03.02: the binary itself. It plays a stimulus out of the codec, records what comes back through the Pod self-loopback into external SDRAM, moves that recording out over the framed console without starving the audio callback, and reports enough numbers that "did audio starve?" is a measurement rather than an assertion.

Its sibling, TASK-038.03.02.01, ships the four console verbs (`RIGCFG`, `CAPSTAT`, `CAPMAX`, `DUMPEND`), the whole-record emit path and the incremental CRC this ticket consumes. Read that ticket's finalization notes for the signatures as actually shipped — the code snippets in parent plan §2 were written before them.

Why a new file rather than growing `podtest.rs` or `main.rs` is argued in TASK-038.03's description; do not relitigate it. `main.rs` and `podtest.rs` stay byte-identical — AC #1 exists because TASK-018.04 pinned podtest's output contract with human ears.

Nothing here needs a board. Every criterion is checkable by build, test, lint, `size -B`, or reading the emitted record grammar in a host test — with one honest exception: whether the numbers are *true* is TASK-038.05's job, which is why each report carries the counters that make starvation visible instead of assuming it away.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `firmware/src/bin/rig.rs` exists and builds under CI's command `cargo build --release --features seed3`, and `git diff --name-only HEAD` shows `firmware/src/bin/main.rs` and `podtest.rs` untouched, so the human-verified podtest output contract from TASK-018.04 cannot regress.
- [ ] #2 Audio runs on a dedicated `InterruptExecutor` pended on `SAI1` with embassy-executor's `executor-interrupt` feature enabled, while the USB drain, LED blink task, `CAPSTAT` emitter and dump writer stay on the thread executor. The type carries no const-generic parameter in 0.10 (`InterruptExecutor<1>` does not compile), `start()` returns a `SendSpawner` and unmasks the IRQ itself, so the priority is set **before** it. The comment cites upstream `examples/looper.rs` lines 27-31 and 132-134 at the pinned commit `ca9bcc9`, and records why the `SAI1` vector is free (the audio driver binds `DMA1_CH0`/`CH1`, `audio.rs:26-29`; embassy-stm32's SAI binds only DMA lines). The numeric relationship between the DMA IRQ priority and the executor priority is recorded with its source: `Config::default()` ships `dma_interrupt_priority` P0 (`embassy-stm32 src/lib.rs:362`), which outranks the P6 executor, and that direction is required because the DMA ISR is what pends `SAI1`.
- [ ] #3 Stimulus kind is a compile-time selection via cargo features `stim-sine`, `stim-ess`, `stim-pulse`, with sine at −20 dBFS when nothing else is set, and mutual exclusion enforced by a `const` assert in the source (cargo cannot express it). CI builds all four combinations so none can rot. The device emits exactly one `RIGCFG` record whose payload embeds the generator's own `describe()` output plus sample rate, capture format, block geometry and `cpu_hz`, so no second description grammar exists.
- [ ] #4 Input capture stores the loop channel as 16-bit mono into the ring defined by `asperitas_logging::capture`, publishing each block through `Filling -> Full -> Dumping -> Free` using that module's transition table, with samples written before the index that publishes them. The producer writes only blocks it found `Free`; when none are free it stops capturing and increments a visible overrun counter instead of overwriting a block being dumped. A `const` assert ties `capture::FRAMES_PER_CALLBACK` to `daisy_embassy::audio::BLOCK_LENGTH`, compared **in samples**: `HALF_DMA_BUFFER_LENGTH` counts 64 `u32` words per callback (32 stereo frames) while `CALLBACK_BYTES` counts 64 bytes of one mono channel, so asserting those two figures equal would pass by coincidence and comparing either against `HALF_DMA_BUFFER_LENGTH * 2` could never pass at all.
- [ ] #5 Per-callback work is bounded to one contiguous copy plus lane truncation. DWT cycle-counter instrumentation reports worst-case callback duration and longest inter-callback gap, brought up with the sequence already proven on this part (`DCB.enable_trace()`, `DWT::unlock()` to clear the H7 software lock, `has_cycle_counter()` probe, enable, read-back liveness check — per `spin_budget.rs:80-97`) and reporting zeros honestly when `CYCCNT` is unavailable; cycles become microseconds via a boot-time calibration against `embassy-time`, since embassy-stm32 0.6.0 exposes no CPU-clock accessor. A periodic `CAPSTAT` record carries delivered blocks, expected blocks, capture overruns, `max_block_us`, `worst_gap_us`, dump progress and the transports' `dropped_full`, giving hardware verification two independent starvation signals.
- [ ] #6 The device reports `CAPMAX total_bytes ring_bytes seconds_max unused_headroom_bytes` computed at runtime from `sdram::SDRAM_SIZE` and the published ring geometry, so capturable duration is measured from the driver constant rather than guessed, and the headroom statement makes clear that live audio DMA buffers remain in internal RAM.
- [ ] #7 The dump writer obtains permission to enqueue from `dump::try_emit_dump`, which consults TASK-038.02's capacity predicate, and never bypasses it; refusals are retried on a `Timer` backoff rather than a busy-wait, and both the refusal count and the longest consecutive stall are counted and reported. Ordinary log and `STATUS` traffic stays lossless during a dump. Its `AUDEND` CRC is accumulated with `frame::crc16_ccitt_update` across chunks, never by re-scanning a block it does not hold.
- [ ] #8 Capture start and end are decided by the device: it captures for a build-time-configured window or until the ring reports full, then begins the dump on its own, because runtime control over the console link belongs to TASK-032.
- [ ] #9 When a dump finishes the device emits one `DUMPEND` record naming blocks, chunks, bytes, elapsed milliseconds and the transport loss counters at that moment, so a caller times and validates a transfer from the captured stream alone.
- [ ] #10 The SDRAM memory model is recorded where a future reader will hit it: `init()` returns `0xC000_0000` while the driver programs its cacheable MPU region at `0xD000_0000`, caches are enabled nowhere in the stack so accesses are uncached and coherent today, nothing here enables caches or changes the MPU base, and the device reports the I-cache and D-cache enable bits it actually observes at boot so the claim is measured rather than asserted. TASK-038.05 receives a **rule** rather than an open question: caches stay off until someone owns the coherence argument for the FMC window and revisits the capture hand-off ordering in the same change. A short factual note lands in `docs/reference/daisy-seed3.md` §4.
- [ ] #11 Rate arithmetic is gated by `const` asserts in `rig.rs`, not prose: the capture window provably fits the ring, and `CAPSTAT` traffic is provably below one percent of the dump's own record traffic — dividing by `console::CAPSTAT_MAX_BODY` from the crate that renders it, not a number typed in here.
- [ ] #12 The transport-less build still links: `cargo build --release --no-default-features --features "seed3 log-defmt"` compiles `rig`, with the console-verb and dump paths behind `#[cfg(feature = "log-usb")]` and `CAPSTAT`-shaped facts going through `log::info!` there, plus one comment line saying plainly that this build has no dump transport. No fake dump over RTT.
- [ ] #13 `cargo fmt --all --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and each non-default stimulus variant build all pass, and CI gains a firmware clippy step (`cargo clippy --release --features seed3 -- -D warnings` inside `firmware/` — `--all-targets` cannot work there, a `no_std` target has no test crate) since firmware is excluded from the root workspace and nothing else lints `rig.rs`. The finalization notes record `rig`'s measured text and `.bss` from `size -B` against `main`'s measured baseline (text 88181, data 1428, bss 8224 — 1.57 % of the 512 KiB AXI SRAM), and state that this ticket's original 86.13 % `.bss` premise had no recorded provenance and contradicts measurement.
- [ ] #14 The stale sentence in `spin_budget.rs:81-86` — which justifies `steal()` by the absence of any `cortex_m::Peripherals::take()` in the crate, daisy-embassy or embassy-stm32 — is updated to say `rig.rs` claims the singleton and `steal()` remains correct because neither user writes `CYCCNT`.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
### Read first, in this order

1. **Parent TASK-038.03.02's plan, sections 0-11 — it is the authority for this ticket and was verified line by line against the pinned sources. Do not re-derive anything in it, and do not import claims from older ticket prose:**
   - §"Five claims … corrected here": `InterruptExecutor` is **not** generic in 0.10; there are **no** `max-tasks-*` features (pool sizing is `#[task(pool_size = N)]`, exhaustion surfaces at the task call); `Config::default()` ships DMA IRQs at P0; Seed3 is **single-core** (STM32H750IB, one M7 — no CPACR trick, probe with `DWT::has_cycle_counter()`, read with `cycle_count()` because `get_cycle_count` is deprecated and would fail `-D warnings`); the 86.13 % `.bss` baseline never existed (measured `main`: text 88181 / data 1428 / bss 8224).
   - §0 established facts · §1 Cargo + CI · §3 boot sequence and the `cortex_m::Peripherals::take()` claim, DWT bring-up, and the 20 ms calibration against `embassy-time` (publish `cpu_hz` from it; never hardcode 480, never write 600) · §4 executor topology and the PRIMASK argument for why the callback touches only SDRAM and atomics · §5 stimulus selection (`Sine::default()` is already −20 dBFS; construct the sweep at boot, never in the callback) · §6 capture producer · §7 dump writer · §8 rate gates · §9 SDRAM memory model · §10 transport-less build · §11 verification ladder and measured sizes.
   - §2 describes the verbs — they are **already shipped** by TASK-038.03.02.01; take the signatures from that crate as it now reads, not from the plan's sketches.
2. `~/.cargo/git/checkouts/daisy-embassy-4e2531dd3689e74c/ca9bcc9/examples/looper.rs` (lines 27-31, 44-51, 63-92, 132-134) — the executor precedent at the exact pinned commit. There is no crates.io `daisy-embassy-0.0.2` tree here; anything quoting `init_interrupts()` or `Priority::P1` describes a different version and is wrong.
3. `firmware/src/bin/main.rs` for the boilerplate every binary duplicates (tracked by TASK-010 — copy the shape, do not refactor five binaries as a side quest) and `podtest.rs:126-131` for the const rate-gate idiom.

### Deliverables

`firmware/Cargo.toml` (add `executor-interrupt`; three `stim-*` features; **no** `[[bin]]` section — `rig.rs` is auto-discovered) → `firmware/src/bin/rig.rs` (boot, DWT + calibration, executor topology, stimulus render site shaped for TASK-019.03, capture producer, dump writer, const rate gates) → `.github/workflows/ci.yml` (two stimulus-variant builds with `--bin rig`, one firmware clippy step) → `docs/reference/daisy-seed3.md` §4 note → `spin_budget.rs` comment. Then the §11 ladder, with its outputs pasted into the finalization notes, including the two `size -B` measurements.

### What the aborted attempts left behind

`rig.rs` was never written — neither attempt reached it. Check `git stash list` anyway: if `stash@{0}` ("wip-038.03.02-uncommitted") still exists after TASK-038.03.02.01 ran, its `spin_budget.rs` and `tests/commit_path_no_panic.rs` hunks are the parts relevant here; review them rather than applying blind, and drop the stash once dispositioned.

### How to run this ticket without dying at the deadline

The parent died twice at the 40-minute execute deadline with `rig.rs` still nonexistent, because the crate-side work and this binary shared one increment. This leaf is firmware only, but it is still the larger half:

- Ping the orchestrator over intercom **at least every 10 minutes**, even mid-edit-stream — a silent worker is killed as if hung, whatever it was actually doing.
- Commit checkpoints as you go: first commit when `rig.rs` exists and cross-builds, more as producer / dump writer / gates land. Ralph requires only that a commit landed; a checkpointed tree survives a cut deadline and the next attempt continues instead of archaeology.
- If you run out of budget with the gates unrun, stop at a committed checkpoint and leave a comment saying exactly which ladder steps passed. That is better than a green claim you did not measure.
<!-- SECTION:PLAN:END -->
