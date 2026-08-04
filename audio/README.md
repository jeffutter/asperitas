# Audio assets

```
audio/
├── instruments/   recorded test corpus — the reference material for golden tests and ear checks
├── goldens/       frozen processor output, regenerated only on purpose
└── tools/         one-off asset preparation
```

## `instruments/` — the recorded corpus

Eight clips, **mono 48 kHz 16-bit PCM**, recorded on real instruments (TASK-008.02).
These are what golden tests and ear checks run against for the rest of the project.

| File | Material | Length | Peak | RMS |
|------|----------|-------:|-----:|----:|
| `mandolin_single_note_soft.wav` | Open A, gentle pick, let ring | 3.00 s | −10.3 dBFS | −30.7 dB |
| `mandolin_single_note_hard.wav` | Open A, aggressive attack | 3.00 s | −3.4 dBFS | −26.3 dB |
| `mandolin_fast_run.wav` | Fast scale run | 2.16 s | −7.4 dBFS | −23.1 dB |
| `mandolin_chord.wav` | Strummed chord, let decay | 3.00 s | −11.4 dBFS | −30.7 dB |
| `octave_mandolin_single_note_soft.wav` | Open A, gentle pick, let ring | 3.00 s | −7.2 dBFS | −28.2 dB |
| `octave_mandolin_single_note_hard.wav` | Open A, aggressive attack | 3.00 s | −3.0 dBFS | −23.5 dB |
| `octave_mandolin_fast_run.wav` | Fast scale run | 1.98 s | −10.2 dBFS | −25.3 dB |
| `octave_mandolin_chord.wav` | Strummed chord, let decay | 3.00 s | −9.0 dBFS | −28.9 dB |

The two fast runs are shorter than 3 s because a fast run is inherently short; they were
not padded.

### Levels are uniformly scaled, not normalized per clip

One global gain (+5.56 dB) was applied to the whole corpus, placing the loudest peak at
−3 dBFS. Every relative level is therefore preserved — both within each soft/hard pair
and between clips.

**Do not normalize these individually.** The soft/hard pairs exist to give the effect
dynamic contrast to react to, and per-clip normalization would erase exactly the property
they were recorded for. Note the ~7 dB peak gap between the soft and hard mandolin plucks:
that gap is the signal.

### Provenance

The originals were recorded at 44.1 kHz stereo (one take mono) at varying length, then
converted by [`tools/prepare_corpus.py`](tools/prepare_corpus.py) — downmix to mono,
resample to 48 kHz, trim, and the single global gain above. That script documents each
step and its reasoning.

**The conversion is lossy and one-way, and the original takes are not in this repo.**
Keep them archived elsewhere; they cannot be recovered from these files.

## `goldens/` — frozen processor output

Twenty-two files: `gain` and `filter` at their default parameters, over three synthetic
signals and all eight instrument clips. Naming is `{processor}_default_{input}.wav`.
Always stereo, because the DSP pipeline works in stereo frames throughout.

Generated and checked by `crates/asperitas-cli/tests/golden_tests.rs`:

```bash
cargo test -p asperitas-cli               # verify against the goldens
UPDATE_GOLDENS=1 cargo test -p asperitas-cli   # regenerate them
```

Regeneration is deliberately opt-in. **A golden diff means "listen to this before
accepting it", not "run the update command"** — that distinction is the entire value of
the mechanism. Goldens are listenable WAVs precisely so you can.

Two caveats worth knowing before reading a diff:

- `GainParams::default()` is 0 dB, so the `gain_default_*` goldens are near-copies of
  their input. They guard the read/widen/process/write chain over real material; the gain
  maths itself is covered by the synthetic cases.
- `FilterParams::default()` is 20 kHz, which at 48 kHz is a mild low-pass rather than a
  true pass-through (α ≈ 0.93), so those goldens do differ audibly little but measurably
  from their input.

## `tools/`

`prepare_corpus.py` is a one-off asset tool, not part of the build or the Nix dev shell.
It needs `python3` and `numpy` only.
