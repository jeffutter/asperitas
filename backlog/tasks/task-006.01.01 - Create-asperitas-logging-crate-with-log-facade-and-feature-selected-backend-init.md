---
id: TASK-006.01.01
title: >-
  Create asperitas-logging crate with log facade and feature-selected backend
  init
status: Done
assignee: []
created_date: '2026-08-01 18:44'
updated_date: '2026-08-01 20:11'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-006.01
priority: high
ordinal: 20000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create a new workspace crate `crates/asperitas-logging/` providing the logging facade.

**What it does:**
- Re-exports `log::*` macros (`info!`, `debug!`, `warn!`, `error!`) so call-sites never depend on a backend
- Provides `init()` function that installs the global `log::Logger` based on Cargo features
- Feature flags: `log-usb` (USB CDC-ACM), `log-defmt` (future)
- When no backend feature is enabled, falls back to no-op logger
- Uses `critical-section` for safe concurrent access from interrupt context

**Implementation plan:**
1. Create `crates/asperitas-logging/` with Cargo.toml — features: `log-usb`, `log-defmt` (both optional)
2. lib.rs: re-export `log` macros, define internal `LogBackend` trait with `write(&mut self, bytes: &[u8])` method
3. Implement no-op backend as default fallback
4. Implement `pub fn init() -> Result<(), ()>` that selects backend by feature gate and calls `log::set_logger`
5. Add stub `backend_usb.rs` module (empty struct placeholder)
6. Add crate to workspace members in root Cargo.toml
7. Verify compilation

**Key files:**
- `crates/asperitas-logging/Cargo.toml`
- `crates/asperitas-logging/src/lib.rs`
- `crates/asperitas-logging/src/backend_usb.rs` (stub)
<!-- SECTION:DESCRIPTION:END -->
