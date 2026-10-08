---
id: TASK-038.03.02.04
title: >-
  Add rig.rs capture producer, SDRAM dump writer, host-computable rate gates and
  CI coverage
status: Done
assignee:
  - '@agent'
created_date: '2026-09-12 12:00'
updated_date: '2026-10-08 02:14'
labels:
  - planned
dependencies:
  - TASK-038.03.02.03
modified_files:
  - firmware/src/bin/rig.rs
  - .github/workflows/ci.yml
parent_task_id: TASK-038.03.02
priority: high
ordinal: 88800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Second half of TASK-038.03.02, running on the topology that TASK-038.03.02.03 lands: the capture producer writing mono `i16` blocks into the SDRAM ring with state-machine bookkeeping and overrun accounting, `CAPMAX` before arming and `CAPSTAT` every second from the thread executor, the dump writer with `AUDEND` per block and one `DUMPEND` after flush, the compile-time rate gates, the CI lint and stimulus-build steps, and the host-verifiable part of the parent's verification ladder with its size readings recorded.

Nothing here is novel API. It is mechanical work that only becomes possible once two executors, a proven cycle counter and a render site exist, which is why it is separate: the sibling ticket can fail on compile risk alone without this work being at fault.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The capture producer runs inside the P6 audio callback and its entire outward surface is SDRAM writes plus atomic stores: no lock, no allocation, no logging call, no `embassy_time` await. Per-callback work is bounded to one contiguous copy plus lane truncation.
- [x] #2 Ring geometry comes from `asperitas_logging::capture`, never restated: `RING_BLOCK_BYTES`, `RING_BLOCKS`, `RING_BYTES`, `FRAMES_PER_CALLBACK`, `CALLBACK_BYTES`, `BYTES_PER_SECOND`, `CAPTURE_WINDOW_SECONDS`, `chunks_per_block()`, `expected_blocks()`. The window is overridable through `option_env!("ASP_RIG_CAPTURE_SECONDS")` so bench runs can shorten it without editing source, and the copied-driver-constant loop is closed with a `const assert!` comparing `capture::FRAMES_PER_CALLBACK` to `daisy_embassy::audio::BLOCK_LENGTH` **in samples**, with the comment explaining why `CALLBACK_BYTES` and `HALF_DMA_BUFFER_LENGTH` are not the same quantity.
- [x] #3 Block states move only along edges `capture::transition_ok` permits (`Free -> Filling -> Full -> Dumping -> Free`), asserted in a debug build against the table rather than a remembered list, and an overrun leaves the block alone and stops capturing instead of overwriting a block that is being dumped.
- [x] #4 Sequence numbers and cursors survive arm/disarm: `PRODUCED` counts blocks since boot, the writer walks `block % RING_BLOCKS` in ring order, and `OVERRUN` counts refusals to fill rather than bytes lost.
- [x] #5 `CAPMAX` is emitted once before arming with `total_bytes`, `ring_bytes`, `seconds_max`, `us_max`, `unused_headroom_bytes` all derived from `capture::` arithmetic, and `CAPSTAT` is emitted from the thread executor about once a second carrying `delivered`, `expected`, `overrun`, `max_block_us`, `worst_gap_us`, `audio_exit`, `dumped`, `dropped_full`. Every duration is converted with the `cycles_per_us` that TASK-038.03.02.03 established, and a raw gap delta at or beyond half the 32-bit counter range is treated as invalid rather than trusted.
- [x] #6 Capture is armed automatically by rig itself on a documented timeline (a fixed delay after the first audio callback so `BOOT`/`RIGCFG`/`CAPMAX` have flushed), ends at the earlier of the window deadline or a full ring, and dumps on the device's own decision. No inbound console channel is assumed to exist, because none does.
- [x] #7 The dump writer obtains permission for every chunk through `asperitas_logging::try_emit_dump`, never bypasses `dump::dump_fits`, retries refusals on a `Timer` backoff instead of busy-waiting, and counts both refusals (`refused`) and the longest stall (`stall_ms`). Ordinary log and `STATUS` traffic stays lossless during a dump - the behaviour `tests/console_dump.rs::log_records_survive_a_saturated_dump` protects.
- [x] #8 Each block ends with `AUDEND` whose CRC covers the raw concatenated bytes in chunk order, computed incrementally as chunks are sliced rather than in a second pass, and the dump ends with one `DUMPEND` carrying `blocks`, `chunks`, `bytes`, `elapsed_ms`, `refused`, `stall_ms` plus a `CONSOLE.snapshot()` reading taken at that instant.
- [x] #9 Compile-time rate gates, expressed as `const assert!` in `rig.rs` where both driver constants and `capture::` are visible: the intended window provably fits the ring in blocks, `worst_gap_us` must stay below one audio period plus slack (21 000 µs against a 20 833 µs period), `max_block_us` below the callback budget, and delivered blocks must equal expected blocks at the end of the window. A gate that cannot be evaluated at compile time is reported at runtime beside the number it judges, never left as prose.
- [x] #10 `.github/workflows/ci.yml` gains three commands inside the existing single-quoted `nix develop --command bash -c '...'` string (no nested single quotes, no new step, no matrix): a firmware lint step `cd firmware && cargo clippy --release --features seed3 -- -D warnings`, which nothing else provides because firmware is excluded from the root workspace and `make clippy` is hard-wired to `--bin main`; then `cargo build --release --features seed3,stim-ess --bin rig` and `cargo build --release --features seed3,stim-pulse --bin rig`. The implicit-sine default needs no new command: neither existing firmware build passes `--bin`, so both already compile `rig` with the default generator.
- [x] #11 The timer-slot budget is stated where it is spent and stays within eight: reporting ticker, LED blink, dump retry, capture deadline, boot including the codec's 2 ms startup delay. Overflow evicts the furthest-out timer and wakes it early rather than panicking, so a blown budget shows up as a mistimed `CAPSTAT`, not an error.
- [x] #12 Host-side measurements recorded in Finalization Notes: `.text`/`.bss` of `rig` versus `main` under both transports; `wire_bytes_per_block() × RING_BLOCKS` for the full-ring dump volume; the parent's §11 rows that need no board; and the dump-bandwidth prediction labelled as a prediction, naming `DUMPEND.elapsed_ms` as the field that will replace it.
- [x] #13 `git stash list` is empty of `wip-038.03.02-uncommitted` after TASK-038.03.02.03 applied its one live hunk, and the parent's notes record the disposition rather than leaving the next reader to diff a stash.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 7552780. This plan is superseded; the ticket's final summary describes what actually landed.
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

2026-10-07: moved Blocked -> Dev Ready, as the note above instructs. Its only dependency, TASK-038.03.02.03, is Done, and `git stash list` is empty (AC #13's precondition). Done in a live session at the owner's request.

### What landed (TASK-038.03.02.04, 2026-10-07)

Nothing of this ticket existed at HEAD 56549e0: `rig.rs` had the boot skeleton from TASK-038.03.02.03 and a no-op `emit_console` shim waiting for these verbs. All of the following is new.

**`firmware/src/bin/rig.rs`**
- Producer (`Producer::on_callback`), called inside the P6 callback after the render: gap + duration timing in raw DWT cycles (`fetch_max` into atomics), then, if armed, lane truncation of the codec INPUT (`(word >> 16) as i16`, left-justified 32-bit PCM, `MONO_WORD` derived from `const MONO_LANE: console::MonoLane`) into a 64-byte stack copy and one `copy_from_slice` into the block. Outward surface: SDRAM writes and atomics only.
- Ring: raw `*mut u8` from `Sdram::init`, `[AtomicU8; RING_BLOCKS]` states, every edge through one `advance(index, from, to)` = `compare_exchange(AcqRel/Acquire)` with `debug_assert!(capture::transition_ok(from, to))`. Overrun = claim of a non-Free block: counts 1, disarms, leaves the block alone.
- `PRODUCED` (blocks since boot, never reset; ring index `seq % RING_BLOCKS`, wire `blk` = seq), `OVERRUN`, `DUMPED`, `ARMED`, `FILLING`, `AUDIO_EXIT`, `MAX_BLOCK_CYCLES`, `WORST_GAP_CYCLES`, `INVALID_GAPS`.
- `CAPTURE_SECONDS` from `option_env!("ASP_RIG_CAPTURE_SECONDS")` via a const parser that fails the build on junk. RIGCFG `window_s` now reports it (was the default constant).
- Const gates: `fs_hz(RIG_FS) == capture::SAMPLE_RATE_HZ` (codec rate now named, not `Default`), `capture::FRAMES_PER_CALLBACK == daisy_embassy::audio::BLOCK_LENGTH` (local `BLOCK_LENGTH = 32` copy deleted, comment on why not `CALLBACK_BYTES` vs `HALF_DMA_BUFFER_LENGTH`), window > 0, `expected_blocks(window) < RING_BLOCKS`, `RING_BYTES <= SDRAM_SIZE`, gap limit strictly between one and two periods, budget <= one period.
- Runtime gates (`judge_capture`, logged at window close beside the numbers): delivered == expected, worst_gap_us < GAP_LIMIT_US, max_block_us < CALLBACK_BUDGET_US, overrun == 0.
- Timeline (`run_capture`, documented in a comment block): wait for first callback, +2 s arm, window deadline (100 ms poll also catches the producer's own overrun disarm), wait for the in-flight block, judge, dump, idle. Single-shot by design; no inbound channel.
- `CAPMAX` once at boot after RIGCFG/RIGGEN; `CAPSTAT` every second from a `Ticker` joined into main's select.
- Dump writer (`dump_ring` / `send_dump_body`), `log-usb` only: Full->Dumping, chunks of `dump::CHUNK_RAW`, CRC folded per chunk as sliced, `audio_body`, `try_emit_dump` retried with `Timer::after_millis(1)`, `refused`/`stall_ms` tallied, `AUDEND`, Dumping->Free, then `DUMPEND` with a `CONSOLE.snapshot()`. RTT-only build gets a second definition that says nothing is dumped instead of faking a DUMPEND.
- Timer-slot budget rewritten where spent (audio_task doc): five named users of eight, all in main's task, so one slot held in practice; overflow evicts the furthest-out timer early.
- SDRAM: `board.sdram.build(&mut cp.MPU, &mut cp.SCB)` + `init(&mut Delay)`, kept bound for main's life with the corrected reason (no `Drop` impl; it owns FMC). Cache/MPU-base rule carried in a comment.
- `audio_exit` made reachable: SAI start failure no longer halts (stores 1, LED Panicked, keeps reporting); SAI callback error stores 2 and RETURNS instead of spinning at P6, which would have starved thread mode so the explaining CAPSTAT could never be sent.

**`crates/asperitas-dsp/src/stimulus.rs`**: `PulseTrain::apply`'s two f64 `clamp(lo, hi)` with runtime upper bounds -> `max(lo).min(hi)`. Hypothesis tested locally: f64::clamp keeps its `min > max` panic, whose `{:?}` message links core's dragon/grisu f64 formatter. Result: rig stim-pulse text 130467 at HEAD (197 B under 128 KiB) -> 119722 with capture added. Without this, AC #10's pulse build overflows FLASH by 12 160 B. No result changes (neither bound can invert). asperitas-dsp tests pass.

**`scripts/gates.sh`** (not ci.yml - see deviations): push-tier `rig-stim-ess-build`, `rig-stim-pulse-build`, `rig-window-override-build` (`ASP_RIG_CAPTURE_SECONDS=30`), placed BEFORE the cross-compile pair per ordering rule 1; commit-tier `rig-stim-ess-clippy`, `rig-stim-pulse-clippy` after the firmware clippy pair (comment #1's follow-on). Ledger re-priced with `GATE_COSTS_BOOTSTRAP=1 scripts/gate-costs.sh --refresh`.

### Deviations from the ticket text, and why

1. **AC #9's numbers were off by a factor of ~31.** "21 000 us against a 20 833 us period" is 1e6/48, i.e. microseconds per 1000 samples; 32 frames at 48 kHz is 666.67 us, which is also what TASK-038.03 and `lib.rs` use. rig derives `PERIOD_US = 666` (floored, it is a deadline), `GAP_LIMIT_US = 832` (one period + a quarter: covers entry jitter, still catches one missed callback - a const assert pins `PERIOD < LIMIT < 2*PERIOD`), `CALLBACK_BUDGET_US = 666`.
2. **AC #10's ci.yml edit is superseded** (comments #1 and #2): CI runs `scripts/gates.sh ci`; firmware clippy over all bins in both cfg sets already exists. The genuinely new gates went into gates.sh as above.
3. **`expected` counts down**, as `console::CaptureStatus::expected` defines it ("blocks the window still expects"), so "delivered equals expected at the end of the window" is judged as delivered-this-run == WINDOW_BLOCKS (CAPSTAT `expected` reaches 0).
4. **The dump runs after the window closes**, not alongside it, so the measured window carries no bulk USB traffic and worst_gap/max_block describe the audio path alone. The ring gate guarantees the window fits, so nothing is lost by waiting.
5. **Window close is wall-clock, and the producer finishes its in-flight block**, so a healthy run delivers exactly `expected_blocks(window)` (879 at 300 s: 300 s is 878.9 blocks and the 879th completes). A missed-callback run comes up short, which is what makes the delivered==expected gate meaningful.
6. `unused_headroom_bytes` is block-granular ((1024 - 879) x 32 768 = 4 751 360), because the ring is consumed in whole blocks.

### Finalization: host-side measurements (AC #12), 2026-10-07, `rust-size -A`, release

| image | .text | .rodata | .data | .bss | flash (text+rodata+data) of 131 072 |
|---|---|---|---|---|---|
| main, console (`seed3`) | 73 104 | 14 500 | 408 | 8 224 | 88 777 (67.7 %) |
| rig, console, sine | 99 900 | 18 356 | 408 | 9 256 | 119 426 (91.1 %) |
| rig, console, stim-ess | - | - | 408 | 9 280 | 123 194 (94.0 %) |
| rig, console, stim-pulse | - | - | 408 | 9 256 | 120 130 (91.7 %) |
| main, RTT-only (`seed3 log-defmt`) | 38 400 | 8 016 | 464 | 2 832 (+1 024 .uninit) | 46 880 |
| rig, RTT-only | 60 164 | 10 868 | 464 | 4 288 (+1 024 .uninit) | 71 496 |

(stim variants from Berkeley `size`: text 122 786 / 119 722, bss 10 304 / 10 280 incl. .uninit-free totals.)

- rig .bss over main, console: +1 032 B - the 1 KiB `BLOCK_STATE` array plus atomics. The ring itself is in SDRAM, not .bss. Internal RAM use stays ~2 % of the 512 KiB AXI SRAM; the "86.13 % .bss, ~69 KB free" premise has no provenance and contradicts measurement, as the parent already noted.
- Flash is now the tight resource: stim-ess rig leaves 7 878 B. The capture/dump path cost ~12 KB (SDRAM init + FMC clocking, dump encoder and builders, CRC table, the join/select state machines). Before this ticket stim-pulse had 197 B left; the dsp fix above is what makes room.
- Full-ring dump volume: `wire_bytes_per_block() x RING_BLOCKS` = 57 788 x 1 024 = 59 174 912 B. The default 300 s window dumps 879 blocks = 50 795 652 B on the wire for 28 803 072 B of PCM.
- Parent §11 rows needing no board, all green in the `GATE_COSTS_BOOTSTRAP=1 scripts/gate-costs.sh --refresh` run (commit, push and ci tiers each passed; --record refuses a failing tier): `cargo fmt --all --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -D warnings`, firmware default build, `stim-ess` and `stim-pulse` rig builds, RTT-only build, firmware clippy both cfg sets plus both stim variants, the 30 s override build. Also checked by hand: `ASP_RIG_CAPTURE_SECONDS=400` fails with "the capture window does not fit the ring", `=3x` fails with "must be a whole number of seconds". `git diff --name-only` lists neither main.rs nor podtest.rs.
- **Dump-bandwidth PREDICTION (not a measurement):** USB FS bulk tops out at 19 x 64 B packets per 1 ms frame = 1.216 MB/s in theory, so the 300 s dump takes at least ~42 s; CDC-ACM through embassy's drain plus the 1 ms retry backoff will be slower, plausibly 60-120 s. `DUMPEND.elapsed_ms` is the field that replaces this guess (TASK-038.05 AC #4), with `refused`/`stall_ms` saying how much of it was waiting on the pipe.

### Hand-off to TASK-038.05 (@human)
Everything above is compile- and host-verified only. Unverified until a board runs it: SDRAM works at all at 0xC000_0000 with this MPU setup, max_block_us/worst_gap_us against 666/832, delivered==expected over 300 s, the dump's throughput and integrity, and which physical jack `MonoLane::Left` is.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-13 00:12
---
Planning note from TASK-060 (2026-09-12): AC #10's FIRST command - 'cd firmware && cargo clippy --release --features seed3 -- -D warnings' - is superseded by TASK-060.03, which lands that gate in pre-commit, pre-push and ci.yml rather than here, placed after CI's existing firmware builds so clippy reuses their artifacts, and adds a second whole-package pass at --no-default-features --features "seed3 log-defmt". Key Decision 5 (whole-package, not --bin rig) survives intact; --bins is just spelled out. WHAT STAYS HERE: AC #10's stim-variant builds (cargo build --release --features seed3,stim-ess --bin rig and stim-pulse), the window-override build, and the rate gates. One follow-on for whoever executes .04: each new stim-variant BUILD should gain a matching -D warnings CLIPPY line (~1-2 s warm each, measured) - cfg-gated stimulus code that no default-feature build compiles is otherwise unlinted, the same blind spot ci.yml:29-38 already closed twice for asperitas-logging. Also: make clippy FEATURES="seed3 stim-ess" already lints one binary today (BINARY defaults to main, so pass BINARY=rig); TASK-060.03 updates the Makefile comment at :264-269 that claims nothing gates this.
---

created: 2026-09-13 04:16
---
Blocking correction from TASK-061.02, filed before anyone executes this ticket as written. AC #10 and plan step 7 both say to append commands inside the existing single-quoted `nix develop --command bash -c '...'` string in ci.yml. That string has not existed since 70c6fc6, and its successor `.github/ci-steps.sh` is deleted by TASK-061.02: the CI step is now `nix develop .#default --command bash scripts/gates.sh ci`.

Adding a check means adding one `gate <tier> "<banner>" <command...>` line in `scripts/gates.sh`, positioned where it should run, tagged with the cheapest tier that should run it. Two consequences worth knowing. Quoting is normal shell argument passing, so the comma form (`--features seed3,stim-ess`) invented to survive a single-quoted script is no longer needed, though it stays legal. And the firmware clippy coverage AC #10 asks to add already exists - `=== firmware clippy (all bins) ===` and its RTT-only twin lint all six bins in both cfg sets with `-D warnings` in every tier - so re-read AC #10 against the script before adding anything: the genuinely new items are the two extra `rig` feature-set builds.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
rig.rs now captures. A P6 producer truncates the codec's left input lane to i16 and copies it into a 32 MiB SDRAM ring. Every block-state edge goes through one compare_exchange that is debug-checked against capture::transition_ok, and an overrun disarms instead of overwriting. rig arms itself 2 s after the first callback and closes the window on wall-clock time, finishing the block in flight. It then judges the runtime rate gates in the log, dumps every block via try_emit_dump with a 1 ms backoff (CRC folded per chunk, AUDEND per block, one DUMPEND with a CONSOLE snapshot), and idles. CAPMAX goes out once at boot and CAPSTAT once a second. Compile-time gates tie the codec rate and callback size to capture::, keep the window (overridable with ASP_RIG_CAPTURE_SECONDS) inside the ring, and pin the gap and budget thresholds. AC #9's thresholds were corrected from 20 833/21 000 us to the real 666 us period: limit 832, budget 666. audio_exit is now reachable because an SAI failure keeps thread mode reporting instead of spinning at P6. Two changes outside rig.rs. PulseTrain's runtime-bound f64 clamp was linking ~20 KB of float formatting, and with it the stim-pulse rig overflowed flash by 12 KB, so it now uses max/min. scripts/gates.sh gains three rig builds and two rig clippy gates; ci.yml itself was superseded per comments #1/#2. The ledger was re-priced and every tier passed. Sizes and the dump-bandwidth prediction are in the notes. Nothing here has run on a board: TASK-038.05 owns that.
<!-- SECTION:FINAL_SUMMARY:END -->
