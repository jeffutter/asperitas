---
id: TASK-155.03
title: >-
  Excitation shaping stage in asperitas-dsp: conditions the instrument signal
  before it drives resonators
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-11-20T00:25:00Z'
labels:
  - m5
  - resonator
  - dsp
dependencies: []
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
  - crates/asperitas-dsp/src/filter.rs
  - crates/asperitas-dsp/src/processor.rs
parent_task_id: TASK-155
priority: high
type: feature
ordinal: 298700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A resonator bank driven by the raw instrument signal rings with the signal's low-frequency body and its loudest transients, which is harsh and level-dependent. The excitation stage is what makes the input a good exciter: it shapes which part of the signal reaches the resonators and keeps their drive bounded. It is a separate, host-testable `Processor` so it can be judged and measured without a bank, and so it ships independently of the owner decision on tuning.

It lives in `asperitas-dsp` (no_std, no allocation, no `Result`) and follows the crate's conventions: its own `ParamSpec` table, smoothing inside the processor, sanitised degenerate inputs. The exact shaping controls (for example how much low end is removed, how much transient emphasis, a drive limit) are the implementer's to propose and record in the ticket notes with the reason, constrained by: mono-in friendly, no level-dependent instability, output bounded for any bounded input, and transparent (near unity gain, flat enough) at its default so the bank's own character is what is heard.

Out of scope: resonators, stereo, hosts wiring.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 An excitation stage implementing Processor is exported from asperitas-dsp with a ParamSpec table and a Default
- [ ] #2 Property tests (proptest) show output is finite and bounded for any bounded input and any legal or sanitised-illegal parameters
- [ ] #3 A test shows the stated control(s) change the measured frequency or level response in the stated direction, with measured numbers in the test
- [ ] #4 A test shows the Default is near transparent: measured gain and response deviation from unity stay within a stated tolerance across the instrument range
- [ ] #5 Silence in gives silence out, reset() is idempotent and deterministic, process_block equals repeated tick, and a parameter step produces no output discontinuity above a stated threshold
- [ ] #6 The crate still builds no_std for thumbv7em-none-eabihf and performs no allocation
- [ ] #7 The stage and its parameters are documented in the resonator docs/reference page created or extended by this ticket, linked from CLAUDE.md's where-things-live list
- [ ] #8 bash scripts/gates.sh push passes
<!-- AC:END -->
