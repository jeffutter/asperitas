| Clip | Voiced s | Correct | Within 10 c | Octave | Other harmonic | Other wrong | No estimate | Unvoiced conf mean / max | Unvoiced confident | Onsets (missed) | Latency median / max ms | Reference |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|---|
| mandolin_chord | - | - | - | - | - | - | - | 0.89 / 0.98 | 95.1% | 0 (0) | - / - | offline 4096-sample NSDF (confidence only) |
| mandolin_fast_run | 1.11 | 89.7% | 62.0% | 1.0% | 0.0% | 8.9% | 0.5% | 0.84 / 1.00 | 72.8% | 1 (0) | 40 / 40 | offline 4096-sample NSDF |
| mandolin_single_note_hard | 2.77 | 98.1% | 96.2% | 1.0% | 0.0% | 0.0% | 1.0% | 0.83 / 0.94 | 91.8% | 1 (0) | 27 / 27 | offline 4096-sample NSDF; -3 c from nominal 440 Hz |
| mandolin_single_note_soft | 2.17 | 98.4% | 92.9% | 0.7% | 0.0% | 0.0% | 0.9% | 0.84 / 0.97 | 81.7% | 1 (0) | 32 / 32 | offline 4096-sample NSDF; -4 c from nominal 440 Hz |
| octave_mandolin_chord | - | - | - | - | - | - | - | 0.87 / 0.97 | 89.3% | 0 (0) | - / - | offline 4096-sample NSDF (confidence only) |
| octave_mandolin_fast_run | 0.98 | 87.2% | 74.5% | 0.0% | 4.1% | 7.9% | 0.8% | 0.79 / 1.00 | 57.9% | 1 (0) | 120 / 120 | offline 4096-sample NSDF |
| octave_mandolin_single_note_hard | 2.98 | 99.0% | 98.6% | 0.0% | 0.0% | 0.0% | 1.0% | 0.22 / 0.44 | 0.0% | 1 (0) | 45 / 45 | offline 4096-sample NSDF; -1 c from nominal 220 Hz |
| octave_mandolin_single_note_soft | 1.81 | 98.1% | 96.9% | 0.0% | 0.0% | 0.0% | 1.9% | 0.86 / 0.96 | 90.4% | 1 (0) | 43 / 43 | offline 4096-sample NSDF; -1 c from nominal 220 Hz |
| sweep | 10.00 | 99.5% | 30.9% | 0.0% | 0.0% | 0.0% | 0.5% | - / - | - | 1 (0) | 53 / 53 | exact (generator) |
| notes | 5.29 | 95.6% | 95.6% | 0.0% | 1.5% | 2.0% | 1.0% | 0.03 / 1.00 | 3.4% | 12 (0) | 13 / 53 | exact (generator) |
| noise | 0.00 | - | - | - | - | - | - | 0.19 / 0.46 | 0.0% | 0 (0) | - / - | exact (generator) |

Correct fraction by reference pitch (voiced frames in band):

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
| noise | - | - | - | - | - | - |
