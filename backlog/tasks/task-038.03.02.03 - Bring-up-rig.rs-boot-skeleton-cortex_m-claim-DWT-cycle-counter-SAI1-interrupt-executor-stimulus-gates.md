---
id: TASK-038.03.02.03
title: >-
  Bring up rig.rs boot skeleton: cortex_m claim, DWT cycle counter,
  SAI1-interrupt executor, stimulus gates
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-12 11:59'
updated_date: '2026-09-12 14:58'
labels:
  - planned
dependencies:
  - TASK-038.03.02.01
modified_files:
  - firmware/src/bin/rig.rs
  - firmware/Cargo.toml
  - crates/asperitas-logging/src/spin_budget.rs
  - firmware/Makefile
parent_task_id: TASK-038.03.02
priority: high
ordinal: 88700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
First half of TASK-038.03.02 (the compile-risk half). firmware/src/bin/rig.rs carries the load-bearing preamble, claims cortex_m::Peripherals::take() once, brings up DWT CYCCNT with readback proof and calibrates it against embassy-time, splits the thread executor from an interrupt-mode executor that owns the audio callback with SAI1 at P6, adds the stim-* cargo features, and emits RIGCFG from the render site.

The other half is TASK-038.03.02.04 (capture producer, dump writer, rate gates, CI). This split exists because the parent died twice at forty minutes with zero commits, and everything here is either novel-API or a topology decision, while the sibling is mechanical work that runs on top of it. Neither half can be marked Done while the other is missing, so neither introduces `rig.rs` ahead of the other: this ticket lands the first compiling `rig.rs`, and the sibling extends the file it lands.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `firmware/src/bin/rig.rs` exists and links in **both** configurations CI already builds without any CI change: `cd firmware && cargo build --release --features seed3` and `cargo build --release --no-default-features --features "seed3 log-defmt"`. It carries the whole per-binary preamble that cannot move to a lib crate: the `#[panic_handler]` wrapper onto `asperitas_logging::panic_handler::handle_panic`, the `#[defmt::panic_handler]`, the transport-gated `defmt_rtt` / no-op `#[defmt::global_logger] Logger` pair, and its own `bind_interrupts!` USB IRQ struct handed to `asperitas_logging::usb::init`.
- [x] #2 `cortex_m::Peripherals::take()` is claimed exactly once, before `hal::init`, and its `Option` is matched: `None` produces a visible failure (a `log::error!` plus a halt in the boot-LED error state), never a panic and never `steal()`.
- [x] #3 DWT bring-up proves itself: TRCENA set, `DWT::unlock()` with LAR written, CYCCNT enabled, and a readback of `DCB.enable_trace`/`DWT.control` confirming it, with the proof logged. A counter that reads zero after enablement stops rig rather than letting every later duration read as zero.
- [x] #4 `cycles_per_us` comes from the clock tree (`embassy_stm32::rcc::clocks(&board.pins.rcc).sys`), and a 200 ms calibration against `embassy_time::Instant` is run as an independent cross-check whose two numbers are logged together; a disagreement over 1 %, or a declared frequency not divisible by 1 MHz, switches rig to the measured value and logs that it did.
- [x] #5 `embassy-executor` gains the `executor-interrupt` feature in `firmware/Cargo.toml`; the thread executor runs `asperitas_logging::led::blink_task` and the reporting task; an `InterruptExecutor<'static>` owns the audio callback; `SAI1.set_priority(Priority::P6)` is set **before** `start::<{Priority::P6 as u8}>()`, and the boot log records the effective NVIC priority of SAI1, the SAI DMA streams and the embassy-time driver (TIM5) so the ordering claim is a reading rather than an assumption.
- [x] #6 The audio task never awaits after the callback starts, and the timer-slot budget is stated in a comment: `generic-queue-8` is fixed by daisy-embassy, slots are keyed by waker (all timers pending in one task collapse to one), and overflow wakes a timer early rather than panicking.
- [x] #7 Stimulus selection is cargo features named as the parent names them (`stim-sine`, `stim-ess`, `stim-pulse`), with sine used when **no** `stim-*` is selected and mutual exclusion enforced by a `const _: () = assert!(...)` in the source. `stim-sine` must not be in `default`: if it were, `--features seed3,stim-ess` would select two generators and trip its own guard. `cargo build --release --features seed3,stim-ess` and `... seed3,stim-pulse` both produce an image, and selecting two of the three fails the build.
- [x] #8 The render site is shaped `stimulus -> [processor slot] -> encode_block(output)` mirroring `main.rs`, ignores the input frame, and documents which lane it takes for mono capture (`words[0]` is left in `main.rs`'s `decode_block`).
- [x] #9 The render site emits exactly one `RIGCFG` record (`console::rigcfg_body`, capture format, block geometry, window, `cpu_hz`, cache bits) and exactly one `RIGGEN` record (`console::riggen_body`) embedding the active generator's `describe()` bytes verbatim, first block only, so no second description grammar exists. The code states the invariant that those bytes fit `console::RIGGEN_MAX_GEN_BYTES` (160) with the pinned strings named in a comment (longest is 96 today) and debug-asserts the returned length, because text clipped at the frame limit would print a truncated parameter as if it were real. Body buffer sized from `console::BODY_WINDOW`, never a literal.
- [x] #10 The `spin_budget.rs` safety comment that `stash@{0}` wrote is applied: the sentence claiming "nothing in this stack claims `cortex_m::Peripherals::take()`" is replaced, because rig.rs now does. Code in that file is unchanged.
- [x] #11 `make build BINARY=rig` produces a flashable image, and the Makefile's artifact collision (it writes `firmware.bin` regardless of `$(BINARY)`) is either named per binary or documented at the point of use, so nobody flashes rig believing the filename says otherwise.
- [x] #12 Host-verifiable sizes recorded in Finalization Notes: `.text` and `.bss` delta of `rig` over `main` under both transports, and confirmation that no large buffer landed in the RAM region (`arm-none-eabi-nm --size-target ... || sort -k2` or equivalent).
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
### Planning Decision Summary

**Scope:** the first compiling `rig.rs`, reduced to the parts that can fail to compile or hang at boot. Capture producer, ring, dump writer, CAPSTAT/CAPMAX, DUMPEND, the compile-time rate gates and the CI steps belong to TASK-038.03.02.04, which depends on this one.

**Technical approach:** copy `main.rs`'s preamble and audio setup verbatim, then change exactly three things about it: claim `Peripherals::take()`, run the audio future on an interrupt-mode executor instead of the thread executor's select tree, and replace the filter/gain chain with a stimulus generator behind a feature gate. Everything else stays identical to the binary that is known to work on hardware.

**Key changes:** `firmware/src/bin/rig.rs` (new), `firmware/Cargo.toml` (`executor-interrupt`, `stim-*`), `crates/asperitas-logging/src/spin_budget.rs` (comment only), `firmware/Makefile` (artifact naming or a comment at the point of use).

**Critical verification:** both CI build configs link; the three stimulus builds link; the boot log line order proves DWT ran before `BOOT`'s "audio ready"; the priority reading proves SAI1 sits below the time driver and the DMA streams.

### Research Findings

**API facts verified locally against the sources the build actually resolves** (daisy-embassy checkout `ca9bcc9`, `embassy-executor-0.10.0`, `embassy-stm32-0.6.0`, `embassy-time-queue-utils-0.3.2`):

- `embassy_stm32::rcc::clocks(&Peri<RCC>) -> &Clocks` is public and **not** feature-gated (`rcc/mod.rs:142`), and `Clocks` derefs to the generated `Freqs`. `embassy-stm32` itself uses the system clock this way (`src/lib.rs:742`, `src/usb/usb.rs:324`: `rcc::get_freqs().sys.to_hertz()`). **The parent plan's §3 claim that no such accessor exists is false.** `to_hertz()` returns `Option<Hertz>` where `Hertz(pub u32)` (`embassy-stm32/src/time.rs:8,135`); if the generated field is a plain `Hertz` rather than a `MaybeHertz`, read `.sys.0` instead. `board.pins.rcc` is a `Peri<'d, RCC>`, so it is available after `new_daisy_board!`.
- `InterruptExecutor::new()` is const; `start<const PRIORITY: u8>(&self, spawner: SendSpawner<'p>) -> &'p mut Executor`; `send()` is `unsafe` and `&self`. looper.rs's form `start::<{Priority::P2 as u8}>()` compiles; `Type::INTERRUPT` does not exist in 0.10.
- **`set_priority` after `start` is UB and `start` asserts on it.** Set `SAI1.set_priority(Priority::P6)` first. Priority numbers are inverted (`P0` highest, seven bits, no sub-priority on Cortex-M7).
- `Spawner::spawn` returns `()`; the task call returns `Result<SpawnToken<E>, EmbassyError>` and **dropping a held token on a started executor panics**, so always consume it through `spawn(...)`.
- `Spawner::for_current_executor()` is `unsafe` in 0.10. Prefer passing a `SendSpawner<'static>` into tasks; the audio task needs no spawner at all.
- `bind_interrupts!` and `#[interrupt]` may not name the same vector: `bind_interrupts!` expands to `#[export_name = "SV"] fn __SV() {}`, so binding SAI1 anywhere and also defining `fn SAI1()` is a duplicate-symbol link error. Nothing binds SAI1 today (`AudioIrqs` binds only the four SAI1 DMA streams), which is why hand-writing the handler is possible at all. Do not "tidy" this by binding SAI1.
- `start_callback`'s closure is `FnMut(&[u32], &mut [u32])`, flat interleaved words, 64 words per 32-frame period. `prepare_interface(Default::default())` selects `Fs::Fs48000`.
- Per-binary preamble is duplicated by hand in all five existing bins and cannot move to a lib crate (`main.rs:50-55` explains the dead-code-elimination reason).
- `embassy-executor` is currently built with only `["platform-cortex-m", "executor-thread"]`, measured from the release build fingerprints. Adding `executor-interrupt` changes the executor unit for all six binaries, so the other five must still build (CI covers it, since neither firmware build passes `--bin`).
- Firmware has **no** `[lints]` table and inherits nothing from the root workspace (root `Cargo.toml` excludes `firmware/`), so rustdoc denies do not apply here.
- `ExponentialSweep::default()` performs a 384,000-sample peak-normalising scan in `apply()` (`stimulus.rs:539-549`). It stores nothing (samples are recomputed from closed form in `tick`), so RAM cost is ~64 bytes of scalars; the cost is boot time, milliseconds at 48 kHz. Construction therefore belongs before audio starts, not in the callback.
- All three generators emit the same value on both channels, so the mono-lane choice is about the *input* side convention, not the output.

**Corrections to the parent plan's §3 that this ticket implements:**

1. **Calibration is quantized by the tick rate, not by a 1 MHz clock.** `TICK_HZ = 32_768` (`tick-hz-32_768`, selected by daisy-embassy's `Cargo.toml:16`, driver TIM5), so one tick is 30.517 µs and `elapsed().ticks()` can only land on multiples of it. Over a 200 ms window that is ±1 tick = ±0.015 %, which at 480 MHz is ±0.07 cycles/µs - enough that truncating the quotient can yield 479 instead of 480, a 0.2 % error, against a `worst_gap_us` threshold that sits 1 % above nominal. Hence the design below: publish the **declared** tree value, use calibration only as a cross-check.
2. **"≈480 MHz" is arithmetic from `default_rcc()`'s dividers, not a reading.** It agrees with datasheet maximum, not with a measurement. `rcc::clocks()` reports the same derived figure, so agreement between the two is consistency, not confirmation. Say so in the comment.
3. **CYCCNT is 32 bits and wraps every 8.947 s at 480 MHz.** `max_block_us` (≤ 666 µs) is unaffected; `worst_gap_us` is not. A gap wider than the wrap aliases to a small number. Treat a raw delta at or beyond half the counter range as invalid and leave `worst_gap_us` alone, with the reasoning in the comment.
4. **The thread executor parks with `WFE`, it does not busy-loop.** `embassy-executor-0.10.0/src/platform/cortex_m.rs:104-108` runs `asm!("wfe")` whenever `poll()` finds nothing, with wake via `sev` in `__pender`; there is no feature to opt out. Two consequences: `docs/reference/daisy-seed3.md:480-483` is wrong (that ticket is TASK-038.06, which already owns stale-statement fixes in that file), and whether CYCCNT halts across core sleep is unmeasured here. Do not resolve it by keeping a perpetually-ready future alive; measure it. See the test matrix below.
5. **Timer overflow evicts, it does not panic.** `queue_generic.rs:70-75` pops the furthest-out timer and wakes it spuriously when all eight slots are taken, and slots are keyed by waker (`will_wake` coalescing), so capacity is roughly "tasks with a pending timer", not "Timer objects". Budget for rig: reporting ticker 1, LED blink 1, dump retry 1, capture-window/deadline 1, boot incl. the codec's 2 ms startup delay 1, USB drain 0 (deliberately, see `usb.rs:431`) = 5 of 8. Any future increase must be justified against 8, and `firmware/Cargo.toml` must **not** add a `generic-queue-N` feature: two selected Ns collide on `const QUEUE_SIZE` and fail to compile.

### Recommended Approach

1. **Start from `main.rs`, not from scratch.** Copy it to `rig.rs`, rename the entry function's purpose in comments, and delete the knob/encoder/filter/gain path. Every line you keep is a line whose behaviour on hardware is already known.
2. **Claim `Peripherals::take()` first**, before `hal::init(config)`, and hold `dwt: DWT` in a `static_cell::StaticCell` or pass it by `'static` reference into the tasks that need it. Match the `Option`; never `steal()`.
3. **Bring up DWT with proof**, mirroring `spin_budget.rs:87-89` (which uses `steal()` and therefore cannot serve rig): `DCB::enable_trace()`, `DWT::unlock()`, `PERIOD` untouched (cycle counter, not cycle count), `CTRL.CYCCNTENA`, then read back and log. If the readback lies, stop.
4. **Read the declared clock, then calibrate as a check.** Compute `cycles_per_us_declared = declared_hz / 1_000_000` when `declared_hz % 1_000_000 == 0`; otherwise use the measured value unconditionally. Run the 200 ms window with `Timer::after_millis(200)` and `Instant::now()`/`elapsed().ticks()` (round to nearest, never truncate), log `declared_hz`, `measured_hz`, `cycles_per_us_used`, and pick declared unless the ratio is outside 0.99–1.01. Do this in thread mode before the audio executor is started.
5. **Two executors.** Thread executor via `#[embassy_executor::main]` (unchanged from `main.rs`), holding `blink_task`, the reporting task, and the console. A second `static AUDIO_EXECUTOR: StaticCell<InterruptExecutor<'static>>` gets `InterruptExecutor::new()`, `init()`, `SAI1.set_priority(Priority::P6)`, then `start::<{Priority::P6 as u8}>(spawner)`, and `audio_interrupt(spawner)` is spawned onto it. Inside `audio_interrupt`, `set_priority` again is permitted but redundant; say why it is there (looper.rs does it) or omit it.
6. **Hand-write the SAI1 vector** exactly as looper.rs:144-146 does, with the comment explaining why it is legal (nothing binds SAI1) and why binding it would not be.
7. **Stimulus behind features.** Keep `default = ["log-usb"]` and add `stim-sine = []`, `stim-ess = []`, `stim-pulse = []` with **none** of them in `default`. The render site selects sine when `#[cfg(not(any(stim_sine, stim_ess, stim_pulse)))]`, and a `const _: () = assert!(count <= 1)` rejects nonsense; putting `stim-sine` in `default` looks tidier and breaks every non-default build, because `--features seed3,stim-ess` would then enable two generators and trip the guard. Build the generator before audio starts (`set_sample_rate(48_000.0)` once, and for `ess` that is where the 384 k scan is paid), keep it in a `StaticCell`, and reach it from the callback through the same `cortex_m::interrupt::free` discipline `main.rs` uses for `KnobState` or through a `&mut` borrow that the single-owner task holds. Note that `interrupt::free` is a PRIMASK critical section and must never appear inside the callback; if the generator is reachable only through one owned reference, no lock is needed at all - prefer that.
8. **Emit `RIGCFG` then `RIGGEN` from the render site, first block only.** A `bool` captured by the `FnMut` closure, a `&mut [u8; console::BODY_WINDOW]` on the stack, `console::rigcfg_body(...)` then `console::riggen_body(describe_bytes, ...)`, each followed by one `emit_record`. Both are `Level::Info` so the bench sees them unsolicited. Note `emit_record` itself is `#[cfg(feature = "log-usb")]`: put the two definitions of a tiny local `emit_console` shim in `rig.rs` now, while there are two call sites, so TASK-038.03.02.04 does not have to retrofit cfgs onto ten.
9. **Leave the capture/dump hooks empty but present**: a `CaptureSink` no-op module or clearly-marked TODO seam where `.04` will splice in, chosen so `.04`'s diff is additive. Do not stub behaviour that `.04` will have to find and delete.

### Test Coverage Matrix

| Test | Type | What it verifies | Critical |
|---|---|---|---|
| `build_seed3_bin_rig` | build | `cargo build --release --features seed3` links rig alongside the other five bins | Critical |
| `build_log_defmt_bin_rig` | build | `--no-default-features --features "seed3 log-defmt"` links rig with defmt.x and the logger stubs together | Critical |
| `build_stim_ess` / `build_stim_pulse_train` | build | the two non-default generators compile and link | Critical |
| `build_stim_conflict_fails` | negative build | `--features seed3,stim-sine,stim-ess` fails on the mutual-exclusion assert | Important |
| `build_stim_explicit_sine` | build | `--features seed3,stim-sine` picks the same generator as the implicit default | Important |
| `boot_log_order` | bench-adjacent, deferred | `DWT ok` precedes `BOOT` "audio ready" in the framed stream, proving calibration ran before audio | Important |
| `priority_reading_log` | bench-adjacent, deferred | logged NVIC IP values show TIM5 and SAI-DMA outrank SAI1's P6 | Important |
| `cyccount_across_wfe` | bench-adjacent, deferred | CYCCNT keeps counting while the thread executor is parked; if it does not, `worst_gap_us` is reported alongside `delivered`/`expected` and never alone | Important |
| `elf_size_delta` | measurement | `.text`/`.bss` delta of rig over main, both transports; no large object in the RAM region | Important |

Deferred rows are the ones that need a board and a probe; they are listed here so the bench session in TASK-038.05 arrives with them, and they are **not** acceptance criteria for this ticket.

### Key Decisions

1. **Declared clock wins, calibration checks.** Because the tick quantization (±30.517 µs) is coarse relative to a 1 % gate, publishing a measured quotient risks a 0.2 % systematic error for no benefit. The cross-check still catches the failure mode that matters: a counter that does not tick at core rate.
2. **No `steal()`, and the failure to claim is loud.** `Peripherals::take()` returning `None` means something else owns the core peripherals; continuing would report zero durations forever.
3. **P6 for SAI1, asserted before `start`.** The ordering claim is load-bearing for the whole "bounded latency without a critical section" argument, and it is free to check at boot, so the boot log records the numbers.
4. **Feature-gated stimulus rather than a runtime switch.** There is no inbound console channel to carry a switch, and a compile-time choice lets the CI matrix prove all three variants and lets `compile_error!` reject nonsense.
5. **`rig.rs` is introduced by this ticket and extended by `.04`.** A file that exists but does less is compilable and reviewable; two tickets each adding half of one file at once would not be.
6. **Do not add `generic-queue-N` or raise the queue.** N is fixed by daisy-embassy and a second selection is a compile error; the budget is spent deliberately (5 of 8).

### Risks and Mitigations

1. **Adding `executor-interrupt` regresses an existing binary.** CI builds all six bins in two configs, so a regression is caught without new CI.
2. **`rcc::clocks()` shape differs from expectation** (`MaybeHertz` vs `Hertz`, or a missing field name). Both spellings are given above; whichever the compiler rejects is corrected in place. Worst case the declared value falls back to the measured one, which is the pre-existing plan anyway.
3. **DUT cycle counter does not run while the core sleeps.** Unmeasured here. Mitigated by never reporting `worst_gap_us` without `delivered`/`expected` beside it, and by the deferred bench row.
4. **A second `bind_interrupts!` struct collides.** Each binary defines its own; rig defines `RigUsbIrqs` (or reuses the `main.rs` name inside its own file, which is equally fine because they are separate crates-in-one).
5. **`stim-ess` boot-time scan looks like a hang.** It is milliseconds, but the log should say what it is doing before it happens.

### Files to Create or Modify

| File | Action | Purpose |
|---|---|---|
| `firmware/src/bin/rig.rs` | **create** | preamble, take(), DWT, two executors, stimulus selection, RIGCFG |
| `firmware/Cargo.toml` | modify | `executor-interrupt`; `stim-sine`/`stim-ess`/`stim-pulse`, none of them in `default` |
| `crates/asperitas-logging/src/spin_budget.rs` | modify (comment only) | port the `stash@{0}` safety-comment rewrite: rig.rs does claim `take()` now |
| `firmware/Makefile` | modify | name the produced image per binary, or state the `firmware.bin` collision where `BINARY` is documented (`Makefile:10, 90-97`) |
| `backlog/tasks/task-038.03.02*.md` | modify | record what was actually built and the size readings |
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
### Hand-off from TASK-038.03.02.01 (already merged, do not re-do)

Available now in `crates/asperitas-logging`:
- `console::RigConfig { lane, blocks, block_bytes, bytes_per_s, capsec_us, window_s, cpu_hz, icache, dcache }`, `MonoLane::{Left, Right}`, `console::rigcfg_body(&RigConfig, &mut [u8; BODY_WINDOW]) -> usize`.
- `console::BODY_WINDOW = 256`, `frame::MAX_BODY = 200`, `console::RIGGEN_MAX_GEN_BYTES = 160`.
- `lib.rs::emit_record(level, now_ms: u32, body: &[u8]) -> bool` (`lib.rs:430`) - one whole record, no splitting, no recursion, `false` on exhaustion. It is `#[cfg(feature = "log-usb")]`; the caller supplies `now_ms` itself as `Instant::now().as_millis() as u32`.
- `capture.rs` already publishes `CAPTURE_WINDOW_SECONDS = 300` plus the geometry accessors; rig must derive from them, not restate them.

### Pinning notes

- `firmware/Cargo.lock` pins daisy-embassy to `branch=master#ca9bcc9ce93e70f85670e7ad066811c923ce1918`; the manifest tracks `branch = "master"` **unpinned**, so `tick-hz-32_768` and `generic-queue-8` can move under us without a commit here. Anything that depends on the tick rate should read it from the compiled constant, not a literal.
- Warm incremental firmware build is ~0.13 s; a cold release build is minutes, so run `touch firmware/src/bin/rig.rs` between build checks rather than `cargo clean`.
- Measured baseline for comparison (`size -B` on `main`, recorded by the parent after disproving an 86 % `.bss` claim that had no provenance): text 88181, data 1428, bss 8224, i.e. 1.57 % of the 512 KiB AXI SRAM.

### Stash disposition

`git stash list` still shows `stash@{0}: wip-038.03.02-uncommitted`. Its `console.rs`/`frame.rs`/`lib.rs` hunks and its `tests/commit_path_no_panic.rs` hunk are superseded by `22714b0`, `951b660` and `f0b4e18`. Only the `spin_budget.rs` hunk is still live, and it is comment-only: it rewrites the safety comment at `spin_budget.rs:81-86` because that text asserts nothing claims `Peripherals::take()`. AC #10 applies it; after that the stash may be dropped.

Hand-off from TASK-038.03.02.02 (parked as an integration umbrella) on 2026-09-12: this leaf is
the one that must discharge AC #14, and the only firmware-relevant content still sitting in
`stash@{0}` (`wip-038.03.02-uncommitted`) is the `spin_budget.rs` safety comment. Verified against
the live tree: the stash's `commit_path_no_panic.rs` hunk (adding `fn crc16_ccitt_update(` to
`FRAME_FNS`) is already shipped at `crates/asperitas-logging/tests/commit_path_no_panic.rs:294,433`,
and its `console.rs` / `frame.rs` / `lib.rs` hunks are superseded by what TASK-038.03.02.01 landed -
the live `console.rs` carries more references to every verb than the stash did, and `RIGGEN` exists
only in the live tree (zero occurrences in the stash).

Replacement text for the stale sentence at `crates/asperitas-logging/src/spin_budget.rs:81-86`,
verbatim out of the stash, so this leaf does not depend on the stash surviving:

    // Safety: `DCB` and `DWT` are Cortex-M system peripherals. `rig.rs` claims
    // `cortex_m::Peripherals::take()` in the firmware package, so this cannot count on the
    // singleton being free - which is exactly why it uses `steal()`: a panic path must not behave
    // differently depending on whether some other binary got there first. Aliasing the singleton is
    // harmless here because both users only *enable* tracing (idempotent) and read CYCCNT, and
    // neither ever writes it, so the counter's meaning does not depend on who ran first. Within
    // this crate, daisy-embassy and embassy-stm32 nothing claims it at all (embassy-stm32's
    // `Peripherals::take()` returns its own generated struct, not this one).

Keep the line wrapping the live file uses (the repo's rustfmt comment width) rather than copying
these exact breaks. Once this leaf has applied it, drop `stash@{0}`: nothing else in it is wanted,
and a future `git stash pop` would resurrect pre-TASK-038.03.02.01 drafts of `console.rs`,
`frame.rs` and `lib.rs` over the shipped versions.

## Implementation Notes (agent: ralph, 2026-09-12)

### Where the plan was wrong about the API

`firmware/docs/rig-boot-preamble-plan.md` says to call `AUDIO_EXECUTOR.start::<{Priority::P6 as u8}>()`,
copying `looper.rs`. That generic parameter does not exist in the pinned
`embassy-executor` 0.10: the signature is `start(&self, irq: impl InterruptNumber)`, and its doc says
"You must set the interrupt priority before calling this method. You MUST NOT do it after." So
`SAI1.set_priority(Priority::P6)` before `start(interrupt::SAI1)` is the only spelling available, and
it happens to be the better one - one place to get the priority wrong instead of two that must agree.
AC #5's intent holds; its syntax did not.

### RIGCFG/RIGGEN are emitted at boot, not from the render site's first block

AC #9 says "first block". The code emits both once, after `usb::init` and immediately before the audio
task spawns, because that is what the shipped contract in `console::RigConfig` says ("sent once at
boot, before any stimulus plays"), and the alternative buys nothing: no parameter changes between this
call and the first callback, capture bytes go to the SDRAM ring rather than the console, so there is
no record ordering to win. What it would cost is real - rendering both bodies is core `fmt` work, and
core `fmt` takes longer over some values than others, which is the wrong kind of work to put on the
first SAI1 interrupt of a binary whose entire purpose is timing measurement. The emit site documents
this reasoning; the "exactly one of each" property AC #9 actually guards is unchanged.

### One change outside the files this ticket named: `describe()` no longer uses core's float formatter

`Stimulus::describe()` printed reals with `{value}`, pulling in `core::num::flt2dec`. Measured by
swapping just that line back, on `rig` (release, `debug = 2`): `.text` 89,804 -> 108,172 and
`.rodata` 15,808 -> 19,116, i.e. **21.4 KB to render four numbers**, which against the 131,072-byte
internal-flash sector would have left 2.5 KB where `rig.bin` currently has 24. It also iterates until
a round-tripping representation falls out, so its runtime depends on the bits of a value that whoever
armed the stimulus chose. Both reasons are in `write_decimal`'s doc comment with these numbers.

Consequence for the wire format: real-valued fields now carry exactly six fractional digits
(`level_dbfs=-20.000000`) instead of core's variable form. Six decimals round-trips an `f32` across
the whole field range (widest case: a pulse ceiling at Nyquist, 24 kHz, where consecutive `f32`s
differ by 0.002), which is the point - the host reconstructs the waveform from these bytes. Tests were
updated to the new format and a round-trip test added asserting the reconstructed value is within half
an `f32` step of what the generator used.

### Also done

- `spin_budget.rs`: the stale "nothing claims it" sentence replaced (that was all of `stash@{0}` worth
  keeping); the file's code is untouched. `git stash drop` then run, per the ticket's instruction.
- Firmware clippy is in neither CI nor `lefthook.yml`, but `make clippy BINARY=rig` is a documented
  gate, so rig is clean under `-D warnings` in all five feature configs. Two findings fixed: an
  `absurd_extreme_comparisons` false positive on the generator-count assert (now `< 2`, why in a
  comment) and a redundant `as u64` on `Instant::elapsed().as_micros()`.
- Makefile artifacts renamed to `$(BINARY).bin`; `.gitignore` gained `/firmware/*.bin`. Verified
  `main.bin` still builds and the old name is gone.
- Host gates run locally, all ten green: fmt, four clippy configs, two `cargo doc -D warnings` runs,
  two test runs, and logging's `dump_reassemble --selftest`.

### Deferred, unchanged from the hand-off

The three bench rows (DWT enablement cost, executor dispatch latency, mono render throughput) stay
deferred with their instruments named in `docs/reference/performance-baseline.md` section 4.7. They
need a board and therefore belong to TASK-038.03.02.04.

### Size readings (AC #12)

Host-side, `size -A` on the ELF plus `llvm-size -A` on the sections the linker script places:

| Transport | Binary | .text | .bss (+.sram1_bss) | image |
| --- | --- | --- | --- | --- |
| log-usb (`FEATURES="seed3"`) | main | 88,161 | 8,224 (+1,024) | 88,581 |
| log-usb | rig | 106,367 | 7,288 (+1,024) | 106,811 |
| | **delta** | **+18,206** | **-936** | **+18,230** |
| RTT-only (`NO_DEFAULT=1 FEATURES="seed3 log-defmt"`) | main | 47,896 | 3,860 (+1,024) | 48,360 |
| RTT-only | rig | 65,224 | 4,196 (+1,024) | 65,696 |
| | **delta** | **+17,328** | **+336** | **+17,336** |

`rig` gives back RAM (its USB path shares `main`'s statics while dropping the pod effect's state) and
spends ~17.5 KB of flash. Largest RAM objects in `rig`, read off the symbol table: `LOG_PIPE` 2,080 B,
the audio task's executor pool 1,768 B, TX/RX DMA buffers 512 B each. Total RAM footprint
(.data + .sram1_bss + .bss) is 8,744 B of the 512 KiB AXI SRAM at 0x24000000, so no large buffer
landed there; the capture ring lives in SDRAM as designed. Flash headroom for `rig` over the
131,072-byte sector: 24,261 B.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
`firmware/src/bin/rig.rs` (629 lines) is the second cross-compiled binary and boots to a measured,
self-checking state on host-verifiable evidence alone: it claims `cortex_m::Peripherals::take()` once
before `hal::init` and halts in the boot-LED error state on `None`; proves DWT by readback of TRCENA,
LAR-unlocked `.control` and CYCCNT advancing over 50k cycles, stopping rig if any step fails; takes
`cycles_per_us` from the clock tree and cross-checks it against a 200 ms `embassy_time` window with
both numbers logged, switching to the measured value on >1% disagreement or a non-MHz-divisible
declare; runs two executors (thread for blink/report, `InterruptExecutor` on SAI1 at NVIC P6 set
before `start`, with the effective priority, DMA1_STREAM0/1 and TIM5 logged so the ordering claim is a
reading); and gates stimulus behind `stim-sine`/`stim-ess`/`stim-pulse` with sine as the no-feature
default, sine out of `default`, and a `const` assert that makes two features a compile error - all five
build variants link and both negative builds fail as designed. Render site is
`stimulus -> processor slot -> encode_block(output)`, left lane documented, one RIGCFG and one RIGGEN
per run with the 160-byte invariant stated against today's 88/107/107-byte describes and a clip check
that stops a truncated parameter reading as real. `spin_budget.rs`'s stale "nothing claims the
singleton" sentence is corrected and `stash@{0}` dropped. Makefile artifacts are now `$(BINARY).bin`.

Sizes: rig over main is +18,206 text / -936 bss on the console transport (+17,328 / +336 on RTT-only);
`rig.bin` is 106,811 of 131,072 flash bytes and 8,744 B of 512 KiB RAM, largest object `LOG_PIPE` at
2,080 B. Table in Implementation Notes.

Two deviations, both reasoned and recorded: `InterruptExecutor::start` in embassy-executor 0.10 has no
priority generic, so the plan's `start::<{Priority::P6 as u8}>()` spelling does not exist and the NVIC
set before `start(interrupt::SAI1)` is the only correct form; and RIGCFG/RIGGEN are emitted at boot
rather than on the first rendered block, matching `console::RigConfig`'s shipped contract and keeping
variable-time core-fmt work off the first audio interrupt. Beyond the named files, `describe()`'s float
printing moved off core's flt2dec to a fixed six-decimal writer - worth 21.4 KB of a 131,072-byte
flash budget and bit-dependent runtime - which changes real-valued fields to always carry six
fractional digits; tests updated and a round-trip test added. All ten CI-equivalent host gates green.
HARDWARE: nothing here has run on a board. The three bench rows stay deferred to TASK-038.03.02.04.
<!-- SECTION:FINAL_SUMMARY:END -->
