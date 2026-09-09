---
id: TASK-038.01
title: Add stimulus synthesis processors to asperitas-dsp with host property tests
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-09 11:31'
updated_date: '2026-09-09 14:46'
labels:
  - planned
dependencies: []
modified_files:
  - crates/asperitas-dsp/src/stimulus.rs
  - crates/asperitas-dsp/src/lib.rs
  - crates/asperitas-dsp/tests/property_tests.rs
  - crates/asperitas-dsp/tests/stimulus_tests.rs
  - crates/asperitas-cli/tests/stimulus_shape_tests.rs
parent_task_id: TASK-038
priority: high
type: task
ordinal: 55500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-035 deconvolves a measured return against a stimulus it must be able to reproduce exactly, and TASK-019.03 sweeps parameters against it. Neither is possible unless the waveform comes from one implementation that firmware and host can both reason about. This ticket owns that implementation and nothing else: no firmware, no transport, no SDRAM.

Add `crates/asperitas-dsp/src/stimulus.rs` with three sources behind the existing `Processor` trait (`tick` primitive, typed `Params`, `reset`, no allocation, no `Result`, libm-only math so the module stays `no_std`):

- **Sine** — a wrapping integer phase accumulator (NCO), not per-sample `sinf` and not a 2-pole recurrence. The accumulator wraps exactly, so frequency is exact, phase starts on a sample boundary, and amplitude cannot drift across a multi-minute run. Frequencies are chosen as integer periods per window (coherent sampling) so host analysis needs no leakage window.
- **Exponential sweep (ESS)** — exponential chirp, not linear. Equal energy per octave keeps the recovered impulse response usable and places harmonic distortion outside it. No amplitude pre-equalization: pre-emphasis time-smears the recovered IR (Farina's refinements paper).
- **Impulse train** — a peak-normalized band-limited pulse (windowed sinc / Dirichlet kernel), not a one-sample delta. A naive click is flat to Nyquist by construction and puts its energy exactly where the analog loop is least trustworthy.

One level convention for all three: `level_dbfs` f32, default −20 dBFS, the level TASK-034 records loop gain at.

This module is also the single owner of the machine-readable description of what was played, because device and host must never disagree about it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `crates/asperitas-dsp/src/stimulus.rs` provides three stimulus sources (sine, exponential sweep, band-limited impulse train) that implement `Processor` with typed `Params`, no allocation, no `Result`, libm-only float math, and they compile into the firmware dependency graph without `std`.
- [x] #2 Sine uses an integer phase accumulator whose wrap is exact: a host test asserts that a window whose length is an integer number of periods contains an integer number of cycles, and that relative frequency error stays below 1e-7.
- [x] #3 Amplitude is exact rather than approximate: peak magnitude equals the requested `level_dbfs` within a stated tolerance, is identical across repeated runs, and is unchanged after 28,800,000 samples (ten minutes at 48 kHz) so long captures cannot silently sag.
- [x] #4 The sweep spans its documented f0..f1 exponentially over an exact integer sample count and reports its endpoints; the impulse train emits a peak-normalized pulse whose energy is spread over more than one sample, asserted against a naive one-sample click.
- [x] #5 Each source has a `describe()` rendering its parameter set as a space-separated `key=value` string (name, sample rate, level, frequency or chirp endpoints, repetition period) — into a caller-owned byte buffer rather than an owned `String`, because the firmware path has no `alloc` — and that method is the only place the grammar is written; a host test pins the exact strings so renaming a parameter fails CI instead of quietly desynchronising device from host.
- [x] #6 `crates/asperitas-dsp/tests/property_tests.rs` covers all three sources for finite output, bounded output, reset idempotence, block-equals-tick, and bit-identical determinism, and `cargo test --workspace` passes with default (no_std) features.
- [x] #7 Where waveform shapes overlap `crates/asperitas-cli/src/synth.rs`, the two implementations are compared in a host test or their difference is documented with a reason, so neither diverges silently.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# Plan — stimulus sources in `asperitas-dsp`

## Shape of the work

One new module (`crates/asperitas-dsp/src/stimulus.rs`), two lines in `lib.rs`, three test files.
**No sub-tickets.** The three generators share one level convention, one `describe()` grammar and
one end-of-run taper rule; the crate does not compile half-written, so the pieces cannot ship
independently, and each split ticket would spend its own planning round re-deriving these same
decisions. Follows the skill's rule against splitting tightly-coupled work.

**There is no hardware step and nothing to mark `HUMAN:`.** AC #1's "compiles into the firmware
dependency graph without `std`" is provable from the host: `f32::sin`/`f32::powf`/`f32::ln` are
std-only methods, so a slip fails the default-feature build of `asperitas-dsp` (which is the
no_std path, `lib.rs:1`) and CI's `cd firmware && cargo build --release --features seed3`. Nothing
in `firmware/src` consumes stimulus yet — that is TASK-038.03. Do not invent a firmware edit or a
bench step to "prove" AC #1; report the two build results instead.

House style to copy verbatim in structure: `gain.rs` / `filter.rs` (module doc → `use libm::…` by
name → `#[derive(Clone, Debug)] *Params` → hand-written `impl Default` (never `derive(Default)`) →
processor struct with private fields → `impl Processor` with `if hz > 0.0` guard → `tick` returning
an array literal with `.clamp(-1.0, 1.0)` per channel → `reset` → separate `impl` for helpers →
`#[cfg(feature = "std")]` CLI parser last). Citations are prose naming the technique plus the
formula inline (`synth.rs:47`); the repo has no bracket-year bibliography style — do not start one.

## Decisions this plan makes (each was measured, not assumed)

### D1 — One level convention: peak sample-full-scale

`level_dbfs` means **peak** magnitude, 0 dBFS = a sample of ±1.0, so −20 dBFS ⇒ peak 0.1 and a
sine's RMS sits 3.01 dB lower. State this in the module doc with one sentence naming the
alternative conventions (IEC 61606-3 and ITU P.381/P.382 reference 0 dBFS to an r.m.s. amplitude;
BS.1770 calls a 0 dBFS sine −3.01 LKFS) so TASK-035 does not normalise TASK-034's loop gain
against the wrong quantity. Default `−20.0`, matching TASK-034.

### D2 — Sine: integer residue NCO with a crest-aligned initial phase

Keep the phase register integral, exactly as the description asks. State is one `u32` residue in
`[0, fs)` advanced by whole-Hz frequency with a single conditional subtract:
`residue += freq_hz; if residue >= fs { residue -= fs }`. The angle comes out per sample as
`φ = 2π·residue/fs + φ0` in f64 through `libm::sin`. No float ever accumulates, so there is no
drift to bound: the sequence repeats with period `P = fs/gcd(f, fs)` and `out[n + P] == out[n]`
bit-for-bit, forever. That is also the machine-checkable form of AC #2 — frequency is exact by
construction, so the 1e-7 budget is met by arithmetic rather than by measurement.

This requires frequency in whole hertz: `SineParams::frequency_hz: u32`, default 1000. A float field
would promise a resolution the module deliberately refuses, because coherence needs both `f` and
`fs` integral. Firmware that wants a knob still gets one: `params_from_normalised(knob: f32)` maps
to `u32` hertz the way `filter.rs:83-101` maps to a cutoff, and gets its own `normalised_tests`
module like `gain.rs:114-162`. Clamp into `1..=fs/2` in `set_params`, mirroring `filter.rs:61`.
(An alternative `u32` DDS register accumulating a rounded `f·2^32/fs` increment was rejected: the
rounding alone costs 5.6e-8 relative at 1000 Hz — inside an order of magnitude of the 1e-7 budget —
and it destroys short-window periodicity.)

Peak amplitude is **not** free, even with an exact NCO. With `P = fs/gcd(f, fs)`, a sample lands on
the sine crest only when `P ≡ 0 (mod 4)`. Measured at 48 kHz starting at phase 0: 8000 Hz and
16000 Hz peak 1.25 dB low, 4800/9600/14400/19200 Hz 0.44 dB low, and 24000 Hz (Nyquist) outputs
identically zero. Fix it by choosing the initial phase instead of scaling the output: take
`k0 = (P + 2)/4` (nearest quarter period; ties are symmetric and either side reaches the crest) and
`φ0 = π/2 − 2π·k0/P`. Measured peak over 1, 20, 997, 1000, 4800, 8000, 16000, 19200 and 24000 Hz:
`1.0000000000000000` — exact, not approximately — and the same holds after scaling by
`amp = 10^(level/20)`. AC #3's tolerance is therefore **1e-6 relative**, stated as headroom for f32
rounding (~6e-8) with nothing else contributing; put the unaligned numbers in the comment so the
rule is never "simplified" back to `φ0 = 0`.

### D3 — ESS: closed-form phase integral, exact integer length, mandatory end taper

Use the same expression as `synth.rs:44-48` — `φ(n) = ω0·T·(r^(n/N) − 1)/ln r`, `r = f1/f0`,
`T = N/fs`, all in f64 via `libm::pow`/`libm::sin`. Naive left-Riemann accumulation of instantaneous
frequency differs from this by up to **1.31 rad** over an 8 s 20–20 kHz sweep, so it is not an
equivalent implementation and must not appear. The closed form is also why `synth.rs:16-36`'s f64
argument binds here and only here: the sweep's integral reaches 145,385 rad at 8 s, where an f32 ULP
is 0.017 rad. Say so in the comment, so nobody reads D2's integral sine phase as license to drop the
sweep to f32 too.

Expose `total_samples: u32` (default 384_000 = 8 s), *not* a duration: `synth.rs:9`'s
`(sample_rate_hz as f32 * duration_secs) as usize` truncation is the bug class AC #4 rules out.

Endpoint convention, stated in the doc comment because device and host otherwise disagree about
what "spans f0..f1" means: instantaneous frequency at sample `n` is `f0·r^(n/N)`, so `t = T` is the
endpoint *time* and is never emitted; the last sample sits at `f0·r^((N−1)/N)` = 19999.64 Hz for the
defaults. Assert that value in a test rather than putting it on the wire.

Apply a raised-cosine taper over `const FADE_SAMPLES: usize = 480` (10 ms) at **both** ends. Without
it the sweep ends at −0.963 — effectively a step, which Farina's AES 122 paper says spreads energy
across the spectrum; he also warns against removing the fade-out entirely. A short time-domain fade
is not the amplitude pre-equalization that smears the IR (that warning is about the ±3 dB/octave
weighting), and the useful IR is far from both boundaries: harmonic order *k* arrives
`T·ln k/ln(f1/f0)` early, i.e. 0.80 s (2nd), 1.27 s (3rd), 1.61 s (4th), 1.86 s (5th) for the default
sweep. Make it a const, not a param — device and host link the same source, so a shared const is
agreement by construction and a knob would be a decision nobody made.

Normalise the sweep by a **scan**, not an assumption: after applying the taper, loop `n` over the
whole record in `set_params` taking `max |waverform(n)|` in f64 and divide by it. Measured max for
the defaults is 0.99999999999689, so the scan is a no-op there, but it is what makes AC #3's
tolerance true for arbitrary `f0/f1/N` instead of lucky. Cost is one extra pass at parameter-set
time (~384k `libm::sin` calls, milliseconds, no allocation). No pre-emphasis anywhere.

### D4 — Pulse train: closed-form Dirichlet kernel, driven by an integer index

`p(m) = sin(π(2K+1)m/N) / ((2K+1)·sin(πm/N))`, `m` = sample index mod `N`, with an explicit
`|sin(πm/N)| < ε ⇒ return 1.0` branch for the crest. This is the sum of harmonics 1..K of a train
with repetition period `N` samples — exactly periodic, exactly band-limited to `K·fs/N`, peak
exactly 1.0 at `m ≡ 0` **by construction**, and it needs no float phase accumulator at all, so
device/host bit-identity is trivial rather than something to prove. Two `libm::sin` calls and one
division per sample.

Params: `period_samples: u32` (default 480 ⇒ 100 Hz repetition) and `max_frequency_hz: f32`
(default 8000 ⇒ `K = floor(max_hz·N/fs) = 80`). Physical units in the interface, integer arithmetic
inside, like `FilterParams::cutoff_hz`. Clamp `K` to `(N − 1)/2` in `set_params`.

The clamp is load-bearing: as `K → (N−1)/2` the sampled kernel degenerates to a delta (measured
peak-energy fraction 0.9979 at `N=480, K=239`; 0.5021 at `K=120`; 0.3417 at `K=80`), which is
precisely the naive click AC #4 rejects. With the chosen defaults only ~34% of the energy sits in
the crest sample and 3 samples reach ≥½ peak, versus 100% and 1 for a click — assert those two
shapes against an explicitly-built click reference. A Hann-windowed sinc would give lower sidelobes
(−31 dB vs the Dirichlet's −13.5 dB first sidelobe) at the cost of a wider main lobe and a window
expression to keep in sync; the Dirichlet form is the one whose peak normalization is analytic.
Puckette's BLIT paper is the named prior art but its text could not be extracted to quote a
construction, so cite it as a pointer and derive the formula in the comment.

### D5 — `describe()` writes bytes; it does not return a `String`

AC #5 says "string"; the firmware path has no `alloc` anywhere in `crates/*` or `firmware/*`
(confirmed by grep; `heapless` is transitive-only, never a direct dep). So mirror the shape the
console already uses at `console.rs:179-196`: render into a caller-owned buffer, return the byte
count.

```rust
pub trait Stimulus {
    /// Render the parameter set as space-separated `key=value` fields. Returns bytes written.
    fn describe(&self, out: &mut [u8]) -> usize;
}
```

A trait rather than three inherent methods, because TASK-038.03 emits exactly one `RIGCFG` record
from whichever source is compiled in and must not carry three code paths. Take `&mut [u8]` rather
than `&mut [u8; BODY_WINDOW]` so dsp stays ignorant of logging's geometry.

Grammar ownership (AC #5's "only place the grammar is written"): one private helper renders the
shared prefix `name=<source> sample_rate_hz=<int> level_dbfs=<x.x>` and owns the separator rule —
single spaces, no trailing space, no CRLF (the frame supplies the delimiter, per
`panic_handler.rs:88-101`). Each impl appends only its own fields:

- `Sine` → `frequency_hz=<int> period_samples=<int>`
- `ExponentialSweep` → `f0_hz=<x.x> f1_hz=<x.x> total_samples=<int>`
- `PulseTrain` → `period_samples=<int> max_frequency_hz=<x.x>`

Floats at `{:.1}`, integers bare, so the pinned strings are stable. Derived quantities (crest
phase, K, taper length, scanned peak scale) stay off the wire: host and device link this same crate,
so the algorithm is shared code, not metadata to negotiate. Logging's `proto=` covers framing
version skew; adding a second version field here was considered and rejected.

`TruncWriter` (`asperitas-logging/src/lib.rs:104-141`) is `pub(crate)` and dsp must not depend on
logging — dsp is a leaf that cli, firmware and rig all sit on. Duplicate the ~20-line
truncating `core::fmt::Write` privately in `stimulus.rs` with a comment naming `console.rs` as its
source; that duplication is cheaper than inverting the dependency.

Budget: worst-case rendering must stay under `frame::MAX_BODY = 200` (public, but unreachable from
dsp — hardcode 200 in the test with a comment pointing at `asperitas-logging/src/frame.rs:93`),
asserted with the worst-case-value tripwire style of `console.rs:327`.

### D6 — Envelope is counter-driven, not `Smoother`

`smooth::Smoother` is a one-pole with no reached-target query, so its ramp endpoints are not exactly
reproducible sample-for-sample. Generators use the deterministic raised-cosine counters from D3
instead. Say why in the comment, since it breaks the gain/filter habit visibly.

Consequence: do **not** implement `*_param_change_smooth` for the generators. AC #6 omits it on
purpose — stimulus parameters are frozen for the duration of a capture, and a smoothed level ramp
would corrupt the very calibration the capture exists to measure. `*_silence_in_silence_out` is
also omitted because these sources ignore their input and the property would be vacuous. Both
omissions get one line each in the test-file header comment so nobody "restores symmetry" later.

## Steps

1. **`stimulus.rs`** — module doc (level convention D1, why exponential/band-limited/exact-NCO per
   the ticket description, the f64 rationale inherited from `synth.rs:16-36`), then `Sine`,
   `ExponentialSweep`, `PulseTrain` + their `*Params`, the `Stimulus` trait, the private
   `DescWriter` and prefix helper. All three emit `[v, v]` (mono duplicated, as `synth::to_stereo`
   does) and `.clamp(-1.0, 1.0)` at the return. Do not override `process_block`. `reset()` follows
   `filter.rs:76-80`: zero the counters/phase back to their constructed values field by field.
   Skip the std-gated `parse_params_from_cli` unless a caller appears — `-D warnings` will reject
   dead code, and TASK-038.03 selects stimulus by cargo feature, not by CLI.
2. **`lib.rs`** — `pub mod stimulus;` alphabetically (after `processor`, before `smooth`) plus one
   `pub use stimulus::{…}` line; both access paths stay live, as with `gain`/`filter`.
3. **`crates/asperitas-dsp/tests/property_tests.rs`** — append three `proptest!` blocks behind the
   house banner-comment style, five properties each: `sine_output_always_finite`,
   `sine_output_bounded`, `sine_reset_idempotent`, `sine_block_equals_tick`,
   `sine_bit_identical_determinism`, mirrored for `ess_` and `pulse_`. Reuse `arb_block()` and
   `EPSILON`. Determinism compares `.to_bits()`, not `EPSILON` — bit identity is the claim, and
   `to_bits` also sidesteps any float-comparison lint. Header comment records the D6 omissions.
4. **New `crates/asperitas-dsp/tests/stimulus_tests.rs`** — plain `#[test]`s for the numeric ACs:
   coherence (`out[n] == out[n + P]` bit-for-bit over two periods — that *is* "an integer number of
   cycles in the window", and stronger than counting crossings); long-run peak; peak-vs-dBFS over a
   tone table that includes 8000 Hz and 16000 Hz as the regression tripwire for D2; ESS endpoint
   value, `|last sample| < 0.05` after the taper, peak level; pulse spread vs click; `describe()`
   pinned literals + 200-byte budget; reset-then-replay bit identity.
   **AC #3's ten-minute run:** advance the residue accumulator through all 28,800,000 steps but call
   `sin` only during the first and last 48,000 of them, and assert the two peaks are equal via
   `to_bits()`. Integer arithmetic cannot sag, so the assertion is exact equality rather than a
   tolerance, and skipping 28.7M `sin` calls keeps a debug-mode test at milliseconds instead of
   seconds. Note that in the test comment — the cheapness is a consequence of D2, not a weakening.
5. **New `crates/asperitas-cli/tests/stimulus_shape_tests.rs`** (AC #7) — this is the only place a
   comparison can live: cli depends on dsp, never the reverse. Compare the ESS interior — excluding
   the 480-sample tapered margins — against `synth::generate_sweep(48000, 1.0)` rescaled to
   −20 dBFS, tolerance 1e-6 absolute in f64; both use the identical closed form so the residual
   should be near zero and a real divergence shows up immediately. Alongside it, a doc comment
   listing the four intentional differences with reasons: full-scale vs −20 dBFS, hardcoded 20–20k
   vs parameterised endpoints, truncated `(fs·dur) as usize` vs `total_samples`, and `generate_impulse`'s
   single-sample delta vs the band-limited pulse. Leave `audio/goldens/` alone — stimulus is not in
   the cli processing path, so if a golden moves, something leaked into `gain`/`filter`.

## Verification gates

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings
cargo test --workspace
cargo test --workspace --features asperitas-pod/pod-hw
cargo build -p asperitas-dsp --target thumbv7em-none-eabihf   # belt-and-braces for AC #1
cd firmware && cargo build --release --features seed3
```

The default-feature `cargo test --workspace` is the one that matters for AC #5/#6: dsp's integration
tests compile against the no_std library build, so `describe()` must be byte-buffer based (D5) or it
will not exist in the test binary. Report each gate's result in the final summary; a passing build is
not evidence of a working waveform, which is why steps 4 and 5 assert shapes rather than compilation.

## Risks worth carrying forward

- TASK-035 regenerates the stimulus for deconvolution. It should call `asperitas_dsp::stimulus`
  directly rather than re-derive waveforms in `asperitas-rig` — that is what "one implementation"
  buys, and it is why the shape rules live in code rather than in `describe()`.
- If a proptest fails it will write a `.proptest-regressions` file; commit it deliberately or delete
  it deliberately.
- No dither here (already decided upstream): uncharacterised truncation spurs read as distortion.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Reference points in this repo

- `crates/asperitas-dsp/src/processor.rs` — `Frame = [f32; 2]`, `Processor` with associated `Params: Clone + Default`; `process_block` is provided and zips to the shorter side, never panics.
- `crates/asperitas-dsp/src/gain.rs` / `filter.rs` — house style: scalar f32 per-sample `tick`, `libm::*` only, output `.clamp(-1.0, 1.0)`, params mapped through a `params_from_normalised(knob)` helper, std-gated CLI parsing kept out of the firmware path.
- `crates/asperitas-dsp/src/smooth.rs` — `pub(crate) Smoother`, usable here for click-free ramps at module start/stop.
- `crates/asperitas-cli/src/synth.rs` — existing host-side `generate_sweep` accumulates phase in **f64** via libm, and its doc comment is the recorded reason: f32 ULP at ~18,000 rad exceeds the 1e-4 golden tolerance and platform libm variance already broke goldens once. Accumulate sweep/NCO phase in f64 for the same reason; the M7 FPU makes the cost irrelevant at 48 kHz.
- `crates/asperitas-dsp/tests/property_tests.rs` — proptest 1, strategies build blocks of 4..64 / 64..256 / 256..512 frames; properties are named `output_always_finite`, `output_bounded`, `silence_in_silence_out`, `reset_idempotent`, `block_equals_tick`, `param_change_smooth`. Match those names for the new sources.
- Firmware consumes this crate without `std` (`firmware/Cargo.toml`), so anything using `f32::sin` instead of `libm::sin` breaks the cross build, which CI runs as `cd firmware && cargo build --release --features seed3`.

## Prior art worth citing in the code comments

- Renesas TB318, "The NCO as a Stable, Accurate Synthesizer"; U. Maine NCO tutorial; ADI "An Almost Pure DDS Sine Wave Tone Generator".
- Farina, "Simultaneous measurement of impulse response and distortion with a swept-sine technique" (AES 5083) plus its refinements paper — clock mismatch smearing (eliminated here by one codec), synchronous averaging cancelling late-tail HF, amplitude pre-equalization time-smearing the IR.
- Puckette et al., "Alias-Free Digital Synthesis of Classic Analog Waveforms"; Acoustics Engineering TN008 (Dirac stimuli note) for MLS vs linear vs exponential tradeoffs.
- Metric vocabulary TASK-035 will need and should not have to invent: tone, THD, THD+N, SNR, SINAD, SFDR, dynamic range, stepped-frequency sweep, crosstalk.

## Decisions already made here, do not re-open

- Exponential, not linear, sweep. Band-limited pulse, not a delta. Integer-period frequencies. Phase accumulation in f64. Level in dBFS with −20 dBFS default.
- Deliberate dithering, if any, belongs to a later ticket that has characterized the noise floor; uncharacterized truncation spurs read as distortion, so leaving dither out until measured is the honest default.

## Superseded by the implementation plan

Three points here predate planning and are now wrong; the plan wins. Property names to match are `sine_*`, `ess_*`, `pulse_*`, and two of the six house properties are deliberately not offered for generators (plan D6). `describe()` renders into a caller-owned byte buffer, not a `String` (plan D5). And only the sweep accumulates phase in f64: the sine keeps its phase register integral, which is exact rather than merely accurate, so the f64 argument above applies to the chirp's phase integral alone (plan D2).
## What writing the tests changed in the code

Two defects surfaced while making the assertions, both in `stimulus.rs` as first drafted. Neither
shows up in a build, which is why they are recorded here.

1. **`Sine::set_sample_rate` re-derived the phase but not the clamp.** Lowering the rate left a
   stale `frequency_hz` above the new Nyquist limit, where the single conditional subtract in `tick`
   stops being a wrap: 1 kHz held at 48 kHz then dropped to an 8 kHz rate emitted a constant 0.1 for
   100,000 samples and overflowed the `u32` accumulator past 268,435 of them. `set_sample_rate` now
   goes through `apply_current()`, matching what `ExponentialSweep` and `PulseTrain` already did.
   Regression test: `lowering_the_sample_rate_reclamps_the_frequency`.
2. **`ExponentialSweep::set_sample_rate` rescanned the record even when the rate did not change.**
   Constructing `default()` and then setting 48 kHz therefore cost 384,000 `sin` evaluations — 100
   seconds across one proptest property on the host, and a whole-record stall wherever firmware sets
   its rate during setup. A no-op rate set now returns before `apply_current()`.

## Where the plan's steps were adapted, and why

- **Step 4's cheap ten-minute run is not reachable from an integration test.** Advancing the residue
  without calling `sin` needs access to a private field; there is no advance-without-output API and
  adding one for a test would put a seam in the module that nothing else uses. The test drives
  `tick()` through all 28,800,000 samples instead. Measured cost: the whole `stimulus_tests` binary
  runs in 12.5 s debug, so the seam was not worth 10 seconds.
- **The long-run comparison is head-vs-tail *and* against a generator that never ran.** Any legal
  period `P` divides 48,000, and the tail window starts at sample 28,752,000 = 48,000 × 599, so every
  period this generator can produce is in phase with that window. Comparing head against tail at a
  deliberate phase offset is not available; equality against a fresh instance is the version with
  teeth, and it fails if the residue drifts by one LSB.
- **`describe()`'s budget test renders `total_samples` with six digits, not eight**, because building
  a maximum-length sweep pays for a 28.8M-sample peak scan (~14 s debug). The test asserts
  `worst + 2 < MAX_BODY` and names the cap that bounds the two remaining digits, so the margin is
  still arithmetic rather than optimism.
- **No `params_from_normalised`, no std-gated CLI parser**: step 1 said skip them unless a caller
  appears, and none does. `-D warnings` would reject either as dead code.
- **One fact is documented rather than asserted**: `PulseTrain`'s Dirichlet kernel carries one
  harmonic of DC — `level/(2K+1)`, measured 44 dB below the crest and equal in magnitude to the top
  bin. Deconvolution reproduces it because the host generates the same waveform, but an AC-coupled
  meter will read the pulse quieter than `level_dbfs`. Now in the type's docs, with the test that
  pins it.
- **A previous run of this ticket left `tests/stimulus_tests.rs` as a 54 MB file** with one test body
  duplicated 25,323 times, each line prefixed `/`. It was deleted and rewritten; the commit looks
  large because of it. No `.proptest-regressions` file exists — no property ever failed.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-09 12:58
---
Planning evidence, so the next reader knows which numbers are load-bearing and which are pointers.

**Measured in this planning session** (reproducible arithmetic, not recalled): ten minutes at 48 kHz is 28,800,000 samples, so AC #3's 17,600,000 described a 366.7 s run — corrected in the criterion. A crest-aligned integer NCO reaches peak exactly 1.0 at 1/20/997/1000/4800/8000/16000/19200/24000 Hz, while an unaligned start leaves 8000 and 16000 Hz 1.25 dB low and 24000 Hz identically zero. Closed-form sweep phase differs from naive accumulation by up to 1.31 rad over an 8 s 20–20 kHz chirp; that phase integral reaches 145,385 rad, where an f32 ULP is 0.017 rad. The last emitted sweep sample sits at 19999.64 Hz and, without a taper, ends at −0.963 — a near step. Dirichlet pulse peak-energy fraction at period 480: 0.9979 (K=239, a delta in disguise), 0.5021 (K=120), 0.3417 (K=80, chosen), 0.2521 (K=60). Harmonic order k arrives `T·ln(k)/ln(f1/f0)` early: 0.80/1.27/1.61/1.86 s for orders 2–5.

**Read in primary sources**: Farina AES 108th Convention paper 134 (exponential sweeps pack distortion peaks at anticipatory times before the linear response; the amplitude modulation belongs on the inverse filter, not the stimulus) and Farina AES 122 (the problem list this ticket cites; the ESS spectrum falls 3 dB/octave; explicitly warns against removing the fade-out, because a non-zero final sample excites the system with a step).

**Named but not quoted — treat as pointers, not verification**: ADI MT-003/MT-085 (fetches refused), the Audio Precision THD-vs-THD+N note (script-rendered page, no extractable text), Puckette's BLIT paper (downloaded but subset-encoded, text unrecoverable). So the metric vocabulary TASK-035 wants — tone, THD, THD+N, SNR, SINAD, SFDR — carries no quoted definition anywhere in this work; TASK-035 should define it or cite it properly rather than inherit it casually. The familiar advice to choose odd or co-prime cycle counts so harmonics miss the DC and Nyquist bins could not be sourced either, and is deliberately absent from the plan.

One caution on this ticket's history: an earlier research pass here stated source quotes and results it had not actually obtained, and that claim was retracted. Everything above came from commands run during planning.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
`crates/asperitas-dsp/src/stimulus.rs` adds three stimulus sources behind `Processor` — `Sine`
(integer-residue NCO, crest-aligned start phase, exact peak), `ExponentialSweep` (closed-form
Farina chirp, exact integer length, raised-cosine fades, peak-normalised by scan), and `PulseTrain`
(closed-form Dirichlet kernel, `K` clamped below the delta degeneracy) — plus a `Stimulus` trait
whose `describe()` renders into a caller-owned buffer, so it survives the `no_std` firmware path.
All seven acceptance criteria are covered by host tests: 20 numeric tests in `stimulus_tests.rs`,
15 proptest properties in `property_tests.rs`, and `stimulus_shape_tests.rs` holding the device
sweep against the golden synthesiser to 6e-8 over the untapered interior. Writing those tests found
two real defects (a missing Nyquist re-clamp on sample-rate change, and a whole-record peak rescan
on a no-op rate set); both are fixed and regression-tested. Gates: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, the same two with
`asperitas-pod/pod-hw`, `cargo build -p asperitas-dsp --target thumbv7em-none-eabihf`, and
`cd firmware && cargo build --release --features seed3` all pass.
<!-- SECTION:FINAL_SUMMARY:END -->
