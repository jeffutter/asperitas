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

### Encoder detent ratio: 4 quadrature counts per physical detent (verified)

**The Pod's encoder is detented at every *fourth* quadrature state, so one physical
detent produces four A/B transitions, not one.** A decoder that emits ±1 per transition —
as `EncoderDecoder` currently does — reports four increments per click.

Measured 2026-08-08: 10 deliberate clockwise detents produced a net +40, and 10
counter-clockwise detents a net −38. Direction is correct (clockwise is positive); only
the ratio is wrong.

Divide-by-four is therefore required, but it **must carry the remainder** rather than
truncate per poll. Three of the ten counter-clockwise detents registered 3 transitions
instead of 4 (contact bounce the LUT filters, and transitions arriving closer together
than the poll period). A per-poll `delta / 4` discards those as zero; an accumulator that
emits one detent per 4 counts and keeps the remainder does not.

### LED drive polarity: active-low (verified)

**The Pod's RGB LEDs are active-low.** Driving a channel pin `Low` lights it; `High`
turns it off. This matches libDaisy, whose LED driver inverts. `LED_ACTIVE_LOW = true`
in `crates/asperitas-logging/src/led.rs` is therefore correct.

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
no Pod module. The pin map above has to be written on our side. It is a small, purely
declarative piece of work (GPIO + ADC channel assignment), and a good candidate to
contribute upstream once it is proven.

## Audio I/O is line level, not instrument level

**The Pod's 3.5 mm audio input is line level.** A mandolin, octave mandolin, upright
bass, bass guitar or jazz guitar pickup plugged straight in will be quiet and impedance-
mismatched — a passive magnetic or piezo pickup expects a high-impedance (≥1 MΩ) input,
and the Pod does not provide one.

This matters more than it sounds: the symptom is thin, noisy, low-level audio, and the
natural instinct is to blame the DSP. Feed the Pod from a DI box, a preamp, or an audio
interface's line output during development, and keep gain staging in mind when
comparing device output against desktop CLI output on the same source material.
