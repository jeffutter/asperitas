---
id: TASK-094
title: >-
  Logging: one init()/Console API in every configuration, no per-binary cfg or
  stand-ins
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - logging
  - firmware
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
- [ ] #1 Binaries contain no cfg(feature = 'log-*') bring-up branches
- [ ] #2 No static mut raw-pointer handles in usb.rs; no static mut GLOBAL_BACKEND
- [ ] #3 Enumeration, a capture and panictest re-checked (hardware recheck ticket)
<!-- AC:END -->
