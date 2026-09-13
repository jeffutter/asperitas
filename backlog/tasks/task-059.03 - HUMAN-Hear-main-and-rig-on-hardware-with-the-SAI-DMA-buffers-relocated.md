---
id: TASK-059.03
title: 'HUMAN: Hear main and rig on hardware with the SAI DMA buffers relocated'
status: To Do
assignee:
  - '@human'
created_date: '2026-09-13 06:57'
labels:
  - planned
dependencies:
  - TASK-059.01
references:
  - docs/reference/daisy-seed3.md
  - firmware/Makefile
parent_task_id: TASK-059
priority: medium
type: task
ordinal: 107800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-059.01 moves the two SAI DMA buffers from `0x24000198` to `0x240021b8..0x240025b8` and shifts
every other `.bss` object down by 0x400. Every host-side check available says that is safe: image
sizes unchanged, total RAM unchanged, `_stack_end` unchanged, initial stack pointer unchanged, D-cache
off repo-wide, and daisy-embassy self-zeroes these buffers at prepare time so their contents do not
depend on where they live. None of it is evidence about sound. This is the ticket that decides whether
the change actually works, and per CLAUDE.md no agent may close it.

Why it cannot be made digital today: the objective loopback bench does not exist.
`task-034 - Fit a permanent Pod self-loopback cable and prove the loop with an independent observer`
is still To Do, and `task-035`'s alignment-and-metrics work depends on it. `rig` also cannot prove
audio digitally yet - the SDRAM capture ring, dump writer and `CAPSTAT` counters exist only as
host-side codec code, and the producer (`task-038.03.02.04`) is Blocked. So the evidence here is ears,
a recording or a scope, plus the console counters the device does emit. Say so in your notes rather
than implying a bench you did not have.

What could plausibly go wrong, ranked, so the listening is aimed at something:

1. A region mistake would put the buffers somewhere DMA1 cannot reach and audio would die with no
   compile error. The fix keeps `> RAM`, our AXI region at `0x24000000`, which DMA1 reaches today; if
   the rule ever got written against daisy-embassy's regions instead, their `memory.x` aliases `RAM`
   to DTCMRAM and silence is the only symptom.
2. A bad `NOLOAD` placement can bus-fault before `main`. That is invisible to USB and LEDs; blinky
   reaching steady green is the cheap witness, and `docs/reference/daisy-seed3.md:183-190` records it.
3. Anything address-sensitive that nobody thought to check. Nothing in this tree takes a raw pointer
   to those buffers, but "we grepped and found nothing" is exactly the kind of claim a bench session
   exists to test.

Bench setup, from `docs/reference/daisy-pod.md:163-165`: board in a Daisy Pod, USB-C to the host for
the console, a **line-level** source into the Pod input (interface line out, DI box or preamp out - a
passive pickup reads as a DSP bug and will send you chasing this ticket for the wrong reason), and the
Pod output to a monitor, amp, or a recorder you can play back.

Evidence convention, same as TASK-030.04 and TASK-038.05: record numbers and observations, not
pass/fail claims, even when everything agrees. If a criterion cannot be judged at the bench right
then, say explicitly that it went unjudged rather than checking the box, and if something sounds
wrong file a bug ticket carrying the timestamps or recording position instead of widening the
threshold.

This ticket depends only on TASK-059.01. TASK-059.02 deletes flags from an `objcopy` invocation,
which cannot change a byte once the layout is fixed, so do not wait for it - if it has landed by the
time you are at the bench, flash with it and note that you did.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: `blinky` built from the post-TASK-059.01 tree is flashed over DFU and reaches steady green, confirming the new link layout does not fault before `main`. Record the first four bytes of the image you flashed as well, since that is the same reading taken from the host side.
- [ ] #2 HUMAN: `main` is flashed and driven from a line-level source. Audio is audible at the Pod output, continuous and unbroken, with no clicks, dropouts or pitch artefacts, and knobs 1 and 2 audibly change filter cutoff and level - which is what proves the callback is running end to end rather than emitting whatever happens to be in the buffer.
- [ ] #3 HUMAN: left and right are not swapped, established through the output (mono source, or a source panned hard one way) rather than assumed. Note which channel carried it.
- [ ] #4 HUMAN: `rig` is flashed with `FEATURES="seed3"` and its stimulus is audible at the Pod output by something other than the device itself - a monitor, or a phone recording you keep. Its console shows `BOOT`, one `RIGCFG` record reading `icache=0 dcache=0`, and `STATUS` records whose `sent=` advances with `dropped_full=0`. Paste the actual counter values, decoded offline with `cargo run -p asperitas-logging --example console_decode` if the console was captured to a file.
- [ ] #5 HUMAN: criteria 2 and 4 are repeated across at least two resets or power cycles. The relocation moves every `.bss` object down 0x400, and a boot-order-dependent fault or a stale-buffer effect would show up intermittently rather than on the first try.
- [ ] #6 HUMAN: anything that sounded wrong, and any criterion you could not judge at the bench, is recorded in this ticket's notes with what you observed - and where something is wrong, a bug ticket is filed carrying the recording position or counter readings, rather than this criterion being checked.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
A runbook, not a design. Take the numbers as you go; the notes are the deliverable.

1. `git log -1 --oneline` and record which commit the images came from, plus the cfg set you built
   (`FEATURES="seed3"` unless you are deliberately testing the RTT build).
2. From `firmware/`: `make build BINARY=blinky && make flash-all BINARY=blinky` (hold BOOT, tap
   RESET, release BOOT to enter the DFU bootloader). Expect steady red-then-green; dark means a fault
   before `main`, in which case read `head -c4 blinky.bin | xxd -e` and compare against `0x24080000`
   before touching anything else - that tells you whether the image itself is wrong or the board never
   took it.
3. Wire the bench: Pod input fed from a line-level source, Pod output to a monitor or recorder, USB-C
   to the host. Record the serial device path you attach to so a later session can reuse it.
4. `make flash-all BINARY=main`, then play something through it. Knob 1 is the low-pass cutoff, knob 2
   the gain; move both slowly and describe what you hear rather than writing "worked". Listen
   specifically for clicks, dropouts and pitch artefacts - the vocabulary TASK-030.04 uses, because
   those are what a starved or corrupted DMA ring sounds like, as opposed to silence, which is what a
   dead SAI sounds like.
5. Establish channel identity with a mono source, or with whatever you have panned hard left, and
   write down which physical output carried it.
6. `make flash-all BINARY=rig`, capture the console to a file while it runs, then decode offline:
   `cargo run -p asperitas-logging --example console_decode -- <capture>`. Paste the `BOOT`, `RIGCFG`
   and a few `STATUS` lines into the notes. `icache=0 dcache=0` must still be what `RIGCFG` reports;
   if it does not, something else changed the cache state and this ticket is the wrong place to fix it.
7. Power-cycle or reset twice and repeat steps 4 and 6. A single clean pass across ~10 minutes of
   running is thin evidence for a change that moved every static in RAM.
8. Write the notes as a table of what you measured next to what was predicted, name anything you could
   not judge, and leave this ticket open if any `HUMAN:` criterion went unjudged. If audio is broken,
   file the bug with the recording position and counter readings attached, and note here that the
   parent stays blocked.
<!-- SECTION:PLAN:END -->
