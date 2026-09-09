---
id: TASK-036
title: Implement probe-based flashing and a lossless log channel
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 01:28'
labels: []
dependencies: []
references:
  - 'https://probe.rs/docs/getting-started/probe-setup/'
documentation:
  - docs/reference/daisy-seed3.md
  - docs/reference/rust-daisy-stack.md
modified_files:
  - firmware/Makefile
  - crates/asperitas-logging/src/lib.rs
  - flake.nix
priority: high
type: feature
ordinal: 47000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
An ST-Link V3 MINIE is on order, and the software side can land ahead of it. Nothing here needs the physical probe to write and compile-verify.

Two constraints disappear once a probe is in use. Every flash currently needs a hand on BOOT and RESET, which is the reason firmware changes cannot be verified in an unattended loop. And every diagnostic byte crosses the USB console link, which loses records — TASK-030 makes that loss visible, but RTT shares nothing with the USB stack at all, so the loss stops being possible rather than becoming reportable. docs/reference/rust-daisy-stack.md already lists probe-rs as the intended tool once a probe exists, and docs/reference/daisy-seed3.md records the class of fault this would have named immediately: the RAM-length mistake hard-faulted before main in every binary, and diagnosing it meant reasoning about the first four bytes of the binary because there was no way in.

Physical unknowns are deliberately not resolved here and belong to TASK-037: the Seed3's extra ST-LINK-V3MINIE-style pads are documented as present only for mechanical alignment and not wired up, so attachment uses the 10-pin Cortex Debug footprint, and whether those pads are reachable with the Seed seated in the Pod is unverified. If they are not reachable, probe work and Pod control-surface work may not be simultaneously possible, which is worth knowing before anything is soldered.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A make target flashes over the probe using attach-under-reset, so no BOOT or RESET interaction is needed, and verifies what was written.
- [ ] #2 Existing DFU targets keep working unchanged — the probe path is additive and a board with no probe attached is still flashable.
- [ ] #3 defmt over RTT is selectable behind a Cargo feature, the existing USB console facade remains selectable, and binaries selecting neither still build.
- [ ] #4 The probe configuration compiles for thumbv7em-none-eabihf and is clippy-clean, which is verifiable without a board.
- [ ] #5 Panic diagnostics reach the host over RTT when that feature is selected, at code level; hardware confirmation belongs to TASK-037.
- [ ] #6 docs/reference/daisy-seed3.md's debugging sections describe both channels and say which to reach for, and rust-daisy-stack.md's toolchain note reflects the probe as available rather than aspirational.
<!-- AC:END -->
