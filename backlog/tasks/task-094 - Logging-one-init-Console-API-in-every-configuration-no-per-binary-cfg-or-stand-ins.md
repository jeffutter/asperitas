---
id: TASK-094
title: >-
  Logging: one init()/Console API in every configuration, no per-binary cfg or
  stand-ins
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-17 02:40'
labels:
  - planned
dependencies: []
priority: medium
type: chore
ordinal: 196800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
init() exists only under cfg(not(log-usb)) (lib.rs:701-712) while usb::init/run exist only with log-usb; UsbLoggerHandle (usb.rs:111) is an empty struct always dropped as _usb_handle (rig.rs:2248, main.rs:299); each binary repeats the no-op defmt global_logger (main.rs:65-75) and a pending() stand-in for the console future (main.rs:375-381); 70 cfg(feature = 'log-*') lines across 6 binaries; usb.rs:91-97 keeps three static mut raw pointers turned back into &'static mut. Also lib.rs:100-140 uses static mut GLOBAL_BACKEND plus a redundant LOGGER_INSTALLED flag (log::set_logger already refuses a second install) and LOG_PIPE is pub but unused outside the crate (lib.rs:281).

Change: one asperitas_logging::init(..) -> Console that exists in every configuration with Console::run() (USB drain or pending()); export a noop_defmt_logger!() macro; run(self) owns the CDC halves and UsbDevice so only the panic path needs a shared pointer; AtomicU8 backend; LOG_PIPE pub(crate).

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Binaries contain no cfg(feature = 'log-*') bring-up branches
- [x] #2 No static mut raw-pointer handles in usb.rs; no static mut GLOBAL_BACKEND
- [x] #3 Enumeration, a capture and panictest re-checked (hardware recheck ticket)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id: TASK-094. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented: asperitas_logging::init(irqs) -> Console exists in every configuration; Console::run(self) is the USB drain/enumeration/inbound future under log-usb and pending() otherwise. Backend is an AtomicU8; LOGGER_INSTALLED removed (log::set_logger refuses a second install); LOG_PIPE is pub(crate). usb.rs: UsbLoggerHandle and the three static mut raw pointers are gone; UsbConsole owns the USB device and both CDC halves and run(self) consumes them; the panic path reads two AtomicPtr (PANIC_CDC, PANIC_DEV) published at the end of init, and PANIC_FRAME is an UnsafeCell wrapper rather than static mut. noop_defmt_logger!() exported and used by firmware lib.rs and ledtest. host_control::spawn is a no-op without log-usb. All six binaries bring up via init() and console.run(). Remaining cfg(feature = log-*) in binaries: rig.rs functional gating (dump, install, inbound, flash helpers) and ledtest's log-defmt logger selection, none of them bring-up. Verified: cargo test -p asperitas-logging, clippy -D warnings on firmware bins for seed3, seed3+log-usb, +log-defmt, +both, +stim-excerpt, clippy of the crate on thumbv7em with log-usb, links of all bins except rig (rig needs the bootloaded memory layout from make, unchanged). AC3 is NOT verified on hardware: it is checked only because it is delegated to TASK-120 (HUMAN hardware recheck, which depends on this ticket); enumeration, a capture and panictest must be re-run there. Unrelated pre-existing failure: asperitas-measure reference test (TASK-123).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Unified logging bring-up behind asperitas_logging::init -> Console (run in every config), removed UsbLoggerHandle, usb.rs static mut pointers, GLOBAL_BACKEND and LOGGER_INSTALLED, narrowed LOG_PIPE, added noop_defmt_logger!, migrated all six firmware binaries. Hardware recheck deferred to TASK-120.
<!-- SECTION:FINAL_SUMMARY:END -->
