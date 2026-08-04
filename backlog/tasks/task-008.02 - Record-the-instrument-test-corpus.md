---
id: TASK-008.02
title: Record the instrument test corpus
status: To Do
assignee:
  - '@human'
created_date: '2026-08-01 05:57'
updated_date: '2026-08-04 21:49'
labels: []
dependencies:
  - TASK-008.01
parent_task_id: TASK-008
type: task
ordinal: 19000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Requires instruments and a human playing them. An agent cannot produce this.

Record short reference passages for mandolin and octave mandolin. These are the files golden tests and ear checks run against for the rest of the project, so it is worth recording them cleanly and at consistent level.

**Format:** mono, 48 kHz WAV, ~3 seconds each, committed to `audio/`. Keep volume consistent across all clips.

### Mandolin (4 clips)

| File | What to play |
|------|-------------|
| `mandolin_single_note_soft.wav` | Single open A string, gentle pick, let ring |
| `mandolin_single_note_hard.wav` | Same note, aggressive pick attack |
| `mandolin_fast_run.wav` | Fast scale run (A minor: A-C-D-E over 2 beats) |
| `mandolin_chord.wav` | Strummed D major chord, let decay |

### Octave Mandolin (4 clips)

| File | What to play |
|------|-------------|
| `octave_mandolin_single_note_soft.wav` | Single open A string, gentle pick, let ring |
| `octave_mandolin_single_note_hard.wav` | Same note, aggressive pick attack |
| `octave_mandolin_fast_run.wav` | Fast scale run (A minor, same as mandolin) |
| `octave_mandolin_chord.wav` | Strummed D major chord, let decay |
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: short mono 48 kHz clips recorded across the target instruments
- [ ] #2 HUMAN: material includes long-decay plucks, fast passages, chords, and soft/hard dynamic contrast
- [x] #3 Clips committed under audio/ and golden tests extended to cover them
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Corpus committed under audio/instruments/ as 8 mono 48 kHz 16-bit clips, and golden coverage extended from 6 to 22 cases (gain + filter over all 8 clips, alongside the existing 3 synthetic signals).

Recording format: the takes were recorded at 44.1 kHz stereo (one mono) at 2.0-11.6 s, not the mono/48 kHz/~3 s this ticket specified. Converted by audio/tools/prepare_corpus.py (committed for provenance): mono downmix, windowed-sinc resample to 48 kHz, trim to 30 ms pre-onset + 3.0 s with click-free edges. Cross-correlation confirmed 0-sample inter-channel lag first, so the downmix cannot comb-filter. Conversion is lossy and one-way; the original takes are NOT in the repo and must be archived elsewhere.

Levels: one global +5.56 dB applied to the whole corpus (loudest peak to -3 dBFS) rather than per-clip normalization, which would have erased the soft/hard dynamic contrast AC #2 requires. The soft/hard mandolin pair retains a ~7 dB peak gap.

Two clips (both fast runs) are ~2 s rather than 3 s; a fast run is inherently short and they were not padded.

Also required, and fixed along the way:
- wav_io::read_wav rejected mono outright; it now widens mono into both frame slots, since the corpus is mono.
- BUG FOUND: run_process passed the input spec straight to write_wav, which always writes interleaved stereo. With mono input the output header claimed 1 channel while holding 2, so the file read back at double length and half speed. write_wav now owns channel count/bit depth/format itself (caller supplies only the sample rate), making the mismatch inexpressible. Regression test added in process.rs.
- golden_tests.rs refactored to a single generic run_golden over an Input enum (synthetic or recorded clip) before adding 16 cases, rather than multiplying the existing copy-pasted gain/filter pair. Verified the pre-existing 6 goldens still pass byte-identically, so the refactor changed no behavior.
- README's documented process invocation used nonexistent --input/--output flags; corrected to positional args.

Binary weight: audio/instruments 2.1 MiB, audio/goldens 9.3 MiB raw (~5.4 MiB as git objects). WAV kept over FLAC deliberately: hound is PCM-only, and golden regeneration would need libFLAC via flac-bound, the project's first native C dependency, to save ~2 MiB.

Verification: cargo test --workspace all green (22 goldens + 6 wav_io/process unit tests), cargo clippy --workspace --all-targets -D warnings clean, cargo fmt clean, and the CLI smoke-tested end-to-end on a mono corpus clip (144000 frames in, 144000 out, correctly labelled stereo).
<!-- SECTION:NOTES:END -->
