---
id: TASK-038.03.02.04
title: >-
  Add rig.rs capture producer, SDRAM dump writer, host-computable rate gates and CI coverage
status: Blocked
assignee:
  - '@agent'
created_date: '2026-09-12 12:00'
updated_date: '2026-09-12 12:05'
labels:
  - planned
dependencies:
  - TASK-038.03.02.03
parent_task_id: TASK-038.03.02
priority: high
ordinal: 88800
modified_files:
  - firmware/src/bin/rig.rs
  - .github/workflows/ci.yml
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Second half of TASK-038.03.02, running on the topology that TASK-038.03.02.03 lands: the capture producer writing mono `i16` blocks into the SDRAM ring with state-machine bookkeeping and overrun accounting, `CAPMAX` before arming and `CAPSTAT` every second from the thread executor, the dump writer with `AUDEND` per block and one `DUMPEND` after flush, the compile-time rate gates, the CI lint and stimulus-build steps, and the host-verifiable part of the parent's verification ladder with its size readings recorded.

Nothing here is novel API. It is mechanical work that only becomes possible once two executors, a proven cycle counter and a render site exist, which is why it is separate: the sibling ticket can fail on compile risk alone without this work being at fault.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The capture producer runs inside the P6 audio callback and its entire outward surface is SDRAM writes plus atomic stores: no lock, no allocation, no logging call, no `embassy_time` await. Per-callback work is bounded to one contiguous copy plus lane truncation.
- [ ] #2 Ring geometry comes from `asperitas_logging::capture`, never restated: `RING_BLOCK_BYTES`, `RING_BLOCKS`, `RING_BYTES`, `FRAMES_PER_CALLBACK`, `CALLBACK_BYTES`, `BYTES_PER_SECOND`, `CAPTURE_WINDOW_SECONDS`, `chunks_per_block()`, `expected_blocks()`. The window is overridable through `option_env!("ASP_RIG_CAPTURE_SECONDS")` so bench runs can shorten it without editing source, and the copied-driver-constant loop is closed with a `const assert!` comparing `capture::FRAMES_PER_CALLBACK` to `daisy_embassy::audio::BLOCK_LENGTH` **in samples**, with the comment explaining why `CALLBACK_BYTES` and `HALF_DMA_BUFFER_LENGTH` are not the same quantity.
- [ ] #3 Block states move only along edges `capture::transition_ok` permits (`Free -> Filling -> Full -> Dumping -> Free`), asserted in a debug build against the table rather than a remembered list, and an overrun leaves the block alone and stops capturing instead of overwriting a block that is being dumped.
- [ ] #4 Sequence numbers and cursors survive arm/disarm: `PRODUCED` counts blocks since boot, the writer walks `block % RING_BLOCKS` in ring order, and `OVERRUN` counts refusals to fill rather than bytes lost.
- [ ] #5 `CAPMAX` is emitted once before arming with `total_bytes`, `ring_bytes`, `seconds_max`, `us_max`, `unused_headroom_bytes` all derived from `capture::` arithmetic, and `CAPSTAT` is emitted from the thread executor about once a second carrying `delivered`, `expected`, `overrun`, `max_block_us`, `worst_gap_us`, `audio_exit`, `dumped`, `dropped_full`. Every duration is converted with the `cycles_per_us` that TASK-038.03.02.03 established, and a raw gap delta at or beyond half the 32-bit counter range is treated as invalid rather than trusted.
- [ ] #6 Capture is armed automatically by rig itself on a documented timeline (a fixed delay after the first audio callback so `BOOT`/`RIGCFG`/`CAPMAX` have flushed), ends at the earlier of the window deadline or a full ring, and dumps on the device's own decision. No inbound console channel is assumed to exist, because none does.
- [ ] #7 The dump writer obtains permission for every chunk through `asperitas_logging::try_emit_dump`, never bypasses `dump::dump_fits`, retries refusals on a `Timer` backoff instead of busy-waiting, and counts both refusals (`refused`) and the longest stall (`stall_ms`). Ordinary log and `STATUS` traffic stays lossless during a dump - the behaviour `tests/console_dump.rs::log_records_survive_a_saturated_dump` protects.
- [ ] #8 Each block ends with `AUDEND` whose CRC covers the raw concatenated bytes in chunk order, computed incrementally as chunks are sliced rather than in a second pass, and the dump ends with one `DUMPEND` carrying `blocks`, `chunks`, `bytes`, `elapsed_ms`, `refused`, `stall_ms` plus a `CONSOLE.snapshot()` reading taken at that instant.
- [ ] #9 Compile-time rate gates, expressed as `const assert!` in `rig.rs` where both driver constants and `capture::` are visible: the intended window provably fits the ring in blocks, `worst_gap_us` must stay below one audio period plus slack (21 000 µs against a 20 833 µs period), `max_block_us` below the callback budget, and delivered blocks must equal expected blocks at the end of the window. A gate that cannot be evaluated at compile time is reported at runtime beside the number it judges, never left as prose.
- [ ] #10 `.github/workflows/ci.yml` gains three commands inside the existing single-quoted `nix develop --command bash -c '...'` string (no nested single quotes, no new step, no matrix): a firmware lint step `cd firmware && cargo clippy --release --features seed3 -- -D warnings`, which nothing else provides because firmware is excluded from the root workspace and `make clippy` is hard-wired to `--bin main`; then `cargo build --release --features seed3,stim-ess --bin rig` and `cargo build --release --features seed3,stim-pulse --bin rig`. The implicit-sine default needs no new command: neither existing firmware build passes `--bin`, so both already compile `rig` with the default generator.
- [ ] #11 The timer-slot budget is stated where it is spent and stays within eight: reporting ticker, LED blink, dump retry, capture deadline, boot including the codec's 2 ms startup delay. Overflow evicts the furthest-out timer and wakes it early rather than panicking, so a blown budget shows up as a mistimed `CAPSTAT`, not an error.
- [ ] #12 Host-side measurements recorded in Finalization Notes: `.text`/`.bss` of `rig` versus `main` under both transports; `wire_bytes_per_block() × RING_BLOCKS` for the full-ring dump volume; the parent's §11 rows that need no board; and the dump-bandwidth prediction labelled as a prediction, naming `DUMPEND.elapsed_ms` as the field that will replace it.
- [ ] #13 `git stash list` is empty of `wip-038.03.02-uncommitted` after TASK-038.03.02.03 applied its one live hunk, and the parent's notes record the disposition rather than leaving the next reader to diff a stash.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
### Planning Decision Summary

**Scope:** everything in TASK-038.03.02 that is not boot bring-up, DWT, the executor split or stimulus selection. Those land first in TASK-038.03.02.03; this ticket extends the `rig.rs` that ticket introduces.

**Technical approach:** SPSC ring between a P6 producer and a thread-mode consumer, published with acquire/release atomics, plus a dump writer that treats USB pipe capacity as the scarce resource and yields on every refusal. Geometry and grammar come from `asperitas_logging::{capture, dump, console}`; rig contributes only policy (when to arm, when to stop) and measurement (cycles to microseconds).

**Key changes:** `firmware/src/bin/rig.rs` (extend), `.github/workflows/ci.yml` (one lint step, three builds).

**Critical verification:** the const gates compile, the three stim builds still link with the capture path present, and the host-computable rows of the parent's §11 ladder are run with their numbers written down.

### Research Findings

**Verified against the shipped code** (`crates/asperitas-logging/src/{capture,dump,console,lib}.rs`, read 2026-09-12):

- `capture.rs` publishes exactly what the plan names: `SAMPLE_RATE_HZ`, `CAPTURE_BYTES_PER_SAMPLE`, `FRAMES_PER_CALLBACK = 32`, `CALLBACK_BYTES`, `RING_BLOCK_BYTES = 32_768`, `RING_BLOCKS = 1_024`, `RING_BYTES`, `BYTES_PER_SECOND = 96_000`, `CAPTURE_WINDOW_SECONDS = 300`, and the functions `callbacks_per_block`, `samples_per_block`, `full_chunks_per_block`, `tail_chunk_bytes`, `chunks_per_block`, `records_per_block`, `tail_frame_bytes`, `audend_frame_bytes`, `wire_bytes_per_block`, `useful_fraction_per_mille`, `ring_seconds_floor`, `ring_duration_micros`, `expected_blocks`, plus `BlockState`, `BlockState::ALL`, `as_u8`, `from_u8`, `transition_ok`. There is **no** `total_capture_bytes`; the caller computes `window_s × BYTES_PER_SECOND` (28.8 MB for 300 s, comfortably `u32`).
- `emit_record(level, now_ms: u32, body: &[u8]) -> bool` is **`#[cfg(feature = "log-usb")]`** (`lib.rs:429-430`). The `console::` builders are ungated, so rig's call sites need their own cfg shim: one local `fn emit_console(...)` with two definitions, gated on `log-usb` and a no-op otherwise, keeps the `--no-default-features --features "seed3 log-defmt"` build compiling. That config is built by CI already, so getting this wrong fails CI rather than surprising nobody.
- Callers supply `now_ms` themselves; the house convention is `embassy_time::Instant::now().as_millis() as u32` (`lib.rs:449`, `usb.rs:129-130`). `Instant::now()` reads the driver counter and does not await, so it is legal in the P6 context; awaiting there is not.
- `dump::CHUNK_RAW`, `dump::audio_body`, `dump::audend_body`, `dump::dump_fits` (`dump.rs:539`) and `try_emit_dump` (`lib.rs:634`) are the writer's whole vocabulary. `console::capstat_body`, `capmax_body`, `dumpend_body` take `&mut [u8; console::BODY_WINDOW]` and return the length; worst-case bodies are 187 (`CAPSTAT`), 133 (`CAPMAX`) and 194 (`DUMPEND`) bytes, all inside `frame::MAX_BODY = 200`, with `DUMPEND` six bytes from the cap - the tightest verb on the wire, which is why adding a field to it is a design change rather than an edit.
- `console::RingCapacity { total_bytes, ring_bytes, seconds_max, us_max, unused_headroom_bytes }`, `CaptureStatus { delivered, expected, overrun, max_block_us, worst_gap_us, audio_exit, dumped, dropped_full }`, `DumpSummary { blocks, chunks, bytes, elapsed_ms, refused, stall_ms, sent, dropped_full }`. Field order in the struct is the wire order; do not reorder.
- Timer overflow semantics, measured upstream (`embassy-time-queue-utils-0.3.2/src/queue_generic.rs:55-75`): slots coalesce per waker, and a full queue pops the furthest-out timer so it fires **early**. Eight slots, set by daisy-embassy, not raisable here (two selected `generic-queue-N` features collide on `const QUEUE_SIZE` and fail to compile).
- The `SdRam` value must be owned by the audio task for the program's lifetime, but **not** for the reason the parent gives: dropping it does **not** release ~55 pins, because `SdRam` has no `Drop` impl (verified in daisy-embassy `ca9bcc9`). Keep it alive because `init(&mut delay)` takes `&mut self` on the value and it owns the FMC instance; say that instead of repeating the false claim.
- MPU region mismatch is real and inherited: `sdram.init()` returns bank 5's base `0xC000_0000` while `SdRamBuilder` programs a cacheable MPU region at `0xD000_0000`, and `MPU_DEFAULT_MMAP_FOR_PRIVILEGED` is what makes the uncached accesses work anyway. Caches are enabled nowhere in this stack, so today every access is uncached and coherent. Recording this belongs to **TASK-038.06**, which already owns `docs/reference/daisy-seed3.md` and has an AC for fixing stale statements found while writing; rig's job is only to carry the rule in a comment: caches stay off until someone owns the coherence argument for the FMC window, and that person revisits the capture hand-off ordering in the same change.

### Recommended Approach

1. **Producer.** Follow the parent's §6 steps verbatim: check state, `Filling`, truncate the even words (`input[2 * i]`, left lane, matching `main.rs`'s `decode_block`) into the block's byte range at `callback_index_in_block * CALLBACK_BYTES` with `(sample.clamp(-1.0, 1.0) * 32767.0) as i16`, and on the last callback `fence(Release)` then store `Full`. One `const MONO_LANE` carries the lane choice with a comment saying the physical jack is TASK-038.05's observation and flipping that constant is the fix.
2. **Publish with the classic SPSC discipline**, `core::sync::atomic` only, payload written before the index that publishes it. Do not reuse `main.rs`'s `UnsafeCell` + `interrupt::free` idiom: it is right for one slow knob writer and wrong for handing buffer ownership across an interrupt boundary.
3. **Arming policy in the thread task, visibility through atomics.** A `ARMED: AtomicBool` plus an `ARM_AT_MS`/`DEADLINE_MS` pair read by the producer. Arm a fixed delay after the first callback so `BOOT`, `RIGCFG` and `CAPMAX` reach the host before the ring starts filling; stop at the earlier of deadline or full ring. Single-shot: after `DUMPEND`, rig logs completion and idles. There is no inbound channel to re-arm through (runtime control is TASK-032), and a bench run is repeated by reset, which `slow-boot` makes safe for DFU. Say all of that in one comment block above the arming code, because it looks like a missing feature otherwise.
4. **Consumer/writer** exactly as §7: `compare_exchange(Full, Dumping)`, slice chunks, `audio_body(...)`, retry `try_emit_dump` with `Timer::after_millis(1)` between refusals, incremental CRC over the raw bytes as they are sliced, `AUDEND`, `store(Free, Release)`. The yield at the await is load-bearing: the USB drain task is what frees the pipe capacity the writer is waiting for.
5. **`CAPSTAT` cadence** from the reporting task's ticker, reading atomics only. Keep the record cheap enough that a 1 s cadence cannot itself threaten the pipe headroom that the dump depends on; if it ever does, the symptom is `dropped_full` climbing in `DUMPEND`.
6. **Rate gates.** Put them adjacent to the capture code with the parent's wording. Where a bound is a runtime fact rather than a constant (`worst_gap_us < 21_000`, `delivered == expected`), enforce it as a check that logs a clear `error!` line naming the threshold and the observed value, and keep the geometric ones (`expected_blocks(300) <= RING_BLOCKS`, i.e. 879 <= 1024) genuinely compile-time.
7. **CI edits.** Append the three commands inside the existing `bash -c '...'` string, after the two existing firmware builds. Use cargo's comma form (`--features seed3,stim-ess`) so nothing needs quoting inside a single-quoted script. No new steps, no matrix, no `timeout-minutes` (the job has none today either).

### Test Coverage Matrix

| Test | Type | What it verifies | Critical |
|---|---|---|---|
| `const_gate_window_fits_ring` | compile-time | `expected_blocks(CAPTURE_WINDOW_SECONDS) <= RING_BLOCKS` | Critical |
| `const_gate_callback_geometry` | compile-time | frames-per-callback matches `daisy_embassy::audio::BLOCK_LENGTH`; block holds a whole number of callbacks | Critical |
| `build_seed3_bin_rig_with_capture` | build | capture plus dump path links under `log-usb` | Critical |
| `build_log_defmt_bin_rig_with_capture` | build | the `cfg` shim keeps the RTT-only image linking with no console emission | Critical |
| `build_stim_ess_with_capture` / `build_stim_pulse_train_with_capture` | build | non-default generators still compile with the capture path present | Important |
| `firmware_clippy_clean` | lint | `cargo clippy --release --features seed3 -- -D warnings` in `firmware/` | Important |
| `window_override_builds` | build | `ASP_RIG_CAPTURE_SECONDS=30 cargo build ...` compiles and the shortened window still passes the ring gate | Important |
| `state_machine_edges` | unit-in-rig (debug assert) | every observed transition is permitted by `capture::transition_ok` | Important |
| `size_readings` | measurement | `.text`/`.bss` delta over `main`, both transports; ring lives in SDRAM, not `.bss` | Important |
| `bench_300s_soak` | bench, deferred | delivered equals expected at 300 s, dump validates offline | Critical (TASK-038.05, `@human`) |

The soak row is hardware work and is already owned by TASK-038.05; it is listed so the hand-off is explicit, and it is not an acceptance criterion here.

### Key Decisions

1. **Automatic arming, single-shot run.** The alternative (wait for a command) requires an inbound channel that does not exist and would make the 300 s soak impossible to run unattended. Cost: rig cannot be re-armed without a reset, which is acceptable for a measurement rig and is what TASK-032 will change.
2. **Policy in thread mode, mechanism in the producer.** Arming decisions involve clocks and timers; the producer may touch neither. Publishing a flag keeps the callback's surface at SDRAM writes and atomic stores.
3. **Overrun stops capture.** Continuing after an overrun means overwriting data the writer has not drained, which turns a measurable loss into silent corruption. Stopping converts it into a count plus a frozen tail.
4. **Incremental CRC.** A second pass over 32 KiB per block inside the writer is easy to write and easy to mistake for free; folding it into the chunk slicing costs nothing extra and removes the temptation to "optimise" it later by skipping verification.
5. **Whole-package firmware clippy in CI, not `--bin rig`.** Measured clean package-wide at planning time (exit 0), consistent with the host workspace's `-D warnings` across all targets, and it means a future binary cannot quietly skip linting. If an unrelated binary later breaks the step, fix that binary or narrow the step, and say which in the commit message.
6. **Docs note moves to TASK-038.06.** Parent AC #11 asks for a note in "`docs/reference/daisy-seed3.md` section 4"; that file has no FMC/MPU section (§4 is "Flashing the Seed3"), and TASK-038.06 already lists the file, already carries the SDRAM/QSPI budget ACs, and already instructs itself to fix stale statements in the same change. Two owners for one file is how the WFE correction gets half-applied twice.

### Risks and Mitigations

1. **Pipe headroom starves the dump.** Gate is `dump_fits`; symptom is `refused`/`stall_ms` in `DUMPEND`. The 1 s `CAPSTAT` cadence is the knob to turn first if it shows.
2. **A 32 MiB ring write pattern exposes the uncached-FMC cost.** Nothing in this repo has measured sustained FMC write throughput. Producer work is one contiguous copy per callback; if the bench sees starvation the candidate causes are the FMC write path and the P6 priority, in that order, and `worst_gap_us` beside `delivered`/`expected` distinguishes them.
3. **Ring wrap assumptions.** The gate proves 300 s fits (879 < 1024); the modulo stays anyway, and `overrun` counts any case where the writer falls behind the producer.
4. **Stimulus builds grow past flash.** `stim-ess` adds a scan at boot, not code bulk; sizes are recorded per AC #12 so growth is visible rather than discovered at flash time.
5. **`option_env!` override silently breaks a gate.** `ASP_RIG_CAPTURE_SECONDS` shorter than the default can only relax the ring gate, never tighten it, so the compile-time assertion still holds in every build.

### Files to Create or Modify

| File | Action | Purpose |
|---|---|---|
| `firmware/src/bin/rig.rs` | modify | capture producer, ring atomics, arming policy, CAPMAX/CAPSTAT, dump writer, DUMPEND, const gates |
| `.github/workflows/ci.yml` | modify | firmware clippy step plus three stim-feature builds, inside the existing `nix develop` bash string |
| `backlog/tasks/task-038.03.02*.md` | modify | size readings, ladder results, finalization notes |
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
### Sequencing

Depends on TASK-038.03.02.03 for: `rig.rs` existing and linking, `Peripherals::take()`, DWT with a proven `cycles_per_us`, the two-executor split with SAI1 at P6, the `stim-*` features, and `RIGCFG` from the render site. Until that lands, this ticket has no file to extend; it is `Blocked` deliberately so the autonomous loop does not select it early. Flip it to Dev Ready when the sibling is Done.

### Numbers worth having before starting

- 300 s window = 28.8 MB raw = 879 blocks of 32 KiB; ring holds 1024 blocks = 32 MiB; unused headroom ≈ 4.7 MiB.
- Real-time capture needs 96 000 B/s raw, about 169 kB/s of wire; full-ring dump volume is `wire_bytes_per_block() × RING_BLOCKS`, roughly 59 MB, and those USB FS bulk ceilings are theory, not measurement - `DUMPEND.elapsed_ms` is the field that replaces them (TASK-038.05 AC #4).
- Audio period is 20 833 µs for 32 frames at 48 kHz; the gap gate is 21 000 µs.
- Warm incremental firmware rebuild is ~0.13 s, so iterate with `touch` rather than cleaning.
<!-- SECTION:NOTES:END -->
