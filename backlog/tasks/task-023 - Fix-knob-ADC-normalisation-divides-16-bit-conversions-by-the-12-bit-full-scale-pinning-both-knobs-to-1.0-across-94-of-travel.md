---
id: TASK-023
title: >-
  Fix: knob ADC normalisation divides 16-bit conversions by the 12-bit full
  scale, pinning both knobs to 1.0 across 94% of travel
status: Done
assignee:
  - '@agent'
created_date: '2026-08-08 18:18'
updated_date: '2026-08-08 18:18'
labels:
  - review-followup
dependencies:
  - TASK-018.02
  - TASK-018.04
documentation:
  - docs/reference/daisy-pod.md
modified_files:
  - crates/asperitas-pod/src/knob.rs
  - crates/asperitas-pod/src/lib.rs
  - docs/reference/daisy-pod.md
priority: high
type: bug
ordinal: 35000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found by the TASK-018.04 hardware run — the first evidence that could have caught it, exactly as that ticket's AC #1 was designed to.

crates/asperitas-pod/src/knob.rs constructed its ADC with AdcConfig { resolution: None, .. } and normalised the result by a hardcoded ADC_12BIT_MAX = 4095.0. Those two are inconsistent. embassy-stm32's resolution: None does not mean 'default to 12 bits' — it means embassy skips the ADC_CFGR.RES write entirely (embassy-stm32-0.6.0/src/adc/v4.rs:222), leaving the register at its reset value. On STM32H7 that reset value is Res::BITS16 == 0b000, i.e. 16-bit. Conversions therefore returned 0..65535 while being divided by 4095, so every reading past raw 4095 — 6.25% of pot travel — clamped to 1.0.

The symptom did not read as a scaling bug; it read as dead pots. A full sweep of both knobs across 1030 podtest samples produced only the values 0, 1, 3, 208, 233, 363, 757, 800, 1000 per mille: full scale at the physical centre, a collapse to 0 within two samples near one stop, and no smooth travel anywhere. Two or three intermediate samples per sweep is what a 6% window looks like at 100 Hz logging.

Fix: program the resolution explicitly (POT_RESOLUTION = Resolution::BITS16) and derive the divisor from embassy's resolution_to_max_count() rather than writing a magic number, with a const assertion tying the two together so they cannot drift apart silently again. 16-bit was chosen over dropping to 12-bit because it is what the hardware was already doing — the observed sweep proves it works — so the change is pure arithmetic with no reconfiguration risk, and Averaging::Samples16 sets OVSS to match, keeping the noise reduction on the same scale.

Also restructured knob.rs to the module's existing convention (see encoder.rs): pure normalisation at the top level, hardware driver behind #[cfg(feature = pod-hw)] in mod hw. The old unit tests lived behind the pod-hw gate, so they never ran on the host, and they re-implemented the formula in the test body rather than calling the code — they would have passed unchanged while the firmware was broken. They now call normalize() and run in cargo test.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 knob.rs programs ADC resolution explicitly rather than relying on the reset value
- [x] #2 the normalisation divisor is derived from the programmed resolution, not hardcoded, and a const assertion fails the build if they disagree
- [x] #3 normalisation is host-testable and the tests exercise the shipped code path rather than a copy of the formula
- [x] #4 a regression test asserts raw 4095 normalises to ~0.0625, not 1.0
- [x] #5 the STM32H7 16-bit reset-value trap is recorded in docs/reference/daisy-pod.md
- [x] #6 cargo test -p asperitas-pod and a release build of the podtest firmware both pass
<!-- AC:END -->
