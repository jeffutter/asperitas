---
id: TASK-038.05
title: >-
  HUMAN: Verify stimulus, capture, dump integrity, and excerpt replay on
  hardware
status: Done
assignee:
  - '@human'
created_date: '2026-09-09 11:43'
updated_date: '2026-10-08 14:28'
labels:
  - planned
dependencies:
  - TASK-034
  - TASK-038.03
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
- [x] #1 HUMAN: with the TASK-034 loopback cable attached and `rig` flashed, each stimulus build mode (sine at −20 dBFS, exponential sweep, impulse train) is confirmed audible at the Pod output by something other than the device itself — scope trace, phone recording, or ears — at an amplitude consistent with the loop gain TASK-034 recorded, and the reading is written down even when it agrees.
- [x] #2 HUMAN: a continuous capture of at least five minutes runs to completion, dumps to the host, and the archived artifact shows the transport's `dropped_full` counter unchanged across the dump **and** delivered block count equal to expected block count with `max_block_us` inside the callback budget, with both numbers quoted rather than summarised as a pass.
- [x] #3 HUMAN: maximum capturable duration is checked against reality — capture until the ring reports full, compare the seconds the device claimed with the wall-clock run, and record the difference along with what the producer did when blocks ran out.
- [x] #4 HUMAN: dump wall-clock time for a known capture length is measured and placed next to the predicted figure from TASK-038.02's arithmetic, together with the achieved bytes-per-second on the link, so the first real full-speed-CDC throughput number in this repo is on record instead of estimated.
- [x] #5 HUMAN: one named excerpt from `audio/instruments/` is installed into QSPI with its erase-inclusive wall-clock time recorded, the device reports `EXCOK` with a matching readback CRC, replay drives the DAC without gaps, and a person confirms by ear that the returned audio is recognisably that clip — the digital claim being exactness of the buffer, the analog judgement staying human.
- [x] #6 HUMAN: the SDRAM memory model is settled with evidence: the observed value of the core cache control register, or a timed pattern write-and-readback through the 0xC000_0000 window, is recorded, and the mismatch between the address `init()` returns and the address the driver's cacheable MPU region covers is resolved in writing in `docs/reference/daisy-seed3.md` rather than left as a comment.
- [x] #7 HUMAN: the capture artifacts and metric summaries from this session are committed and referenced from TASK-019.03 and TASK-035, and the README measurement-rig section carries the observed numbers, since those tickets treat a recorded number as the evidence standard.
- [x] #8 HUMAN: the archived capture artifact is shown to contain the stimulus itself in the mono lane rig recorded — peak or RMS amplitude consistent with the loop gain TASK-034 wrote down, and the named channel (left or right) written into docs/reference/daisy-pod.md — because matching delivered and expected block counts with dropped_full at zero passes just as happily on a lane carrying silence, and only a person with the cable in hand can say which lane the loop actually drives.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the bench session commits eed633b..HEAD (eed633b fix, 4c95d04 archive, 89b5871 SDRAM, dddd11e lanes, 358853d README). This plan is superseded; the ticket's final summary describes what actually landed.
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

### AC #8 evidence, 2026-10-08 (three-run lane experiment, uncommitted rig edits, reverted)

Stereo cable from OUT to IN, 1 kHz -20 dBFS, 10 s windows, all 30 blocks proved by dump_reassemble on every run. Output L only, recorded Left: peak -20.42 dBFS, RMS -23.43. Output L only, recorded Right: peak 2 LSB (-84.29), RMS -93.31. Output R only, recorded Right: peak -20.40, RMS -23.42. The loop is straight (word 0 to word 0, word 1 to word 1), both paths have equal gain, and there is no crosstalk above the 16-bit floor. Written into docs/reference/daisy-pod.md, 'Self-loopback channel mapping (measured)'. The archived 300 s capture's lane (Left) carries the stimulus at -20.41 dBFS. Open: whether word 0 is the TRS tip on the jack; one headphone listen settles it. Not ticked: awaiting the owner's sign-off.

### AC #1 material, 2026-10-08 (device captures, not the outside observation AC #1 requires)

All three stimulus builds captured through the loop, 30 s windows, 88/88 blocks proved on each run:
- sine: peak -20.41 dBFS, 1 kHz, steady (from the 300 s run).
- stim-ess: peak -20.38 dBFS. The sweep runs 154 Hz at capture start to ~15.6 kHz, then ends about 6 s in. The level is flat to ±0.03 dB (RMS 2200-2215) from 150 Hz to 11 kHz. Finding: the sweep is one-shot and starts at audio start, 2 s before the capture arms, so the archive misses 20-112 Hz. Filed as TASK-038.08.
- stim-pulse: peak -20.54 dBFS, RMS -42.50, 3004 pulses exactly 480 samples (10.0 ms) apart.
WAVs were handed to the owner for listening. They are the device's own recordings, so they do not discharge AC #1's 'something other than the device'.

### AC #1, #6 and #8 closed with the owner, 2026-10-08

The owner reviewed the evidence recorded above and the three loopback WAVs (sine, ESS, pulse train) and agreed each criterion is met.
- AC #1: the owner listened to the captured stimuli and judged them correct. Recorded honestly: the observation was of the device's own loop recordings, not a live outside listen at the Pod output, and the owner accepted that as sufficient. Amplitudes: sine -20.41, ESS -20.38, pulse -20.54 dBFS peak, against a loop gain of -0.41 dB.
- AC #6: register readings plus the pattern readback, resolved in docs/reference/daisy-seed3.md (89b5871).
- AC #8: the archived 300 s capture's Left lane carries the stimulus at -20.41 dBFS, and the channel mapping is in docs/reference/daisy-pod.md (dddd11e). Whether word 0 is the TRS tip stays a documented open point, which the owner accepted.

Remaining: AC #5 (excerpt replay, blocked on TASK-038.04) and AC #7 (cross-references from TASK-019.03 / TASK-035 and the README figures).

### AC #7 work, 2026-10-08

- Capture artifact: audio/captures/2026-10-07-rig-sine-300s.console.zst (committed in 4c95d04).
- Metric summaries: README 'Measurement rig', with a table of the bench readings and their sources.
- Cross-references: TASK-019.03 and TASK-035 each carry the artifact as a ref, plus a note with the baseline numbers and caveats. TASK-038.06 has a note to extend the README section instead of writing a second one.
Not ticked: awaiting the owner's sign-off.

AC #7 closed with the owner, 2026-10-08. The owner reviewed the README section and cross-references above and agreed.

### AC #5 moved to TASK-038.09; ticket closed with the owner, 2026-10-08

Excerpt replay needs TASK-038.04, which is unbuilt and unplanned (QSPI slot storage, the first inbound console path, MDMA staging). At the owner's instruction, AC #5 is moved verbatim to TASK-038.09 (@human, depends on TASK-038.04) and ticked here as moved, not as measured, the same way AC #3 went to TASK-038.07. Dependencies on TASK-038.04 and TASK-038.06 are dropped: the first now belongs to TASK-038.09, and this ticket did not need 038.06's documentation (AC #7's README section was written here). TASK-034 stays listed. Its loop-gain and hum numbers come from this session (recorded in its notes), but its own criteria (cable labelled and left in place, outside observer, lab-setup note) are still the owner's to close.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Bench verification of rig, done live with the owner on 2026-10-07/08 with a Pod OUT to IN cable.

- **A blocking bug found and fixed (eed633b):** f64 stimulus math was soft-float, the callback took 794 us of 666, and audio died on the first read. Building for cortex-m7 brought it to 64-72 us.
- **300 s run:** 879/879 blocks, 0 overruns, dropped_full 0. The dump took 69.4 s (414.9 kB/s PCM, 731.7 kB/s wire) and every block was proved; archived in audio/captures/.
- **Loop:** -0.41 dB at -20 dBFS, flat to +/-0.03 dB from 150 Hz to 11 kHz, at the 16-bit noise floor, channels straight.
- **SDRAM memory model settled from register reads** (the window is Device memory, so the MPU region at 0xD000_0000 is inert) and documented.
- **README:** a measurement-rig section with the observed numbers.
- **Follow-ups filed:** TASK-038.07 (ring-fill run, AC #3 moved), TASK-038.08 (the sweep starts before the capture), TASK-038.09 (excerpt replay bench check, AC #5 moved).
<!-- SECTION:FINAL_SUMMARY:END -->
