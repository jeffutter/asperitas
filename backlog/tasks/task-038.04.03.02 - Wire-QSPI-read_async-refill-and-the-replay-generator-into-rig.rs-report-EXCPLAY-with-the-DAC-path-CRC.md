---
id: TASK-038.04.03.02
title: >-
  Wire QSPI read_async refill and the replay generator into rig.rs, report
  EXCPLAY with the DAC-path CRC
status: Blocked
assignee:
  - '@agent'
created_date: '2026-10-08 15:39'
updated_date: '2026-10-08 15:54'
labels:
  - task
  - planned
dependencies:
  - TASK-038.04.03.01
  - TASK-038.04.05
parent_task_id: TASK-038.04.03
priority: high
ordinal: 138800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Scope: firmware/src/bin/rig.rs plus Cargo feature (e.g. stim-excerpt, mutually exclusive with the other generators via the existing GENERATORS_SELECTED assert) and slot choice. Staging buffers in internal RAM; refill task on the thread-mode executor calls Flash::read_async over MDMA (bindings shared with the install path from TASK-038.04.05); the SAI1 callback only pops from the ping-pong core and never touches QSPI. Must resolve D-cache coherency for MDMA writes into cacheable AXI SRAM (invalidate after refill or place staging in a non-cacheable region) and state which. After one full pass emit one console record (EXCPLAY slot, bytes, crc16, underruns) built via the one whole-record emit path; code and docs state that this CRC covers only what was handed to the DAC encoder and says nothing about the codec or cable (TASK-035 AC #4 posture). Confirm callback max_block_us headroom claim is not worsened (about 590 us of 666 us today) and add a CI build of the new feature. No bench claim. Depends on the staging core and on TASK-038.04.05.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Cargo feature stim-excerpt added to firmware/Cargo.toml, counted by GENERATORS_SELECTED so it is mutually exclusive with stim-sine/ess/pulse; slot chosen at build time from ASP_RIG_EXCERPT_SLOT (const-parsed like ASP_RIG_CAPTURE_SECONDS, malformed or >= SLOT_COUNT fails the build)
- [ ] #2 An ExcerptPlayer generator implementing Processor and Stimulus: the SAI1 callback path only calls ReplayCore::take_block (TASK-038.04.03.01), converts s16 to f32 as s/32768 into both output channels, and touches no QSPI, flash mutex, await or allocation; describe() reports slot, bytes and header crc16 within the RIGGEN 160-byte budget
- [ ] #3 A thread-mode refill task claims an Empty half, calls Flash::read_async over the shared Flash mutex (TASK-038.04.05) for the next HALF_BYTES of the slot's PCM, then finish_fill; staging statics live in AXI SRAM, 32-byte aligned; after each read the task invalidates the D-cache range if SCB::dcache_enabled() is true, and a comment states the coherency argument for both cache states
- [ ] #4 Boot reads and validates the slot header (async) before the generator is built; an empty or invalid slot halts with a console record and a red LED rather than playing silence; the header's pcm crc16 is the 'want' value
- [ ] #5 On pass completion exactly one EXCPLAY record is emitted through the whole-record emit path: slot, bytes, crc16 of samples handed to the DAC encoder, underruns, clean flag, and a want= field with the stored crc; code comment and record docs state the CRC says nothing about what returns through codec and cable (TASK-035 AC #4 posture)
- [ ] #6 rig builds for thumbv7em with stim-excerpt in both log-usb and log-defmt configurations, CI builds the feature, clippy clean, other stimulus builds unchanged; callback worst-case instruction cost added by the replay path is estimated in a comment against the ~590 us headroom; no bench claim (TASK-038.09)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 6a99698

Depends on TASK-038.04.03.01 (ReplayCore) and TASK-038.04.05 (Flash<Async> in a shared Mutex, RigFlashIrqs, slot header read helpers). Edits firmware/src/bin/rig.rs and firmware/Cargo.toml; CI workflow for the extra build.

Findings from rig.rs:
- The callback does generator.process_block(&frames_in, &mut frames_out); encode_block(&frames_out, output) (rig.rs ~940), where generator: &mut ActiveGenerator is a build-time type alias chosen by cfg (lines ~145-162) and built with GENERATOR.init(ActiveGenerator::default()) then set_sample_rate. ActiveGenerator must implement dsp's Processor (process_block, set_sample_rate) and Stimulus (describe). So replay is a fourth cfg arm: type ActiveGenerator = ExcerptPlayer, defined in rig.rs because it holds firmware statics. GENERATORS_SELECTED (lines 125-140) needs a fourth cfg!(feature = stim-excerpt) term and the 'less than 2' assert already covers exclusivity; the default (no feature) sine arm cfg must exclude stim-excerpt too.
- ActiveGenerator::default() cannot work for a player that needs a header read from flash. Under the feature, build it after the async header read in main (between flash build and GENERATOR.init), via a cfg'd construction path; keep the other arms untouched.
- Capture: the callback also runs Producer.on_callback; nothing there changes. Replay output must be mono-duplicated stereo; the capture lane remains the input.
- Memory: default .bss lands in AXI SRAM (memory.x RAM = 0x24000000). Put the 2x8192-byte staging in a #[repr(align(32))] static (UnsafeCell wrapper, Sync with a documented safety contract: the half state in ReplayCore says who owns which half). Do NOT place staging in SDRAM (Device memory, no unaligned access; slower) and not in .sram1_bss (SAI DMA's).
- D-cache: RIGCFG reports dcache from the hardware and the SDRAM ring comment (rig.rs ~287) establishes only the FMC window is uncached by memory type; AXI SRAM follows the default map and IS cacheable if the cache is enabled. Whether rig enables it should be checked at implementation (grep SCB::enable_dcache in rig and in daisy_embassy init; rig logs the reading). Write the code to be correct either way: after read_async completes, if SCB::dcache_enabled() then invalidate_dcache_by_slice on the half (needs 32-byte alignment and a multiple-of-32 length, both hold for 8192), and before handing a half to MDMA nothing is dirty because the callback only reads. State both cases in the comment.
- MDMA reaches AXI SRAM (it is an AXI master); QUADSPI/MDMA interrupts bound by .05. Interrupt priorities were read back there; reuse the reading.

Steps:
1. Cargo feature + cfg arms + ASP_RIG_EXCERPT_SLOT const parse (copy parse_seconds shape; reject >= excerpt::SLOT_COUNT at compile time).
2. ExcerptPlayer { core: &'static ReplayCore, bufs: &'static Staging, slot, bytes, want_crc, scratch } : Processor::process_block loops its 32 frames: core.take_block(halves, &mut i16x32) -> f32 conversion -> frames. On pass end, set an AtomicBool/finish result for the reporter; never emit from the callback (emit takes locks and does core fmt, which the existing code deliberately keeps out of the callback).
3. refill_task(flash_mutex, core, bufs, slot): loop { match core.claim_fill() { Some(fill) => { lock flash; read_async(slot_pcm_base + fill.offset, &mut half[..fill.len]).await; unlock; cache invalidate; core.finish_fill(..) } None => Timer::after_millis(2).await } }. Poll interval chosen against the margin table in replay.rs: it must be well under the half duration (85 ms); say how it was chosen. A Notify/Signal from the callback is allowed only if it is a plain atomic wake (no lock in the callback); prefer polling.
4. Pass reporter in thread mode: wait for core.finish() then emit EXCPLAY via the existing one-record emit path (add the body builder next to the other rig verbs in crates/asperitas-logging/src/console.rs with a saturated-length compile-time check like RIGCFG's table, plus a host test pinning the format; this keeps the wire contract host-tested).
5. Join the refill and reporter futures into the existing select in main; only under the feature.
6. CI: add a build of the feature next to the existing rig builds (see .github/workflows/ci.yml) in both log configurations.

Verify: cargo build --release for the firmware workspace with --features seed3,stim-excerpt (ASP_RIG_EXCERPT_SLOT=0) and the existing configs; cargo clippy; cargo test -p asperitas-logging (EXCPLAY body test). Hardware not available: say so in the final summary and leave audibility, observed margin and CRC match to TASK-038.09.

Risks: the Mutex shared with the install task means a replay refill could wait behind a long erase if install and replay overlap - refuse EXCSTART while a pass is running (and vice versa) or document that they are exclusive per boot; QSPI throughput unmeasured so the staging margin is an assumption; building the player after an async flash read changes boot order relative to the SAI start, keep the header read before prepare_interface so the audio start timing is unchanged.
<!-- SECTION:PLAN:END -->
