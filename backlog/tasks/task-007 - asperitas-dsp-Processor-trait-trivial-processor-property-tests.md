---
id: TASK-007
title: 'asperitas-dsp: Processor trait, trivial processor, property tests'
status: To Do
assignee:
  - '@agent'
created_date: '2026-08-01 05:46'
updated_date: '2026-08-01 05:56'
labels: []
dependencies:
  - TASK-002
priority: high
type: feature
ordinal: 7000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Establish the interface every other part of the project is organised around, and the test harness the real DSP work will lean on. Deliberately paired with a trivial processor (gain and a one-pole filter) — the point is the boundary and the tests, not the algorithm.

Sketch:
```rust
pub type Frame = [f32; 2];

pub trait Processor {
    type Params: Clone + Default;
    fn set_sample_rate(&mut self, hz: f32);
    fn set_params(&mut self, params: &Self::Params);
    fn tick(&mut self, input: Frame) -> Frame;
    fn reset(&mut self);
    fn process_block(&mut self, input: &[Frame], output: &mut [Frame]) { /* provided */ }
}
```

Design intent, worth preserving:
- **`tick` is the primitive, `process_block` is provided.** Callers who want blocks get them free; implementers write the simple thing. Inverting this forces every processor to reimplement buffering.
- **Params are an associated type, not a bag of floats.** Knob-to-parameter *mapping* is policy belonging to the caller (Pod, CLI, TUI); the DSP owns parameter *semantics*.
- **No allocation, no `Result`.** A real-time path that can fail or block is a design error. Clamp, saturate, and make degenerate parameter values ordinary ones rather than erroring.
- **Parameter smoothing lives inside processors**, so all four hosts don't reimplement it.
- **`set_sample_rate` separate from construction**, so one instance works at the device's 48 kHz and at whatever rate a WAV file happens to be.

Build on `dasp` primitives (ring buffers, interpolation, frame/sample traits) — from scratch, per the DSP-learning goal, rather than adopting a graph library.

Property tests with `proptest`, written as a reusable suite any `Processor` implementation can be run through, since every later processor needs the same guarantees:
- output always finite — no NaN, no infinity, for any parameter combination
- output bounded; no runaway feedback under any legal parameter set
- silence in, silence out once any tail has decayed
- `reset()` idempotent; a reset processor given identical input produces identical output
- `process_block` agrees sample-for-sample with repeated `tick`
- parameter changes produce no discontinuity above a threshold — this is the test that catches missing smoothing
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `Processor` trait defined in asperitas-dsp with the shape above
- [ ] #2 A gain and a one-pole filter processor implement it
- [ ] #3 The property-test suite is generic over `Processor` and reusable by future implementations
- [ ] #4 All listed invariants are covered by proptest cases and pass
- [ ] #5 asperitas-dsp remains `#![no_std]` with no hardware dependencies
<!-- AC:END -->
