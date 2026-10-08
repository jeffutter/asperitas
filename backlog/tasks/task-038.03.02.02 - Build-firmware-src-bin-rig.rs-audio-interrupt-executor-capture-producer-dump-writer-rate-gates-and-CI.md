---
id: TASK-038.03.02.02
title: >-
  Build firmware/src/bin/rig.rs: audio interrupt executor, capture producer,
  dump writer, rate gates and CI coverage
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-11 15:53'
updated_date: '2026-10-08 02:33'
labels:
  - task
  - planned
dependencies:
  - TASK-038.03.02.01
  - TASK-038.03.02.03
  - TASK-038.03.02.04
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

Its sibling, TASK-038.03.02.01, ships the five console verbs (`RIGCFG`, `RIGGEN`, `CAPSTAT`, `CAPMAX`, `DUMPEND`), the whole-record emit path and the incremental CRC this ticket consumes. Read that ticket's finalization notes for the signatures as actually shipped - the code snippets in parent plan §2 were written before them.

Why a new file rather than growing `podtest.rs` or `main.rs` is argued in TASK-038.03's description; do not relitigate it. `main.rs` and `podtest.rs` stay byte-identical — AC #1 exists because TASK-018.04 pinned podtest's output contract with human ears.

Nothing here needs a board. Every criterion is checkable by build, test, lint, `size -B`, or reading the emitted record grammar in a host test - with one honest exception: whether the numbers are *true* is TASK-038.05's job, which is why each report carries the counters that make starvation visible instead of assuming it away.

**This ticket no longer carries the work itself.** It was split again on 2026-09-12 into TASK-038.03.02.03 (boot skeleton, DWT, executor topology, stimulus gates) and TASK-038.03.02.04 (capture producer, dump writer, rate gates, CI), because the parent had already died twice at the forty-minute execute deadline with `rig.rs` still nonexistent, and this leaf was still the larger half of the crate-side work. What remains here is integration: the acceptance criteria below stay authoritative as the definition of done, the disposition table maps each of them to the leaf that discharges it, and this ticket is closed by checking them against the shipped code once both leaves are Done.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `firmware/src/bin/rig.rs` exists and builds under CI's command `cargo build --release --features seed3`, and `git diff --name-only HEAD` shows `firmware/src/bin/main.rs` and `podtest.rs` untouched, so the human-verified podtest output contract from TASK-018.04 cannot regress.
- [ ] #2 Audio runs on a dedicated `InterruptExecutor` pended on `SAI1` with embassy-executor's `executor-interrupt` feature enabled, while the USB drain, LED blink task, `CAPSTAT` emitter and dump writer stay on the thread executor. The type carries no const-generic parameter in 0.10 (`InterruptExecutor<1>` does not compile), `start()` returns a `SendSpawner` and unmasks the IRQ itself, so the priority is set **before** it. The comment cites upstream `examples/looper.rs` lines 27-31 and 132-134 at the pinned commit `ca9bcc9`, and records why the `SAI1` vector is free (the audio driver binds `DMA1_CH0`/`CH1`, `audio.rs:26-29`; embassy-stm32's SAI binds only DMA lines). The numeric relationship between the DMA IRQ priority and the executor priority is recorded with its source: `Config::default()` ships `dma_interrupt_priority` P0 (`embassy-stm32 src/lib.rs:362`), which outranks the P6 executor, and that direction is required because the DMA ISR is what pends `SAI1`.
- [ ] #3 Stimulus kind is a compile-time selection via cargo features `stim-sine`, `stim-ess`, `stim-pulse`, with sine at −20 dBFS when nothing else is set, and mutual exclusion enforced by a `const` assert in the source (cargo cannot express it). CI builds all four combinations so none can rot. The device emits exactly one `RIGCFG` record (capture format, block geometry, window, `cpu_hz`, cache bits) and exactly one `RIGGEN` record embedding the generator's own `describe()` output verbatim, so no second description grammar exists. `rig.rs` calls `describe()` into a buffer sized `console::RIGGEN_MAX_GEN_BYTES` and debug-asserts the returned length against that budget; a text clipped at the frame limit would otherwise print a truncated parameter as if it were real.
- [ ] #4 Input capture stores the loop channel as 16-bit mono into the ring defined by `asperitas_logging::capture`, publishing each block through `Filling -> Full -> Dumping -> Free` using that module's transition table, with samples written before the index that publishes them. The producer writes only blocks it found `Free`; when none are free it stops capturing and increments a visible overrun counter instead of overwriting a block being dumped. A `const` assert ties `capture::FRAMES_PER_CALLBACK` to `daisy_embassy::audio::BLOCK_LENGTH`, compared **in samples**: `HALF_DMA_BUFFER_LENGTH` counts 64 `u32` words per callback (32 stereo frames) while `CALLBACK_BYTES` counts 64 bytes of one mono channel, so asserting those two figures equal would pass by coincidence and comparing either against `HALF_DMA_BUFFER_LENGTH * 2` could never pass at all.
- [ ] #5 Per-callback work is bounded to one contiguous copy plus lane truncation. DWT cycle-counter instrumentation reports worst-case callback duration and longest inter-callback gap, brought up with the sequence already proven on this part (`DCB.enable_trace()`, `DWT::unlock()` to clear the H7 software lock, `has_cycle_counter()` probe, enable, read-back liveness check - per `spin_budget.rs:80-97`) and reporting zeros honestly when `CYCCNT` is unavailable; cycles become microseconds via a boot-time calibration against `embassy-time`, since embassy-stm32 0.6.0 exposes no CPU-clock accessor. A periodic `CAPSTAT` record carries delivered blocks, expected blocks, capture overruns, `max_block_us`, `worst_gap_us`, `audio_exit`, dump progress (`dumped`) and the transports' `dropped_full`, giving hardware verification two independent starvation signals. Refusals and the longest stall are *not* in `CAPSTAT` — they belong to the one dump they describe and ride on `DUMPEND`.
- [ ] #6 The device reports `CAPMAX total_bytes ring_bytes seconds_max unused_headroom_bytes` computed at runtime from `sdram::SDRAM_SIZE` and the published ring geometry, so capturable duration is measured from the driver constant rather than guessed, and the headroom statement makes clear that live audio DMA buffers remain in internal RAM.
- [ ] #7 The dump writer obtains permission to enqueue from `dump::try_emit_dump`, which consults TASK-038.02's capacity predicate, and never bypasses it; refusals are retried on a `Timer` backoff rather than a busy-wait, and both the refusal count and the longest consecutive stall are counted and reported. Ordinary log and `STATUS` traffic stays lossless during a dump. Its `AUDEND` CRC is accumulated with `frame::crc16_ccitt_update` across chunks, never by re-scanning a block it does not hold.
- [ ] #8 Capture start and end are decided by the device: it captures for a build-time-configured window or until the ring reports full, then begins the dump on its own, because runtime control over the console link belongs to TASK-032.
- [ ] #9 When a dump finishes the device emits one `DUMPEND` record naming blocks, chunks, bytes, elapsed milliseconds, the dump's `refused` / `stall_ms`, and the transport loss counters at that moment, so a caller times and validates a transfer from the captured stream alone.
- [ ] #10 The SDRAM memory model is recorded where a future reader will hit it: `init()` returns `0xC000_0000` while the driver programs its cacheable MPU region at `0xD000_0000`, caches are enabled nowhere in the stack so accesses are uncached and coherent today, nothing here enables caches or changes the MPU base, and the device reports the I-cache and D-cache enable bits it actually observes at boot so the claim is measured rather than asserted. TASK-038.05 receives a **rule** rather than an open question: caches stay off until someone owns the coherence argument for the FMC window and revisits the capture hand-off ordering in the same change. A short factual note lands in `docs/reference/daisy-seed3.md` §4.
- [ ] #11 Rate arithmetic is gated by `const` asserts in `rig.rs`, not prose: the capture window provably fits the ring, and `CAPSTAT` traffic is provably below one percent of the dump's own record traffic — dividing by `console::CAPSTAT_MAX_BODY` from the crate that renders it, not a number typed in here.
- [ ] #12 The transport-less build still links: `cargo build --release --no-default-features --features "seed3 log-defmt"` compiles `rig`, with the console-verb and dump paths behind `#[cfg(feature = "log-usb")]` and `CAPSTAT`-shaped facts going through `log::info!` there, plus one comment line saying plainly that this build has no dump transport. No fake dump over RTT.
- [ ] #13 `cargo fmt --all --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and each non-default stimulus variant build all pass, and CI gains a firmware clippy step (`cargo clippy --release --features seed3 -- -D warnings` inside `firmware/` — `--all-targets` cannot work there, a `no_std` target has no test crate) since firmware is excluded from the root workspace and nothing else lints `rig.rs`. The finalization notes record `rig`'s measured text and `.bss` from `size -B` against `main`'s measured baseline (text 88181, data 1428, bss 8224 — 1.57 % of the 512 KiB AXI SRAM), and state that this ticket's original 86.13 % `.bss` premise had no recorded provenance and contradicts measurement.
- [ ] #14 The stale sentence in `spin_budget.rs:81-86` — which justifies `steal()` by the absence of any `cortex_m::Peripherals::take()` in the crate, daisy-embassy or embassy-stm32 — is updated to say `rig.rs` claims the singleton and `steal()` remains correct because neither user writes `CYCCNT`.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
### Disposition of the acceptance criteria

| AC | Discharged by | Note |
|---|---|---|
| #1 rig.rs builds, main/podtest untouched | .03 | both CI configs, no `--bin` needed |
| #2 interrupt executor, SAI1 priority before `start` | .03 | plus a boot-log reading of the effective NVIC priorities |
| #3 stimulus features, RIGCFG + RIGGEN | .03 | `stim-sine` must stay out of `default`; CI variant builds land in .04 |
| #4 capture producer, ring states, geometry assert | .04 | |
| #5 bounded callback, DWT, CAPSTAT | .03 (DWT) + .04 (CAPSTAT, durations) | see correction C1: `rcc::clocks()` exists |
| #6 CAPMAX from driver constants | .04 | |
| #7 dump writer, `dump_fits`, incremental CRC | .04 | |
| #8 device decides start and end | .04 | automatic arming timeline, single-shot run |
| #9 DUMPEND | .04 | |
| #10 SDRAM memory model recorded | .04 rule-in-comment + **TASK-038.06** for the doc note | see correction C5: that file has no §4 FMC/MPU section |
| #11 const rate gates | .04 | |
| #12 transport-less build links | .03 shim, .04 keeps it compiling | |
| #13 fmt/test/clippy/stim variants + size readings | .04 (CI) + .03 (sizes of the skeleton) | final sizes recorded here |
| #14 spin_budget stale sentence | .03 | the live hunk of `stash@{0}` |

### Read first, in this order

0. **"Plan corrections after TASK-038.03.02.01 landed" at the bottom of parent TASK-038.03.02's file. Where it contradicts §0-§11 below, the corrections win.** They were verified against the resolved sources on 2026-09-12.
1. **Parent TASK-038.03.02's plan, sections 0-11 - it is the technical authority and was verified line by line against the pinned sources. Do not re-derive anything in it, and do not import claims from older ticket prose:**
   - §"Five claims … corrected here": `InterruptExecutor` is **not** generic in 0.10; there are **no** `max-tasks-*` features (pool sizing is `#[task(pool_size = N)]`, exhaustion surfaces at the task call); `Config::default()` ships DMA IRQs at P0; Seed3 is **single-core** (STM32H750IB, one M7 — no CPACR trick, probe with `DWT::has_cycle_counter()`, read with `cycle_count()` because `get_cycle_count` is deprecated and would fail `-D warnings`); the 86.13 % `.bss` baseline never existed (measured `main`: text 88181 / data 1428 / bss 8224).
   - §0 established facts · §1 Cargo + CI · §3 boot sequence and the `cortex_m::Peripherals::take()` claim, DWT bring-up, and the 20 ms calibration against `embassy-time` (publish `cpu_hz` from it; never hardcode 480, never write 600) · §4 executor topology and the PRIMASK argument for why the callback touches only SDRAM and atomics · §5 stimulus selection (`Sine::default()` is already −20 dBFS; construct the sweep at boot, never in the callback) · §6 capture producer · §7 dump writer · §8 rate gates · §9 SDRAM memory model · §10 transport-less build · §11 verification ladder and measured sizes.
   - §2 describes the verbs — they are **already shipped** by TASK-038.03.02.01; take the signatures from that crate as it now reads, not from the plan's sketches.
2. `~/.cargo/git/checkouts/daisy-embassy-4e2531dd3689e74c/ca9bcc9/examples/looper.rs` (lines 27-31, 44-51, 63-92, 132-134) — the executor precedent at the exact pinned commit. There is no crates.io `daisy-embassy-0.0.2` tree here; anything quoting `init_interrupts()` or `Priority::P1` describes a different version and is wrong.
3. `firmware/src/bin/main.rs` for the boilerplate every binary duplicates (tracked by TASK-010 — copy the shape, do not refactor five binaries as a side quest) and `podtest.rs:126-131` for the const rate-gate idiom.

### Deliverables

None of `rig.rs`, `firmware/Cargo.toml`, `.github/workflows/ci.yml`, `spin_budget.rs` or `docs/reference/daisy-seed3.md` are edited by this ticket any more; they belong to .03, .04 and TASK-038.06 respectively. What lands here is verification and the record:

1. Both leaves Done.
2. Every acceptance criterion above re-checked against the shipped tree, not against the leaves' claims: build both transports, build both stimulus variants, run `cargo fmt --all --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, the firmware lint step, `size -B` on `rig` and `main`, and confirm `main.rs`/`podtest.rs` are still untouched (`git diff --name-only HEAD`).
3. Finalization notes naming: the measured sizes against `main`'s baseline (text 88181 / data 1428 / bss 8224), which ladder rows from parent §11 passed, and what remains for TASK-038.05's bench session.
4. Only then mark this Done, which is what unblocks the parent umbrella TASK-038.03.02.

### What the aborted attempts left behind

`rig.rs` was never written — neither attempt reached it. Check `git stash list` anyway: if `stash@{0}` ("wip-038.03.02-uncommitted") still exists after TASK-038.03.02.01 ran, its `spin_budget.rs` and `tests/commit_path_no_panic.rs` hunks are the parts relevant here; review them rather than applying blind, and drop the stash once dispositioned.

### How to run this ticket without dying at the deadline

The parent died twice at the 40-minute execute deadline with `rig.rs` still nonexistent, because the crate-side work and this binary shared one increment. The leaves carry that risk now, and each leaf's own plan repeats the operating rules. For this umbrella:

- If either leaf is not Done, there is nothing to do here. Do not start editing firmware to "help"; say so in a comment and stop.
- Ping the orchestrator over intercom **at least every 10 minutes** while running the checklist - a silent worker is killed as if hung, whatever it was actually doing.
- A verification pass that finds a gap opens a ticket for the gap rather than fixing it inline, so the fix gets reviewed where it belongs. Small mechanical misses (a missing size reading, an unstuck comment) may be committed directly, with the commit message naming which AC it closes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Parked 2026-09-12: both leaves are still open, so there is nothing to verify here

Ran the verification ladder far enough to establish that this ticket cannot be closed, then stopped
per the plan's own rule ("If either leaf is not Done, there is nothing to do here. Do not start
editing firmware to help"). Evidence from the tree rather than from ticket statuses:

- `firmware/src/bin/` contains `blinky.rs`, `ledtest.rs`, `main.rs`, `panictest.rs`, `podtest.rs` -
  **no `rig.rs`**. So AC #1 fails at its first clause and every downstream AC (#2 through #14) has
  nothing to check against.
- Leaf status: TASK-038.03.02.03 is Dev Ready with 0/12 AC checked; TASK-038.03.02.04 is Blocked
  behind it. Neither has been executed.
- Working tree is clean and HEAD is `eaca742`, the planning commit that created these two leaves.
  No partial work is at risk.

No acceptance criteria checked: none is discharged by an absent file, and checking them here would
be precisely the "marked Done with unchecked/unearned ACs" failure the workflow forbids.

### Next actionable step

Run **TASK-038.03.02.03** (it is the only 038-family task `backlog task list -s "Dev Ready" --ready`
returns today), then TASK-038.03.02.04, then re-select this ticket to run the full ladder and record
the `size -B` numbers against `main`'s baseline (text 88181 / data 1428 / bss 8224).

Note for whoever selects work: this umbrella carries ordinal 83700, lower than .03's 88700, so a
selector that orders by ordinal without filtering on dependencies picks the one thing it must not
do. `--ready` filters it correctly now that it is Blocked.

### Stash disposition (part of this ticket's remaining work, delegated)

`stash@{0}` = `wip-038.03.02-uncommitted` was reviewed hunk by hunk against the live tree rather
than applied. Its `commit_path_no_panic.rs` and `console.rs` / `frame.rs` / `lib.rs` hunks are
superseded by what TASK-038.03.02.01 shipped; the only wanted content is the `spin_budget.rs` safety
comment that closes AC #14, which belongs to .03. Verbatim replacement text and the drop instruction
are recorded in TASK-038.03.02.03's notes so nothing depends on the stash surviving. Left in place
rather than dropped here because this run made no code changes and a parked umbrella should not be
the thing that destroys state - .03 drops it once the comment is applied.

2026-10-07: moved Blocked -> Dev Ready at the owner's request. All three dependencies (TASK-038.03.02.01, .03, .04) are Done; .04 already ran the §11 ladder and recorded sizes in its notes, so this umbrella's remaining work is to confirm that against the tree and close.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-13 00:11
---
Planning note from TASK-060 (2026-09-12): AC #13's firmware-clippy-in-CI clause is superseded by TASK-060 / TASK-060.03, which adds whole-package cross clippy with -D warnings to pre-commit, pre-push and ci.yml, plus an RTT-only feature-set pass, placed after CI's existing firmware builds (measured local: 20 s cold standalone, 11 s after the build, ~2 s warm). AC #13's other clauses remain this ticket's. Two corrections for whoever executes .02: (a) the AC says root 'cargo fmt --all --check' will cover rig.rs - it cannot, Cargo.toml:3 excludes firmware/, and that blind spot is exactly why rig.rs drifted unformatted since 41f9cae (TASK-060.02 closes it); (b) firmware/Makefile's make clippy target is now at :264-269, not :252-255, and its comment claiming it is the only place firmware clippy runs becomes false when TASK-060.03 lands.
---
<!-- COMMENTS:END -->
