---
id: TASK-145.04
title: >-
  asperitas-live knobs turn from the keyboard while audio plays, in a thin
  crossterm terminal view
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-19 22:09'
updated_date: '2026-10-19 23:34'
labels:
  - planned
  - m4
  - live
  - host
dependencies:
  - TASK-145.03
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
parent_task_id: TASK-145
priority: high
type: feature
ordinal: 280400
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Startup flags alone mean you cannot turn a knob while listening, which defeats a live host. Owner decision (2026-10-19): a thin terminal view on `crossterm` only, no `ratatui`, no layout work. The two knobs on the keyboard, a value readout, a quit key. Revisit ratatui only if M5's extra controls make layout real work (doc-001 M4).

The view the owner picked:

```
 asperitas-live  src: loop audio/instruments/mandolin.wav

 cutoff [q/a]  ######....  0.62  (2.1 kHz)
 gain   [w/s]  #####.....  0.50  (0.0 dB)

 shift = fine step   esc = quit
```

q/a raise and lower knob 0 (cutoff), w/s knob 1 (gain); shift gives a fine step; esc (and ctrl-c) quit. The readout shows the normalised knob and the real unit. Real units come from asperitas-dsp (the `ParamSpec` tables or the helper TASK-145.02 added), never a mapping in this crate. Knob values are written into the shared control path TASK-145.03 built (atomics read by the engine at its 32-frame boundary), so a key press enters upstream of the effect's smoothing exactly where a turned pot does on the device. Startup `--params` sets the initial positions.

The terminal must be restored (raw mode off, cursor shown) on every exit path including panic. When stdout is not a TTY the binary keeps working with fixed knobs and says so, so `--seconds` unattended runs and CI still work.

New dependency: `crossterm` (current release), in `crates/asperitas-live/Cargo.toml` only. Pure Rust, no system library, so the nix and CI setup need no change; confirm CI stays green.

Code comments describe current state only: no ticket numbers, dates, measurement history, or line numbers of other files.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Key handling is a pure function from (current Controls, key event) to an action (new Controls, quit, or nothing), unit-tested: q/a and w/s move the right knob by the coarse step, shift-modified keys by the fine step, values clamp at 0 and 1, esc and ctrl-c quit, other keys do nothing
- [x] #2 The readout line for a knob is a pure function unit-tested against at least one cutoff and one gain value, with the real unit computed through asperitas-dsp (no taper or range constants in asperitas-live)
- [x] #3 The step sizes are named constants (coarse 0.05 so end to end is 20 presses, fine 0.005, unless the notes record a reason for other values) with a comment saying why
- [x] #4 Raw mode is left and the cursor restored on normal quit, on error and on panic (a drop guard plus a panic hook); the ticket notes say how this was checked
- [x] #5 With stdout not a TTY, asperitas-live --input <clip> --seconds 5 runs with fixed knobs, prints that the view is disabled, and exits 0 (run it with stdout piped and record the output in the notes)
- [x] #6 crossterm is added only to crates/asperitas-live; CI on main is green after push (checked with gh)
- [x] #7 bash scripts/gates.sh push passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 4d4b78b. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Pure logic in crates/asperitas-live/src/view.rs (handle_key, readout_line, render_view; 8 unit tests); terminal shell in src/terminal.rs; backend::run now takes a wait closure that receives the ControlSlot. crossterm 0.29 added to asperitas-live only. Steps: COARSE_STEP 0.05, FINE_STEP 0.005. Real units via Controls::values and ASPERITAS_PARAMS units, no constants in the crate.
AC4 evidence: ran the binary under a python pty. While running, ICANON was off (raw). After esc and after ctrl-c the output ended with the show-cursor escape and the process exited 0. A temporary panic after raw-mode entry (not committed) printed show-cursor before the panic message, exit 101: the RawGuard drop plus panic hook restore the terminal on normal, error and panic exits.
AC5 evidence: 'asperitas-live --input audio/instruments/mandolin_chord.wav --seconds 5 | cat' printed 'running at 48000 Hz, looping clip', 'stdout is not a terminal: knob view disabled, knobs fixed', 'stream errors: 0, underruns: 0, overruns: 0', exit 0.
AC6: CI run 39341613624 on main succeeded. AC7: scripts/gates.sh push passed (19 gates).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
asperitas-live now shows a crossterm knob view: q/a cutoff, w/s gain, shift for fine steps, esc/ctrl-c quit, writing into the shared ControlSlot. Falls back to fixed knobs when stdout is not a TTY. Shipped in 4d4b78b.
<!-- SECTION:FINAL_SUMMARY:END -->
