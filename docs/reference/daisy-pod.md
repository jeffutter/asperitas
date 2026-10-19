# Daisy Pod — Hardware Reference

The Pod is a carrier board: it provides controls, jacks and MIDI around a Seed module
that plugs into it. Since the [Seed3](./daisy-seed3.md) is pin-for-pin compatible with
the original Seed pinout, it seats in the Pod normally. daisy-embassy PR #80's author
verified Seed3-in-Pod specifically: buttons, encoder, LEDs and both knobs all work.

## Controls

- 2 potentiometers ("knobs")
- 1 rotary encoder with push switch
- 2 pushbuttons
- 2 RGB LEDs
- Stereo audio in / out on 3.5 mm
- MIDI in / out
- SD card, gate in/out

## Pin map

Transcribed from `libDaisy/src/daisy_pod.cpp` (the Rev3/Rev4 pinout — the current one;
libDaisy retains Rev1/Rev2 maps only as commented-out dead code). Pin names are Seed
`D<n>` designators.

| Function | Seed pin |
|---|---|
| Button 1 (`SW_1`) | `D27` |
| Button 2 (`SW_2`) | `D28` |
| Encoder A | `D26` |
| Encoder B | `D25` |
| Encoder click | `D13` |
| LED 1 — red | `D20` |
| LED 1 — green | `D19` |
| LED 1 — blue | `D18` |
| LED 2 — red | `D17` |
| LED 2 — green | `D24` |
| LED 2 — blue | `D23` |
| Knob 1 | `D21` |
| Knob 2 | `D15` |

Knobs are analog and read via ADC. Knob 1 is `PC4` (ADC1_INP4), Knob 2 is `PC0`
(ADC1_INP10).

### The STM32H7 ADC resets to 16-bit, not 12-bit (verified the hard way)

**`ADC_CFGR.RES` on the STM32H7 has a reset value of `0b000`, which selects 16-bit
resolution.** A conversion therefore returns 0..65535, not 0..4095.

This is a trap rather than a curiosity, because of how embassy-stm32 exposes it.
`AdcConfig { resolution: None, .. }` does not mean "use the default 12 bits" — it means
embassy skips the `RES` write entirely and leaves the reset value in place
(`embassy-stm32-0.6.0/src/adc/v4.rs:222`). Pair that with the reflex of dividing by
4095 to normalise, and every reading past 4095 — **6.25% of pot travel** — clamps to
1.0.

The symptom does not look like a scaling bug. It looks like broken pots: the knob reads
full scale across roughly 94% of its rotation, including at the physical centre, then
collapses to 0 in the last few degrees near one stop. A `podtest` capture of a full
sweep showed only `0`, `1`, `3`, `208`, `363`, `757`, `800`, `1000` per mille across
1030 samples, with just two or three intermediate values per sweep and no smooth travel
at all.

`crates/asperitas-pod/src/knob.rs` now programs the resolution explicitly and derives
its normalisation divisor from `resolution_to_max_count()`, with a `const` assertion
tying the two together so they cannot drift apart again silently.

Note also that `Averaging::Samples16` sets the oversampling right-shift (OVSS) to match
the sample count, so hardware averaging returns results on the same scale as a single
conversion. Averaging does not change the full-scale count, and is not part of this
trap.

### Knob rotation direction: clockwise increases (verified)

**Both pots read *higher* as they turn clockwise.** This is the conventional direction;
nothing needs inverting in software.

Verified on hardware 2026-08-08 against a corrected build, with a capture whose protocol
fixed the starting position: each knob was turned fully clockwise to the stop first, then
swept slowly to the opposite stop and back. Both sweeps rise on the first move and reach
raw 65535 at the clockwise stop and raw 0 at the counter-clockwise stop.

An earlier note here recorded the opposite as a suspicion. It was wrong. The inference
came from a capture taken *through* the 12-bit-divisor bug described above, where the
value clamps to 1.0 across 94% of travel and collapses near one stop — which reads as an
inverted sweep when only the endpoints are visible. That is worth remembering on its own:
**a broken normalisation can masquerade as a direction error.** Establish scaling before
concluding anything about polarity.

### Encoder detent ratio: 4 quadrature transitions per physical detent (verified)

**The Pod's encoder is detented at every *fourth* quadrature state, so one physical
detent produces four A/B transitions, not one.** A decoder that emits ±1 per transition
reports four increments per click.

Measured 2026-08-08: 10 deliberate clockwise detents produced a net +40, and 10
counter-clockwise detents a net −38, with per-detent clusters
`[4,4,3,4,4,3,4,4,4,4,4]` and `[-4,-4,-3,-4,-4,-4,-3,-4,-3,-4]`. Direction is correct
(clockwise is positive); only the ratio was wrong. Note that neither cluster array sums
to its stated net (+42 against +40, −37 against −38) — the record is aggregate, and the
host tests assert carry invariants rather than those totals.

**Shipped as of TASK-024 (2026-09-09):** `QUARTER_STEPS_PER_DETENT = 4` in
`crates/asperitas-pod/src/encoder.rs`. `EncoderDecoder::update()` still counts raw
transitions; `EncoderDecoder::drain_detents()` converts them to whole detents and is what
`ControlSurface::poll()` calls, so `ControlEvent::EncoderDelta` carries detents.

Two properties of that conversion are load-bearing:

- **The remainder is carried across drains, not truncated per poll.** Three of the ten
  counter-clockwise detents registered 3 counted transitions instead of 4 — contact bounce
  the transition table filters, plus two edges arriving closer together than the poll
  period, which the decoder sees as a both-bits change and counts as nothing. Draining
  keeps those quarter-steps, so the next click completes sooner.
- **Truncation is toward zero** (plain Rust `/`), never `div_euclid`: `(-3).div_euclid(4)`
  is `-1`, which would report a detent whose four transitions never arrived. Toward zero,
  a detent is never fabricated and the retained remainder keeps the rotation's sign.

What this rules out and what it does not: the divide-by-four removes the 4× unit error at
any poll rate, but it cannot invent a transition that was never sampled. A click whose
transitions straddle one poll window contributes fewer than four counts and therefore
**defers** a step rather than losing one — replaying the recorded counter-clockwise run
through the decoder reports nine detents for ten clicks and retains the leftover
quarter-step. Whether that deficit survives at the corrected ~1 kHz poll rate is
TASK-029's question, on hardware, with TASK-029.01's raw A/B logging supplying the
per-transition data this capture never recorded.

### LED drive polarity: active-low (verified)

**The Pod's RGB LEDs are active-low.** Driving a channel pin `Low` lights it; `High`
turns it off. This matches libDaisy, whose LED driver inverts. `LED_ACTIVE_LOW = true`
in `crates/asperitas-pod/src/led.rs` (shared by the Pod LED 2 driver and the logging boot LED) is therefore correct.

Verified on hardware 2026-08-01 with `firmware/src/bin/ledtest.rs`, which drives
D20/D19/D18 directly and walks a fixed five-step sequence. Observed colours were white,
red, green, blue, black — one step out of phase with the source order, because the
observer joins mid-cycle. Mapping back:

| Step | R / G / B levels | Observed |
|---|---|---|
| 1 | Low, High, High | red |
| 2 | High, Low, High | green |
| 3 | High, High, Low | blue |
| 4 | High, High, High | black (all off) |
| 5 | Low, Low, Low | white (all on) |

`Low` → lit in every row. Under active-high the sequence would have read as its
complement (cyan, magenta, yellow, white, black), which is unambiguously different — so
this is a positive identification, not an absence of contradiction.

## Defaults worth matching

libDaisy's `DaisyPod::Init` sets an audio **block size of 48**, and `daisy_pod.cpp`
defines `SAMPLE_RATE` as **48014.f** — not 48000. That odd figure is a consequence of
the achievable clock divider, not a typo. Nothing forces us to match it, but it explains
any small discrepancy when comparing against C++ reference behaviour.

## No Pod support exists in daisy-embassy

`daisy-embassy` has `src/pins/pins_seed.rs` and `src/pins/pins_patch_sm.rs` — there is
no Pod module. Neither daisy-embassy nor this repo has a Pod pin map module: firmware
takes the board pins directly, and no upstream contribution is planned.

## Audio I/O is line level, not instrument level

**The Pod's 3.5 mm audio input is line level.** A mandolin, octave mandolin, upright
bass, bass guitar or jazz guitar pickup plugged straight in will be quiet and impedance-
mismatched — a passive magnetic or piezo pickup expects a high-impedance (≥1 MΩ) input,
and the Pod does not provide one.

This matters more than it sounds: the symptom is thin, noisy, low-level audio, and the
natural instinct is to blame the DSP. Feed the Pod from a DI box, a preamp, or an audio
interface's line output during development, and keep gain staging in mind when
comparing device output against desktop CLI output on the same source material.

## Self-loopback channel mapping (measured)

Measured 2026-10-08 with a stereo 3.5 mm cable from the Pod's output jack to its input jack and
`rig`'s 1 kHz sine at -20 dBFS (TASK-038.05 AC #8). In the codec's interleaved SAI words, word 0 is
what `rig` and `main.rs` call **Left** (`console::MonoLane::Left`) and word 1 is **Right**. Each run
drove one output word and recorded one input word, 10 s each, with every block proved by
`dump_reassemble`:

| Output driven | Lane recorded | Peak | RMS |
|---|---|---:|---:|
| Left only | Left | -20.42 dBFS | -23.43 dBFS |
| Left only | Right | -84.29 dBFS (2 LSB) | -93.31 dBFS |
| Right only | Right | -20.40 dBFS | -23.42 dBFS |

So the loop is straight. Output Left returns on input Left and Right on Right, both paths have the
same gain (-0.41 dB end to end at -20 dBFS; the 300 s run in TASK-038.05 repeats it), and Left-to-Right
crosstalk is below the 16-bit capture floor, more than 64 dB down. `rig` records Left by default, and
that lane carries the stimulus.

The loopback proves both jacks use the same contact for word 0. It does not prove that contact is
the tip, which is the left channel by the usual TRS convention. One listen settles it: play a
left-only tone into headphones and hear which ear it is in.

## Lab setup: permanent self-loopback (measured)

Recorded 2026-10-15 with the owner at the bench (TASK-034).

- **Cable:** a male-to-male 3.5 mm TRS (stereo) cable, roughly 1 ft, coiled, from the Pod's output
  jack to its input jack. It stays in place; every automated `rig` run assumes it is there. To use
  headphones or a phone on the output, unplug the output end for the duration and put it back.
- **Loop gain:** -0.41 dB end to end at a -20 dBFS, 1 kHz sine (peak -20.41 dBFS, RMS -23.42 dBFS),
  no clipping. Normalise later loopback measurements against this.
- **Hum, buzz, DC:** a 30 s capture shows DC of -0.50 LSB and a worst 20 to 120 Hz bin of -122.5 dBFS
  at 60 Hz, about 100 dB below the tone. The loudest spur outside the tone is the 2 kHz harmonic at
  -114.6 dBFS. After a fitted 1 kHz sine is removed the residual is 0.35 LSB RMS (-99.5 dBFS), at the
  16-bit capture floor. There is no audible or measurable hum; the loop is limited by the capture format.
- **Independent observer:** headphones on the output jack, with `rig` playing the sine. The owner heard
  a steady, clean 1 kHz tone at a sane level in both ears. This is the check the device cannot make
  about itself: a shared clock fault or a lane error inside the capture code would still produce a
  tidy self-measurement, but would be audible as a wrong pitch or level. No scope was used.
- **Stimulus level:** -20 dBFS, the `rig` default.
- **How it was measured:** `rig` with the default `stim-sine`, `ASP_RIG_CAPTURE_SECONDS=30`, reset over
  the probe; raw console bytes captured to a file, rebuilt with `dump_reassemble` (every block proved,
  0 discarded bytes), then analysed offline.

## Listening with `main`

Durable facts from a bench listening session; the full record is in
[pod-listening-session.md](../investigations/pod-listening-session.md).

- **The audio callback must run on an interrupt executor** (SAI1, priority P6). A callback has about
  1.3 ms per half-transfer before `read` returns `sai::Error::Overrun`; after an overrun the audio loop
  ends and stays ended.
- **Do not attach a probe while listening.** Attaching halts the core and overruns the SAI ring.
- **Seat plugs fully**; a half-inserted input cable gave odd output-jack behaviour.
- **Judge a low-pass on broadband material**, not a sine.
- **Line level:** a Mac headphone output is far below line level; play at 50 to 80% and keep Mac mic
  input volume at 5 to 15% when recording the Pod's output.

## Both output jacks in use: the channels mix (observed, cause unknown)

Observed at the bench with `main` running and the Mac feeding the Pod input.

- **Use one output jack at a time when judging left versus right.** With headphones in *either* the
  Pod's headphone out *or* its line out (never both), a right-only source came out the right side only
  and a left-only source came out the left side only.
- **With both output jacks plugged, a one-channel signal came out of both sides.** The codec was not at
  fault: with a right-only source the SAI buffers read RX left -79.7 dBFS, RX right -26.2 dBFS, and TX
  left -79.3 dBFS, TX right -28.8 dBFS (`/tmp/rxlevel.py`-style `probe-rs read` of `0x30000000` and
  `0x30000200`). Whatever mixes the sides sits after the codec output, in the jack or output stage. The
  mechanism is not known; it was not measured with a scope.
- **It likely explains the earlier half-plug oddity**, where the output was only audible when a second
  cable was partly inserted. Expect a signal on only one channel to look like a stereo signal as soon as
  a second output jack, or the permanent loopback cable's output end, is connected. Unplug the other
  output before a left/right check, and before trusting a hard-panned listening test.
- **Does not contradict the loopback measurement** in *Self-loopback channel mapping*: that run used the
  output-to-input cable only, with nothing else on the output jacks.
- **Buffer addresses:** in the bootloaded layout the SAI DMA buffers read at `0x30000000` (RX) and
  `0x30000200` (TX); the non-bootloaded layout differs.
