# Decision 002: M6 modulation routing, Pod mapping, tracked-root behaviour, first-listen defaults

Status: accepted, 2026-11-21, decided live with the owner (ASP-157.5).

## Context

M6 adds an envelope follower and a smoothed random walk that modulate the resonator, and a pitch
tracker that drives the resonator's root. decision-001 spent every Pod control: knob 1 structure,
knob 2 damping, encoder root, click fine-tune, button 1 excitation preset, button 2 reserved for
the root source, LED 1 root colour, LED 2 input level.

## 1. Modulation targets, polarity, depth law

**Chosen: the follower modulates damping (unipolar); the random walk modulates spread (bipolar).
The routing is one constant table, so changing a target is a one-line edit, not a control.**

- Candidate targets: structure, damping, spread, root offset in cents, excitation drive. Each source
  names one target in the table; the default table is follower -> damping, random walk -> spread.
- Follower polarity is unipolar (depth only lengthens the ring from the knob value, so the knob
  stays the floor); random walk is bipolar around the knob value, clamped to the parameter range.
- Depth law is squared (`depth_eff = depth^2`), for fine control at low depth and a safe first
  listen.

| Alternative | Why not the default |
|---|---|
| Structure | Strongly colouring and fights knob 1, which already sets the base value. |
| Root offset (cents) | Reads as out of tune at larger depth and must be off while the root is tracked. Stays available through the table. |
| Excitation drive | Close to what the excitation stage already does. |
| Linear depth | Coarse at low depth. |

## 2. Pod mapping for depth, rate and on/off

**Chosen: no new controls. Depth and rate are fixed per excitation preset (button 1 recalls
them); button 2 is on/off for the tracked root as reserved in decision-001; modulation is on
whenever the preset's depth is above 0.**

- LED 2 shows modulation activity while modulation is on, and input level otherwise. LED 1 keeps
  the root colour and is tinted while the root is tracked.
- Alternatives: a shift layer on the click (knob positions stop matching values after a layer
  change, and the click already toggles fine-tune); an encoder page (slow, and two LEDs cannot show
  the page). Live depth control can be added after hardware use shows what is reached for.

## 3. Tracked-root behaviour

**Chosen, every value a named constant in one place:**

| Setting | Value | Alternative and cost |
|---|---|---|
| Confidence threshold | 0.8 (the tracker's own) | Lower catches weak notes but lets chords through as wrong roots. |
| Low confidence or unvoiced | Hold the last root | Return to the manual root jumps in pitch; fade needs a fade time. |
| Glide time | 20 ms | 0 ms clicks on estimate errors; long glides sound like portamento. |
| Semitone snapping | On | Off follows bends but leaves the bank a few cents out of tune. |
| Combination with the manual root | Encoder is a transposition offset (semitones and cents) added to the tracked root | Button 2 alone as a switch leaves the encoder dead while tracking. |

Known limits carried over from the tracker (docs/reference/pitch-tracker.md): a close chromatic
cluster reads as a confident wrong note and a major triad reads as no estimate.

## 4. First-listen defaults

Modulation depth 0.15 (squared law), follower attack 5 ms and release 200 ms, random walk rate
0.3 Hz, tracking off, manual root A2 as in decision-001. The first render has depth 0 so it can be
compared A/B with the unmodulated bank. Values are expected to move during the listening round.
