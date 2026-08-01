---
id: TASK-006.01.04
title: Implement custom panic handler with LED strobe and USB serial message
status: To Do
assignee: []
created_date: '2026-08-01 18:46'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-006.01
priority: high
ordinal: 23000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace `panic-halt` with a custom panic handler that provides visible and serial feedback on panic.

**What it does:**
- Replaces `panic-halt = "0.2"` dependency with a custom `#[panic_handler]`\n- Sets LED to fast-strobe red (panicked state) immediately\n- Attempts to print panic location and message over USB serial if available\n- Loops forever after (same as panic-halt)\n- Also removes the no-op defmt logger block from binaries (replaced by real logging)

**Implementation plan:**
1. In `asperitas-logging/src/panic.rs`:
   - Define `#[panic_handler]` function taking `&PanicInfo`\n   - Call LED panicked state first (synchronous, always works)\n   - Attempt synchronous USB write of panic message:\n     - Use a global static `USBCDC_WRITER` protected by critical section\n     - Write formatted panic string via direct register-level USB or embassy's sync path\n     - If USB not yet initialized or disconnected, silently skip\n   - Enter infinite loop\n2. Remove `panic-halt` from firmware/Cargo.toml\n3. Replace no-op defmt logger in main.rs and blinky.rs with:\n   - `use asperitas_logging::panic_handler;`\n   - Keep defmt no-op ONLY if needed for linker symbols (test if embassy-stm32 still requires it)\n4. The panic handler must be defined in the binary crate (not lib) due to Cortex-M RT requirements — so re-export it from asperitas-logging as a macro or require each binary to call an init function

**Design notes:**
- The `#[panic_handler]` attribute can only be used once per binary crate\n- We'll provide a `pub fn install_panic_handler()` or use a module that the binary includes\n- For USB writes during panic: since the async executor is halted, we cannot use embassy-usb's async API. Options:\n  a. Direct register access to USB OTG FS (complex but reliable)\n  b. Skip USB write during panic, rely on LED only (simpler, acceptable fallback)\n  c. Use `cdc_acm::write_packet` without await (won't work — it's async)\n- **Decision:** Use option (a) for best UX — write directly to USB CDC endpoints via PAC registers. This is ~30 lines of register manipulation using `embassy_stm32::pac`. If this proves too complex, fall back to option (b).
<!-- SECTION:DESCRIPTION:END -->
