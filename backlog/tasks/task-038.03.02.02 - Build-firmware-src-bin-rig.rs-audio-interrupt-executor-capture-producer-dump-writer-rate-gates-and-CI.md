---
id: TASK-038.03.02.02
title: >-
  Build firmware/src/bin/rig.rs: audio interrupt executor, capture producer,
  dump writer, rate gates and CI coverage
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-11 15:53'
updated_date: '2026-10-08 02:45'
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
- [x] #1 `firmware/src/bin/rig.rs` exists and builds under CI's command `cargo build --release --features seed3`, and `git diff --name-only HEAD` shows `firmware/src/bin/main.rs` and `podtest.rs` untouched, so the human-verified podtest output contract from TASK-018.04 cannot regress.
- [x] #2 Audio runs on a dedicated `InterruptExecutor` pended on `SAI1` with embassy-executor's `executor-interrupt` feature enabled, while the USB drain, LED blink task, `CAPSTAT` emitter and dump writer stay on the thread executor. The type carries no const-generic parameter in 0.10 (`InterruptExecutor<1>` does not compile), `start()` returns a `SendSpawner` and unmasks the IRQ itself, so the priority is set **before** it. The comment cites upstream `examples/looper.rs` lines 27-31 and 132-134 at the pinned commit `ca9bcc9`, and records why the `SAI1` vector is free (the audio driver binds `DMA1_CH0`/`CH1`, `audio.rs:26-29`; embassy-stm32's SAI binds only DMA lines). The numeric relationship between the DMA IRQ priority and the executor priority is recorded with its source: `Config::default()` ships `dma_interrupt_priority` P0 (`embassy-stm32 src/lib.rs:362`), which outranks the P6 executor, and that direction is required because the DMA ISR is what pends `SAI1`.
- [x] #3 Stimulus kind is a compile-time selection via cargo features `stim-sine`, `stim-ess`, `stim-pulse`, with sine at −20 dBFS when nothing else is set, and mutual exclusion enforced by a `const` assert in the source (cargo cannot express it). CI builds all four combinations so none can rot. The device emits exactly one `RIGCFG` record (capture format, block geometry, window, `cpu_hz`, cache bits) and exactly one `RIGGEN` record embedding the generator's own `describe()` output verbatim, so no second description grammar exists. `rig.rs` calls `describe()` into a buffer sized `console::RIGGEN_MAX_GEN_BYTES` and debug-asserts the returned length against that budget; a text clipped at the frame limit would otherwise print a truncated parameter as if it were real.
- [x] #4 Input capture stores the loop channel as 16-bit mono into the ring defined by `asperitas_logging::capture`, publishing each block through `Filling -> Full -> Dumping -> Free` using that module's transition table, with samples written before the index that publishes them. The producer writes only blocks it found `Free`; when none are free it stops capturing and increments a visible overrun counter instead of overwriting a block being dumped. A `const` assert ties `capture::FRAMES_PER_CALLBACK` to `daisy_embassy::audio::BLOCK_LENGTH`, compared **in samples**: `HALF_DMA_BUFFER_LENGTH` counts 64 `u32` words per callback (32 stereo frames) while `CALLBACK_BYTES` counts 64 bytes of one mono channel, so asserting those two figures equal would pass by coincidence and comparing either against `HALF_DMA_BUFFER_LENGTH * 2` could never pass at all.
- [x] #5 Per-callback work is bounded to one contiguous copy plus lane truncation. DWT cycle-counter instrumentation reports worst-case callback duration and longest inter-callback gap, brought up with the sequence already proven on this part (`DCB.enable_trace()`, `DWT::unlock()` to clear the H7 software lock, `has_cycle_counter()` probe, enable, read-back liveness check - per `spin_budget.rs:80-97`) and reporting zeros honestly when `CYCCNT` is unavailable; cycles become microseconds via a boot-time calibration against `embassy-time`, since embassy-stm32 0.6.0 exposes no CPU-clock accessor. A periodic `CAPSTAT` record carries delivered blocks, expected blocks, capture overruns, `max_block_us`, `worst_gap_us`, `audio_exit`, dump progress (`dumped`) and the transports' `dropped_full`, giving hardware verification two independent starvation signals. Refusals and the longest stall are *not* in `CAPSTAT` — they belong to the one dump they describe and ride on `DUMPEND`.
- [x] #6 The device reports `CAPMAX total_bytes ring_bytes seconds_max unused_headroom_bytes` computed at runtime from `sdram::SDRAM_SIZE` and the published ring geometry, so capturable duration is measured from the driver constant rather than guessed, and the headroom statement makes clear that live audio DMA buffers remain in internal RAM.
- [x] #7 The dump writer obtains permission to enqueue from `dump::try_emit_dump`, which consults TASK-038.02's capacity predicate, and never bypasses it; refusals are retried on a `Timer` backoff rather than a busy-wait, and both the refusal count and the longest consecutive stall are counted and reported. Ordinary log and `STATUS` traffic stays lossless during a dump. Its `AUDEND` CRC is accumulated with `frame::crc16_ccitt_update` across chunks, never by re-scanning a block it does not hold.
- [x] #8 Capture start and end are decided by the device: it captures for a build-time-configured window or until the ring reports full, then begins the dump on its own, because runtime control over the console link belongs to TASK-032.
- [x] #9 When a dump finishes the device emits one `DUMPEND` record naming blocks, chunks, bytes, elapsed milliseconds, the dump's `refused` / `stall_ms`, and the transport loss counters at that moment, so a caller times and validates a transfer from the captured stream alone.
- [x] #10 The SDRAM memory model is recorded where a future reader will hit it: `init()` returns `0xC000_0000` while the driver programs its cacheable MPU region at `0xD000_0000`, caches are enabled nowhere in the stack so accesses are uncached and coherent today, nothing here enables caches or changes the MPU base, and the device reports the I-cache and D-cache enable bits it actually observes at boot so the claim is measured rather than asserted. TASK-038.05 receives a **rule** rather than an open question: caches stay off until someone owns the coherence argument for the FMC window and revisits the capture hand-off ordering in the same change. A short factual note lands in `docs/reference/daisy-seed3.md` §4.
- [x] #11 Rate arithmetic is gated by `const` asserts in `rig.rs`, not prose: the capture window provably fits the ring, and `CAPSTAT` traffic is provably below one percent of the dump's own record traffic — dividing by `console::CAPSTAT_MAX_BODY` from the crate that renders it, not a number typed in here.
- [x] #12 The transport-less build still links: `cargo build --release --no-default-features --features "seed3 log-defmt"` compiles `rig`, with the console-verb and dump paths behind `#[cfg(feature = "log-usb")]` and `CAPSTAT`-shaped facts going through `log::info!` there, plus one comment line saying plainly that this build has no dump transport. No fake dump over RTT.
- [x] #13 `cargo fmt --all --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and each non-default stimulus variant build all pass, and CI gains a firmware clippy step (`cargo clippy --release --features seed3 -- -D warnings` inside `firmware/` — `--all-targets` cannot work there, a `no_std` target has no test crate) since firmware is excluded from the root workspace and nothing else lints `rig.rs`. The finalization notes record `rig`'s measured text and `.bss` from `size -B` against `main`'s measured baseline (text 88181, data 1428, bss 8224 — 1.57 % of the 512 KiB AXI SRAM), and state that this ticket's original 86.13 % `.bss` premise had no recorded provenance and contradicts measurement.
- [x] #14 The stale sentence in `spin_budget.rs:81-86` — which justifies `steal()` by the absence of any `cortex_m::Peripherals::take()` in the crate, daisy-embassy or embassy-stm32 — is updated to say `rig.rs` claims the singleton and `steal()` remains correct because neither user writes `CYCCNT`.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 887b2df. This plan is superseded; the ticket's final summary describes what actually landed.
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

## Verification pass, 2026-10-07 (closes the umbrella)

Re-checked all 14 ACs against the tree at ee6c6d3, not against the leaves' claims. `scripts/gates.sh push` (fmt both workspaces, workspace test + clippy, firmware clippy all bins in both cfg sets plus stim-ess/stim-pulse, rig stim-ess / stim-pulse / 30 s override builds, both cross-compiles, ELF provenance, load addresses, docs) passed before and after the fixes below. `git diff --name-only HEAD` never listed main.rs or podtest.rs.

### Gaps found and closed inline in 887b2df (small mechanical misses, per the plan)
- **AC #11**: the CAPSTAT traffic gate did not exist in rig.rs, though console.rs:427 says it does. Added `CAPSTAT_PERIOD_MS` (now drives the Ticker), `BLOCK_FILL_MS` (341), `CAPSTAT_MAX_PER_BLOCK` (2), and `const _: () = assert!(CAPSTAT_MAX_PER_BLOCK * console::CAPSTAT_MAX_BODY * 100 < capture::wire_bytes_per_block())`, which is 40 000 < 57 788, per parent plan §8.
- **AC #2**: the SAI1 and priority comments lacked the source citations. Lines verified against the pinned checkouts and added: looper.rs:27-31 / :132-134 at ca9bcc9, audio.rs:26-29 (DMA1_STREAM0/1 for DMA1_CH0/CH1), and embassy-stm32 0.6.0 src/lib.rs:362 (`dma_interrupt_priority: Priority::P0`), plus why the direction is required: the DMA ISR pends SAI1.
- **AC #12**: under RTT-only, CAPSTAT went into the discard shim, so the only info! output was the judge lines at window close, without `audio_exit`. report_capstat now logs all eight CAPSTAT fields with info! each second in that build.

### AC dispositions that differ from the letter of the text
- **AC #3**: `describe()` overflow is a runtime check that logs an error and sends no RIGGEN at all, not a debug_assert. That is stricter, because it holds in release too. The stim variant builds run in gates.sh and therefore in CI, which runs `gates.sh ci`.
- **AC #10**: the code-comment rule is in rig.rs:285-289 and RIGCFG reports the icache/dcache bits. The doc note was reassigned to TASK-038.06, which owns daisy-seed3.md (its description records that the file has no §4 and what the note must say). It is not done here.
- **AC #13**: comment #1 supersedes the CI firmware-clippy clause. TASK-060 put firmware clippy for both cfg sets in gates.sh, and .04 added the stim variants.
- **AC #5 / #9**: .04 corrected the period figures to 666 / 832 us; the AC text's 20 833 us was off by about 31x.

### Measured sizes, `rust-size -B`, release, after 887b2df
| image | text | data | bss |
|---|---|---|---|
| main, console (`seed3`) | 88 369 | 408 | 9 248 |
| rig, console, sine | 119 010 | 408 | 10 280 |
| main, RTT-only | 47 080 | 464 | 4 880 |
| rig, RTT-only | 71 696 | 464 | 6 336 |

Berkeley bss includes `.uninit`, so it reads about 1 KiB above the `rust-size -A` .bss figures in .04's table. main has also grown by about 190 B text and 1 KiB bss since the ticket's quoted baseline (88181 / 1428 / 8224). rig adds about 1 KiB bss over main, which is the per-block state array; the ring itself is in SDRAM. Internal RAM use stays around 2 % of the 512 KiB AXI SRAM, so the original 86.13 % .bss premise has no provenance and contradicts measurement. Flash is the tight resource: rig sine uses 119 418 of 131 072 B.

### Left for TASK-038.05 (bench, @human)
Everything is compile- and host-verified only. Still unverified: SDRAM at 0xC000_0000 working at all, the effective NVIC priorities in the boot log, max_block_us < 666 and worst_gap_us < 832, delivered == expected over 300 s, dump throughput (DUMPEND.elapsed_ms) and integrity, and which jack MonoLane::Left is. Also open for TASK-038.06: the daisy-seed3.md SDRAM/cache note.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-13 00:11
---
Planning note from TASK-060 (2026-09-12): AC #13's firmware-clippy-in-CI clause is superseded by TASK-060 / TASK-060.03, which adds whole-package cross clippy with -D warnings to pre-commit, pre-push and ci.yml, plus an RTT-only feature-set pass, placed after CI's existing firmware builds (measured local: 20 s cold standalone, 11 s after the build, ~2 s warm). AC #13's other clauses remain this ticket's. Two corrections for whoever executes .02: (a) the AC says root 'cargo fmt --all --check' will cover rig.rs - it cannot, Cargo.toml:3 excludes firmware/, and that blind spot is exactly why rig.rs drifted unformatted since 41f9cae (TASK-060.02 closes it); (b) firmware/Makefile's make clippy target is now at :264-269, not :252-255, and its comment claiming it is the only place firmware clippy runs becomes false when TASK-060.03 lands.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed the rig.rs umbrella. Re-ran the full verification ladder (gates.sh push: fmt, tests, clippy for host and firmware in both cfg sets, all stimulus variants, the window-override build, RTT-only build) against the tree and checked each of the 14 ACs in source. Three gaps were closed inline in 887b2df: the AC #11 CAPSTAT-vs-dump const rate gate was missing; the AC #2 upstream source citations (looper.rs, audio.rs, embassy-stm32 lib.rs:362) were missing; and the RTT-only build discarded CAPSTAT instead of logging it (AC #12). Measured rig 119 010 text / 10 280 bss against main 88 369 / 9 248. AC #10's doc note belongs to TASK-038.06, and all hardware truth belongs to TASK-038.05.
<!-- SECTION:FINAL_SUMMARY:END -->
