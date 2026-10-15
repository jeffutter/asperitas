---
id: TASK-032.03
title: >-
  Wire the control channel into the firmware: version, reset reason, restart,
  override
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-15 05:59'
updated_date: '2026-10-15 06:26'
labels:
  - task
  - planned
dependencies:
  - TASK-032.01
  - TASK-032.02
parent_task_id: TASK-032
priority: high
ordinal: 157800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Firmware wiring (non-host-testable part): a task in the firmware binaries that read inbound records, dispatch them using the TASK-032 protocol module, apply overrides to the knob-boundary layer, and emit replies on the framed console. Provide firmware version (from Cargo package version plus git describe if available), reset reason of the current boot read from the STM32H7 RCC reset status register captured before it is cleared, and RESTART via a software system reset. Restart must never make a faulted board harder to recover: it must be serviced without depending on the audio path or channel backpressure, must not mask BOOT/RESET/DFU entry, and the inbound path must not block the panic handler. Document the chosen design in docs/reference/daisy-seed3.md. Must build for thumbv7em targets via the firmware Makefile; compiling is not hardware evidence (TASK-033.01 verifies on the board).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A firmware task reads InboundRecords, routes them with asperitas_logging::control, and emits VER/RSN/ack replies on the framed console without blocking on channel backpressure
- [x] #2 CTL SET/REL apply to asperitas_pod::knob_override::KnobOverride at the knob boundary used by main.rs (and podtest.rs where it reads knobs)
- [x] #3 CTL VER reports Cargo package version plus git describe when available (build.rs), CTL RSN reports the RCC reset status captured before it is cleared
- [x] #4 CTL RST performs a software system reset (SCB::sys_reset) serviced independent of the audio path and not masking BOOT/RESET/DFU entry; inbound path cannot block the panic handler
- [x] #5 Design documented in docs/reference/daisy-seed3.md; firmware builds for thumbv7em via the firmware Makefile and host tests/clippy pass; no claim of hardware verification (TASK-033.01)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by ce55f5e. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added firmware/src/host_control.rs (shared via #[path] by main and podtest): control task, KnobOverride table merged in the main audio callback, RCC_RSR capture then RMVF clear, RST = ack + 20 ms + SCB::sys_reset. build.rs exports ASPERITAS_VERSION. Pure RSR mapping control::reset_reason_from_rsr is host-tested. podtest accepts/holds overrides but logs raw ADC counts unmerged (pot-jitter measurement); rig not wired (no inbound channel, tight flash). Verified: cargo test --workspace, workspace clippy, firmware clippy --bins for console and RTT sets, make build main/podtest (main.bin 107621 B). NOT hardware verified; TASK-033.01 owns that.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Wired CTL VER/RSN/RST/SET/REL into main and podtest firmware via shared host_control.rs; documented in docs/reference/daisy-seed3.md. Build-verified only; hardware verification is TASK-033.01.
<!-- SECTION:FINAL_SUMMARY:END -->
