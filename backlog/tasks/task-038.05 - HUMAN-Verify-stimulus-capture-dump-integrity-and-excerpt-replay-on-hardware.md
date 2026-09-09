---
id: TASK-038.05
title: >-
  HUMAN: Verify stimulus, capture, dump integrity, and excerpt replay on
  hardware
status: To Do
assignee:
  - '@human'
created_date: '2026-09-09 11:43'
updated_date: '2026-09-09 11:45'
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
- [ ] #2 HUMAN: a continuous capture of at least five minutes runs to completion, dumps to the host, and the archived artifact shows the transport's `dropped_full` counter unchanged across the dump **and** delivered block count equal to expected block count with `max_block_us` inside the callback budget, with both numbers quoted rather than summarised as a pass.
- [ ] #3 HUMAN: maximum capturable duration is checked against reality — capture until the ring reports full, compare the seconds the device claimed with the wall-clock run, and record the difference along with what the producer did when blocks ran out.
- [ ] #4 HUMAN: dump wall-clock time for a known capture length is measured and placed next to the predicted figure from TASK-038.02's arithmetic, together with the achieved bytes-per-second on the link, so the first real full-speed-CDC throughput number in this repo is on record instead of estimated.
- [ ] #5 HUMAN: one named excerpt from `audio/instruments/` is installed into QSPI with its erase-inclusive wall-clock time recorded, the device reports `EXCOK` with a matching readback CRC, replay drives the DAC without gaps, and a person confirms by ear that the returned audio is recognisably that clip — the digital claim being exactness of the buffer, the analog judgement staying human.
- [ ] #6 HUMAN: the SDRAM memory model is settled with evidence: the observed value of the core cache control register, or a timed pattern write-and-readback through the 0xC000_0000 window, is recorded, and the mismatch between the address `init()` returns and the address the driver's cacheable MPU region covers is resolved in writing in `docs/reference/daisy-seed3.md` rather than left as a comment.
- [ ] #7 HUMAN: the capture artifacts and metric summaries from this session are committed and referenced from TASK-019.03 and TASK-035, and the README measurement-rig section carries the observed numbers, since those tickets treat a recorded number as the evidence standard.
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
