# Decision 001: M5 resonator - tuning model, voice budget, Pod mapping, default voicing

Status: accepted, 2026-11-19, decided live with the owner (ASP-151.1).

## Context

The M5 resonator needs four choices before the bank and the Pod mapping are built: what the
resonators are tuned to, how many voices there are, what each Pod control does, and where the
first listening round starts. Setup: 48 kHz, 32-sample blocks (667 us per block), mono in, stereo
out, Daisy Pod with 2 knobs, an encoder with click, 2 buttons and 2 RGB LEDs.

## 1. Tuning model

**Chosen: root note plus a structure series, with the root as an explicit input to the bank.**
The end goal is pitch tracked from the input signal.

- M5 ships the manual root: one root pitch, set from the encoder, and a Structure control that
  morphs the voice ratios from harmonic to stretched/inharmonic.
- The owner's stated end goal is a tracked root. The bank takes the root as a plain input, so a
  pitch tracker can later drive the same input with no redesign of the bank. The tracker is not
  part of M5; it belongs with M6 (musical interactivity) and must not be dropped from it.

Alternatives considered:

| Alternative | Why not for M5 |
|---|---|
| Fixed pitches per instrument tuning (mandolin, octave mandolin, upright bass, bass guitar, jazz guitar) | Needs a preset table per instrument and has no inharmonic range, so it plays less like the Rings-style effect the plan names. |
| Pitch tracked from input | The chosen end goal, deferred: a tracker adds its own CPU cost and misbehaves on low bass notes and chords, and is a project in itself. The root input keeps the door open. |

## 2. Voice budget

**Chosen: 8 voices, at most 25% of the 667 us block budget (about 165 us per block).**

Enough for a root plus a rich partial series and stereo spread, with headroom left for the
excitation stage and later the tracker and M6 modulation. The target is checked on the rig in
ASP-151.13; there is no measured CPU number yet, and the CPU-cost harness is M7.

Alternatives: 4 voices at 15% (thin; limits Structure), 16 voices at 40% (leaves too little room
for the tracker and modulation).

## 3. Pod control mapping

| Control | Resonator control |
|---|---|
| Knob 1 | Structure (harmonic to inharmonic) |
| Knob 2 | Damping / decay |
| Encoder turn | Root note, semitone steps |
| Encoder click | Fine-tune toggle: while on, encoder turn moves the root in cents |
| Button 1 | Excitation shaping preset, cycles 3 presets |
| Button 2 | Reserved: root source manual/tracked. No effect until a tracker exists |
| LED 1 | Root note colour |
| LED 2 | Input level |

Why: the two knobs stay on the sound you adjust while playing, the root is a set-and-forget
choice that suits a detented encoder, and button 2 gives the tracker its switch without a remap.

Alternatives: click cycles knob pages (more parameters, but the knob position stops matching the
parameter value after a page change, and two LEDs cannot show the page well); root on a knob
(continuous pitch is a poor way to hit an exact note, and the tracker replaces it anyway).

## 4. Default voicing for the first listening round

Root A2 (110 Hz), Structure 0.0 (pure harmonic), Damping 0.5 (about 1 s ring), excitation preset 1
(gentle), stereo spread wide, 8 voices.

Chosen as the safest first listen: the ring is clearly audible and a tuning or DSP fault stands out.
Exact values are expected to move during the listening round (ASP-151.10).
