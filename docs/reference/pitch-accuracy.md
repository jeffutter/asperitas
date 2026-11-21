# Pitch tracker accuracy

How well `PitchTracker` ([pitch-tracker.md](pitch-tracker.md)) does on the recorded corpus in
`audio/instruments/` and on synthetic signals with exact truth. This is a measurement, not a tuning
pass: nothing here changes the tracker.

The analysis lives in `crates/asperitas-measure/src/pitch_accuracy.rs` (reference pitch, scoring,
onset detection, synthetic signals; no I/O). The commands, WAV reading, CSV and PNG are in
`crates/asperitas-cli/src/pitch_report.rs`, in `asperitas-cli` because `asperitas-measure` has no
WAV or PNG dependency and `asperitas-cli` already has both, the same split as
[analyze.md](analyze.md).

## Commands

```
asperitas-cli pitch <input> <prefix> [--chordal] [--nominal-hz F] [--clarity-threshold T] [--tolerance-cents C]
asperitas-cli pitch-corpus <out-dir> [--corpus-dir audio/instruments] [--clarity-threshold T] [--tolerance-cents C]
```

`<input>` is a WAV file (mono or stereo, 8 to 384 kHz; stereo is averaged) or `synthetic:sweep`,
`synthetic:notes` or `synthetic:noise`. `pitch` writes `<prefix>.csv` and `<prefix>.png` and prints
the table row. `pitch-corpus` runs every `.wav` in the corpus directory in name order and the three
synthetic signals, writes `<clip>.csv`, `<clip>.png` and `table.md` into the output directory, and
prints the tables. A clip with `chord` in its name is scored on confidence only; one with
`single_note` in its name is checked against its open A (220 Hz octave mandolin, 440 Hz mandolin).
Bad flags, an unreadable WAV and an unknown synthetic name are refused with `error: ...` and exit 1.

- CSV columns: `t_s,estimate_hz,confidence,reference_hz,reference_voiced,level_db,error_cents`, one
  row per 128-sample hop, numbers as `{:.9e}`, `error_cents` empty unless both the reference is
  voiced and an estimate is held.
- PNG, 960 by 560: the top panel plots the estimate (red) and reference (dark grey) on a log
  frequency axis from 30 Hz to 2.4 kHz with a line at every A (55, 110, ... 1760 Hz); the bottom
  panel plots confidence (blue) from 0 to 1 with the clarity threshold in orange. Time runs from 0
  to the last frame across both, and green columns mark reference-voiced time.
- Output is byte-deterministic: the same input and flags give the same CSV, PNG and table on a
  given machine (tested). Cross-platform identity of the floating-point text is not claimed.

The committed results are `docs/reference/pitch-accuracy/table.md` and one PNG per clip. The CSVs
are 1.4 MB together, so they are not committed; regenerate everything with
`cargo run --release -p asperitas-cli -- pitch-corpus docs/reference/pitch-accuracy`, which also
writes the CSVs next to the PNGs.

## How the numbers are obtained

The tracker runs over the clip at 48 kHz and its held estimate and confidence are sampled once per
analysis hop (128 samples, 2.7 ms), exactly as the device would see them. Frame `k` is stamped
`(k + 1) * 128 / rate`: the tracker has heard the clip up to that time. The tracker lags a changing
pitch by about half of its own window, and that lag is inside every score below.

**Reference.** The corpus has no ground truth. The reference is an offline analysis of the same
samples: the normalised square difference function over a 4096-sample (85 ms) window at the full
sample rate (no decimation), first key maximum above 0.9 of the highest, parabolic refinement,
lags for 38 to 2400 Hz, window centred on the frame time. A frame is voiced when the window RMS is
at least -50 dBFS and the peak clarity is at least 0.9. It is the same family of method as the
tracker with a longer and finer view, so it is not independent of the method's weaknesses; it is
independent of the tracker's decimation, windows and band split. Checks on it:

- A sine at 41.2, 110, 440 and 1760 Hz is recovered within 1 cent (unit test).
- On the four single-note clips its median voiced frame is within 4 cents of the nominal open A:
  -3 and -4 c on the mandolin, -1 c on the octave mandolin. Nothing was adjusted to make it so.
- It is not always right. On `mandolin_single_note_soft` it reports 880 Hz (the second harmonic) for
  59 voiced frames near the end of the ring, and the tracker's held 440 Hz is scored wrong there.
  That is why the single-note rows are off from 100% by a few frames. Read the PNG: the green
  columns carry the reference.

**Synthetic truth.** `sweep` is a 10 s exponential glide from 41 to 1760 Hz, partials at `1/k` for
`k` up to 4. `notes` is twelve half-second plucked-style notes at 41.2, 55, 82.4, 110, 164.8, 220,
330, 440, 659.3, 880, 1318.5 and 1760 Hz, a 1 ms attack, partial `k` at `1/k` decaying with time
constant `0.4/k` s, and a 60 ms silent gap after each. `noise` is 3 s of deterministic white noise
at -14 dBFS RMS. Their truth is the generator's instantaneous frequency, so there is no reference
error. A note boundary counts against the tracker for about its latency, because the truth changes
at once and the tracker needs a window.

**Metrics.** Over frames where the reference (or truth) is voiced:

- *Correct*: the held estimate is within the tolerance of the truth. *Within 10 c* is the same at
  10 cents, the tolerance [pitch-tracker.md](pitch-tracker.md) states for settled synthetic notes.
- *Octave*: not correct, but within the tolerance of 2x or 0.5x the truth. *Other harmonic*: within
  the tolerance of 3x, 1.5x, 4x, 2/3, 1/3 or 1/4. *Other wrong*: any other estimate. *No estimate*:
  none held yet.
- *Unvoiced conf mean / max* and *Unvoiced confident*: the tracker's confidence over frames where
  the signal is present (window RMS at least -50 dBFS) but the reference is not voiced, and the
  fraction of those frames at or above the clarity threshold of 0.8, so how often the tracker would
  act on a pitch that the reference does not find. For chord clips every present frame is counted.
  On the real clips this set is mostly the ring-out tail where the reference itself has given up
  (clarity under 0.9), so it is not a false-pitch rate there; the `noise` row is the clean figure.
- *Onsets (missed)*: onsets found as the first 128-sample block whose peak envelope (30 ms release)
  is above -50 dBFS and 20 dB above the quietest of the previous eight blocks, at least 150 ms
  apart, that have a voiced reference within 400 ms; missed ones are those where the tracker never
  held a confident, correct estimate within 600 ms. *Latency*: time from the onset to the first
  frame whose confidence is at least the threshold and whose estimate is within the tolerance of the
  reference.

**Tolerance.** Correct means within 50 cents, a quarter tone: the most a root-follow stage can be off
before its semitone snapping lands on the wrong note. This is a provisional figure chosen here.
the decision behind the tracked-root stage sets the real one, and that stage confirms or changes it; `--tolerance-cents` re-runs the table at any value.

## Results

Default clarity threshold 0.8, tolerance 50 cents, 48 kHz. Generated by `pitch-corpus`; the file
is `pitch-accuracy/table.md`, whose Reference column reads "offline 4096-sample NSDF" where it
reads "offline" here.

| Clip | Voiced s | Correct | Within 10 c | Octave | Other harmonic | Other wrong | No estimate | Unvoiced conf mean / max | Unvoiced confident | Onsets (missed) | Latency median / max ms | Reference |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|---|
| mandolin_chord | - | - | - | - | - | - | - | 0.89 / 0.98 | 95.1% | 0 (0) | - / - | offline (confidence only) |
| mandolin_fast_run | 1.11 | 89.7% | 62.0% | 1.0% | 0.0% | 8.9% | 0.5% | 0.84 / 1.00 | 72.8% | 1 (0) | 40 / 40 | offline |
| mandolin_single_note_hard | 2.77 | 98.1% | 96.2% | 1.0% | 0.0% | 0.0% | 1.0% | 0.83 / 0.94 | 91.8% | 1 (0) | 27 / 27 | offline; -3 c from nominal 440 Hz |
| mandolin_single_note_soft | 2.17 | 98.4% | 92.9% | 0.7% | 0.0% | 0.0% | 0.9% | 0.84 / 0.97 | 81.7% | 1 (0) | 32 / 32 | offline; -4 c from nominal 440 Hz |
| octave_mandolin_chord | - | - | - | - | - | - | - | 0.87 / 0.97 | 89.4% | 0 (0) | - / - | offline (confidence only) |
| octave_mandolin_fast_run | 0.98 | 87.2% | 74.5% | 0.0% | 4.1% | 7.9% | 0.8% | 0.79 / 1.00 | 57.9% | 1 (0) | 120 / 120 | offline |
| octave_mandolin_single_note_hard | 2.98 | 99.0% | 98.6% | 0.0% | 0.0% | 0.0% | 1.0% | 0.22 / 0.44 | 0.0% | 1 (0) | 45 / 45 | offline; -1 c from nominal 220 Hz |
| octave_mandolin_single_note_soft | 1.81 | 98.1% | 96.9% | 0.0% | 0.0% | 0.0% | 1.9% | 0.86 / 0.96 | 90.4% | 1 (0) | 43 / 43 | offline; -1 c from nominal 220 Hz |
| sweep | 10.00 | 99.5% | 30.9% | 0.0% | 0.0% | 0.0% | 0.5% | - | - | 1 (0) | 53 / 53 | exact |
| notes | 5.29 | 95.6% | 95.6% | 0.0% | 1.5% | 2.0% | 1.0% | 0.83 / 1.00 | 77.4% | 12 (0) | 13 / 53 | exact |
| noise | 0.00 | - | - | - | - | - | - | 0.19 / 0.46 | 0.0% | 0 (0) | - / - | exact |

Correct fraction by reference pitch, with the number of voiced frames in each band (a frame is
2.7 ms):

| Clip | 0-60 Hz | 60-120 Hz | 120-250 Hz | 250-500 Hz | 500-1000 Hz | 1000+ Hz |
|---|--:|--:|--:|--:|--:|--:|
| mandolin_fast_run | - | - | 89.6% (48) | 89.7% (370) | - | - |
| mandolin_single_note_hard | - | - | - | 98.1% (1040) | - | - |
| mandolin_single_note_soft | - | - | - | 99.1% (755) | 89.8% (59) | - |
| octave_mandolin_fast_run | - | 91.9% (136) | 84.5% (232) | - | - | - |
| octave_mandolin_single_note_hard | - | - | 99.0% (1119) | - | - | - |
| octave_mandolin_single_note_soft | - | - | 98.1% (678) | - | - | - |
| sweep | 95.0% (379) | 100.0% (692) | 100.0% (732) | 100.0% (691) | 100.0% (692) | 100.0% (564) |
| notes | 88.8% (329) | 92.4% (331) | 96.4% (331) | 97.9% (331) | 98.8% (330) | 99.1% (330) |

The clip PNGs are `pitch-accuracy/<clip>.png`.

## Failure modes the numbers show

- **Chords are not rejected.** On the real strummed chords the tracker's confidence is at or above
  the 0.8 threshold on 95.1% of present frames (mean 0.89) for the mandolin and 89.4% (mean 0.87) for
  the octave mandolin, and it holds one estimate for most of the clip (median 146.5 Hz and 73.4 Hz, within a hertz of the
  reference's own median; I infer these are the chord's common sub-harmonic, not a note anyone
  played). This contradicts the synthetic voicings in [pitch-tracker.md](pitch-tracker.md), where
  open-string and triad voicings read 0.48 to 0.54: a real strum with a sustained ring is far more
  periodic than five equal-decay partials. A confidence gate alone will follow a chord.
- **Low bass lags.** The `notes` signal scores 88.8% below 60 Hz and 92.4% from 60 to 120 Hz against
  97.9% to 99.1% above 250 Hz, with 0% octave errors; the remainder is boundary frames. I infer the
  cause is latency: the worst onset latency is 53 ms (the 41 Hz note) in a 440 ms note, 12%.
  Under glide the sweep is correct 95.0% below 60 Hz and 100% above. The octave mandolin runs
  (reference 60 to 120 Hz) score 91.9% there.
- **Fast runs are the weakest real material.** 89.7% (mandolin) and 87.2% (octave mandolin) correct,
  with 8.9% and 7.9% wrong by something that is not an octave or harmonic, and within 10 cents only
  62.0% and 74.5%. The PNGs show the estimate stepping behind each new note and, in the octave
  mandolin run, a long-held low note that the estimate holds correctly while its confidence
  stays high. Onset detection found only one onset in each run (legato notes do not rise 20 dB), so
  no per-note latency is measured for runs; the 120 ms and 40 ms medians are the first note.
- **Noisy attacks cost little.** The hard and soft plucks differ by under 20 ms median latency
  (mandolin 27 vs 32 ms, octave mandolin 45 vs 43 ms) and by under 1 point in correct fraction. The
  hard mandolin pluck has 1.0% octave frames at the very start (visible in its PNG) and 1.0% of the
  voiced time before the first estimate.
- **Ring-out tails mislead.** In the decay after a note the confidence is still high on 81.7% to
  91.8% of the frames where the reference has stopped finding a pitch (three of the four single-note
  clips; the fourth, `octave_mandolin_single_note_hard`, reads 0.22 mean and 0%), and the held estimate sometimes
  moves to a wrong value there (visible at the right of `mandolin_single_note_soft.png`). Pure
  noise is rejected: mean 0.19, max 0.46, 0% above threshold.
- **The reference can octave-err too.** See the soft mandolin note above; scores on real clips have a
  floor of a few frames from the reference's own mistakes.

## Verdict against the root-follow tolerance

The tolerance the root-follow stage needs is not decided yet. As a provisional bar, the stage needs a
tracker that is within 50 cents on at least 95% of single-note time, whose median onset latency is
under 60 ms on a pluck, that is within 50 cents on at least 90% of fast-run time, and whose confidence
gate rejects a chord (at most 10% of chord frames confident). Against it:

| Requirement | Result | Meets |
|---|---|---|
| Single notes, 50 c, at least 95% | 98.1% to 99.0% on all four clips | yes |
| Median onset latency under 60 ms on a pluck | 27 to 45 ms on the clips, 13 ms median and 53 ms worst on the synthetic notes | yes |
| Fast runs, 50 c, at least 90% | 89.7% and 87.2% | no, narrowly |
| Chord rejected by the confidence gate | 95.1% and 89.4% of frames confident | no |
| Noise rejected | 0% of frames confident | yes |

The tracker is good enough for tracked single notes and not for a chord or a fast run without more
than the confidence gate. The tracked-root stage has to add its own protection (for example
requiring a stable estimate before following), or the tracker needs a polyphony measure, or the
owner accepts a chord being followed as its sub-harmonic root. This ticket does not choose.
