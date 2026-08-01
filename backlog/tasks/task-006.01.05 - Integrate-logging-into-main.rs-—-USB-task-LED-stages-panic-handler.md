---
id: TASK-006.01.05
title: 'Integrate logging into main.rs — USB task, LED stages, panic handler'
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-08-01 18:46'
updated_date: '2026-08-01 20:43'
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
