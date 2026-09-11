---
id: TASK-038.03
title: >-
  Build rig firmware binary: interrupt-executor audio, stimulus playback, SDRAM
  capture ring
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 11:37'
updated_date: '2026-09-11 01:29'
labels: []
dependencies:
  - TASK-038.01
  - TASK-038.02
documentation:
  - docs/reference/daisy-seed3.md
modified_files:
  - firmware/src/bin/rig.rs
  - firmware/Cargo.toml
  - .github/workflows/ci.yml
parent_task_id: TASK-038
priority: high
type: task
ordinal: 57500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Nothing in the firmware today generates stimulus or records the return: `main.rs` runs a filter/gain chain, `podtest.rs` never touches audio at all. This ticket creates the binary that closes the measurement loop, and it deliberately creates a **new** binary rather than mutating either one.

Why not grow `podtest.rs`: it owns no audio peripherals, its `main` awaits a flat two-future select where `main.rs` uses a nested three-future one, and its output vocabulary is pinned by a human-verified criterion in TASK-018.04. Turning it into a mode rig means restructuring the thing a person already signed off on, for no gain. New file, same conventions. Why not grow `main.rs`: it is the shipping effect binary, and a measurement harness that can corrupt live audio does not belong inside the product image.

Three things happen here.

**Audio moves to its own interrupt executor.** Today audio, USB drain, and LED blink share one cooperative executor through nested `select`, so any burst of work in one poll delays the callback directly and "the dump cannot starve audio" is a promise about discipline rather than a property of the schedule. Upstream `examples/looper.rs` already shows the fix: an `InterruptExecutor` bound to `SAI1` with the audio `Interface` spawned onto it. Adopting that shape in `rig.rs` makes preemption structural, at the cost of enabling embassy-executor's `executor-interrupt` feature and putting shared state behind atomics.

**Capture writes 16-bit mono into a fixed SDRAM ring.** One channel, because the loopback is one signal path and every extra byte here costs dump time on a full-speed link. Blocks are 32 KiB with explicit `filling -> full -> dumping -> free` ownership published by atomic indices: samples are written before the index that publishes them, the consumer claims only `full` blocks, and the producer never writes a block that is not `free`. When nothing is free it stops capturing and counts an overrun rather than overwriting a block mid-dump, which is the invariant AC #2 asks for and the failure mode its wording warns about.

**Starvation gets measured instead of asserted.** There is no audio-overrun counter in this repo at all, so the parent's "drop counter stayed at zero" claim would be about console drops only. `rig.rs` therefore reports worst-case callback duration and longest inter-callback gap from the DWT cycle counter, alongside delivered-versus-expected block counts, so hardware verification in TASK-038.05 has two independent numbers rather than one misleading one.

Stimulus type and level are compile-time selections (cargo features, sine at −20 dBFS as the no-feature default) because the host-initiated control channel is TASK-032 and this ticket must not queue behind it; the device emits one `RIGCFG` record built from the stimulus module's own `describe()` string so the host always knows exactly what was played.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `firmware/src/bin/rig.rs` exists, builds under CI's `cargo build --release --features seed3`, and `firmware/src/bin/main.rs` and `podtest.rs` are untouched, so the human-verified podtest output contract from TASK-018.04 cannot regress.
- [ ] #2 Audio runs on a dedicated `InterruptExecutor` bound to `SAI1` with embassy-executor's `executor-interrupt` feature enabled, while the USB drain, LED task, and dump writer stay on the thread executor; the interrupt priority assignment is stated in code and the rationale cites the upstream `looper.rs` precedent, so dump work is preempted by audio structurally rather than by convention.
- [ ] #3 Stimulus type and level are compile-time selections (a cargo feature per stimulus kind, sine at −20 dBFS when no feature is set), every non-default combination is built by CI so it cannot rot, and the device emits exactly one `RIGCFG` record whose payload is the stimulus module's own `describe()` string plus sample rate, capture format, and block size, so no second description grammar exists.
- [ ] #4 Input capture stores the loop channel as 16-bit mono into a fixed 32 MiB SDRAM ring of 1,024 blocks of 32 KiB at 96,000 bytes/s, publishing each block through `filling -> full -> dumping -> free` with atomic indices and release-before-publish ordering; the producer writes only blocks it found `free`, and when none are free it stops capturing and increments a visible overrun counter instead of overwriting a block being dumped.
- [ ] #5 Per-callback work is bounded to one contiguous copy plus lane truncation, and DWT cycle-counter instrumentation reports worst-case callback duration and longest inter-callback gap; a periodic `CAPSTAT` record carries delivered block count, expected block count, `max_block_us`, `worst_gap_us`, capture overruns, dump progress, and the transport's `dropped_full`, giving hardware verification two independent starvation signals rather than one.
- [ ] #6 The device reports `CAPMAX total_bytes ring_bytes seconds_max unused_headroom_bytes` computed at runtime from `sdram::SDRAM_SIZE` and the published ring geometry, so maximum capturable duration is measured from the driver constant rather than guessed by the caller, and the headroom statement makes clear that live audio DMA buffers remain in internal RAM.
- [ ] #7 A host unit test derives ring geometry and wrap arithmetic from the same public constants the firmware uses — 512 callbacks per block, 219 chunks per block, 1,024 blocks, 349.5 s ring capacity — so an off-by-one in geometry fails CI with no board attached.
- [ ] #8 The dump writer obtains permission to enqueue from TASK-038.02's capacity policy function and never bypasses it, which is what keeps ordinary log and status traffic lossless during a dump.
- [ ] #9 The SDRAM memory model is recorded where a future reader will hit it: `init()` returns 0xC000_0000 while the driver programs its cacheable MPU region at 0xD000_0000, caches are enabled nowhere in the stack so accesses are uncached and coherent today, nothing here enables caches or changes the MPU base, and the open question is handed to TASK-038.05 with the SDRAM and QSPI budgets written into `docs/reference/daisy-seed3.md`.
- [ ] #10 Capture start and end are decided by the device, not by a host command channel this ticket does not have: it captures for a build-time-configured window or until the ring reports full, then begins the dump on its own, because runtime control over the console link belongs to TASK-032 and waiting for it would stall the measurement chain for an unrelated reason.
- [ ] #11 When a dump finishes the device emits one summary record naming blocks, chunks, bytes, elapsed milliseconds, and the transport loss counters at that moment, so a caller times and validates a transfer from the captured stream alone; that verb joins the record grammars TASK-038.06 documents.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Geometry the numbers fall out of

- `daisy_embassy::audio::BLOCK_LENGTH = 32` frames, `HALF_DMA_BUFFER_LENGTH = 64` u32 words; the callback signature is `FnMut(&[u32], &mut [u32])` (`start_callback`, audio.rs:157-172) and fires once per 32 stereo frames, i.e. every **667 µs at 48 kHz**. Block length is hardcoded, not configurable via `AudioConfig` (documented in `docs/reference/rust-daisy-stack.md`). Capture granularity must be a multiple of 32 frames.
- Codec lanes are u32, 32-bit left-justified in a 64-bit SAI frame (`docs/reference/daisy-seed3.md`), so capture is a shift-and-truncate of raw lanes, no float round trip. `main.rs:92-116` (`decode_block` / `encode_block`) is the existing conversion reference.
- Ring arithmetic, exact: block = 32,768 B = 16,384 int16 samples = **341.33 ms**, filled in exactly **512 callbacks**; ring = 32 MiB = **1,024 blocks** = 16,777,216 samples = **349.5 s** of capture, which clears the five-minute run TASK-019.03 AC #2 asks for with about fifty seconds spare. Dump side: 32,768 / 150 = **219 `AUDIO` chunks plus one `AUDEND` per block**.
- Footprint to publish: **96,000 B/s** mono 16-bit (and 192,000 B/s if anyone later stores 32-bit lanes — do not make that a runtime option, change the constant and let the geometry test recompute).
- Wire cost prediction: 96,000 B/s of capture needs 640 records/s and roughly 146 kB/s on the wire at 150-of-228 efficiency, against an unmeasured full-speed-CDC ceiling. Full-speed bulk is commonly reported near 900 KiB/s best case; this repo's drain pulls `DRAIN_BUF_SIZE = 256` per wakeup. Treat predictions as predictions until TASK-038.05 measures them.

## SDRAM facts verified in the vendored driver (commit ca9bcc9)

- `board.sdram` is a `SdRamBuilder<'a>` field of `DaisyBoard` carrying `FMC` plus about 55 GPIO `Peri`s; `build(self, mpu: &mut MPU, scb: &mut SCB) -> SdRam<Fmc<'a, FMC>, FmcDevice>` consumes it and needs `cortex_m::Peripherals::take()`, which **nothing in this repo calls today**, so it is free to claim here.
- `sdram.init(&mut delay) -> *mut u32` returns **0xC000_0000** (`stm32-fmc` maps SDRAM target bank 1 to `FmcBank::Bank5` = 0xC000_0000, which is also what libDaisy and every linker script use). The driver's single cacheable MPU region is programmed at **0xD000_0000** (`sdram.rs:33`), the *other* bank's window, where nothing is connected. No caches are enabled anywhere in daisy-embassy, embassy-stm32 0.6.0, or cortex-m-rt startup, so every SDRAM access is uncached and coherent today by construction, and the mis-set region is inert but misleading. Do not quietly "fix" the base address: doing so turns on read caching and changes the coherence argument for every DMA master. Record the finding, verify it on hardware in TASK-038.05, decide separately.
- Neither the driver nor `stm32-fmc` self-tests SDRAM. Take the raw pointer and carve slices by hand as `examples/looper.rs:45-52` does; **no linker-script change is needed**, and `firmware/memory.x` must keep claiming only FLASH 128 K at 0x08000000 and RAM 512 K at 0x24000000.
- Keep the `SdRam` value alive for the program's lifetime (spawn it into a task, or store it); dropping it frees 55 pins at the type level and lets someone claim them twice.
- Pin sanity: QSPI uses PF6-PF10 + PG6, SDRAM uses PD/PE/PF/PH/PI ranges, and the codec sits on PE2-PE6, in the gap between SDRAM's PE0/PE1 and PE7-PE15. Anything that `Peri::steal()`s a pin in those banks must be checked against the SDRAM list.
- Existing quirk, leave it alone: our `memory.x` has no `RAM_D2` and no `.sram1_bss` output section, so the SAI DMA buffers land in AXI SRAM around 0x24000194 rather than D2 SRAM. It works because SAI uses DMA1/DMA2, not the region-restricted BDMA. Do not "correct" this while touching anything else.

## Build and CI constraints

- Binaries are auto-discovered from `firmware/src/bin/*.rs`; there are no `[[bin]]` sections, so adding `rig.rs` needs no Cargo.toml edit. CI (`.github/workflows/ci.yml`) and lefthook pre-push both run `cd firmware && cargo build --release --features seed3` with no `--bin`, so every bin and every default-off feature combination must compile or the branch is red. Add the new stimulus features to that CI step list rather than leaving them uncompiled.
- `firmware/Cargo.toml` has no `[profile.release]` block, so release builds are `opt-level = 3`, `lto = false`, `codegen-units = 16`, `panic = "unwind"`. Before concluding a capture loop cannot make its deadline, look at this: daisy-embassy's own profile is `lto = "fat"`, `opt-level = "s"`, `codegen-units = 1`. Deciding whether to match it belongs with whoever measures `max_block_us`.
- `embassy-executor` currently enables `platform-cortex-m` and `executor-thread`; `InterruptExecutor` additionally requires `executor-interrupt`.
- Boilerplate each bin duplicates (`#[panic_handler]` wrapper, `#[defmt::panic_handler]`, `#[defmt::global_logger] struct Logger`, `bind_interrupts!`, `default_rcc` + `hal::init` + `new_daisy_board!`, discarding `board.usb_peripherals`, `led::init(d20,d19,d18)`, `usb::init(UsbIrqs)`) is tracked by TASK-010. Copy the shape from `main.rs`; do not refactor five binaries as a side quest.
- Sharing idiom to copy structurally but not literally: `main.rs:63-90` `KnobState` is `UnsafeCell<[f32; 2]>` with mutual exclusion from `cortex_m::interrupt::free` at the call site — adequate for one slow writer and one fast reader, wrong-shaped for a producer publishing buffer ownership. Use `core::sync::atomic::{AtomicUsize, AtomicU32}` with explicit Acquire/Release for ring indices, as upstream `looper.rs` does with `AtomicBool`, and keep the `StaticCell` for heap-free statics.
- Log-line convention to honour: `podtest.rs:274-275` states field order is fixed and space-separated so host tooling can parse it, and `console.rs:198-199` says the STATUS field set and order are the contract with a host test that fails on change. New `RIGCFG` / `CAPSTAT` / `CAPMAX` bodies need the same treatment: one owner, one pinned host test.

## Rate arithmetic comment

`podtest.rs:117` carries a compile-time gate (`const _: () = assert!(ENCRAW_WORST_CASE_LINES_PER_SECOND < 58)`) proving its logging rate fits the link. Put the equivalent arithmetic for `CAPSTAT` cadence and dump chunk rate somewhere the compiler or a host test can check it, not in prose.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-11 01:29
---
Stale dump arithmetic in your Implementation Notes — recompute before planning. They say '219 AUDIO chunks plus one AUDEND per block' and '640 records/s ... roughly 146 kB/s ... at 150-of-228 efficiency', both computed from the 150-raw-bytes-per-record figure TASK-038.02 proved impossible at MAX_BODY = 200. Actual pinned geometry: CHUNK_RAW = 129, so a 32,768-byte ring block is 254 full chunks plus a 2-byte tail = 255 chunks, 256 records counting the AUDEND, and the wire cost is 745 records/s at ~169 kB/s (pinned by published_efficiency_matches_the_encoder, tests/console_dump.rs:946).

That is exactly MAX_CHUNKS_PER_BLOCK = 255 (dump.rs:444, asserted :477; MAX_BLOCK_BYTES = 32,895), so the 32 KiB block size sits on the ceiling with zero margin. Going one chunk further is a runtime refusal from the encoder (dump.rs:605, :661) during a dump, not a compile error, because nothing in asperitas-logging can see the ring's geometry. Recommend AC #7 derive chunks-per-block from dump::CHUNK_RAW and const-assert ring_block_bytes <= dump::MAX_BLOCK_BYTES instead of pinning a literal. See TASK-038.02's finalization notes for the rest.
---
<!-- COMMENTS:END -->
