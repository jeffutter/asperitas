---
id: TASK-038.03.02
title: >-
  Build firmware/src/bin/rig.rs: interrupt-executor audio, stimulus playback,
  SDRAM capture ring, console dump
status: Blocked
assignee:
  - '@agent'
created_date: '2026-09-11 13:28'
updated_date: '2026-10-08 02:09'
labels:
  - task
  - planned
dependencies:
  - TASK-038.03.02.01
  - TASK-038.03.02.02
modified_files:
  - firmware/src/bin/rig.rs
  - firmware/Cargo.toml
  - .github/workflows/ci.yml
  - crates/asperitas-logging/src/console.rs
  - crates/asperitas-logging/src/lib.rs
  - crates/asperitas-logging/src/spin_budget.rs
  - docs/reference/daisy-seed3.md
parent_task_id: TASK-038.03
priority: high
type: task
ordinal: 83500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
**This ticket is an umbrella as of 2026-09-11; its plan below stays the authority for both leaves.** Two execute attempts were cut at the 40-minute deadline having produced no commit — the second one died roughly eight minutes short of the file this ticket exists to create, still repairing syntax damage the first had left uncommitted. The increment was too large for one pass: 13 criteria, five subsystems, host crate and bare-metal binary together. It is now split along the host/firmware seam:

- **TASK-038.03.02.01** — the four console verbs, their host pinning tests, one whole-record emit path, and the incremental CRC. Crate-side only, testable with no board.
- **TASK-038.03.02.02** — `rig.rs` itself, its cargo features, CI coverage, the SDRAM memory-model note.

What remains on this ticket is the integration check: run §11's verification ladder once over the joined result and record the measured sizes. Nothing here is executable until both leaves are Done.

The binary that closes TASK-038's measurement loop. It does four things that no existing binary does: it plays a stimulus out of the codec, records what comes back through the Pod self-loopback into external SDRAM, moves that recording out over the framed console without starving the audio callback, and reports enough numbers that "did audio starve?" is a measurement rather than an assertion.

Why a new file rather than growing `podtest.rs` or `main.rs` is argued in the parent description; do not relitigate it. `main.rs` and `podtest.rs` stay byte-identical — AC #1 exists because TASK-018.04 pinned podtest's output contract with human ears.

Three structural decisions are already made and are not open to revision here:

**Audio leaves the cooperative executor.** Today one thread executor polls audio, the USB drain, and the LED blink through nested `select`, so "the dump cannot starve audio" is a promise about discipline. Moving the audio interface onto an `InterruptExecutor` pended on `SAI1` makes preemption a property of the NVIC. Upstream `examples/looper.rs` at the exact commit this build pins (`ca9bcc9`) is the precedent, lines 27-32 and 132-134.

**Capture is 16-bit mono into a fixed ring whose geometry this ticket does not own.** The numbers come from `asperitas_logging::capture` (TASK-038.03.01). This ticket adds the producer, the atomic indices, and the overrun counter, plus the assertion that ties the copied driver constant back to the real one.

**Starvation gets two independent signals.** There is no audio-overrun counter anywhere in this repo, so the parent's "drop counter stayed at zero" would describe console drops only. DWT cycle-counter instrumentation gives worst-case callback duration and longest inter-callback gap; delivered-versus-expected block counts give a second, arithmetic signal.

Out of scope: host-initiated control (TASK-032), QSPI excerpt playback (TASK-038.04), bench verification (TASK-038.05), prose documentation beyond the SDRAM memory-model note (TASK-038.06).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 firmware/src/bin/rig.rs exists and builds under CIs command cargo build --release --features seed3, and git diff --name-only shows firmware/src/bin/main.rs and podtest.rs untouched, so the human-verified podtest output contract from TASK-018.04 cannot regress.
- [ ] #2 Audio runs on a dedicated InterruptExecutor pended on SAI1 with embassy-executor's executor-interrupt feature enabled, while the USB drain, LED blink task, CAPSTAT emitter and dump writer stay on the thread executor. The type carries no const-generic parameter in 0.10 (InterruptExecutor<1> does not compile), start() returns a SendSpawner and unmasks the IRQ itself, so the priority is set before it. The comment cites upstream examples/looper.rs lines 27-31 and 132-134 at the pinned commit ca9bcc9, and records why the SAI1 vector is free (the audio driver binds DMA1_CH0/CH1, audio.rs:26-29, and embassy-stm32's SAI binds only DMA lines). The numeric relationship between the DMA IRQ priority and the executor priority is recorded with its source: Config::default() ships dma_interrupt_priority P0 (embassy-stm32 src/lib.rs:362), which outranks the P6 executor, and that direction is required because the DMA ISR is what pends SAI1.
- [ ] #3 Stimulus kind is a compile-time selection via cargo features stim-sine, stim-ess and stim-pulse, with sine at -20 dBFS when nothing else is set, and mutually-exclusive selection enforced by a const assert. CI builds all four combinations so none can rot. The device emits exactly one RIGCFG record (capture format, block geometry, window, cpu_hz, cache bits) and exactly one RIGGEN record embedding the generators own describe() output verbatim, so no second description grammar exists. See §2 for why the text travels in its own record.
- [ ] #4 Input capture stores the loop channel as 16-bit mono into the ring defined by asperitas_logging::capture, publishing each block through Filling -> Full -> Dumping -> Free using that modules transition table, with samples written before the index that publishes them. The producer writes only blocks it found Free; when none are free it stops capturing and increments a visible overrun counter instead of overwriting a block being dumped. A const assert ties capture::FRAMES_PER_CALLBACK to daisy_embassy::audio::BLOCK_LENGTH, compared in samples rather than bytes: HALF_DMA_BUFFER_LENGTH counts 64 u32 words per callback (32 stereo frames) while CALLBACK_BYTES counts 64 bytes of one mono channel, so asserting those two figures equal would pass by coincidence and comparing either against HALF_DMA_BUFFER_LENGTH * 2 could never pass at all.
- [ ] #5 Per-callback work is bounded to one contiguous copy plus lane truncation. DWT cycle-counter instrumentation reports worst-case callback duration and longest inter-callback gap, brought up with the sequence already proven on this part (DCB enable_trace, DWT unlock to clear the H7 software lock, has_cycle_counter probe, enable, read-back liveness check, per spin_budget.rs:80-97) and reporting zeros honestly when CYCCNT is unavailable; cycles become microseconds via a boot-time calibration against embassy-time, since embassy-stm32 0.6.0 exposes no CPU-clock accessor. A periodic CAPSTAT record carries delivered blocks, expected blocks, capture overruns, max_block_us, worst_gap_us, dump progress and the transports dropped_full, giving hardware verification two independent starvation signals.
- [ ] #6 The device reports CAPMAX total_bytes ring_bytes seconds_max unused_headroom_bytes computed at runtime from sdram::SDRAM_SIZE and the published ring geometry, so capturable duration is measured from the driver constant rather than guessed, and the headroom statement makes clear that live audio DMA buffers remain in internal RAM.
- [ ] #7 RIGCFG, CAPSTAT, CAPMAX and the dump-summary verb are implemented as body builders in crates/asperitas-logging/src/console.rs beside status_body, each pinned by a host unit test in that file, and committed through one public whole-record entry point modelled on the existing emit path. Nothing in the dump or status path calls usb::emit_blocking.
- [ ] #8 The dump writer obtains permission to enqueue from dump::try_emit_dump, which consults the TASK-038.02 capacity predicate, and never bypasses it; refusals are retried on a Timer backoff rather than a busy-wait, and both the refusal count and the longest consecutive stall are counted and reported. Ordinary log and STATUS traffic stays lossless during a dump.
- [ ] #9 Capture start and end are decided by the device: it captures for a build-time-configured window or until the ring reports full, then begins the dump on its own, because runtime control over the console link belongs to TASK-032.
- [ ] #10 When a dump finishes the device emits one DUMPEND record naming blocks, chunks, bytes, elapsed milliseconds and the transport loss counters at that moment, so a caller times and validates a transfer from the captured stream alone.
- [ ] #11 The SDRAM memory model is recorded where a future reader will hit it: init() returns 0xC000_0000 while the driver programs its cacheable MPU region at 0xD000_0000, caches are enabled nowhere in the stack so accesses are uncached and coherent today, nothing here enables caches or changes the MPU base, the device reports the I-cache and D-cache enable bits it actually observes at boot so the claim is measured rather than asserted, and TASK-038.05 receives a rule rather than an open question: caches stay off until someone owns the coherence argument for the FMC window and revisits the capture hand-off ordering in the same change. A short factual note is added to docs/reference/daisy-seed3.md section 4.
- [ ] #12 Rate arithmetic is gated by const asserts in rig.rs, not prose: the capture window provably fits the ring, and CAPSTAT traffic is provably below one percent of the dumps own record traffic.
- [ ] #13 cargo fmt --all --check, cargo test --workspace, cargo clippy --workspace --all-targets -- -D warnings, the defmt-only whole-package build and each non-default stimulus variant build all pass, and CI gains a firmware clippy step (cargo clippy --release --features seed3 -- -D warnings inside firmware/, which passes as of planning; --all-targets cannot work there because a no_std target has no test crate) since firmware is excluded from the root workspace and nothing else lints rig.rs. The finalization notes record rig's measured text and .bss from size -B against main's measured baseline (text 88181, data 1428, bss 8224, i.e. 1.57 percent of the 512 KiB AXI SRAM), and state that this ticket's original 86.13 percent .bss premise had no recorded provenance and contradicts measurement.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
**Read "Plan corrections after TASK-038.03.02.01 landed" in Implementation Notes at the bottom of this file first.** It is dated 2026-09-12 and supersedes §0-§11 wherever they disagree; the corrections are numbered C1-C9 and each names what it replaces.

## What this ticket actually is now

Two leaves carry the implementation; this umbrella carries the corrections below (which stay the authority for both), the integration check, and the mapping from its thirteen acceptance criteria onto work someone else did.

| AC | Leaf |
| --- | --- |
| #7 - verbs as builders in `console.rs`, pinned by host tests, one whole-record emit path | `.01` (Done) |
| #2 interrupt executor and SAI1 priority, #5's DWT bring-up and clock question, #3's stimulus features and `RIGCFG`/`RIGGEN`, #12's transport-less shim, #14's `spin_budget.rs` sentence | `.03` |
| #4 capture producer and ring geometry, #5's `CAPSTAT` durations, #6 `CAPMAX`, #7's dump writer and incremental CRC, #8 device-decided start and end, #9 `DUMPEND`, #10's rule-in-comment, #11 rate gates, #13 CI and sizes | `.04` |
| #11's `pub const CAPSTAT_MAX_BODY` and its saturated-render bound | `.01` (the crate that renders the record owns the number `.02` divides by) |
| #10's note in `docs/reference/daisy-seed3.md` | **TASK-038.06**, not `.04`; see correction C5 - that file has no FMC/MPU section to add it to |

The umbrella's own work, once both leaves are Done: run §11's ladder over the joined tree — `cargo fmt --all --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, then inside `firmware/` the default build, both stimulus variants, the defmt-only build, and firmware clippy — plus `size -B` for `rig` against `main`, and paste the outputs into Final Summary. Builds are incremental after the first; expect well under the phase budget, but ping every 10 minutes regardless.

Do not re-plan either leaf from here. If one of them turns out wrong, fix its ticket, not this prose.

## Read before writing anything

Authoritative sources, on this machine, in this order:

- **Upstream example at the commit this build pins** — `~/.cargo/git/checkouts/daisy-embassy-4e2531dd3689e74c/ca9bcc9/examples/looper.rs`:
  lines **27-31** are the executor declaration plus its `SAI1` handler, **44-51** the SDRAM carve, **63-92** the callback
  registration, **132-134** the priority/start/spawn sequence. There is **no crates.io `daisy-embassy-0.0.2` tree here**;
  anything quoting `init_interrupts()` or `Priority::P1` describes a different version and is wrong.
  `firmware/Cargo.lock:228-230` resolves `0.2.3` at `ca9bcc9`, and `sdram.rs:9` there reads `SDRAM_SIZE = 64 MiB`.
- **`~/.cargo/registry/src/*/embassy-executor-0.10.0/`** — `src/platform/cortex_m.rs` (the `InterruptExecutor` this build
  actually links, including the `start()` doc-comment contract at 176-198) and `src/spawner.rs` (spawn tokens, `SpawnError`).
- **`~/.cargo/registry/src/*/embassy-stm32-0.6.0/`** — `src/lib.rs:360-366` (`Config::default()` DMA IRQ priorities),
  `src/dma/dma_bdma.rs:436-455` (where those priorities are applied), `src/sai/mod.rs:560,579,609` (what SAI binds: DMA lines only).
- **`crates/asperitas-logging/src/spin_budget.rs:70-104`** — the DWT bring-up that has already been proven on this exact part.
- `docs/reference/daisy-seed3.md` line 16 (**STM32H750IB, Cortex-M7F @ 480 MHz — single core**), §4 FMC/MPU, §5 pin banks.
- `firmware/src/bin/main.rs` for the boilerplate every binary duplicates (tracked by TASK-010 — copy the shape, do not
  refactor five binaries as a side quest); `podtest.rs:126-131` for the const rate-gate idiom.
- `crates/asperitas-logging/src/{capture.rs,console.rs,dump.rs,frame.rs,lib.rs}` for ring geometry, verb convention and commit path.

### Five claims from the earlier draft of this plan that were wrong, corrected here

Do not re-import them from older ticket prose or from any C++/libDaisy memory:

1. **`InterruptExecutor` is not generic.** `embassy-executor` 0.10 declares `pub struct InterruptExecutor` with **no
   const-generic parameter**: `InterruptExecutor<1>` does not compile. Upstream's own line 27 is the bare form.
2. **There are no `max-tasks-*` features.** They were removed in 0.8. Pool sizing in 0.10 is
   `#[task(pool_size = N)]`, and exhaustion surfaces where the token is made, not at `spawn`: a task fn returns
   `Result<SpawnToken<S>, SpawnError>` (`spawner.rs:60-66`), which is why upstream writes
   `spawner.spawn(defmt::unwrap!(run_audio(...)))`.
3. **AC #2's open question resolves by default.** `Config::default()` sets `dma_interrupt_priority: Priority::P0`
   (`embassy-stm32 src/lib.rs:362`) and `hal::init` applies it to `DMA1_CH0`/`CH1` (`dma_bdma.rs:443`), so the audio DMA
   completion ISR outranks the P6 executor. That is the direction we want — see §4.
4. **Seed3 is not dual-core.** STM32H750IB is a single-core M7. There is no "second core fused off" case, and no CPACR
   trick to write. The correct bring-up is the four-step sequence in `spin_budget.rs:80-97`; the correct capability probe
   is `DWT::has_cycle_counter()`, and the counter is read with `cycle_count()` — `get_cycle_count` has been deprecated
   since cortex-m 0.7.4, so using it would fail AC #13's `-D warnings`.
5. **The "86.13 % `.bss` / ≈69 KB free" baseline does not exist.** Measured on the checked-in release ELFs with
   `size -B firmware/target/thumbv7em-none-eabihf/release/<bin>`: `main` = text 88181 / data 1428 / **bss 8224**, i.e.
   1.57 % of the 512 KiB AXI SRAM in `firmware/memory.x`, with ~503 KB free. The figure appears nowhere except planning
   prose (introduced by commit `9ba7f1b`) with no command or size output behind it. Budget against the measured numbers in §11.

Two smaller ones: `new_daisy_board!` has exactly one form, `new_daisy_board!(p)` (`ca9bcc9/src/lib.rs:292-389`) — there
is **no `Type::Pod` variant** in this crate, so Pod identity comes from which pins you use afterwards, exactly as
`main.rs:191` and `podtest.rs:153` do it. And `embassy-stm32` 0.6.0's `rcc` exposes only `frequency::<T: RccPeripheral>()`
(`src/rcc/mod.rs:589`): **there is no public CPU-clock accessor**, so §3 derives cycles-per-microsecond by calibration
instead of reading a number that isn't there.

## 0. Facts already established (do not rediscover)

- **Ring geometry** — `capture.rs`: `RING_BLOCK_BYTES` 32 768 (:105), `RING_BLOCKS` 1 024 (:112), `RING_BYTES`
  33 554 432 (:118), `CALLBACK_BYTES` 64 (:87), `FRAMES_PER_CALLBACK` 32 (:84), `BYTES_PER_SECOND` 96 000 (:121),
  `callbacks_per_block()` 512 (:136), `full_chunks_per_block()` 254 (:148), `tail_chunk_bytes()` 2 (:155),
  `chunks_per_block()` 255 (:161), `records_per_block()` 256 (:167), `wire_bytes_per_block()` 57 788 (:202),
  `ring_duration_micros()` 349 525 333 (:235), `expected_blocks(seconds: usize)` rounds **up** — 879 for 300 s (:249).
  `RING_BLOCK_BYTES ≤ dump::MAX_BLOCK_BYTES` and `chunks_per_block() ≤ MAX_CHUNKS_PER_BLOCK` are already asserted there,
  the latter with **zero slack**.
- **Block hand-off** — `capture::BlockState{Free,Filling,Full,Dumping}` (:330) stored as `u8`,
  `from_u8 -> Option` (:369, deliberately never defaults to `Free`), `transition_ok(from,to)` (:390) with exactly four
  legal edges and no self-transitions. `AtomicU8` is lock-free on ARMv7-M, so `portable-atomic` stays out.
- **Dump path** — `dump::CHUNK_RAW` 129 (:435), `MAX_BODY` 200 and `MAX_FRAME` 228 (`frame.rs:99,108`),
  `audio_body(block_index: u32, chunks: u16, chunk_index: u16, raw: &[u8], out: &mut [u8; MAX_BODY]) -> Result<usize, BodyError>`
  (:596), `audend_body(block_index: u32, chunks: u16, total_bytes: u32, crc: u16, out)` (:650) where **the caller computes**
  `crc16_ccitt` over the raw concatenated block bytes in chunk order (`dump.rs:79,646`), and
  `try_emit_dump(body: &[u8]) -> bool` (`lib.rs:570`), the only route dump bytes take into `LOG_PIPE` and the only place
  `dump_fits` is acted on.
- **Console path** — `console::boot_body` (:180) / `status_body` (:206) render into a caller-owned
  `&mut [u8; BODY_WINDOW]` through the crate-private `TruncWriter` and return `w.filled()`; the pinning test is
  `status_body_pins_field_names_and_order` (:317). Loss counters: `console::CONSOLE.snapshot() -> ConsoleCounters` with
  public `u32` fields `records_sent, dropped_full, bytes_dropped, truncated, endpoint_errors, seq_next` (:58-71, :146).
- **`console`, `dump`, `capture`, `frame` are ungated modules** (`lib.rs:201-207`); only the commit path (`lib.rs:264+`)
  and `usb` are behind `log-usb`. So new body builders and their tests compile and run under plain `cargo test --workspace`.
- **One lock, one commit function.** `tests/commit_path_no_panic.rs` is a source-text guard requiring exactly **one**
  `RECORD_BUFS.lock(` call site and ≥2 `commit_records(` call sites. A new emitter that takes its own lock fails that suite.
- **CI builds the whole firmware package** (never `--bin`), in two configs: `cargo build --release --features seed3` and
  `cargo build --release --no-default-features --features "seed3 log-defmt"`. `lefthook.yml`'s pre-push hook runs the first.
- **`firmware/` is excluded from the root workspace** (`Cargo.toml:3`), so `cargo clippy --workspace` never sees `rig.rs`.
  Measured today: `cd firmware && cargo clippy --release --features seed3 -- -D warnings` is clean. (`--all-targets`
  cannot work there — it builds test targets, and a no_std target has no `test` crate: E0463.)
- **Nothing calls `cortex_m::Peripherals::take()` anywhere in the repo.** `spin_budget.rs:87` uses `steal()` with a
  doc-comment justifying it by that absence.

## 1. Cargo and CI

`firmware/Cargo.toml` — add `executor-interrupt` to the existing dependency, keep everything else:

```toml
embassy-executor = { version = "0.10.0",
  features = ["platform-cortex-m", "executor-thread", "executor-interrupt"] }

[features]
stim-sine  = []   # default when nothing else is selected
stim-ess   = []
stim-pulse = []
```

No `[[bin]]` section: `rig.rs` is auto-discovered, and both CI steps therefore pick it up automatically. Guard against two
`stim-*` features at once with a `const _: ()` assert **in the source** — cargo cannot express mutual exclusion.

`.github/workflows/ci.yml` — two additions to the existing single `check` step:

```bash
echo "=== rig stimulus variants ==="
cd firmware
cargo build --release --features seed3,stim-ess  --bin rig
cargo build --release --features seed3,stim-pulse --bin rig

echo "=== firmware clippy (rig.rs is outside the workspace) ==="
cargo clippy --release --features seed3 -- -D warnings
```

Use `--bin rig` for the stimulus variants: the default-config whole-package build already covers sine + every other
binary, and three full package rebuilds would roughly triple that step's wall time for no extra coverage (see TASK-052's
interest in CI wall time). The clippy line is the only thing that can lint `rig.rs` at all, and it passes as of this
planning pass; do not add `--all-targets` to it.

## 2. Record verbs belong to `console.rs`, not to the binary

`RIGCFG`, `CAPSTAT`, `CAPMAX` and `DUMPEND` are wire grammar. The convention at `console.rs:171-178` is that a verb's
field set and order are a contract **pinned by a unit test in the same file**; bodies written ad hoc in `rig.rs` would have
no host test at all, because the firmware package has no host-test target.

Add five builders beside `status_body`, each following its shape exactly (`TruncWriter` + `core::write!`, `proto=1` second,
all fields in one format string, returning `w.filled()`), each with a pinning test naming every field in order, plus one
saturated-counters test per builder asserting the worst-case render is `< frame::MAX_BODY` — model it on
`status_body_renders_saturated_counters_as_u32_max` (:327), which exists because a 255-byte body silently truncating at
200 is the failure nobody notices until a host parser rejects it.

**The grammar below was corrected on 2026-09-12 while planning `.01`.** The draft carried thirteen `CAPSTAT` fields and
put the generator's free text inside `RIGCFG`; both are arithmetically impossible against `frame::MAX_BODY` = 200, as the
table in Implementation Notes shows (saturated `CAPSTAT` = 284 bytes, `RIGCFG` carrying `describe()` ≈ 291). Every field
below still earns its bytes, and each verb's worst case is now provably inside one record:

```
RIGCFG  proto=1 capture=mono16 lane=<L|R> blocks=<n> block_bytes=<n> bytes_per_s=<n> capsec_us=<n>
        window_s=<n> cpu_hz=<hz> icache=<0|1> dcache=<0|1>                              177 B worst case
RIGGEN  proto=1 <generators describe() output verbatim, <= console::RIGGEN_MAX_GEN_BYTES>   <= 175 B
CAPSTAT proto=1 delivered=<n> expected=<n> overrun=<n> max_block_us=<n> worst_gap_us=<n>
        audio_exit=<n> dumped=<n> dropped_full=<n>                                      187 B worst case
CAPMAX  proto=1 total_bytes=<n> ring_bytes=<n> seconds_max=<n> us_max=<n> unused_headroom_bytes=<n>  133 B
DUMPEND proto=1 blocks=<n> chunks=<n> bytes=<n> elapsed_ms=<n> refused=<n> stall_ms=<n>
        sent=<n> dropped_full=<n> bytes_dropped=<n>                                     194 B worst case
```

Four decisions inside the corrected grammar:

- **The generator's text gets its own verb.** `RIGCFG` kept every numeric field the draft gave it (177 B, provable from
  the field-name table alone) and lost only the free text, because a body mixing one unbounded string with ten numbers
  has no checkable bound. `RIGGEN proto=1 <describe>` carries the same bytes, last in the record, so clipping shows up as
  a missing field rather than a mangled number. `console::RIGGEN_MAX_GEN_BYTES` names the budget (160 B, so `RIGGEN` tops
  out at 175); `asperitas-dsp`'s own `describe_respects_the_frame_budget_and_truncates_rather_than_panicking` measures
  the real thing at 83 / 92 / 97 bytes for the three defaults and ~116 for absurd-but-representable parameters, so the
  budget is not a guess and `rig.rs` debug-asserts the length `describe()` actually returned.
- **One fact, one place.** `sent` and `bytes_dropped` left `CAPSTAT`: `STATUS` already carries them, with `seq_next` for
  bracketing an interval, so repeating them in every status record duplicated a number instead of reporting it.
  `free` left too - it is `RING_BLOCKS − (delivered − dumped)` minus the block being filled, all published constants or
  fields already present. `refused` and `stall_ms` moved to `DUMPEND`, which is the record about one dump.
- **`dumped` replaces `dumping`.** A count of blocks whose `AUDEND` has been committed is progress; a block index is not,
  and it pairs with `DUMPEND`'s final `blocks`.
- **No second sample-rate field.** `Stimulus::describe(&self, out: &mut [u8]) -> usize`
  (`crates/asperitas-dsp/src/stimulus.rs:122`) already renders `name=… sample_rate_hz=… level_dbfs=…`; re-emitting a
  competing `rate=` would create two sources of truth, which is what AC #3 forbids. Everything else rig knows about the
  signal (capture format, block geometry, window, clock) rides alongside it, in `RIGCFG`.
- **`cpu_hz` is measured, not declared** (§3), and `icache`/`dcache` come from `SCB::icache_enabled()` /
  `SCB::dcache_enabled()` (`cortex-m-0.7.7 src/peripheral/scb.rs:376,446` — plain associated functions, no `&mut`).
  That turns AC #11's "caches are enabled nowhere" from prose into a number on the wire, which is exactly the evidence
  TASK-038.05 AC #6 asks for.
- **`audio_exit` is its own counter.** `start_interface()` returns `Result<_, sai::Error>` and `start_callback()` returns
  `Result<Infallible, sai::Error>`; if either ever returns, capture stops and `delivered` simply freezes, which looks
  identical to starvation in the stream. Store 0 = running, 1 = `start_interface` failed, 2 = `start_callback` failed.
  Starvation and death must not share a symptom.

Then expose **one** public whole-record entry point in `lib.rs`, gated `#[cfg(feature = "log-usb")]` like
`try_emit_dump`:

```rust
pub fn emit_record(level: Level, now_ms: u32, body: &[u8]) -> bool
```

It must go through the existing `commit_records` (`lib.rs:368`) — take `RECORD_BUFS` there, `CONSOLE.take_seq()`,
`frame::encode`, `frame::write_whole(framed, LOG_PIPE.free_capacity(), …)`, count the outcome — because
`tests/commit_path_no_panic.rs` counts lock sites. Model it on the private `emit_status` (`lib.rs:503`) but take
`now_ms` as an argument: `Instant::now()` must be read **outside** the lock, the way `lib.rs:421` and `lib.rs:571` do.
Callers pass `Level::Info`, matching BOOT/STATUS: these are device facts, not diagnostics, and a Debug filter should not
silence a measurement.

**Nothing here may call `usb::emit_blocking`.** It bypasses `LOG_PIPE` and drives the endpoint directly while
`usb::run()`'s drain task owns it; it exists for the case where the executor is dead (TASK-046/TASK-047 are the failure
class produced by long-lived work in masked contexts).

## 3. Boot sequence and peripheral claims

Order follows `main.rs:189-232`, with two things inserted ahead of it.

**Claim the cortex-m singleton first, in `rig.rs`, before anything else:**

```rust
let mut cp = cortex_m::Peripherals::take().expect("cortex_m::Peripherals already claimed");
```

Today nothing claims it (§0), so this is the *first* claimer, and `take()` returning `None` is the only symptom a future
collision will produce — hence the explicit message and a comment saying that any binary claiming it earlier breaks
`rig.rs` at boot, not at compile time. Destructure rather than move the whole struct, because `sdram.build()` wants
`&mut cp.MPU, &mut cp.SCB` while DWT and SCB stay useful:

```rust
let sdram = board.sdram.build(&mut cp.MPU, &mut cp.SCB);   // ca9bcc9/src/sdram.rs:16
```

While here, `spin_budget.rs:81-86` justifies its `steal()` by "no `cortex_m::Peripherals::take()` exists in this crate, in
daisy-embassy, or in embassy-stm32". After this ticket that sentence is stale. Update it to say `rig.rs` claims the
singleton and `steal()` remains correct because neither user resets CYCCNT — see the next paragraph for why that matters.

**DWT: copy `spin_budget.rs:80-97` verbatim** rather than inventing a bring-up. It is the sequence already proven on this
part and it handles the trap the old draft missed: H7 software-locks the DWT after power-on, so without
`DWT::unlock()` (LAR ← `0xC5ACCE55`) every CYCCNT read is 0 and every derived microsecond is a lie.

```rust
cp.DCB.enable_trace();                        // DEMCR.TRCENA: CM7 may ignore CYCCNTENA without it
cortex_m::peripheral::DWT::unlock();          // LAR: H7 locks the DWT after power-on
let dwt_ok = cortex_m::peripheral::DWT::has_cycle_counter()   // CTRL.NOCYCCNT
    && { cp.DWT.enable_cycle_counter();
         cortex_m::peripheral::DWT::cycle_counter_enabled() }; // readback is the liveness proof
```

Read with `cortex_m::peripheral::DWT::cycle_count()` (associated fn; `get_cycle_count` is deprecated → `-D warnings`).
**Nobody may write CYCCNT** — no `set_cycle_count(0)`, ever: `spin_budget` reads the same counter for its spin budget, and
both users rely on unsigned wrapping deltas, which stay correct across a wrap (≈8.95 s at 480 MHz) and stop being correct
the moment someone resets it underneath the other.

If `dwt_ok` is false, report `cpu_hz=0` in `RIGCFG` and zeros in every `*_us` field, with a comment saying zero means "not
a measurement", never "measured zero".

**Deriving microseconds without a CPU-clock accessor.** `embassy-stm32` 0.6.0 exposes only
`rcc::frequency::<T: RccPeripheral>()` — kernel clocks of peripherals, not the core (AHB is ÷2 from SYSCLK anyway, so
substituting an `hclk` figure would be wrong by half). Calibrate instead, in thread mode at boot, before starting the
audio executor:

```rust
let c0 = DWT::cycle_count();
let t0 = embassy_time::Instant::now();
Timer::after_millis(20).await;                 // << the 65 ms wrap of the 1 MHz tick counter
let cycles = DWT::cycle_count().wrapping_sub(c0) as u64;
let micros = t0.elapsed().as_micros() as u64;  // embassy-time tick = 1 MHz => +-1 count, ~0.005 %
let cycles_per_us = cycles * 1_000_000 / micros;
```

Publish `cpu_hz = cycles_per_us * 1_000_000` in `RIGCFG`. Two independent references agree on the result at the bench:
calibration says ≈480, and one filled block must then read ≈341 333 µs against `capture::ring_duration_micros()`'s
per-block share. Say so in the comment; do not hardcode 480 MHz, and never hardcode 600 MHz.

Then, following `main.rs:189-232`: `default_rcc()` → `hal::init` → `new_daisy_board!(p)` (**single-argument form**) →
drop `board.flash` and discard `board.usb_peripherals` unread (`main.rs:196` — `usb::init` steals `USB_OTG_FS`/PA11/PA12)
→ `asperitas_logging::led::init(board.pins.d20, board.pins.d19, board.pins.d18)` → `usb::init(UsbIrqs)` → spawn drain,
blink, CAPSTAT and dump tasks → `board.audio_peripherals.prepare_interface(Default::default()).await`.

Do **not** annotate the board as `DaisyBoard<'_>` the way `main.rs:191` does: the audio task needs
`Interface<'static, Idle>`, and `looper.rs` proves that inference gives 'static when the peripherals come straight from
`hal::init`. If the borrow checker disagrees, destructure `p`/`board` into separate bindings immediately rather than
storing the board, and reach for the `StaticCell` idiom already used at `main.rs:104,217` before reaching for `Box`.

## 4. Executor topology

Copy `looper.rs` in shape, with the real 0.10 API:

```rust
// looper.rs:27-31 at ca9bcc9 — note: NOT InterruptExecutor<1>. In embassy-executor 0.10 the
// type has no const-generic parameter; the IRQ is chosen at start(), not in the type.
static AUDIO_EXECUTOR: InterruptExecutor = InterruptExecutor::new();

#[interrupt]
unsafe fn SAI1() {
    // Safety: called only from this handler, and only after AUDIO_EXECUTOR.start() below
    // (cortex_m.rs:167-173). An unexpected SAI1 event before start() would land in
    // cortex-m-rt's DEFAULT_HANDLER infinite loop instead — which is what every SAI1 event
    // did before this file installed a handler here.
    unsafe { AUDIO_EXECUTOR.on_interrupt() }
}
```

```rust
// looper.rs:132-134. start() unmasks the IRQ itself (cortex_m.rs:212) and the docs are explicit:
// set the priority BEFORE start(), never after.
embassy_stm32::interrupt::SAI1.set_priority(Priority::P6);
let audio_spawner = AUDIO_EXECUTOR.start(embassy_stm32::interrupt::SAI1);
audio_spawner.spawn(run_audio(interface, sdram, ring).expect("audio task pool"));
```

Record in the comment, with numbers, why this is safe and why it is shaped this way:

- **The `SAI1` vector is genuinely ours.** `daisy-embassy` binds `DMA1_CH0`/`DMA1_CH1` only
  (`ca9bcc9/src/audio.rs:26-29`) and `embassy-stm32`'s SAI module binds only DMA line interrupts
  (`src/sai/mod.rs:560,579,609`), so no `SAI1` handler exists in the graph to collide with.
- **DMA outranks the executor, and must.** `Config::default()` ships `dma_interrupt_priority: Priority::P0`
  (`embassy-stm32 src/lib.rs:362`), applied to `DMA1_CH0`/`CH1` during `hal::init` (`dma_bdma.rs:443`), while the audio
  executor sits at P6. That ordering is load-bearing in this direction: the DMA completion ISR is what wakes the audio
  task, and the pender works by `NVIC.request(SAI1)` (`cortex_m.rs:29-36`) — an executor that could mask its own wake
  source would stall the very thing it exists to service. Note the mirror image: USB OTG_FS and every other unmasked IRQ
  keeps the NVIC reset priority 0, so they too outrank P6. The callback's latency bound is other ISRs plus PRIMASK
  sections (§4 last bullet), not the thread-mode dump writer.
- **`start()` hands back a `SendSpawner`** (`cortex_m.rs:199`), so `spawn` needs `SpawnToken<S>` with `S: Send`
  (`spawner.rs:223`) — the audio future must be `Send`, which upstream's `run_audio(Interface<'static, Idle>, Sdram<..>)`
  demonstrates. Keep the task's captured state to `'static` things: the interface, the `SdRam`, and a `'static` slice of
  the ring.
- **Pool sizing.** Exhaustion appears as `Err(SpawnError::Busy)` from the task *call*, not from `spawn`. If it ever
  fires, widen with `#[embassy_executor::task(pool_size = N)]` on that task and note the RAM cost. Do not look for
  `max-tasks-*` features; they are gone.

**The audio callback must never touch the console, and the reason is PRIMASK, not priority.** `RECORD_BUFS` is a
`CriticalSectionRawMutex` (`lib.rs:283`), which on this target masks **all** interrupts regardless of NVIC priority. Two
directions, both worth writing down:

- Thread mode holding the lock masks SAI1 *and* the DMA IRQs for the hold duration — one ≤256 B copy plus CRC plus pipe
  write, i.e. bounded latency for audio, not starvation. Saying "P6 makes logging unable to starve audio" is false as
  reasoning; the true invariant is that the hold time is short and constant.
- A producer at P6 that took the lock while thread mode held it would deadlock outright: it spins at P6, thread mode
  never resumes, the lock is never released.

So the callback's entire outward surface is SDRAM writes and atomic stores, and `CAPSTAT` is emitted from the thread
executor by reading atomics.

## 5. Stimulus selection

Output is the stimulus alone; the input frame is ignored. Keep the render site shaped like
`stimulus → [processor slot] → encode_block(output)`, because TASK-019.03 inserts the effect chain there and should need
one line, not a restructure. All three generators implement `Processor`, whose `tick` ignores input, and
`set_sample_rate(&mut self, hz: f32)` (`stimulus.rs:286`) — call it once with `48_000.0` before audio starts.

- `Sine::default()` is already −20 dBFS at 1 kHz (`:261-275` ends with `apply(&SineParams::default())`, and
  `SineParams::default()` is `{ level_dbfs: -20.0, frequency_hz: 1_000 }`). Do not "correct" the level.
- `ExponentialSweep::default()` costs one 384 000-sample peak-normalising scan at construction (`:449-470`, and
  `apply()` scans every sample — cheap at boot, catastrophic in a callback). Construct it in setup, never in the callback.
- `PulseTrain::default()` is one band-limited pulse per period; the sweep's record is ≈8 s at default parameters, so the
  window that covers a whole generator differs per build — `RIGCFG`'s `window_s` and `describe()` make that visible to the
  caller instead of leaving them to guess.
- Truncate to the mono lane with `(sample.clamp(-1.0, 1.0) * 32767.0) as i16`. No dither, deliberately (TASK-038.01 left
  it out until the noise floor is characterised) — say so in the comment.

## 6. Capture producer (runs at P6)

Carve the ring exactly as `looper.rs:44-51` does: `sdram.init(&mut delay)` returns the bank base, cast to `*mut u8`, then
`slice::from_raw_parts_mut` over `capture::RING_BYTES`. No linker-script change — `memory.x` keeps claiming FLASH 128 K
and RAM 512 K only. **Keep the `SdRam` value alive for the program's lifetime** by owning it in the audio task: dropping
it releases ~55 pins at the type level and lets someone claim them twice.

```rust
static BLOCK_STATE: [AtomicU8; capture::RING_BLOCKS] = ...;      // 1 KiB of .bss
static WRITE_BLOCK: AtomicUsize;   // producer cursor
static PRODUCED:    AtomicUsize;   // published high-water mark, in blocks since boot
static OVERRUN:     AtomicUsize;
static MAX_BLOCK_CYCLES: AtomicU32;
static WORST_GAP_CYCLES: AtomicU32;
static AUDIO_EXIT: AtomicU32;
```

Per callback, bounded to one contiguous copy plus lane truncation (AC #5):

1. `state = BLOCK_STATE[produced_or_write_index % RING_BLOCKS].load(Acquire)`; anything other than `Free` ⇒
   `OVERRUN += 1`, **stop capturing** (leave the block alone; never overwrite one being dumped), return.
2. Store `Filling` (assert the edge with `capture::transition_ok` in a debug build; the table is the authority, not a
   remembered edge list).
3. Truncate the 64 interleaved `u32` lanes to 32 `i16` samples of **one** channel and copy them into the block's byte
   range at `callback_index_in_block * CALLBACK_BYTES`. Take the left lane (`input[2 * i]`, the even words, matching
   `main.rs:110-115`'s decode) and put the choice in one `const MONO_LANE` with a comment: which physical jack the TASK-034
   cable actually loops is TASK-038.05's observation, and flipping one constant is the fix if it is the other one.
4. On the 512th callback: `fence(Release)`, store `Full`, advance the cursor, `PRODUCED.fetch_add(1, AcqRel)`.

Ordering discipline, stated in code and following the classic SPSC rule (write the payload, publish it with release
semantics, consumer acquires before reading — `rtrb`'s `RingBuffer` is the modern Rust reference if a reader wants a
worked example): samples are written before the index that publishes them. Use `core::sync::atomic` here, **not**
`main.rs:63-101`'s `UnsafeCell` + `interrupt::free` idiom, which is adequate for one slow knob writer and one fast reader
and wrong for publishing buffer ownership between an interrupt-mode producer and a thread-mode consumer.

Close the loop on the copied driver constants, comparing **samples**:

```rust
const _: () = assert!(asperitas_logging::capture::FRAMES_PER_CALLBACK
                      == daisy_embassy::audio::BLOCK_LENGTH);   // 32 == 32 frames
```

`CALLBACK_BYTES` (64 **bytes** of one mono channel) and `HALF_DMA_BUFFER_LENGTH` (64 **`u32` words** per callback) are
different quantities that happen to print alike: asserting them equal passes by coincidence, and comparing either against
`HALF_DMA_BUFFER_LENGTH * 2` cannot compile at all. `capture::SAMPLE_RATE_HZ` is likewise a deliberate copy of
`AudioConfig::default` (`ca9bcc9/src/audio.rs:220-224`), which is not a `const` and so cannot be named in an assert — say
that in a comment and note that `RIGCFG` carries `sample_rate_hz` from `describe()`, making a mismatch visible on the wire
rather than silent.

Capture window: `const CAPTURE_SECONDS: usize = 300;`, overridable with `option_env!("ASP_RIG_CAPTURE_SECONDS")` so
TASK-038.05 can shorten bench runs without editing source. Capture ends at the window deadline **or** when the ring
reports full, whichever comes first, and the dump then begins on the device's own decision (AC #9 — runtime control
belongs to TASK-032).

## 7. Dump writer (thread executor)

Walk blocks in ring order from 0 to `PRODUCED.load()`, indexing `block % capture::RING_BLOCKS`. §8's gate proves the
intended window never wraps; the modulo stays because "provably never happens" is not how a ring survives a future edit.

1. `compare_exchange(Full, Dumping, AcqAcq, Relaxed)` — failure means the block is not ours; continue.
2. For `c in 0..capture::chunks_per_block()`: slice `chunk_raw = min(dump::CHUNK_RAW, remaining)`, build with
   `dump::audio_body(block_index as u32, chunks as u16, c as u16, raw, &mut body)`, then retry
   `asperitas_logging::try_emit_dump(&body[..len])` until true, awaiting `Timer::after_millis(1)` between refusals. Count
   refusals (`refused`) and track the longest consecutive streak in milliseconds (`stall_ms`) for `DUMPEND`, which is the
   record about one dump. Never bypass
   `dump_fits`: that predicate (`dump.rs:539`) is what keeps ordinary log and STATUS traffic lossless during a dump
   (AC #8), and `tests/console_dump.rs`'s `log_records_survive_a_saturated_dump` is the behaviour being protected.
3. Emit `AUDEND` with `total_bytes = capture::RING_BLOCK_BYTES` and
   `crc = frame::crc16_ccitt(&block_bytes[..])` — over the raw concatenated bytes in chunk order, never the base64 text
   (`dump.rs:79`).
4. `store(Free, Release)`.

Yielding at the `Timer` await is what keeps the USB drain running; a busy-wait starves the very task that frees pipe
capacity. Packetisation is not this writer's problem: `usb.rs:290-321`'s drain loop owns the 64-byte short-packet rule and
the ZLP, and `LOG_PIPE` is the only thing rig writes to.

At the end emit one `DUMPEND` with blocks, chunks, bytes, elapsed ms, `refused`, `stall_ms` and `CONSOLE.snapshot()`'s
 counters at that instant (AC #10), so a caller can time and validate a transfer from the captured stream alone.

Dump wall time is a prediction, not a fact, and the arithmetic belongs in the comment: a full ring is
`capture::wire_bytes_per_block() × RING_BLOCKS` ≈ 59.2 MB of wire traffic; keeping pace with real-time capture needs
96 000 B/s raw ⇒ ≈169 kB/s of wire, and a full-speed bulk class ceiling in the 0.8–1.0 MB/s range would empty the ring in
roughly 60–75 s. Those ceilings come from USB FS bulk theory, **not** from anything measured in this repo — TASK-038.05
AC #4 exists to replace them with the first real number, which is why `DUMPEND` carries `elapsed_ms`.

## 8. Rate gates, as arithmetic not prose

`podtest.rs:126-131` proves its logging rate fits the link with a `const` assert. Do the equivalent in `rig.rs`, where
both the driver constants and `capture::` are visible, using `.is_multiple_of` style wherever lints apply:

```rust
// The window provably fits the ring (879 < 1024 for 300 s).
const _: () = assert!(capture::expected_blocks(CAPTURE_SECONDS) < capture::RING_BLOCKS);

// CAPSTAT can never approach one percent of the dump's own traffic. Over the time one block
// takes to fill, at most this many CAPSTAT records can occur; compare their worst-case bytes
// against the wire bytes that same block generates. Both sides come from published constants.
const BLOCK_FILL_MS: usize =
    capture::callbacks_per_block() * capture::FRAMES_PER_CALLBACK * 1000
        / capture::SAMPLE_RATE_HZ as usize;                  // 341
const CAPSTAT_MAX_PER_BLOCK: usize = BLOCK_FILL_MS / CAPSTAT_PERIOD_MS + 2;  // allow one phase slip
const _: () = assert!(CAPSTAT_MAX_PER_BLOCK * console::CAPSTAT_MAX_BODY * 100
                      < capture::wire_bytes_per_block());    // 2*200*100 < 57_788
```

Define `pub const CAPSTAT_MAX_BODY: usize = 200;` in `console.rs` next to the builder, with the saturated-render test
asserting both `len < CAPSTAT_MAX_BODY` and `CAPSTAT_MAX_BODY <= frame::MAX_BODY`, so `rig.rs` gates on a number the crate
owns and a host test checks. The gate is relative on purpose: the link ceiling is unmeasured, so the honest claim is that
status traffic cannot dominate the dump, not an invented baud figure. `RING_BLOCK_BYTES % CALLBACK_BYTES == 0` is already
asserted in `capture.rs:262-311` — do not duplicate it.

## 9. SDRAM memory model — record it, do not fix it

`sdram.init()` returns `0xC000_0000` (bank 1's default FMC mapping per RM0433; AN4891's `0xD000_0000` wording is itself
erroneous, per ST's own forum correction), while `ca9bcc9/src/sdram.rs:33` programs the driver's single cacheable MPU
region at `0xD000_0000` — the other bank's window, where nothing is connected. Caches are enabled nowhere in
daisy-embassy, embassy-stm32 0.6.0, or the cortex-m-rt startup, so every access today is uncached and coherent by
construction, and the mis-set region is inert but misleading.

Do **not** quietly correct the base. Beyond scope, the constant lives in a third-party crate: changing it means either an
upstream PR or bypassing `SdRamBuilder::build` and configuring FMC and the MPU here — and the moment caching turns on, the
coherence argument for every master touching that window (including the capture ring's producer/consumer hand-off) becomes
real work. `RIGCFG`'s `icache`/`dcache` bits record the actual state at boot rather than asserting it from prose.

Hand TASK-038.05 a **rule**, not an open question, in the code comment: caches stay off until someone owns the coherence
argument for the FMC window, and that person must revisit `capture`'s hand-off ordering at the same time. Then add the
short factual note to `docs/reference/daisy-seed3.md` §4 near the existing FMC/MPU discussion. Budgets and workflow prose
belong to TASK-038.06, not here.

Also record in the comment: neither the driver nor `stm32-fmc` self-tests the SDRAM (`ca9bcc9/src/sdram.rs:103` drops the
handle), so the first evidence the part is alive is this ticket's first successful capture — which is why `CAPMAX` prints
before any block is trusted, and why `unused_headroom_bytes` is computed from `sdram::SDRAM_SIZE - capture::RING_BYTES`
at runtime rather than asserted in prose (AC #6), with a note that live audio DMA buffers stay in internal RAM
(`ca9bcc9/src/audio.rs:20-23`).

## 10. The transport-less build must still compile

CI's second config is `--no-default-features --features "seed3 log-defmt"`, and it builds the whole package, so `rig.rs`
must link there. `try_emit_dump` and the future `emit_record` are `log-usb`-only. Gate the console-verb and dump paths
with `#[cfg(feature = "log-usb")]` the way `main.rs:261-266` gates `console_fut`, and in the defmt-only build keep stimulus
playback plus the capture ring running with `CAPSTAT`-shaped facts going through `log::info!` (the `defmt_log` bridge),
plus one line saying plainly that this build has no dump transport. Do not fake a dump over RTT.

## 11. Verification ladder and measured baselines

Run in this order; paste the outputs into the finalization notes.

```
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd firmware
cargo build --release --features seed3                    # default = stim-sine
cargo build --release --features seed3,stim-ess   --bin rig
cargo build --release --features seed3,stim-pulse --bin rig
cargo build --release --no-default-features --features "seed3 log-defmt"
cargo clippy --release --features seed3 -- -D warnings
git diff --name-only HEAD                                 # must not list main.rs or podtest.rs
```

Size, against **measured** numbers, not the phantom 86.13 %:

```
size -B target/thumbv7em-none-eabihf/release/rig
size -B target/thumbv7em-none-eabihf/release/main     # baseline: text 88181 / data 1428 / bss 8224
```

Report `rig`'s `.text` against the 128 KiB FLASH (main sits at 67.3 %) and its `.bss` against the 512 KiB AXI SRAM
(main sits at 1.57 %, ≈503 KB free). Expected new internal-RAM cost is the 1 KiB state array plus the dump task's
`[u8; MAX_BODY]` scratch and a handful of atomics — write down the measured number, and note in the finalization that the
ticket's original "86.13 % `.bss`, ≈69 KB free" premise had no recorded provenance and contradicts measurement.

Nothing in this ticket may be marked done on the strength of a green build. Every claim needing ears, a board, a cable or
a stopwatch lives in TASK-038.05 (`@human`) and is not satisfied here: audibility per stimulus build (AC #1), the
five-minute capture with `dropped_full` unchanged and `delivered == expected` (AC #2), ring-full behaviour (AC #3), the
first real link throughput number (AC #4), and the SDRAM/cache settlement (AC #6). `RIGCFG`, `CAPSTAT`, `CAPMAX` and
`DUMPEND` exist so that each of those is read off the wire rather than re-instrumented.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Interface facts from TASK-038.03.01, now published in asperitas_logging::capture (measured there, so this ticket need not rediscover them):

- expected_blocks takes usize and rounds UP: expected_blocks(300) == 879, not the 878 that appeared in the planning prose. This ticket's `capture::expected_blocks(CAPTURE_SECONDS as usize)` snippet compiles as written; do not "fix" the cast to u32.
- Its const assert should read capture::RING_BLOCK_BYTES.is_multiple_of(capture::CALLBACK_BYTES), not `% .. == 0`: manual_is_multiple_of is denied under -D warnings wherever root workspace lints apply. firmware/ inherits none of them today, so the % form would build — it is style debt here, not a failure.
- The FRAMES_PER_CALLBACK vs daisy_embassy::audio::BLOCK_LENGTH assert compares samples (32 == 32). CALLBACK_BYTES (64 bytes of one mono channel) and HALF_DMA_BUFFER_LENGTH (64 u32 words per callback) coincide numerically and must not be compared.
- Block state hand-off lives in capture::BlockState / capture::transition_ok: exactly four legal edges (Free->Filling, Filling->Full, Full->Dumping, Dumping->Free), no self-transitions, and BlockState::from_u8 returns Option — an unrecognised status byte is None, never Free. Store these in [AtomicU8; capture::RING_BLOCKS]; AtomicU8 is lock-free on ARMv7-M (LDREXB/STREXB), so portable-atomic stays out.
- Ring geometry: `RING_BLOCK_BYTES` 32_768, `RING_BLOCKS` 1_024, `RING_BYTES` 33_554_432 (half the Seed3's 64 MB SDRAM), `BYTES_PER_SECOND` 96_000, 349 s floor / 349,525,333 us exact, 255 chunks + 1 AUDEND = 256 records per block, 57,788 wire bytes per block (exact, encoder-confirmed).

### Why this ticket was split (2026-09-11, after two failed execute attempts)

Both attempts were cut at the 40-minute execute deadline having landed **zero commits**. Neither hung: each was actively editing files when killed, exactly one budget after its last intercom ping. Attempt 1 left `frame.rs` with a syntax error uncommitted; attempt 2 spent ~25 of its 40 minutes repairing that before starting its own work, and died ~8 minutes short of ever creating `firmware/src/bin/rig.rs`. Thirteen criteria across five subsystems, spanning a host crate and a bare-metal binary, do not fit one increment.

The seam is host versus firmware: `.03.02.01` takes the crate-side verbs (parent AC #7 plus `CAPSTAT_MAX_BODY` and the incremental CRC §7 step 3 needs), `.03.02.02` takes `rig.rs` and everything cross-compiled. This umbrella keeps the plan below as their shared authority and carries only §11's integration ladder.

Salvage: `git stash list` holds `stash@{0}` ("wip-038.03.02-uncommitted", 687 insertions: `console.rs` +564 rig verbs, `frame.rs`, `lib.rs`, `spin_budget.rs`, `tests/commit_path_no_panic.rs`) from the aborted attempts. It was never reviewed or committed. `.03.02.01` dispositions of it first; whatever remains gets dropped, not archived.

### §2's grammar could not satisfy its own byte limit (2026-09-12, while planning `.01`)

`frame::MAX_BODY` is 200 and every rig verb is one record, so each verb's *worst-case* render must be under it. Rendered at `u32::MAX` (ten digits), the draft did not:

| verb as drafted | worst case | verdict |
| --- | --- | --- |
| `CAPSTAT`, thirteen fields | **284 B** | over by 84 |
| `RIGCFG` carrying `describe()` | **≈291 B** | over by ~91 |
| `CAPMAX`, five fields | 133 B | fits |
| `DUMPEND`, seven fields | 155 B | fits |

AC #3 ("exactly one RIGCFG embedding `describe()`") and AC #2/#7's saturated-render test are therefore unsatisfiable *as written*: they demand both "render §2 verbatim" and "every verb < 200 B". No field-name arithmetic closes an 84-byte gap, so §2 was corrected rather than argued around. The method is worth keeping: the model reproduces the live pinned `STATUS` literal exactly (short 100 B, saturated 155 B, consistent with its `< frame::MAX_BODY` test), which is what makes the 284 trustworthy.

Two further numbers make the free-text decision concrete. `Stimulus::describe()` for the three defaults measures 83 / 92 / 97 bytes (`stimulus_tests.rs:100-107` pins those strings), and the crate's own budget test accepts anything under 200, so absurd-but-representable parameters reach ≈116. `RIGCFG`'s ten numeric fields alone are 177 B, leaving 22 bytes, less than the *default* `pulse_train` string. That is why the generator text moved to `RIGGEN` behind a named 160-byte budget instead of squeezing `RIGCFG`.

Consequences downstream, all reflected in §2 and in `.01`/`.02`: `refused` and `stall_ms` describe one dump and moved to `DUMPEND`; `sent`, `bytes_dropped` and `free` left `CAPSTAT` (`STATUS` carries the first two; the third is `RING_BLOCKS − (delivered − dumped)` minus the block being filled); `dumping` became `dumped`, because progress is a count of completed blocks, not an index. Parent AC #5 is unchanged: `CAPSTAT` still carries dump progress and the transport's `dropped_full`.

### Plan corrections after TASK-038.03.02.01 landed (2026-09-12, while splitting `.02` again)

Every item below was verified against the sources the build actually resolves (daisy-embassy checkout `ca9bcc9`, `embassy-executor-0.10.0`, `embassy-stm32-0.6.0`, `embassy-time-queue-utils-0.3.2`, this tree as of `f0b4e18`). **Where these contradict §0-§11 above, these win.**

- **C1 - `embassy-stm32` does expose a clock accessor, so §3's premise is false.** `pub fn rcc::clocks(&Peri<RCC>) -> &Clocks` is public and ungated (`rcc/mod.rs:142`) and derefs to the generated `Freqs`, whose `.sys` the crate itself reads for exactly this purpose (`src/lib.rs:742`, `src/usb/usb.rs:324`). The line §3 cites (`mod.rs:589`) is `frequency::<T>()`, a *per-peripheral* kernel-clock helper; the absence of a CPU-clock accessor was inferred from it wrongly. Revised rule, implemented in `.03`: publish `cpu_hz` and derive `cycles_per_us` from the declared tree value when it divides evenly by 1 MHz, and run the embassy-time calibration as a cross-check whose two numbers are logged together, switching to measured on >1 % disagreement or a non-divisible rate. Reason: calibration is quantized by `TICK_HZ = 32_768`, one tick = 30.517 µs, so over 200 ms it carries ±0.015 %, which at 480 MHz is ±0.07 cycles/µs - enough for a truncated quotient to yield 479 where 480 is right, a 0.2 % error spent against a gap gate that sits 1 % above nominal. §3's "calibrate, then publish" survives; its precision argument does not.
- **C2 - "≈480 MHz" is arithmetic, not a measurement.** It comes from `default_rcc()`'s PLL dividers and agrees with datasheet maximum. `rcc::clocks()` reports the same derived figure, so the two agreeing is consistency, not confirmation. Say that in the comment rather than implying a measurement.
- **C3 - CYCCNT is 32 bits and wraps every 8.947 s at 480 MHz.** `max_block_us` (≤666 µs) is unaffected; `worst_gap_us` is not - a wider gap aliases to a small number, which is precisely the failure the field exists to reveal. Treat a raw delta at or beyond half the counter range as invalid and leave `worst_gap_us` alone.
- **C4 - the timer budget is eight slots, set upstream, and overflow wakes a timer early instead of panicking.** `tick-hz-32_768` and `generic-queue-8` come from daisy-embassy's own `Cargo.toml:16`, not from this repo, and the manifest tracks `branch = "master"` unpinned. Slots are keyed by waker (`queue_generic.rs:55-60` coalesces every timer pending inside one task/select into one slot), and a full queue pops the furthest-out timer so it fires spuriously (`:70-75`). Rig's spend is about five: reporting ticker, LED blink, dump retry, capture deadline, boot including the codec's 2 ms startup delay; the USB drain holds none deliberately (`usb.rs:431`). Raising N from `firmware/Cargo.toml` is not available: selecting a second `generic-queue-N` duplicates `const QUEUE_SIZE` and fails to compile.
- **C5 - there is no §4 FMC/MPU section in `docs/reference/daisy-seed3.md`, so AC #11's "note in §4" cannot be satisfied literally.** Its sections are What-is-and-isn't-different, The codec is strapped, SAI configuration, Flashing the Seed3, and the cache/MPU prose sits inside the ST-Link section around lines 485-508. `FMC` appears nowhere in `docs/`. The note therefore lands in **TASK-038.06**, which already lists that file, already carries the SDRAM/QSPI budget criteria, and already instructs itself to fix stale statements in the same change; suggested position is a new `## SDRAM memory model` sibling after "SAI configuration". Two sentences in that file also go stale the moment rig ships: the claim at `:480-483` that "the embassy executor busy-loops" (embassy-executor 0.10 runs `asm!("wfe")` whenever `poll()` finds nothing, `platform/cortex_m.rs:104-108`, with no feature to opt out), and `:504-506`'s "nothing here calls it" about `SdRamBuilder::build`, which rig will.
- **C6 - `SdRam` has no `Drop` impl.** §6's "dropping it releases ~55 pins at the type level" is not what the code does. Keep the value alive anyway, because `init(&mut delay)` needs `&mut self` on it and it owns the FMC instance; use that reason, not the pin one.
- **C7 - whether CYCCNT keeps counting across the core's `WFE` park is unmeasured here.** If it halts, `worst_gap_us` under-reports idle time. Until a board answers it (a TASK-038.05 row), never present `worst_gap_us` on its own: `delivered` versus `expected` is the starvation authority, and the DWT pair is corroboration.
- **C8 - `emit_record` is `#[cfg(feature = "log-usb")]` and takes `&[u8]`** (`lib.rs:429-430`), while the `console::` builders are ungated. rig needs its own two-definition shim or the `--no-default-features --features "seed3 log-defmt"` build CI already runs will not link. `.03` builds the shim with its two records; `.04` inherits it.
- **C9 - `stim-sine` must stay out of `[features] default`.** Putting it there looks like the tidy way to express "sine unless told otherwise" and silently breaks every variant build: `--features seed3,stim-ess` would enable two generators and trip the mutual-exclusion assert that exists to catch exactly that. Select sine in source when no `stim-*` is on.

### Why `.02` split again the next day

`.02` arrived as "everything cross-compiled": eleven of its fourteen criteria, one new 1 000-line binary, four novel API surfaces (`InterruptExecutor`, `Peripherals::take`, `SdRam`, DWT bring-up) and a CI edit. That is the same shape that killed the parent twice, and the umbrella had already learned that the seam which matters is *novel-API risk versus mechanical work*, not host versus firmware. So `.02.03` takes boot skeleton, DWT, executor topology and stimulus gates (the parts that can fail to compile or hang at boot), and `.02.04` takes capture producer, dump writer, rate gates and CI (mechanical, and only possible once the first exists). `.02` keeps its acceptance criteria as the definition of done and becomes the integration owner. Children numbered `.03`/`.04` rather than `.02.01`/`.02.02` because `.02.01` already exists and is Done, and a fifth level of ID depth is worse for everyone who has to type it.

Stash disposition (recorded by TASK-038.03.02.04, 2026-10-07): `stash@{0}` `wip-038.03.02-uncommitted` no longer exists - `git stash list` shows no wip-038 entry. Its one live hunk, the `spin_budget.rs` safety comment that wrongly claimed nothing takes `cortex_m::Peripherals`, was applied by 41f9cae (TASK-038.03.02.03) and is at `crates/asperitas-logging/src/spin_budget.rs:81-88`. Every other hunk (`console.rs`, `frame.rs`, `lib.rs`, `tests/commit_path_no_panic.rs`) was superseded by 22714b0, 951b660 and f0b4e18, as TASK-038.03.02.03's notes record hunk by hunk. Nothing from it is outstanding, so nobody needs to diff a stash.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-13 00:11
---
Planning note from TASK-060 (2026-09-12): AC #13's clause "CI gains a firmware clippy step (cargo clippy --release --features seed3 -- -D warnings inside firmware/)" is superseded by TASK-060 and its leaves TASK-060.01-.04. The gate lands there instead, running 'cd firmware && cargo clippy --release --features seed3 --bins -- -D warnings' plus an RTT-only pass (--no-default-features --features "seed3 log-defmt"), in pre-commit, pre-push AND ci.yml, placed after CI's existing firmware builds. Whole-package coverage is preserved, so this ticket's Key Decision 5 intent survives; --bins is spelled explicitly rather than relying on a lib-less package defaulting to all bins (measured: omitting --bin works too and covers blinky, ledtest, main, panictest, podtest, rig). Nothing else in AC #13 changes here: cargo fmt --all --check / cargo test --workspace / cargo clippy --workspace --all-targets stay this ticket's, and note that root cargo fmt --all has never seen firmware/ (Cargo.toml:3 exclude), which is TASK-060.02's reason for existing.
---

created: 2026-09-13 04:16
---
Path update from TASK-061.02: the note that lefthook's pre-push hook runs the console cross-build is now phrased "the `push` tier of `scripts/gates.sh` runs it". The two firmware cross-builds live there, console first then RTT-only, with nothing building firmware after them. Same commands, same order, one file instead of three lists. Relevant to TASK-062 too: whichever of those two builds ran last is what `release/main` names on the bench.
---
<!-- COMMENTS:END -->
