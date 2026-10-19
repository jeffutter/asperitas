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

Fifteen files: `gain`, `filter` and `asperitas` (the full effect chain) at default and
non-default parameters, over synthetic signals and a few instrument clips. Naming is
`{processor}_{variant}_{input}.wav`, where the variant is `default` or a tag for the
parameters (`cutoff500`, `boost12`, ...). **32-bit float stereo**, so levels above full
scale are kept and quiet tails are not quantised away.

Generated and checked by `crates/asperitas-cli/tests/golden_tests.rs`:

```bash
cargo test -p asperitas-cli               # verify against the goldens
UPDATE_GOLDENS=1 cargo test -p asperitas-cli   # regenerate them
```

Regeneration is deliberately opt-in. **A golden diff means "listen to this before
accepting it", not "run the update command"** — that distinction is the entire value of
the mechanism. Goldens are listenable WAVs precisely so you can.

Each case runs through `asperitas_cli::process::run_process`, the same entry point as the
CLI, and is compared sample by sample at an absolute tolerance of 1e-6 (last-bit float
variation only).

Two caveats worth knowing before reading a diff:

- `GainParams::default()` is 0 dB, an identity, so there is only one default-parameter gain
  golden (`gain_default_pluck`); it guards the read/process/write chain. The gain maths is
  covered by the `cut12`, `boost6` and `boost12` cases, the last of which exceeds full scale.
- `FilterParams::default()` is 20 kHz, which at 48 kHz is a mild low-pass rather than a
  true pass-through (alpha about 0.93). The `cutoff500` impulse golden is the one to read for
  the filter's slope and decaying tail.

## `captures/` (repo root) - on-device evidence

Rig evidence lives in one tree at the repository root, one directory per hardware verification
ticket:

```
captures/TASK-x/<date>-rig-<what>.console.zst   raw framed-console capture, zstd
captures/TASK-x/<date>-rig-<what>.report.json    its `asperitas-measure analyze` verdict
captures/TASK-x/<name>.txt.zst                   text output of other bench tools, zstd
```

The console capture is the artifact, not the PCM: it carries the device's own `CAPSTAT`,
`DUMPEND` and gate lines beside the audio, and the PCM is re-derived from it, refusing any block
it cannot prove:

```sh
zstd -dc captures/TASK-x/<name>.console.zst \
  | cargo run -p asperitas-host --release --bin dump_reassemble -- --out capture.pcm
```

A report's `capture` field is the capture's path relative to the repository root, so a report
names its raw capture in any checkout. It is the path as given when the capture lies outside the
repository, and `null` in the two `exploratory-*` reports, whose raw captures were never
committed. A capture that a test names as a fixture stays in its ticket directory; the test
points there.

Size policy: plain git, no LFS. Captures are already compressed, so every text capture is
zstd-compressed (`zstd -19`) and JSON reports stay uncompressed because they are small and
diffable. Growth is about 3 MB a day in hardware weeks and the tree is about 32 MB. Review it
when `captures/` passes about 100 MB, and then consider archiving the older evidence outside the
repository. Purging old blobs from history needs the owner.

## `tools/`

`prepare_corpus.py` is a one-off asset tool, not part of the build or the Nix dev shell.
It needs `python3` and `numpy` only.
