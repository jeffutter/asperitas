# Pitch tracker

`PitchTracker` in `crates/asperitas-dsp/src/pitch.rs` estimates the fundamental frequency of a mono
input and says how sure it is. It is `no_std`, allocation-free and fixed-memory, exported from
`asperitas-dsp` with `PitchParams`, `PITCH_PARAMS` and `PITCH_HOP`, behind the `pitch` cargo feature
(`std` implies it, so host test runs cover it). It is off by default because the firmware's
internal-flash image has only a few hundred bytes of headroom and adding the module, unused, moved
inlining in the resonator enough to overflow `main` by 32 bytes; a firmware that uses the tracker
enables `pitch` and has to find the flash for it. It is a control source, not a
`Processor`. It does not decide what the resonator does with a low-confidence estimate. Only the
accuracy harness (`asperitas-cli pitch`) uses it; the effect chain, live runner, rig and firmware do
not yet. Its accuracy on the recorded corpus, with the failure modes, is in
[pitch-accuracy.md](pitch-accuracy.md).

## API

| call                                | meaning                                                         |
|-------------------------------------|-----------------------------------------------------------------|
| `PitchTracker::new(rate, &params)`  | tracker with no estimate and zero confidence                    |
| `tick(x)` / `process_block(&[f32])` | feed input; a block equals repeated ticks bit for bit            |
| `estimate_hz()`                     | last estimate whose confidence reached the threshold, else 0.0   |
| `confidence()`                      | clarity of the latest analysis, 0..1                            |
| `set_params`, `set_sample_rate`     | threshold (0.5..0.99, default 0.8); rate                        |
| `reset()`                           | forget input and estimate; idempotent and deterministic          |

`estimate_hz` is always finite. Non-finite input samples count as silence.

## Method

McLeod's Pitch Method (McLeod and Wyvill 2005): the normalised square difference function over a
window, key maxima after the first zero crossing, the first key maximum within 0.9 of the highest,
parabolic interpolation, and the peak height as confidence. Chosen over YIN because the normalised
peak height is a 0..1 periodicity measure at no extra cost and key-maximum picking guards against
octave errors on plucked strings. No octave error appeared in the tests below, so the planned
fallback to YIN was not needed.

Two bands keep the cost down (a 41 Hz period is 1171 samples at 48 kHz):

| band | filter                    | rate    | window          | lags (Hz at 48 kHz) |
|------|---------------------------|---------|-----------------|---------------------|
| low  | 4th-order Butterworth 1.5 kHz | 6 kHz   | 384 samples, 64 ms | 8..160 (750..38 Hz) |
| high | 4th-order Butterworth 9 kHz   | 24 kHz  | 160 samples, 6.7 ms | 10..48 (2400..500 Hz) |

The high band is used when its peak is at 550 Hz or above with clarity 0.9 or more; otherwise the
low band decides. A signal that has just stopped gets no estimate from either band (see
[Hard cuts](#hard-cuts)). An analysis runs every 128 input samples (2.7 ms, four 32-frame blocks). A DC
blocker (15 Hz) precedes both bands. At another sample rate the lag limits keep their sample
counts, so the range in Hz scales with the rate.

## Measured accuracy and latency

From `crates/asperitas-dsp/tests/pitch_tests.rs` (run with `PITCH_REPORT=1 cargo test -p
asperitas-dsp --features pitch --release --test pitch_tests -- --nocapture`), 48 kHz, 32-sample blocks, notes
starting at sample 0 with amplitude 0.5. Timbres: sine, band-limited sawtooth, and a pluck (20
partials, partial `k` at amplitude `1/k`, decaying at `1.5 + 0.8 k` per second).

- Tolerance: 10 cents. Worst error after settling across all 21 cases was 5.6 cents (sawtooth,
  1760 Hz); sines are within 0.6 cents.
- Settle time: 100 ms is asserted for every note; measured 4 to 65 ms (41 Hz sine 61 ms, 82 Hz
  64 ms, 110 Hz 29 ms, 220 Hz 16 ms, 440 Hz 8 to 13 ms, 880 Hz 8 ms, 1760 Hz 5 ms).
- Onset latency (silence, then a plucked note; time from the note start to the first block whose
  estimate is within 10 cents): 41 Hz 52 ms, 82 Hz 39 ms, 110 Hz 28 ms, 220 Hz 15 ms, 440 Hz
  12 ms, 880 Hz 7 ms, 1760 Hz 4 ms. The low notes are limited by needing about one period plus
  the settling of the window (one 41 Hz period is 24 ms).

## Rejection

The confidence ceiling is 0.8 (the default clarity threshold, so a rejected input never updates
`estimate_hz`). Measured peak confidence once the windows are full:

| input                                                              | peak confidence |
|--------------------------------------------------------------------|-----------------|
| silence                                                            | 0 exactly       |
| white noise, amplitudes 0.05, 0.5, 0.9                             | 0.32 to 0.33    |
| six open guitar strings (E2 A2 D3 G3 B3 E4, five harmonics each)  | 0.54            |
| C3 E3 G3 Bb3 D4 voicing                                            | 0.48            |
| C3 Eb3 F#3 A3 (diminished seventh)                                 | 0.48            |

## Hard cuts

The low band's 64 ms window outlives a hard cut, and it cannot see a fundamental above about 750 Hz
(lag 8 at 6 kHz), so on a note cut at 880 Hz or higher it would find the second lag, the octave
below, with clarity 0.97 for about 60 ms. The cut also leaves a slow decay in the filtered windows
(the 15 Hz DC blocker's step response), so the band windows do not look silent for that long. The
tracker therefore watches the raw input: it keeps the energy of each of the last 8 hops (21 ms), and
when the newest hop holds under 1% of their mean energy the analysis reports confidence 0 and keeps
the held estimate. The check costs one multiply-add per sample and eight stored floats.

Effect, measured on a decaying 880, 1318.5 and 1760 Hz note cut at once or faded over 5 ms: no
frame after the cut has confidence 0.8 or more at an estimate below 700 Hz, where before 19 to 22
frames (about 55 ms) did. The recorded mandolin and octave mandolin clips decay rather than cut,
and their accuracy rows in [pitch-accuracy.md](pitch-accuracy.md) did not move; only the octave
mandolin chord's confident fraction changed, by 0.1 point. A cut tone of 659 Hz or lower never showed
it, because the low band reads its own fundamental.

## Known failure modes

- A chromatic cluster (C4 C#4 D4 Eb4, adjacent semitones) inside one 64 ms window
  is a single beating tone: peak confidence 0.91 and an estimate near 286 Hz. It is not rejected.
- A major triad (C3 E3 G3, close to 4:5:6) is a harmonic series on its root, 65 Hz, which is
  below the range; it read as confidence 0.51 with no estimate.
- Notes below about 38 Hz are out of range. The low band needs 64 ms of input, so very low notes
  report late, and a note that decays below the energy floor (mean square 1e-8) drops the
  confidence to 0.
- The high band is only trusted at 0.9 clarity, so a bright tone in the 500 to 750 Hz overlap with
  a weak fundamental falls through to the low band.

## Cost

`cargo run --release -p asperitas-dsp --features pitch --example pitch_cost` (host, not a Cortex-M7), against the
667 us block (32 frames at 48 kHz). One block in four runs an analysis; the others only filter and
decimate.

| input        | average  | p90 (analysis block) | p99.9    |
|--------------|----------|----------------------|----------|
| 41 Hz sine   | 8.7 us   | 32.6 us (4.9 %)      | 81 us    |
| 1760 Hz sine | 1.2 us   | 4.0 us (0.6 %)       | 4.3 us   |
| white noise  | 8.6 us   | 33 to 34 us (5 %)    | 39 us    |
| silence      | 0.5 us   | 1.1 us (0.2 %)       | 1.3 us   |

The maximum over 100000 blocks ranged from 33 us to several ms between runs; that is host
scheduling, not the algorithm. An analysis is roughly 55 000 multiply-adds (low band about 48 000,
high about 7 000), which I infer will cost a few times more on the Cortex-M7; that is not
measured, and needs the board.
