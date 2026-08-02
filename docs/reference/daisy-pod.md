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

Knobs are analog and read via ADC.

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
