---
id: TASK-038.05
title: >-
  HUMAN: Verify stimulus, capture, dump integrity, and excerpt replay on
  hardware
status: To Do
assignee:
  - '@human'
created_date: '2026-09-09 11:43'
updated_date: '2026-10-08 13:58'
labels:
  - planned
dependencies:
  - TASK-038.03
  - TASK-038.04
  - TASK-038.06
  - TASK-034
documentation:
  - docs/reference/daisy-pod.md
modified_files:
  - docs/reference/daisy-seed3.md
parent_task_id: TASK-038
priority: high
type: task
ordinal: 59500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Everything in TASK-038.01 through .04 can be finished green in CI while producing no sound at all: stimulus synthesis has property tests, the dump codec has synthetic streams, the ring geometry is arithmetic a host test can check, and the flash path has a state machine. None of that proves a waveform reaches the Pod jack, comes back through the patch cable, survives a dump, and still lines up with what was played. Compiling is not evidence, which is the failure mode this project's assignee convention exists to prevent.

This ticket is the bench session where the measurement rig is graded by things an agent cannot observe: ears, a scope, a stopwatch, and a cable that has to stay plugged in. It also settles two questions that would otherwise be decided by whoever got there first — whether the real dump rate matches the arithmetic in TASK-038.02, and whether SDRAM accesses are actually uncached given that the driver programs its cacheable MPU region at an address nothing is connected to.

Record numbers, not pass/fail claims. A criterion here is satisfied by writing down what the instrument read, even when it agrees with the device, because the whole arrangement is the device grading its own homework and the only outside witness is this session. Where reality disagrees with a threshold, open a bug ticket rather than loosening the threshold — the same rule TASK-019.03 AC #6 states.

TASK-034 must have happened first: the cable, the independent observation, and the loop gain recorded at a fixed digital level. Without those, the amplitudes measured here have nothing to be compared against.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: with the TASK-034 loopback cable attached and `rig` flashed, each stimulus build mode (sine at −20 dBFS, exponential sweep, impulse train) is confirmed audible at the Pod output by something other than the device itself — scope trace, phone recording, or ears — at an amplitude consistent with the loop gain TASK-034 recorded, and the reading is written down even when it agrees.
- [x] #2 HUMAN: a continuous capture of at least five minutes runs to completion, dumps to the host, and the archived artifact shows the transport's `dropped_full` counter unchanged across the dump **and** delivered block count equal to expected block count with `max_block_us` inside the callback budget, with both numbers quoted rather than summarised as a pass.
- [x] #3 HUMAN: maximum capturable duration is checked against reality — capture until the ring reports full, compare the seconds the device claimed with the wall-clock run, and record the difference along with what the producer did when blocks ran out.
- [x] #4 HUMAN: dump wall-clock time for a known capture length is measured and placed next to the predicted figure from TASK-038.02's arithmetic, together with the achieved bytes-per-second on the link, so the first real full-speed-CDC throughput number in this repo is on record instead of estimated.
- [ ] #5 HUMAN: one named excerpt from `audio/instruments/` is installed into QSPI with its erase-inclusive wall-clock time recorded, the device reports `EXCOK` with a matching readback CRC, replay drives the DAC without gaps, and a person confirms by ear that the returned audio is recognisably that clip — the digital claim being exactness of the buffer, the analog judgement staying human.
- [ ] #6 HUMAN: the SDRAM memory model is settled with evidence: the observed value of the core cache control register, or a timed pattern write-and-readback through the 0xC000_0000 window, is recorded, and the mismatch between the address `init()` returns and the address the driver's cacheable MPU region covers is resolved in writing in `docs/reference/daisy-seed3.md` rather than left as a comment.
- [ ] #7 HUMAN: the capture artifacts and metric summaries from this session are committed and referenced from TASK-019.03 and TASK-035, and the README measurement-rig section carries the observed numbers, since those tickets treat a recorded number as the evidence standard.
- [ ] #8 HUMAN: the archived capture artifact is shown to contain the stimulus itself in the mono lane rig recorded — peak or RMS amplitude consistent with the loop gain TASK-034 wrote down, and the named channel (left or right) written into docs/reference/daisy-pod.md — because matching delivered and expected block counts with dropped_full at zero passes just as happily on a lane carrying silence, and only a person with the cable in hand can say which lane the loop actually drives.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Prerequisites

TASK-034 done (cable permanently attached and labelled, independent observation recorded, loop gain noted at a fixed digital level), plus TASK-038.01 through .04 merged. If TASK-034 has not run, stop here: amplitudes measured without its loop gain have nothing to be compared against, and the criterion becomes a number nobody can interpret.

Board in a Daisy Pod, loopback cable in place, USB-C to the host. Flashing is over DFU (`dfu-util`, no probe): `cd firmware && make flash BINARY=rig FEATURES="seed3"` — the Makefile greps dfu-util output for `File downloaded successfully` rather than trusting exit code 74, so read its message, not its status.

Capture the console to a file rather than watching it: TASK-031's runner does not exist yet, so this session uses raw device-node redirection (`screen /dev/cu.usbmodem… 115200` with logging, or `cat /dev/cu.usbmodem… > capture.txt &`). Records are framed lines beginning `~`; `cargo run -p asperitas-logging --example console_decode -- capture.txt` validates them and prints the loss counters.

## Run order

1. **Boot sanity and self-description.** Flash default mode, capture the first seconds, confirm `BOOT proto=1 … maxbody=200`, one `RIGCFG` record whose payload matches the stimulus parameters you built, and `CAPMAX total_bytes ring_bytes seconds_max unused_headroom_bytes`. Quote all three verbatim into the ticket.
2. **Independent amplitude check** (criterion 1). For each of the three stimulus modes, put a scope or a phone recording on the Pod output and compare the observed level with the loop gain from TASK-034. Record the outside reading even when it matches the device's own — that agreement is the point of the criterion.
3. **Five-minute run** (criteria 2 and 3). Start a capture, let it run past five minutes without touching anything, then dump. Pipe the dump through `examples/dump_reassemble.rs` and require: every block complete, block CRCs matching, `dropped_full` unchanged across the whole dump, delivered blocks equal to expected blocks, and `max_block_us` inside budget. Write the actual integers down. Then repeat until the ring reports full and compare claimed versus wall-clock seconds.
4. **Dump throughput measurement** (criterion 4). Time the dump of a known capture length, compute bytes-per-second on the link, and put it beside the prediction from TASK-038.02's arithmetic (mono 16-bit at 96 kB/s of capture implies roughly 146 kB/s on the wire at 150-of-228 efficiency). This is the repo's first real full-speed-CDC number; if it lands far below prediction, open a bug ticket naming the drain path (`usb::run`, `DRAIN_BUF_SIZE = 256`) rather than adjusting the prediction silently.
5. **Excerpt install and replay** (criterion 5). Generate the install stream (`cargo run -p asperitas-logging --release --example excerpt_stream -- audio/instruments/<clip>.wav > /tmp/exc.bin`), redirect it to the device node, time it including erase pauses, wait for `EXCOK` or `EXCFAIL`, then switch to excerpt replay mode and listen. The clip must be recognisable, and the device's reported DAC-path CRC must equal the host-computed one.
6. **SDRAM evidence** (criterion 6). Read the core cache control register value by whatever route exists at that moment (debug record, or ST-Link via TASK-037 if it has landed) and run a timed pattern write-and-readback through the 0xC000_0000 window. Record what you saw and settle the MPU-region mismatch in `docs/reference/daisy-seed3.md`.
7. **Archive** (criterion 7). Commit the capture artifacts, reference them from TASK-019.03 and TASK-035, and put the observed numbers into the README section written by the parent ticket.

## What counts as finishing

Every criterion here is satisfied by a recorded reading plus the artifact that produced it. A summary sentence claiming success is not acceptance, and a threshold quietly widened to make a run pass is a bug to file, not a fix.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Bench session 2026-10-07 (live with the owner, no loopback cable yet)

Not criteria evidence: no TASK-034 cable, and 30 s windows (ASP_RIG_CAPTURE_SECONDS=30), not 300 s. Recorded because they change what the real session will see.

- **rig as shipped could not play audio.** RTT boot log: callback max_block_us=794 against a 666 us period, then 'audio callback stopped with an SAI error' 1.3 ms after entering the loop (RX ring Overrun, the only error read() can return). Cause: the bare thumbv7em-none-eabihf target emits soft-float for f64 (__aeabi_dmul/ddiv/dadd present, zero vmul.f64), and the stimulus generators run f64 libm per sample. Fixed in eed633b (target-cpu=cortex-m7). After: max_block_us 64-70, worst_gap_us 666-667, audio_exit=0.
- **Capture + dump end to end, USB console build, 30 s window:** delivered 88 == expected 88, overrun 0, DUMPEND blocks=88 chunks=22440 bytes=2883584 elapsed_ms=7502 refused=5626 stall_ms=1 dropped_full=0 bytes_dropped=0. dump_reassemble: all 88 blocks proved, exit 0, 5098472 wire bytes all accounted for, 0.5656 useful/wire. That is about 384 kB/s PCM and 680 kB/s on the wire, a first data point for AC #4 (a 300 s run is still needed).
- **Captured PCM was silence:** peak 2 LSB (-84.3 dBFS), RMS 0.7, mean -0.5. Expected with no cable. AC #8 still needs the cable.
- **Bench trap: probe-rs zeroes the DWT.** After 'make probe-flash' (probe-rs download --reset), DEMCR, DWT_CTRL and CYCCNT all read 0 and CAPSTAT reports max_block_us=0 / worst_gap_us=0, which looks like 'within budget'. A RESET-button boot of the same image reported 65-70 / 667. So timing numbers are only valid after a button reset or a power cycle, or under 'probe-rs run', which stays attached. Also, a probe-rs read halts the core long enough to overrun SAI (audio_exit flipped to 2), so never read memory mid-capture.
- **USB console drops boot records.** RIGCFG, RIGGEN and CAPMAX go out before the host opens the port, so the capture started at seq 0x0b-0x0c. Opening the port before reset does not help, because the device re-enumerates. Run 1 of the plan (quote BOOT/RIGCFG/CAPMAX verbatim) needs a way to get them: RTT, or a host runner (TASK-031).

## 300 s loopback run, 2026-10-07 (owner at the bench, cable fitted, RESET-button boot)

rig at eed633b, default build (seed3, sine -20 dBFS 1 kHz, 300 s window). Host reader: one process holding /dev/cu.usbmodem1101 open, re-running 'stty raw' on each reopen.

**Device (verbatim):**
- `rig: capture armed for 300 s (879 blocks)`
- `rig: gate delivered 879 == expected 879: pass`
- `rig: gate worst_gap_us 714 < 832: pass (0 gaps too wide to measure)`
- `rig: gate max_block_us 72 < 666: pass`
- `rig: gate overrun 0 == 0: pass`
- `DUMPEND proto=1 blocks=879 chunks=224145 bytes=28803072 elapsed_ms=69421 refused=52064 stall_ms=1 sent=225704 dropped_full=0 bytes_dropped=0`
- dropped_full was 0 in every CAPSTAT and STATUS from boot through DUMPEND.

**Host (dump_reassemble exit 0):** blocks complete=879 failed=0 abandoned=0; chunks stored=224145 duplicate=0 conflict=0 late=0; bad_frames=0 resyncs=0 discarded_bytes=0; 50992365 bytes pushed, all accounted for; 28803072 PCM bytes.

**Throughput (AC #4):** 28 803 072 PCM bytes in 69.421 s = **414.9 kB/s of PCM**, and 879 x 57 788 = 50 795 652 dump wire bytes = **731.7 kB/s on the wire** (0.5649 useful/wire). Against the predictions: TASK-038.03.02.04 predicted at least ~42 s from the USB FS bulk ceiling (1.216 MB/s) and plausibly 60-120 s through CDC; measured 69.4 s, so about 60 % of the theoretical bulk ceiling. This plan's older 146 kB/s figure is 5x pessimistic. refused=52064 with stall_ms=1: the writer retries often but almost never waits a whole millisecond.

**Audio content (AC #8, device side):** 300.032 s, peak 3126 (-20.41 dBFS), DC -0.49 LSB, per-second RMS 2209.38-2209.49 across all 300 s. The 1 kHz phase fitted over the first and last second agrees to 4 decimal places (0.3852 rad), so not one sample was lost, added or reordered across 14.4 M samples. Max |second difference| 56 LSB against 53.5 for an ideal sine (quantisation), so there are no splices. The loop drives the lane rig records as MonoLane::Left; which physical jack that is still needs a person.

**Earlier attempt, discarded as an artifact:** the same run dumped through a bash 'cat' loop lost 19 blocks (0x45-0x57) on the host. The 'cat' stopped reading, macOS buffered about 1 MB, and killing the reader discarded it; the device-side sequence numbers prove the records were sent. That dump's elapsed_ms=538629 includes stall_ms=469245 of waiting on the host, so it is not a throughput figure. Use a single long-lived reader. Python's tty.setraw on this port left reads blocked; 'stty -F <dev> raw' works.

**Not yet done:** the artifact (48.6 MB console capture, 28.8 MB PCM) is in a session scratch dir, not archived (AC #7). Still to do: the ring-full run (AC #3), the ESS and pulse builds (AC #1 needs an outside observer), excerpt replay (blocked on TASK-038.04), and the SDRAM cache evidence (AC #6).

### AC #2 and #4 closed with the owner, 2026-10-07

The owner was at the bench for the 300 s run above and agreed on the evidence for each criterion. AC #2: delivered 879 == expected 879, max_block_us 72 < 666 budget, dropped_full 0 across the whole dump, archived as audio/captures/2026-10-07-rig-sine-300s.console.zst (3.7 MB zstd of the 50 992 365-byte console capture; decompresses byte-identical, and dump_reassemble on it reproduces the same 28 803 072-byte PCM with 879/879 blocks). AC #4: 69 421 ms for a 300 s capture, 414.9 kB/s PCM and 731.7 kB/s wire, set beside the 42 s floor and the 60-120 s estimate. AC #7's cross-references from TASK-019.03 / TASK-035 and the README figures are still open.

### AC #3 moved to TASK-038.07, 2026-10-08 (owner's instruction)

rig cannot fill the ring as built: the window is const-gated below RING_BLOCKS. At the owner's instruction, the ring-fill check now lives in TASK-038.07 (firmware mode TASK-038.07.01 @agent, bench run TASK-038.07.02 @human), and AC #3 is ticked here as moved, not as measured. Its evidence will appear on TASK-038.07.02.

### AC #6 evidence, 2026-10-08 (probe reads off the running board)

SCB_CCR=0x00040200 (DC=0, IC=0); FMC_BCR1=0x800030db (BMAP=00); FMC_SDCR1=0x19e9 (bank 1 at 0xC000_0000, 64 MiB); FMC_SDCR2/SDTR2 at reset values (bank 2 unconfigured); MPU_CTRL=5; the only MPU region is 0xD000_0000, RASR=0x03030033 (64 MiB write-back cacheable). Probe patterns read back at 0xC000_0000, 0xC1FF_FFF0 and 0xC3FF_FFF0, with no aliasing at 32 MiB; 0xD000_0000 returns an AP bus error. Resolved in writing in docs/reference/daisy-seed3.md, 'External SDRAM: address, MPU and caches (measured)': the SDRAM is at 0xC000_0000, the MPU region is inert, and the window is default-map Device memory, so it stays uncached even if the D-cache is enabled. Correction to the earlier rule: what changes coherence is an MPU region over 0xC000_0000, not CCR.DC. rig.rs's comment is updated to match. Not ticked: awaiting the owner's sign-off.
<!-- SECTION:NOTES:END -->
