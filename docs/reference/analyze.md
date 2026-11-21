# Resonator analysis

`asperitas-cli analyze <prefix> [--params key=value ...] [--sample-rate 48000] [--length-s S]`
shows what the resonator bank does at a setting without listening: its impulse response and its
magnitude frequency response, as CSV to diff and a PNG to look at, plus the detected peaks on
stdout. The code is `crates/asperitas-cli/src/analyze.rs`.

It lives in `asperitas-cli`, not `asperitas-measure`: `asperitas-measure` analyses rig captures and
depends on the host and wire crates, while `asperitas-cli` already holds the `process` renderer, the
`--params` parser (`parse_resonator_controls`, so the keys and refusals are the ones `process`
gives), and the PNG drawing the review page uses. The review page embeds the PNG
(`[[response]]` in [review.md](review.md)).

## Command

`--params` takes the real-unit keys of [resonator.md](resonator.md#controls). An unknown key, a
non-numeric value or an out-of-range value is refused with a message naming it; so are a
`--sample-rate` outside 8000 to 384000 Hz and a `--length-s` outside 0.01 to 10 s. Exit 1 on any
refusal.

The input is one unit impulse on both channels at sample 0 through the bank at the given
parameters, with the excitation stage at its preset (the default preset is transparent). Smoothed
parameters are snapped first, so spread and decay are at their targets from the first sample.
`--length-s` defaults to the root T60 (`10 s * 0.01^damping`, at most 10 s). The response is the
FFT of the whole impulse response, zero-padded to a power of two, with no window; a ring still
audible when the length ends shows as ripple around the peaks.

## Files

`<prefix>_impulse.csv`: `n,t_s,left,right`, one row per sample.

`<prefix>_response.csv`: `freq_hz,left_db,right_db,diff_db`, bins 0 to Nyquist, magnitude in dB
(20 log10, floored at -240 dB), `diff_db = left_db - right_db`, the stereo spread's left/right
difference.

Numbers are `{:.9e}`. The output is deterministic: the same arguments give byte-identical files.

`<prefix>.png`, 960 by 560 pixels, left channel red, right blue:

- Top: the impulse response as a min/max stroke per column, scaled to its largest absolute value,
  zero in the middle, time from 0 at the left edge to the length at the right.
- Bottom: magnitude response on a log frequency axis from 20 Hz at the left to Nyquist at the right.
  The top is the next multiple of 10 dB above the strongest bin, with 90 dB of range below it.
  Vertical lines at 100 Hz, 1 kHz and 10 kHz, horizontal lines every 20 dB.

## Peaks on stdout

One line per local maximum of the left/right mean power response that is within 50 dB of the
strongest and beats every bin within a quarter of the root. `peak_hz` is refined by a parabola
through the three bins around it. `partial` is the nearest of the expected pitches
`root * n * sqrt(1 + 0.02 * structure * (n^2 - 1))` below 0.49 of the sample rate, `cents` the
difference from it, and `left_db`/`right_db` the level of each channel at the peak.

```
$ asperitas-cli analyze out/default
partial expected_hz peak_hz cents db left_db right_db
      1      110.00  109.90   -1.7   -3.0    -2.3     -4.0
      2      220.00  219.90   -0.8   -3.1    -5.2     -1.7
      3      330.00  329.93   -0.4   -3.1    -1.3     -6.6
      4      440.00  439.95   -0.2   -3.1    -8.4     -0.8
      5      550.00  549.97   -0.1   -3.0    -0.4    -10.7
      6      660.00  659.98   -0.0   -3.0   -14.2     -0.2
      7      770.00  770.04   +0.1   -3.1    -0.1    -20.1
      8      880.00  880.05   +0.1   -3.1   -29.6     -0.1
```

That is the default voicing (110 Hz, structure 0, damping 0.5, spread 1): eight peaks on the
harmonic series within 2 cents, odd partials stronger on the left and even partials on the right.
`--params structure=1` moves partial 8 to about 12 times the root, `spread=0` makes the two
channels identical, and a larger `damping` shortens the impulse response and lowers the upper peaks
relative to the root.

## Trace

`asperitas-cli trace <in.wav> <prefix> [--params key=value ...] [--automation FILE]` is the sibling
for a clip: `analyze` takes no input, so it cannot show how the follower, the random walk and the
tracked root move over a real signal. It renders the clip exactly as
`process --processor resonator` does (the same `TrackedResonator` on the same 32-frame block grid,
the same `--params` and `--automation` keys with the same refusals, see
[resonator.md](resonator.md#command-line) and [automation.md](automation.md#resonator-keys)) and
records the stage's signals once per block. The code is `crates/asperitas-cli/src/trace.rs`. Exit 1
on any refusal.

`<prefix>_trace.csv` has one row per block, numbers as `{:.9e}`, identical between runs:

| column | meaning |
| --- | --- |
| `block` | block index from 0 |
| `t_s` | time of the end of the block in seconds |
| `follower` | envelope follower level, 0 to 1 |
| `walk` | random walk value, -1 to 1 |
| `estimate_hz` | the tracker's held estimate; 0 before the first confident one |
| `confidence` | the tracker's clarity at its latest analysis, 0 to 1 |
| `root_hz` | the root the bank is tuned to: tracked root and modulation included |

`<prefix>_trace.png` has three panels over the clip's duration, left edge to right edge:
the follower (red) and walk (blue) on one scale from -1 at the bottom to 1 at the top with the zero
line in the middle; the confidence (red) from 0 to 1 with a line at the 0.8 root-follow threshold;
and the estimate (blue) and `root_hz` (red) on a log frequency axis spanning the octaves they use,
a line at each octave. The follower and walk are traced whether or not their depths are above 0.
