---
id: TASK-145.04
title: >-
  asperitas-live knobs turn from the keyboard while audio plays, in a thin
  crossterm terminal view
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-19 22:09'
updated_date: '2026-10-19 22:09'
labels:
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
- [ ] #1 Key handling is a pure function from (current Controls, key event) to an action (new Controls, quit, or nothing), unit-tested: q/a and w/s move the right knob by the coarse step, shift-modified keys by the fine step, values clamp at 0 and 1, esc and ctrl-c quit, other keys do nothing
- [ ] #2 The readout line for a knob is a pure function unit-tested against at least one cutoff and one gain value, with the real unit computed through asperitas-dsp (no taper or range constants in asperitas-live)
- [ ] #3 The step sizes are named constants (coarse 0.05 so end to end is 20 presses, fine 0.005, unless the notes record a reason for other values) with a comment saying why
- [ ] #4 Raw mode is left and the cursor restored on normal quit, on error and on panic (a drop guard plus a panic hook); the ticket notes say how this was checked
- [ ] #5 With stdout not a TTY, asperitas-live --input <clip> --seconds 5 runs with fixed knobs, prints that the view is disabled, and exits 0 (run it with stdout piped and record the output in the notes)
- [ ] #6 crossterm is added only to crates/asperitas-live; CI on main is green after push (checked with gh)
- [ ] #7 bash scripts/gates.sh push passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Provisional; the planner re-checks it against the engine and control path TASK-145.03 actually shipped.

1. `crates/asperitas-live/src/view.rs`: `enum Action { Set(Controls), Quit, None }`; `fn on_key(c: &Controls, ev: KeyEvent) -> Action` (crossterm `KeyEvent`; shift arrives as an uppercase char and/or `KeyModifiers::SHIFT`, handle both). `const COARSE: f32 = 0.05; const FINE: f32 = 0.005;`. `fn readout(knob: usize, value: f32) -> String` producing the bar, the normalised value and the real unit from `ASPERITAS_PARAMS[knob]` (`from_normalised`, `unit`) or the TASK-145.02 helper; format Hz as kHz above 1000.
2. `fn run_view(shared: &SharedControls, src_label: &str, until: Option<Instant>)`: enable raw mode behind a guard struct whose Drop disables it and shows the cursor; install a panic hook that does the same before chaining the old hook; poll `crossterm::event::poll` with a ~30 ms timeout, apply `on_key`, write to `SharedControls`, redraw the four lines in place (MoveTo + Clear(CurrentLine)); return on Quit or deadline.
3. `main.rs`: if `std::io::stdout().is_terminal()` run the view on the main thread after starting the stream; otherwise print `view disabled: stdout is not a terminal` and sleep until `--seconds` or Ctrl-C.
4. Tests for `on_key` and `readout` (AC #1, #2). Manual check of terminal restore: run in a TTY, quit with esc, then with ctrl-c, then trigger a debug-only panic key if added (or a `--panic-after` test-only flag behind `cfg(debug_assertions)`), and confirm `stty -a` shows `icanon echo` afterwards; record in notes.
5. Piped run (AC #5): `cargo run -p asperitas-live --release -- --input audio/instruments/mandolin_chord.wav --seconds 5 | cat`.
6. `bash scripts/gates.sh push`, commit, push main, `gh run watch`.
<!-- SECTION:PLAN:END -->
