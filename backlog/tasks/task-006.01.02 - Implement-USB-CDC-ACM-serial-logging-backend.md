---
id: TASK-006.01.02
title: Implement USB CDC-ACM serial logging backend
status: Done
assignee: []
created_date: '2026-08-01 18:45'
updated_date: '2026-08-01 20:11'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-006.01
priority: high
ordinal: 21000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the USB CDC-ACM backend for `asperitas-logging`. Gated behind `log-usb` feature flag.

**What it does:**
- Initializes USB CDC-ACM using `embassy_usb::class::cdc_acm` with daisy-embassy peripherals
- Implements `log::Log` trait so all `log::*` macro calls route to USB serial
- Handles connection lifecycle (`wait_connection()`) and buffer flushing
- Provides `try_flush()` for explicit flush from panic handler path
- Uses `StaticCell` for driver buffers (no heap)

**Implementation plan:**
1. Add dependencies in asperitas-logging/Cargo.toml under `log-usb` feature: `embassy-usb`, `embassy-stm32`, `static_cell`, `embassy-executor`, `embassy-futures`
2. In `backend_usb.rs`:
   - Define interrupt binding struct with `bind_interrupts!(OTG_FS)`\n   - Implement `UsbLogger` struct holding a reference to `CdcAcmClass`\n   - Implement `log::Log` trait — format record into buffer, call `write_packet()`\n   - Implement `pub fn init_usb(board: &DaisyBoard) -> (UsbFuture, UsbLogger)` that:\n     a. Creates the USB driver with FS config, vbus_detection=false\n     b. Sets Windows-compatible descriptors (class 0xEF, sub 0x02, proto 0x01, composite_with_iads=true)\n     c. Builds CdcAcmClass with 64-byte packets\n     d. Returns the USB run future + logger handle\n3. Provide `try_flush()` method on logger that does a non-blocking write of any buffered data
4. Handle `EndpointError::Disabled` as disconnect (silently drop pending writes)
5. Wire into facade's `init()` — when `log-usb` is enabled, this becomes the active backend

**Key configuration values (from daisy-embassy example):**
- `config.vbus_detection = false` (Pod has no VBUSEN pin)\n- Device class: 0xEF, sub-class: 0x02, protocol: 0x01, composite_with_iads: true\n- EP_OUT_BUFFER: [u8; 256] via StaticCell\n- Packet size: 64 bytes
<!-- SECTION:DESCRIPTION:END -->
