---
id: TASK-006.01.05
title: 'Integrate logging into main.rs — USB task, LED stages, panic handler'
status: Done
assignee:
  - '@ralph'
created_date: '2026-08-01 18:46'
updated_date: '2026-08-01 21:28'
labels:
  - task
  - planned
dependencies:
  - TASK-006.01.01
  - TASK-006.01.02
  - TASK-006.01.03
  - TASK-006.01.04
parent_task_id: TASK-006.01
priority: high
ordinal: 24000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Wire up all logging infrastructure in `firmware/src/bin/main.rs`. This is the integration ticket that ties everything together.

**What it does:**
- Replaces no-op defmt logger with real asperitas-logging init\n- Spawns USB CDC task alongside audio passthrough\n- Sets LED through boot stages (pre-init → running)\n- Enables `log-usb` feature for firmware build\n- Updates blinky.rs similarly (simpler — just LED + logging, no audio)

**Implementation plan:**
1. Update `firmware/Cargo.toml`:\n   - Add `asperitas-logging = { path = "../crates/asperitas-logging", features = ["log-usb"] }`\n   - Remove `panic-halt` dependency\n   - Keep `defmt` only if still needed for embassy-stm32 linker symbols\n2. Rewrite `main.rs`:\n   - Init logging at top of `main()`: `asperitas_logging::init().unwrap();`\n   - Set LED to PreInit state immediately\n   - After board init, spawn USB task with `embassy_executor::Spawner::spawn` or use `join!`\n   - After audio starts, set LED to Running state\n   - Use `info!()` macros throughout for boot progress messages\n3. Update `blinky.rs`:\n   - Init logging, set LED states, use info! macros\n   - Simpler since no audio — just USB + LED loop\n4. Verify compilation with `cargo build --release --features seed3 log-usb`\n5. Check binary size stays within 128 KB internal flash limit
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Integrate asperitas-logging into main.rs and blinky.rs. All four dependencies (TASK-006.01.01–04) are Done.\n\n## Step 1: Update firmware/Cargo.toml\n\n- Add dependency: `asperitas-logging = { path = "../crates/asperitas-logging", features = ["log-usb"] }`\n- Remove `panic-halt = "0.2.0"` — replaced by `asperitas_logging::panic_handler`\n- Keep `defmt` — embassy-stm32 still requires its linker symbols at build time\n- Verify `embassy-time` present (already there at 0.5, needed by any async delays)\n\n## Step 2: Rewrite main.rs\n\n### Imports and module-level items\n\n- `use daisy_embassy::{DaisyBoard, hal, new_daisy_board};`\n- `use daisy_embassy::hal::{bind_interrupts, peripherals, usb};`\n- `use asperitas_logging::usb;`\n- `use asperitas_logging::info;`\n- `use asperitas_logging::panic_handler as _;` — registers #[panic_handler], prevents unused warning\n- Keep the defmt global_logger block unchanged — required by embassy-stm32\n- Remove `use panic_halt as _;` and `_defmt_panic()`\n- Add interrupt binding:\n  ```rust\n  bind_interrupts!(pub struct UsbIrqs {\n      OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;\n  });\n  ```\n\n### main() body\n\n1. `let p = hal::init(daisy_embassy::default_rcc());`\n2. `let board: DaisyBoard<'_> = new_daisy_board!(p);`\n3. Register panic LED: `asperitas_logging::panic_handler::set_panic_led(board.pins.d20, board.pins.d19, board.pins.d18);` — consumes the RGB pins (PC1/PA6/PA7), installs them in the panic handler's static\n4. Init USB logging: `let (usb_fut, _) = usb::init(board.usb_peripherals.usb_otg_fs, board.usb_peripherals.pins.DP, board.usb_peripherals.pins.DN, UsbIrqs);`\n5. Spawn USB task: `_spawner.spawn(async { usb_fut.await }).ok();` —  silently ignores Busy; audio works without USB\n6. `info!("USB logging initialized");`\n7. Use `board.user_led.on()` for simple running indicator (PC7, single-color onboard LED)\n8. Audio init (existing code): prepare_interface → start_interface → start_callback loop\n9. Add `info!("Audio interface ready")` before the callback loop\n\n### Why no BootLed?\n\nBoth `BootLed::new()` and `set_panic_led()` consume the same three Peri tokens (PC1, PA6, PA7). You can only move each token once. Since `set_panic_led` must be called for the panic handler to light the LED, and it's idempotent (stores in a static), we call it first and use `board.user_led` (PC7) for the simple boot-stage indication. This avoids the ownership conflict without sacrificing panic visibility.\n\n## Step 3: Rewrite blinky.rs\n\nSame pattern as main.rs but without audio:\n\n1. Same imports, interrupt binding, defmt block, panic_handler registration\n2. In main():\n   - Init clocks + board\n   - `set_panic_led(board.pins.d20, board.pins.d19, board.pins.d18);`\n   - Init USB + spawn task\n   - Continue blinking `board.user_led` (PC7) at ~1.6 Hz as before\n   - Add `info!("Blinky running")` after USB init\n\n## Step 4: Verify compilation\n\n- `cargo build --release --features seed3` succeeds\n- No unused-import warnings (especially `panic_handler as _`)\n- No linker errors from defmt symbols\n- Binary size check: should fit within 128 KB internal flash\n\n## Key design decisions\n\n- **No BootLed in binaries**: `BootLed::new()` and `set_panic_led()` both consume the same RGB pins (Peri<'static, PC1/PA6/PA7>). Can't call both. Panic handler gets priority since it's safety-critical.\n- **user_led for boot indication**: PC7 (onboard single-color LED) provides on/off feedback during boot. Simpler than RGB but functional.\n- **defmt global_logger stays**: Required by embassy-stm32 at link time. Only panic-halt is removed.\n- **USB spawn tolerance**: `spawn().ok()` discards Busy errors gracefully. Audio pipeline works even if USB fails to enumerate.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation

Fixed three build issues discovered during integration:

1. **main.rs**: Made `board` mutable so `board.user_led.on()` compiles
2. **defmt version mismatch**: Changed `defmt = "1"` to `defmt = "0.3"` in firmware/Cargo.toml to match embassy-stm32's dependency (0.3.100). The v1.x `#[defmt::panic_handler]` macro had an incompatible signature.
3. **Missing _defmt_panic symbol**: Added `#[defmt::panic_handler]` fn to both main.rs and blinky.rs. embassy-usb uses defmt internally for debug logging, which requires this linker symbol even when the binary doesn't directly use defmt macros.
4. **Unused imports in blinky.rs**: Removed `hal` and `new_daisy_board` imports that were unused because fully qualified paths were used instead.

Also ran `cargo fmt` to fix pre-existing formatting drift in asperitas-logging/src/{panic_handler,usb}.rs.

Binary sizes: main ~65.5 KB, blinky ~49.8 KB — both well within 128 KB flash limit.
<!-- SECTION:NOTES:END -->
