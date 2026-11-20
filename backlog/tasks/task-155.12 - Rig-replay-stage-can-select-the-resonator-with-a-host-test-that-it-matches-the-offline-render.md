---
id: TASK-155.12
title: >-
  Rig replay stage can select the resonator, with a host test that it matches
  the offline render
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-11-20T00:26:00Z'
labels:
  - m5
  - resonator
  - rig
  - firmware
dependencies:
  - TASK-155.06
documentation:
  - docs/reference/protocol.md
  - docs/reference/asperitas-rig.md
  - docs/reference/automation.md
  - crates/asperitas-live/tests/equivalence_device.rs
  - crates/asperitas-stimulus/src/fx_chain.rs
parent_task_id: TASK-155
priority: medium
type: feature
ordinal: 307700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The device is verified without ears through the rig: a firmware that plays stimuli or replays captured audio through a selectable effect stage (bypass or the Asperitas patch, chosen over the wire with `CTL FX` and a wire code), plus the host capture and analysis tools. For the resonator to be verified on the board the same way, the stage must be able to run it, the wire code must exist, and the rig's knob session must be able to drive its controls, with a host test proving the stage computes what the offline render computes before any hardware is involved.

Add the resonator as a selectable stage kind in `asperitas-stimulus` (`FxKind`, its wire code and the `asperitas-wire` documentation of it), reachable from `asperitas-rig` and `asperitas-host` (`CTL FX`, `CTL SET`, and the automation session from M4.5), following docs/reference/protocol.md and docs/reference/asperitas-rig.md. Bypass remains the default and still touches no sample, and the baseline captures must not change. Follow `crates/asperitas-live/tests/equivalence_device.rs` for the host-side equivalence test of the stage against the CLI render. Also check the rig firmware (`firmware/`, bin `rig`) still builds, since the stage runs there.

Out of scope: running on the board (TASK-155.14), the Pod `main` firmware (TASK-155.13).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The resonator is a selectable replay stage with a documented wire code; selecting an unknown code still changes nothing and returns false as today
- [ ] #2 Bypass remains the default and renders exactly the bits of the bare generator; existing baseline captures, wire tests and the Asperitas stage behave unchanged and pass
- [ ] #3 A host test shows the resonator stage's output equals asperitas-cli process for the same input and controls bit for bit, including under an automation file, with a negative control
- [ ] #4 The rig's session and CTL SET paths drive the resonator's controls from the same real-unit keys, and a bad key or out-of-range value is refused with a message naming it
- [ ] #5 The rig firmware cross-builds for thumbv7em-none-eabihf (make check, clippy and build for the rig binary) and the stage performs no allocation or lock in the callback
- [ ] #6 docs/reference/protocol.md and docs/reference/asperitas-rig.md document the new wire code and how to select and drive the resonator stage
- [ ] #7 bash scripts/gates.sh push passes
<!-- AC:END -->
