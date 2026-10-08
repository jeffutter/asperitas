---
id: TASK-038.03.02
title: >-
  Build firmware/src/bin/rig.rs: interrupt-executor audio, stimulus playback,
  SDRAM capture ring, console dump
status: Done
assignee:
  - '@agent'
created_date: '2026-09-11 13:28'
updated_date: '2026-10-08 14:31'
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
- [x] #1 firmware/src/bin/rig.rs exists and builds under CIs command cargo build --release --features seed3, and git diff --name-only shows firmware/src/bin/main.rs and podtest.rs untouched, so the human-verified podtest output contract from TASK-018.04 cannot regress.
- [x] #2 Audio runs on a dedicated InterruptExecutor pended on SAI1 with embassy-executor's executor-interrupt feature enabled, while the USB drain, LED blink task, CAPSTAT emitter and dump writer stay on the thread executor. The type carries no const-generic parameter in 0.10 (InterruptExecutor<1> does not compile), start() returns a SendSpawner and unmasks the IRQ itself, so the priority is set before it. The comment cites upstream examples/looper.rs lines 27-31 and 132-134 at the pinned commit ca9bcc9, and records why the SAI1 vector is free (the audio driver binds DMA1_CH0/CH1, audio.rs:26-29, and embassy-stm32's SAI binds only DMA lines). The numeric relationship between the DMA IRQ priority and the executor priority is recorded with its source: Config::default() ships dma_interrupt_priority P0 (embassy-stm32 src/lib.rs:362), which outranks the P6 executor, and that direction is required because the DMA ISR is what pends SAI1.
- [x] #3 Stimulus kind is a compile-time selection via cargo features stim-sine, stim-ess and stim-pulse, with sine at -20 dBFS when nothing else is set, and mutually-exclusive selection enforced by a const assert. CI builds all four combinations so none can rot. The device emits exactly one RIGCFG record (capture format, block geometry, window, cpu_hz, cache bits) and exactly one RIGGEN record embedding the generators own describe() output verbatim, so no second description grammar exists. See §2 for why the text travels in its own record.
- [x] #4 Input capture stores the loop channel as 16-bit mono into the ring defined by asperitas_logging::capture, publishing each block through Filling -> Full -> Dumping -> Free using that modules transition table, with samples written before the index that publishes them. The producer writes only blocks it found Free; when none are free it stops capturing and increments a visible overrun counter instead of overwriting a block being dumped. A const assert ties capture::FRAMES_PER_CALLBACK to daisy_embassy::audio::BLOCK_LENGTH, compared in samples rather than bytes: HALF_DMA_BUFFER_LENGTH counts 64 u32 words per callback (32 stereo frames) while CALLBACK_BYTES counts 64 bytes of one mono channel, so asserting those two figures equal would pass by coincidence and comparing either against HALF_DMA_BUFFER_LENGTH * 2 could never pass at all.
- [x] #5 Per-callback work is bounded to one contiguous copy plus lane truncation. DWT cycle-counter instrumentation reports worst-case callback duration and longest inter-callback gap, brought up with the sequence already proven on this part (DCB enable_trace, DWT unlock to clear the H7 software lock, has_cycle_counter probe, enable, read-back liveness check, per spin_budget.rs:80-97) and reporting zeros honestly when CYCCNT is unavailable; cycles become microseconds via a boot-time calibration against embassy-time, since embassy-stm32 0.6.0 exposes no CPU-clock accessor. A periodic CAPSTAT record carries delivered blocks, expected blocks, capture overruns, max_block_us, worst_gap_us, dump progress and the transports dropped_full, giving hardware verification two independent starvation signals.
- [x] #6 The device reports CAPMAX total_bytes ring_bytes seconds_max unused_headroom_bytes computed at runtime from sdram::SDRAM_SIZE and the published ring geometry, so capturable duration is measured from the driver constant rather than guessed, and the headroom statement makes clear that live audio DMA buffers remain in internal RAM.
- [x] #7 RIGCFG, CAPSTAT, CAPMAX and the dump-summary verb are implemented as body builders in crates/asperitas-logging/src/console.rs beside status_body, each pinned by a host unit test in that file, and committed through one public whole-record entry point modelled on the existing emit path. Nothing in the dump or status path calls usb::emit_blocking.
- [x] #8 The dump writer obtains permission to enqueue from dump::try_emit_dump, which consults the TASK-038.02 capacity predicate, and never bypasses it; refusals are retried on a Timer backoff rather than a busy-wait, and both the refusal count and the longest consecutive stall are counted and reported. Ordinary log and STATUS traffic stays lossless during a dump.
- [x] #9 Capture start and end are decided by the device: it captures for a build-time-configured window or until the ring reports full, then begins the dump on its own, because runtime control over the console link belongs to TASK-032.
- [x] #10 When a dump finishes the device emits one DUMPEND record naming blocks, chunks, bytes, elapsed milliseconds and the transport loss counters at that moment, so a caller times and validates a transfer from the captured stream alone.
- [x] #11 The SDRAM memory model is recorded where a future reader will hit it: init() returns 0xC000_0000 while the driver programs its cacheable MPU region at 0xD000_0000, caches are enabled nowhere in the stack so accesses are uncached and coherent today, nothing here enables caches or changes the MPU base, the device reports the I-cache and D-cache enable bits it actually observes at boot so the claim is measured rather than asserted, and TASK-038.05 receives a rule rather than an open question: caches stay off until someone owns the coherence argument for the FMC window and revisits the capture hand-off ordering in the same change. A short factual note is added to docs/reference/daisy-seed3.md section 4.
- [x] #12 Rate arithmetic is gated by const asserts in rig.rs, not prose: the capture window provably fits the ring, and CAPSTAT traffic is provably below one percent of the dumps own record traffic.
- [x] #13 cargo fmt --all --check, cargo test --workspace, cargo clippy --workspace --all-targets -- -D warnings, the defmt-only whole-package build and each non-default stimulus variant build all pass, and CI gains a firmware clippy step (cargo clippy --release --features seed3 -- -D warnings inside firmware/, which passes as of planning; --all-targets cannot work there because a no_std target has no test crate) since firmware is excluded from the root workspace and nothing else lints rig.rs. The finalization notes record rig's measured text and .bss from size -B against main's measured baseline (text 88181, data 1428, bss 8224, i.e. 1.57 percent of the 512 KiB AXI SRAM), and state that this ticket's original 86.13 percent .bss premise had no recorded provenance and contradicts measurement.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the leaf commits (latest 887b2df) and verified on the bench in TASK-038.05. This plan is superseded; the final summary describes what landed.
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

## Closed 2026-10-08: verified against the tree and on hardware

The leaves are all Done (.01, .02, .03, .04), and .02 re-verified the same criteria against the tree on 2026-10-07. Spot-checked again at HEAD: `scripts/gates.sh push` exit 0 (fmt, workspace tests and clippy, firmware clippy for both cfg sets and all stim variants, rig stim builds, RTT build). console.rs has rigcfg_body / riggen_body / capstat_body / capmax_body / dumpend_body beside status_body, committed through lib.rs emit_record; nothing in rig.rs or dump.rs calls emit_blocking. The rig then ran on hardware in TASK-038.05 (300 s, 879/879 blocks, dump proved).

Dispositions, rather than literal readings:
- **#9 'or until the ring reports full':** not built. The window is const-gated below the ring. Moved to TASK-038.07.01 (ring-fill mode), as TASK-038.05 AC #3 was.
- **#11:** the 'section 4' note landed as docs/reference/daisy-seed3.md 'External SDRAM: address, MPU and caches (measured)' (89b5871). The bench corrected the rule: the window is default-map Device memory, so what changes coherence is an MPU region over 0xC000_0000, not CCR.DC. rig.rs carries the corrected rule.
- **#13:** the CI firmware-clippy clause is superseded by TASK-060 (gates.sh). Sizes are recorded in .02's notes.
- **Bench-found defects** (soft-float f64 starving the callback, eed633b; the sweep starting before the capture, TASK-038.08) were outside these criteria and are tracked where they were found.
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Umbrella over rig.rs. All leaves Done, criteria re-checked against HEAD (push gates green) and exercised on hardware in TASK-038.05. The ring-full end condition moved to TASK-038.07.01.
<!-- SECTION:FINAL_SUMMARY:END -->
