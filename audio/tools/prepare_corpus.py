#!/usr/bin/env python3
"""Convert raw instrument takes into the committed test corpus.

This is a one-off asset tool, not part of the build or the dev shell. It exists so
the transformation applied to the original recordings is auditable and repeatable
rather than folklore. Requires only numpy.

    python3 audio/tools/prepare_corpus.py <raw-takes-dir> [--out audio/instruments]

The raw takes are 44.1 kHz stereo (one mono) at varying length and level. The
corpus is mono 48 kHz 16-bit, trimmed to ~3 s, as TASK-008.02 specifies.

Four transformations, in this order:

1. **Downmix to mono** as (L + R) / 2. Safe here because cross-correlation showed
   0-sample inter-channel lag on every take, so summing cannot comb-filter. The
   channels differ only in level (L ran ~1.2x R).
2. **Resample 44.1 -> 48 kHz** by Kaiser-windowed sinc interpolation, 65 taps,
   beta = 8.6, cutoff at the input Nyquist. Sidelobes land far below the 16-bit
   noise floor.
3. **Trim** to 30 ms before onset, then 3.0 s. Onset is the first sample exceeding
   5% of the clip's peak. Clips already shorter than that are left whole -- the
   fast runs are ~2 s because a fast run is inherently short. A 5 ms fade-in and
   30 ms fade-out prevent a click where the cut lands mid-decay.
4. **Apply one global gain** across all eight clips, scaling so the loudest peak
   in the corpus sits at -3 dBFS.

Step 4 is deliberately a single scalar for the whole corpus, not per-clip
normalization. TASK-008.02 requires soft/hard dynamic contrast, and normalizing
each clip independently would erase exactly that -- the soft and hard plucks would
come out equally loud. A uniform gain preserves every relative level, both within
each soft/hard pair and across clips, while moving the whole corpus into usable
headroom.

Note that this conversion is lossy and one-way: keep the original takes archived
somewhere outside the repo.
"""

import argparse
import os
import wave

import numpy as np

SRC_RATE = 44100
DST_RATE = 48000

# Half-width of the resampling kernel in input samples, and the Kaiser shape
# parameter. 32/8.6 puts stopband ripple well under -90 dB.
KERNEL_HALF_WIDTH = 32
KAISER_BETA = 8.6

TARGET_PEAK_DBFS = -3.0
TRIM_SECONDS = 3.0
PRE_ROLL_SECONDS = 0.030
ONSET_THRESHOLD = 0.05
FADE_IN_SECONDS = 0.005
FADE_OUT_SECONDS = 0.030

# Raw take -> corpus filename. The corpus names come from TASK-008.02.
CLIPS = [
    ("mandolin-a-pluck-soft.wav", "mandolin_single_note_soft.wav"),
    ("mandolin-a-pluck-hard.wav", "mandolin_single_note_hard.wav"),
    ("mandolin-scale.wav", "mandolin_fast_run.wav"),
    ("mandolin-chord.wav", "mandolin_chord.wav"),
    ("octave-mandolin-a-pluck-soft.wav", "octave_mandolin_single_note_soft.wav"),
    ("octave-mandolin-a-pluck-hard.wav", "octave_mandolin_single_note_hard.wav"),
    ("octave-mandolin-scale.wav", "octave_mandolin_fast_run.wav"),
    ("octave-mandolin-chord.wav", "octave_mandolin_chord.wav"),
]


def read_mono(path):
    """Read a 16-bit WAV and return it as mono f64 in [-1, 1]."""
    with wave.open(path) as w:
        if w.getsampwidth() != 2:
            raise SystemExit(f"{path}: expected 16-bit, got {8 * w.getsampwidth()}-bit")
        if w.getframerate() != SRC_RATE:
            raise SystemExit(f"{path}: expected {SRC_RATE} Hz, got {w.getframerate()} Hz")
        channels = w.getnchannels()
        raw = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2")
    samples = raw.astype(np.float64) / 32768.0
    return samples.reshape(-1, channels).mean(axis=1)


def resample(x):
    """Resample from SRC_RATE to DST_RATE by windowed-sinc interpolation."""
    step = SRC_RATE / DST_RATE
    n_out = int(np.floor((len(x) - 1) / step)) + 1
    t = np.arange(n_out) * step
    base = np.floor(t).astype(np.int64)
    frac = t - base

    # Zero-pad so kernel taps at the edges read in-bounds.
    padded = np.concatenate(
        [np.zeros(KERNEL_HALF_WIDTH), x, np.zeros(KERNEL_HALF_WIDTH + 2)]
    )
    out = np.zeros(n_out)
    norm = np.i0(KAISER_BETA)
    for offset in range(-KERNEL_HALF_WIDTH + 1, KERNEL_HALF_WIDTH + 1):
        d = offset - frac
        inside = np.clip(1.0 - (d / KERNEL_HALF_WIDTH) ** 2, 0.0, 1.0)
        window = np.i0(KAISER_BETA * np.sqrt(inside)) / norm
        out += padded[base + offset + KERNEL_HALF_WIDTH] * np.sinc(d) * window
    return out


def trim(y):
    """Trim to PRE_ROLL before onset plus TRIM_SECONDS, with click-free edges."""
    peak = np.abs(y).max()
    loud = np.nonzero(np.abs(y) > ONSET_THRESHOLD * peak)[0]
    onset = int(loud[0]) if len(loud) else 0
    start = max(0, onset - int(PRE_ROLL_SECONDS * DST_RATE))
    y = y[start : start + int(TRIM_SECONDS * DST_RATE)].copy()

    fade_in = int(FADE_IN_SECONDS * DST_RATE)
    fade_out = int(FADE_OUT_SECONDS * DST_RATE)
    y[:fade_in] *= np.linspace(0.0, 1.0, fade_in)
    y[-fade_out:] *= np.linspace(1.0, 0.0, fade_out)
    return y


def write_mono(path, y):
    quantized = np.round(np.clip(y, -1.0, 1.0) * 32767).astype("<i2")
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(DST_RATE)
        w.writeframes(quantized.tobytes())
    return len(quantized)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("raw_dir", help="directory holding the original takes")
    parser.add_argument("--out", default="audio/instruments", help="corpus output directory")
    args = parser.parse_args()

    os.makedirs(args.out, exist_ok=True)

    prepared = {}
    for source, target in CLIPS:
        path = os.path.join(args.raw_dir, source)
        if not os.path.exists(path):
            raise SystemExit(f"missing raw take: {path}")
        prepared[target] = trim(resample(read_mono(path)))

    # One gain for the whole corpus -- see the module docstring on why this is not
    # per-clip normalization.
    corpus_peak = max(np.abs(y).max() for y in prepared.values())
    gain = 10.0 ** (TARGET_PEAK_DBFS / 20.0) / corpus_peak
    print(f"global gain {20 * np.log10(gain):+.2f} dB applied to all clips\n")

    for name, y in prepared.items():
        y = y * gain
        frames = write_mono(os.path.join(args.out, name), y)
        peak_db = 20 * np.log10(np.abs(y).max())
        rms_db = 20 * np.log10(np.sqrt((y**2).mean()))
        print(f"{name:42s} {frames / DST_RATE:4.2f}s  peak {peak_db:6.1f} dBFS  rms {rms_db:6.1f} dB")


if __name__ == "__main__":
    main()
