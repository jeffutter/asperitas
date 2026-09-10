---
id: TASK-030.04
title: >-
  HUMAN: Confirm the framed console on hardware - legible in a terminal, zero
  bad frames, no audio regression
status: To Do
assignee:
  - '@human'
created_date: '2026-09-09 03:25'
updated_date: '2026-09-10 13:34'
labels:
  - planned
dependencies:
  - TASK-030.02
parent_task_id: TASK-030
priority: high
type: task
ordinal: 51500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: TASK-030. `@human` by necessity: it needs the board, a terminal, and ears.

TASK-033 is the real proof that framed capture loss is gone, but it also waits on TASK-032's control
channel. This ticket is the cheap check that should happen immediately after TASK-030.02 lands, before
TASK-031 and TASK-032 are built on top of console protocol v1. Two things can go wrong in ways no host
test can see: the new prefix could make a plain terminal unreadable for the humans who still have to
verify behaviour by eye (`README.md:191-199` documents exactly such a procedure), and formatting plus CRC
plus a whole-record commit now happens with interrupts disabled while a 48 kHz audio block arrives every
~667 µs on the same executor.

Nothing here needs the rig runner: flash, attach a terminal, tee the bytes to a file, and decode with
`cargo run -p asperitas-logging --example console_decode -- capture.raw` from TASK-030.01. Record numbers,
not pass/fail claims — that is this project's evidence convention. If something fails, file the bug
ticket with the offending timestamps rather than checking a box.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: Flash the build containing TASK-030.02 and attach a plain terminal (screen /dev/cu.usbmodem...). Confirm podtest knob, encoder and button lines are still readable by eye with the new prefix and CRC suffix, and that blinky or panictest boot messages appear normally.
- [ ] #2 HUMAN: Capture at least 60 seconds of podtest to a raw file while turning the encoder through several detents and pressing and releasing both buttons several times; decoding the file reports zero records failing the integrity check.
- [ ] #3 HUMAN: Every encoder click, press and release performed during that capture is present in the decoded output - this reproduces the specific loss that ate two button presses in TASK-018.04.
- [ ] #4 HUMAN: Record the numbers in this ticket notes: decoded record count, achieved records per second, device-reported drop counters from the STATUS records, and host-side integrity failures.
- [ ] #5 HUMAN: With logging running, confirm audio output through main.rs is free of clicks, dropouts or pitch artefacts attributable to the logging path, or state explicitly that the bench was not rigged for audio at that moment.
- [ ] #6 HUMAN: If any criterion above fails, file a bug ticket carrying the offending timestamps from the capture rather than checking the box.
- [ ] #7 HUMAN: Confirm the captured raw file ends on a complete record rather than a truncated tail - the trailing-full-packet case no host test can see - and note the byte length of the last record modulo 64.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# Plan — one flash, one terminal, recorded numbers

Nothing here needs TASK-031's rig runner, and nothing here can be done by an agent: it needs the board, a
terminal, and ears. Procedure only — the criteria themselves say what must be true.

1. **Build.** Take the tree containing TASK-030.02. From `firmware/`: `make flash-all BINARY=podtest`
   (build + DFU flash; see `docs/reference/daisy-seed3.md` for the DFU sequence if the board needs a manual
   entry into the bootloader).
2. **Attach and tee.** `screen /dev/cu.usbmodem<N> 115200` for the eye test, and capture raw bytes to a
   file in parallel (`screen -L -Logfile capture.raw …`, or `cat /dev/cu.usbmodem<N> > capture.raw` if no
   other reader is attached). Both are wanted: the first proves legibility, the second is the evidence.
3. **Exercise it for ≥60 s.** Turn the encoder through several detents in both directions, press and release
   BTN1 and BTN2 several times each, and note the count you performed as you perform it — criterion #3 is
   comparing against your hands, not against the log.
4. **Decode.** `cargo run -p asperitas-logging --example console_decode -- capture.raw`. Zero integrity
   failures is the bar; the summary line also gives records decoded and bytes discarded.
5. **Record numbers in this ticket's notes**, not pass/fail prose: decoded record count, records per second
   (count ÷ capture seconds), the device's own `sent` / `dropped_full` / `bytes_dropped` / `trunc` / `ep_err`
   from the last STATUS record seen, host-side integrity failures, and whether a BOOT record appeared and
   how many times (a second BOOT mid-capture means the board restarted, which explains any seq gap).
6. **Audio.** If the bench is rigged with something listening to `main.rs`'s output, confirm no clicks,
   dropouts or pitch artefacts traceable to logging; if it is not rigged, say that plainly instead of
   implying you listened. The locked region TASK-030.02 introduces is the specific thing being checked.
7. **Panictest spot check.** `make flash-all BINARY=panictest`, attach within the countdown, confirm the
   countdown lines and the `PANIC:` line arrive as framed records with valid CRCs.
8. **On any failure**, open a bug ticket carrying the offending `t_ms`/`seq` values from the capture rather
   than checking a box. If the framing itself turns out wrong — unreadable in a terminal, or audibly costing
   the audio loop — that finding goes back into TASK-030's plan (§4 and §8), because its decision to keep
   printable ASCII and to hold ~200 B of work with interrupts disabled is exactly what this checks.

Why this exists next to TASK-033: TASK-033 also waits on TASK-032's control channel, so without this cheap
check a bad v1 decision would not be discovered until the rig runner and the command protocol were already
built on top of it.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Bench observation requested from TASK-046 (now done): when you rig the console capture and flash panictest, note (a) whether the PANIC: record still arrives over CDC, and (b) roughly how long the pre-halt pause is before the red LED. That single observation distinguishes the two branches of the new spin budget: ~3 s means DWT CYCCNT counts on real silicon after TRCENA+LAR unlock (cycle bound fired); milliseconds means the DWT stayed locked and the budget reported itself spent immediately. Either way the board halts rather than spinning forever; we just want to know which path ran. Source-level evidence cannot answer this — it needs a probe-free board and a stopwatch.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-10 12:31
---
Bench observation requested by TASK-046's plan (planning-time pass; no new rig setup needed, this rides the `panictest` capture you are already going to take).

TASK-046 replaces `usb::emit_blocking`'s `EMIT_TIMEOUT`, which compared `embassy_time::Instant::now()` against a deadline and therefore depended on the TIM5 time-driver ISR being able to run, with a bound measured in DWT CYCCNT processor cycles plus an unconditional poll-count ceiling. One claim in that design cannot be checked from source and is the only thing your bench can settle: whether CYCCNT actually counts on this STM32H750 after `DEMCR.TRCENA` + the DWT `LAR` unlock under a bare panic path.

What to note while flashing `firmware/src/bin/panictest` (with and without the host attached to the Pod's USB-C):

1. Whether the final `PANIC:` record still arrives on the wire. It should, when interrupts are live, because the cycle bound does not change the happy path.
2. Roughly how long the pause is before the board goes red-LED-halt if the record does NOT arrive (e.g. unplug the host first). About 3 s means the cycle bound expired as designed; effectively immediate means DWT came back unavailable or locked and the code took its fast-exit branch. Both outcomes are safe and neither fails TASK-046; the observation just tells us which branch real silicon takes, so the doc comment can stop hedging about it.

Deliberately not a sub-task of TASK-046: a `@human` child would inherit onto that parent and leave it unclosable with no agent work remaining, which is the TASK-004 failure mode written up in CLAUDE.md. This ticket already rigs the console and flashes `panictest`, so the check costs nothing here.
---
<!-- COMMENTS:END -->
