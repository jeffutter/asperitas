---
id: TASK-038.03.02
title: >-
  Build firmware/src/bin/rig.rs: interrupt-executor audio, stimulus playback,
  SDRAM capture ring, console dump
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-11 13:28'
updated_date: '2026-09-11 15:27'
labels:
  - task
  - planned
dependencies:
  - TASK-038.03.01
modified_files:
  - firmware/src/bin/rig.rs
  - firmware/Cargo.toml
  - .github/workflows/ci.yml
  - crates/asperitas-logging/src/console.rs
  - crates/asperitas-logging/src/lib.rs
  - docs/reference/daisy-seed3.md
parent_task_id: TASK-038.03
priority: high
type: task
ordinal: 83500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
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
- [ ] #2 Audio runs on a dedicated InterruptExecutor pended on SAI1 with embassy-executors executor-interrupt feature enabled, while the USB drain, LED blink task, CAPSTAT emitter and dump writer stay on the thread executor. The priority assignment is stated in code, the comment cites upstream examples/looper.rs lines 27-32 and 129-134 at the pinned commit ca9bcc9, and records why the SAI1 vector is free (the audio driver binds DMA1_CH0/CH1, audio.rs:26-28). The numeric relationship between the DMA IRQ priority and the executor priority is checked against embassy-stm32 0.6.0 and the result recorded either way.
- [ ] #3 Stimulus kind is a compile-time selection via cargo features stim-sine, stim-ess and stim-pulse, with sine at -20 dBFS when nothing else is set, and mutually-exclusive selection enforced by a const assert. CI builds all four combinations so none can rot. The device emits exactly one RIGCFG record whose payload embeds the generators own describe() output plus sample rate, capture format, block geometry and cpu_hz, so no second description grammar exists.
- [ ] #4 Input capture stores the loop channel as 16-bit mono into the ring defined by asperitas_logging::capture, publishing each block through Filling -> Full -> Dumping -> Free using that modules transition table, with samples written before the index that publishes them. The producer writes only blocks it found Free; when none are free it stops capturing and increments a visible overrun counter instead of overwriting a block being dumped. A const assert ties capture::FRAMES_PER_CALLBACK to daisy_embassy::audio::BLOCK_LENGTH, compared in samples rather than bytes: HALF_DMA_BUFFER_LENGTH counts 64 u32 words per callback (32 stereo frames) while CALLBACK_BYTES counts 64 bytes of one mono channel, so asserting those two figures equal would pass by coincidence and comparing either against HALF_DMA_BUFFER_LENGTH * 2 could never pass at all.
- [ ] #5 Per-callback work is bounded to one contiguous copy plus lane truncation. DWT cycle-counter instrumentation reports worst-case callback duration and longest inter-callback gap, guarded for the Seed3 dual-core case and reporting zeros honestly when CYCCNT is unavailable. A periodic CAPSTAT record carries delivered blocks, expected blocks, capture overruns, max_block_us, worst_gap_us, dump progress and the transports dropped_full, giving hardware verification two independent starvation signals.
- [ ] #6 The device reports CAPMAX total_bytes ring_bytes seconds_max unused_headroom_bytes computed at runtime from sdram::SDRAM_SIZE and the published ring geometry, so capturable duration is measured from the driver constant rather than guessed, and the headroom statement makes clear that live audio DMA buffers remain in internal RAM.
- [ ] #7 RIGCFG, CAPSTAT, CAPMAX and the dump-summary verb are implemented as body builders in crates/asperitas-logging/src/console.rs beside status_body, each pinned by a host unit test in that file, and committed through one public whole-record entry point modelled on the existing emit path. Nothing in the dump or status path calls usb::emit_blocking.
- [ ] #8 The dump writer obtains permission to enqueue from dump::try_emit_dump, which consults the TASK-038.02 capacity predicate, and never bypasses it; refusals are retried on a Timer backoff rather than a busy-wait, and both the refusal count and the longest consecutive stall are counted and reported. Ordinary log and STATUS traffic stays lossless during a dump.
- [ ] #9 Capture start and end are decided by the device: it captures for a build-time-configured window or until the ring reports full, then begins the dump on its own, because runtime control over the console link belongs to TASK-032.
- [ ] #10 When a dump finishes the device emits one DUMPEND record naming blocks, chunks, bytes, elapsed milliseconds and the transport loss counters at that moment, so a caller times and validates a transfer from the captured stream alone.
- [ ] #11 The SDRAM memory model is recorded where a future reader will hit it: init() returns 0xC000_0000 while the driver programs its cacheable MPU region at 0xD000_0000, caches are enabled nowhere in the stack so accesses are uncached and coherent today, nothing here enables caches or changes the MPU base, and the open question is handed to TASK-038.05 with a short factual note added to docs/reference/daisy-seed3.md section 4.
- [ ] #12 Rate arithmetic is gated by const asserts in rig.rs, not prose: the capture window provably fits the ring, and CAPSTAT traffic is provably below one percent of the dumps own record traffic.
- [ ] #13 cargo fmt --all --check, cargo test --workspace, cargo clippy --workspace --all-targets -- -D warnings and every stimulus feature cross-build pass, and the finalization notes record the release size plus the .bss delta against the current 86.13 percent baseline.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Read before writing anything

Authoritative sources, in this order:

- Upstream example at **the commit this build actually pins** — `~/.cargo/git/checkouts/daisy-embassy-4e2531dd3689e74c/ca9bcc9/examples/looper.rs`. Lines 27-32 are the executor declaration, 38-51 the SDRAM carve, 63-92 the callback registration, 129-134 the start sequence. There is **no crates.io `daisy-embassy-0.0.2` tree present on this machine**; anything quoting `init_interrupts()` or `Priority::P1` is describing a different version and is wrong. `firmware/Cargo.lock:228-230` resolves `0.2.3` at `ca9bcc9`, and `sdram.rs:9` there reads `SDRAM_SIZE = 64 MiB`.
- `docs/reference/daisy-seed3.md` §4 (FMC/MPU/sync-DMA) and §5 (pin banks). Its hardware table line 17 confirms Seed3 carries 64 MB SDRAM, so `SDRAM_SIZE` is right and the ring below deliberately uses half the chip.
- `firmware/src/bin/main.rs` for the boilerplate each binary duplicates (tracked by TASK-010 — copy the shape, do not refactor five binaries as a side quest).
- `crates/asperitas-logging/src/{dump.rs,console.rs}` for the commit path and the verb convention.

## 1. Cargo and CI

`firmware/Cargo.toml`:

```toml
embassy-executor = { version = "0.10.0",
  features = ["platform-cortex-m", "executor-thread", "executor-interrupt"] }

[features]
stim-sine  = []          # default when nothing else is selected
stim-ess   = []
stim-pulse = []
```

No `[[bin]]` section is needed — `rig.rs` is auto-discovered. Guard against two stimulus features at once with a `const _: ()` assert *in the source*, since cargo cannot express mutual exclusion.

`.github/workflows/ci.yml`: every default-off feature combination must compile or the branch is red, because both CI and lefthook build with no `--bin`. Add after the existing cross-build step:

```yaml
- name: Build rig firmware (each stimulus selection)
  run: |
    cd firmware && cargo build --release --features seed3
    cd firmware && cargo build --release --features seed3,stim-sine
    cd firmware && cargo build --release --features seed3,stim-ess
    cd firmware && cargo build --release --features seed3,stim-pulse
```

## 2. Record verbs belong to `console.rs`, not to the binary

`RIGCFG`, `CAPSTAT`, `CAPMAX` and the dump-summary verb are wire grammar. The convention recorded at `console.rs:198-201` is that a verb's field set and order are a contract *pinned by a unit test in the same file* (`status_body_pins_field_names_and_order` at `console.rs:318` is the model). Bodies written ad hoc in `rig.rs` would have no host test at all, since the firmware package has no host-test target.

So: add `xxx_body(...)` builders to `crates/asperitas-logging/src/console.rs` next to `status_body`/`boot_body`, each with its own pinning test, and expose one public whole-record commit entry point in `lib.rs` shaped like the existing private `emit_status` — take `RECORD_BUFS`, pre-check capacity, commit via `frame::write_whole`. Call it `emit_record(level, now_ms, body) -> bool`. Do not reach for `usb::emit_blocking`: it drives the CDC endpoint directly while `usb::run()`'s drain task owns it, it exists for the case where the executor is dead, and TASK-046/TASK-047 are exactly the failure class produced by long-lived work inside masked contexts.

Field sets (fixed order, space-separated, absolute since-boot counts — the host owns differencing):

```
RIGCFG  stim=<describe()> rate=<hz> capture=mono16 block_bytes=<n> blocks=<n>
        bytes_per_s=<n> capsec=<n> cpu_hz=<hz>
CAPSTAT delivered=<n> expected=<n> overrun=<n> max_block_us=<n> worst_gap_us=<n>
        dumping=<n> free=<n> sent=<n> dropped_full=<n> bytes_dropped=<n>
CAPMAX  total_bytes=<n> ring_bytes=<n> seconds_max=<n> unused_headroom_bytes=<n>
DUMPEND blocks=<n> chunks=<n> bytes=<n> elapsed_ms=<n> sent=<n> dropped_full=<n> bytes_dropped=<n>
```

`RIGCFG` carries the stimulus description straight out of `Stimulus::describe(&self, out: &mut [u8]) -> usize` (`stimulus.rs:122`) plus `cpu_hz`, because DWT counts CPU cycles and the host cannot decode `max_block_us` without knowing the clock. One `describe()` call, one grammar — AC #3's reason for existing.

## 3. Boot sequence and peripheral claims

Under the `seed3` feature, `board.rs:221-234` calls `cortex_m::Peripherals::take()` and **panics if something else already claimed it**. Nothing in this repo calls it today, and this ticket claims it first, in `rig.rs`, before `new_daisy_board!`. Put a comment there saying so: any future binary that takes it earlier breaks `rig.rs` at boot, not at compile time.

Destructure rather than move the whole struct, because `sdram.build()` wants `&mut MPU, &mut SCB` while DWT stays useful:

```rust
let mut cp = cortex_m::Peripherals::take().unwrap();
// ... later, in the audio task:
let sdram = board.sdram.build(&mut cp.MPU, &mut cp.SCB);
```

DWT needs the coprocessor enabled and Seed3 is a dual-core part where CYCCNT is only available when the second core is fused off:

```rust
if cp.DWT.cyccnt_read() != 0 || (DCB.cpacr.read() as u32 & CLUSTERLITEN) == 0 {
    cp.DWT.enable_cycle_counter();   // cortex-m 0.7.7, dwt.rs:125
}
```

Read the counter with `DWT::get_cycle_count()` (an associated function, `dwt.rs:151`). If the guard fails, report `max_block_us=0 worst_gap_us=0` and say so in `RIGCFG` rather than reporting numbers that are not measurements.

Then, following `main.rs:178-236`: `default_rcc()` → `hal::init` → `new_daisy_board!(p, Type::Pod)` → drop `board.qspi`, discard `board.usb_peripherals` → `led::init(board.d20, board.d19, board.d18)` → `asperitas_logging::usb::init(UsbIrqs)` → spawn the drain and blink tasks → `board.audio_peripherals.prepare_interface(Default::default()).await`. Take `cpu_hz` from the clocks object `hal::init` returns; never hardcode 600 MHz.

## 4. Executor topology

Copy `looper.rs` verbatim in shape:

```rust
static AUDIO_EXECUTOR: InterruptExecutor<1> = InterruptExecutor::new();

#[interrupt]
unsafe fn SAI1() { unsafe { AUDIO_EXECUTOR.on_interrupt() } }
```

This is safe only because the audio driver binds `DMA1_CH0`/`DMA1_CH1` and never `SAI1` (`audio.rs:26-28`), which leaves the `SAI1` vector unclaimed. State that in the comment — it is the non-obvious part of AC #2.

```rust
interrupt::SAI1.set_priority(Priority::P6);           // looper.rs:132
let spawner = AUDIO_EXECUTOR.start(interrupt::SAI1);
spawner.spawn(run_audio(interface, sdram, ...));      // thread executor keeps drain + blink + dump
```

Any enabled NVIC interrupt preempts thread mode, so P6 gives structural preemption of the dump writer regardless of the exact number. Two things to verify rather than assume, and to record either way:

1. **DMA priority must stay numerically below P6**, so the DMA completion ISR can still wake the audio task promptly. Check `embassy-stm32-0.6.0/src/dma/mod.rs` for the priority assigned to `DMA1_CH0`/`CH1`. If it is not strictly higher urgency than P6, pick a priority strictly between them and update the comment.
2. **Task-pool sizing.** `embassy-executor` 0.10 sizes pools from `max-tasks-*` features; the audio task's future is large because it owns the `Interface`, the `SdRam`, and the ring cursor. If `spawn` returns `TaskPoolOverflow`, bump the per-executor pool in `[features]` and note the RAM cost.

If `Interface<'a>` will not unify with `'static` when moved into the spawned task under our manual `#[no_mangle] async fn main` (upstream uses `#[embassy_executor::main]`), leak it through the `StaticCell` already used elsewhere in this repo rather than reaching for `Box`.

**The audio callback must never log.** `RECORD_BUFS` is a `CriticalSectionRawMutex`, i.e. PRIMASK. A producer running at P6 that takes a lock held by the thread-mode dump writer spins forever with interrupts masked. The callback's entire outward surface is SDRAM writes and atomic stores. This is also why `CAPSTAT` is emitted from the thread executor by reading atomics.

## 5. Stimulus selection

Output is the stimulus alone; the input frame is ignored. Keep the render site narrow and shaped like `stimulus → [processor slot] → encode_block(output)`, because TASK-019.03 will insert the effect chain there and should need one line, not a restructure.

`Sine::default()` is already `-20 dBFS at 1 kHz` — verified: the impl ends with `s.apply(&SineParams::default())` (`stimulus.rs:266-272`), and `SineParams::default()` is `{ level_dbfs: -20.0, frequency_hz: 1_000 }`. Do not "correct" the level.

Call `set_sample_rate(hz)` once, before audio starts. For `ExponentialSweep`, note in a comment that `apply()` peak-normalises by scanning **every** sample of the record (`stimulus.rs:540-546`) — cheap at boot, catastrophic in a callback. Default sweep parameters give roughly a 6 s record; `PulseTrain::default()` is one band-limited pulse, so the ESS or pulse runs need a capture window that covers their record length, which `RIGCFG` makes visible to the caller.

Truncate to int16 lanes with `(sample.clamp(-1.0, 1.0) * 32767.0) as i16`; document that truncation is deliberate and undithered (TASK-038.01 left dither out until the noise floor is characterised).

## 6. Capture producer (runs at P6)

Carve the ring exactly as `looper.rs:44-51` does — `sdram.init(&mut delay)` returns `0xC000_0000`, cast to `*mut u8`, then `slice::from_raw_parts_mut` over `capture::RING_BYTES`. No linker-script change: `memory.x` keeps claiming FLASH 128 K and RAM 512 K only. **Keep the `SdRam` value alive for the program's lifetime** by owning it in the audio task; dropping it releases ~55 pins at the type level and lets someone claim them twice.

State:

```rust
static BLOCK_STATE: [AtomicU8; capture::RING_BLOCKS] = ...;   // 1 KiB of .bss
static WRITE_BLOCK: AtomicUsize;                              // producer's cursor
static PRODUCED: AtomicUsize;                                 // published high-water mark
static OVERRUN: AtomicUsize;
```

Per callback, bounded to exactly one contiguous copy plus lane truncation (AC #5):

1. Read `BLOCK_STATE[write_block]`. Anything other than `Free` ⇒ `OVERRUN += 1`, stop capturing (leave the block alone; never overwrite one being dumped), and return.
2. Store `Filling`.
3. Truncate the 64 `u32` lanes to 32 `i16` samples and copy them into the block's byte range at `callback_index_in_block * CALLBACK_BYTES`.
4. On the 512th callback: `core::sync::atomic::fence(Ordering::Release)`, then store `Full`, advance `write_block`, `PRODUCED.fetch_add(1, AcqRel)`.

Ordering discipline, stated in code: samples are written before the index that publishes them (Release store / `fence`), and the consumer loads state with Acquire before reading bytes. Use `core::sync::atomic` here, not `main.rs:63-90`'s `UnsafeCell` + `interrupt::free` idiom — that shape is adequate for one slow knob writer and one fast reader, and wrong for publishing buffer ownership across executors.

Capture window: `const CAPTURE_SECONDS: u64 = 300;` (overridable at build time via `option_env!` so TASK-038.05 can shorten bench runs without editing source). Capture ends at the window deadline **or** when the ring reports full, whichever comes first, and the dump begins on the device's own decision — AC #10, because runtime control belongs to TASK-032.

Close the loop on the copied driver constant. Compare **sample counts**, not bytes: `daisy_embassy::audio::InterleavedBlock` is `[u32; HALF_DMA_BUFFER_LENGTH]`, i.e. 64 words for 32 stereo frames, while `CALLBACK_BYTES` is 64 **bytes** of one mono channel — two different quantities that happen to share a number. The earlier form of this assert, `CALLBACK_BYTES == HALF_DMA_BUFFER_LENGTH * 2`, works out as 64 == 128 and cannot compile.

```rust
const _: () = assert!(asperitas_logging::capture::FRAMES_PER_CALLBACK
                      == daisy_embassy::audio::BLOCK_LENGTH);
```

## 7. Dump writer (thread executor)

Walk blocks in ring order from `0` to `PRODUCED`, and for each:

1. `compare_exchange(Full, Dumping)` — if it fails the block is not ours; continue.
2. For `c in 0..chunks_per_block()`: build the body with `dump::audio_body(block_index, chunks, c, raw, &mut body)` and retry `dump::try_emit_dump(&body[..len])` until it returns true, waiting `Timer::after_ms(1)` between refusals. Count refusals and track the longest consecutive refusal streak as `dump_stalls` / `dump_stall_ms` for `CAPSTAT`. Never bypass `dump_fits` — that predicate is what keeps ordinary log and STATUS traffic lossless during a dump (AC #8).
3. Emit `AUDEND` via `dump::audend_body`.
4. `store(Free, Release)`.

Yielding at the `Timer` await is what keeps the USB drain running; a busy-wait here starves the very task that frees pipe capacity.

At the end, emit one `DUMPEND` naming blocks, chunks, bytes, elapsed milliseconds, and `CONSOLE.snapshot()`'s loss counters at that instant (AC #11), so a caller can time and validate a transfer from the captured stream alone.

Dump wall-time is a prediction, not a fact: a full 349.5 s capture produces ≈59 MB of wire traffic, which needs 169 kB/s just to keep pace with real time and lands around 64 s at a best-case full-speed bulk rate. Say that in a comment and leave the measurement to TASK-038.05.

## 8. Rate gates, as arithmetic not prose

`podtest.rs:117` proves its logging rate fits the link with a `const` assert. Do the equivalent here, in `rig.rs` where both the driver constants and `capture::` are visible:

```rust
const _: () = assert!(capture::RING_BLOCK_BYTES % capture::CALLBACK_BYTES == 0);
const _: () = assert!(capture::expected_blocks(CAPTURE_SECONDS as usize) < capture::RING_BLOCKS);
// CAPSTAT must stay beneath one percent of the dump's own traffic:
const _: () = assert!(MAX_CAPSTAT_BODY * 100 < capture::records_per_block() * dump::FULL_AUDIO_FRAME_LEN);
```

The last one is relative on purpose: the link ceiling is unmeasured, so the honest gate is that status traffic cannot dominate the dump, not an invented baud figure.

## 9. SDRAM memory model — record it, do not fix it

`sdram.init()` returns `0xC000_0000` (`stm32-fmc` maps bank 1 to `FmcBank::Bank5`), while the driver programs its single cacheable MPU region at `0xD000_0000` (`sdram.rs:33`) — the other bank's window, where nothing is connected. Caches are enabled nowhere in daisy-embassy, embassy-stm32 0.6.0, or the cortex-m-rt startup, so every access is uncached and coherent today by construction, and the mis-set region is inert but misleading. **Do not quietly correct the base address**: that turns on read caching and changes the coherence argument for every DMA master.

Write a code comment stating all of the above, hand the open question to TASK-038.05, and add a short factual note to `docs/reference/daisy-seed3.md` §4 near the existing FMC/MPU discussion. Budgets and workflow prose belong to TASK-038.06, not here.

Also record in the comment: neither the driver nor `stm32-fmc` self-tests SDRAM (`sdram.rs:103` drops the handle), so the first evidence the part is alive is this ticket's first successful capture — which is why `CAPMAX` prints before any block is trusted.

## 10. Verification ladder

Run in this order; paste the outputs into the finalization notes.

```
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd firmware && cargo build --release --features seed3            # and each stim-* variant
git diff --name-only HEAD                                          # must not list main.rs or podtest.rs
```

Record the release size output and the `.bss` delta against the current 86.13% baseline (≈69 KB free). New internal-RAM cost is the 1 KiB state array plus two caller buffers (`MAX_BODY` + `MAX_FRAME`) and the DWT scratch — well under that, but write down the measured number rather than the estimate.

Nothing in this ticket may be marked done on the strength of a green build. Correctness claims that need ears, a board, or a cable live in TASK-038.05 (`@human`) and are not satisfied here.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Interface facts from TASK-038.03.01, now published in asperitas_logging::capture (measured there, so this ticket need not rediscover them):

- expected_blocks takes usize and rounds UP: expected_blocks(300) == 879, not the 878 that appeared in the planning prose. This ticket's `capture::expected_blocks(CAPTURE_SECONDS as usize)` snippet compiles as written; do not "fix" the cast to u32.
- Its const assert should read capture::RING_BLOCK_BYTES.is_multiple_of(capture::CALLBACK_BYTES), not `% .. == 0`: manual_is_multiple_of is denied under -D warnings wherever root workspace lints apply. firmware/ inherits none of them today, so the % form would build — it is style debt here, not a failure.
- The FRAMES_PER_CALLBACK vs daisy_embassy::audio::BLOCK_LENGTH assert compares samples (32 == 32). CALLBACK_BYTES (64 bytes of one mono channel) and HALF_DMA_BUFFER_LENGTH (64 u32 words per callback) coincide numerically and must not be compared.
- Block state hand-off lives in capture::BlockState / capture::transition_ok: exactly four legal edges (Free->Filling, Filling->Full, Full->Dumping, Dumping->Free), no self-transitions, and BlockState::from_u8 returns Option — an unrecognised status byte is None, never Free. Store these in [AtomicU8; capture::RING_BLOCKS]; AtomicU8 is lock-free on ARMv7-M (LDREXB/STREXB), so portable-atomic stays out.
- Ring geometry: RING_BLOCK_BYTES 32_768, RING_BLOCKS 1_024, RING_BYTES 33_554_432 (half the Seed3's 64 MB SDRAM, BYTES_PER_SECOND 96_000, 349 s floor / 349,525,333 us exact, 255 chunks + 1 AUDEND = 256 records per block, 57,788 wire bytes per block (exact, encoder-confirmed).
EOF
)
<!-- SECTION:NOTES:END -->
