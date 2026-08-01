---
id: TASK-004.01
title: Blinky firmware and DFU build pipeline
status: Done
assignee:
  - '@ralph'
created_date: '2026-08-01 05:56'
updated_date: '2026-08-01 14:22'
labels: []
dependencies: []
documentation:
  - docs/reference/daisy-seed3.md
parent_task_id: TASK-004
type: feature
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Write the blinky application and the objcopy/dfu-util invocation that turns it into a flashable image. Based on daisy-embassy's `examples/blinky.rs`.

Keep it in `firmware/src/bin/` permanently as a known-good diagnostic — when something later goes wrong, being able to flash a thing that definitely works is worth a lot.

Build: `cargo objcopy --release --features seed3 --bin blinky -- -O binary firmware.bin`
Flash: `dfu-util -a 0 -s 0x08000000:leave -D firmware.bin`

`0x08000000` is the STM32H750's 128 KB internal flash via the built-in system bootloader. Sufficient for now; larger applications later need the Daisy bootloader relocating into QSPI.

The agent cannot verify this works — no board access. Produce the artifact and the documented command sequence; TASK-004.02 confirms it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `cargo objcopy` produces a valid raw .bin from the firmware workspace
- [x] #2 Blinky source retained in firmware/src/bin/ as a permanent diagnostic
- [x] #3 The full build-and-flash command sequence is documented
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
<!-- PLAN:BEGIN -->
### Context

- Current `firmware/src/bin/main.rs` initializes the board but loops forever — no observable behavior.
- `daisy-embassy` PR #80 (pinned at commit `477083b0227d`) provides `seed3` feature with `user_led` on PC7.
- No debug probe available: must use `panic-halt`, not `panic-probe`; no `defmt-rtt` transport.
- `embassy-time` is a transitive dependency via `daisy-embassy` but needs to be a direct dependency for `Timer::after_millis()`.
- `memory.x` declares `FLASH LENGTH = 2M` which is incorrect for the H750IBKx's 128 KB internal flash. Not blocking for blinky (binary will be tiny), but should be noted for later tasks.

### Steps

#### Step 1: Add `embassy-time` as direct dependency

Add `embassy-time = "0.5"` to `firmware/Cargo.toml` `[dependencies]`. This is needed for `embassy_time::Timer::after_millis()`. It's already pulled transitively by `daisy-embassy` but we need it directly for the blinky binary.

#### Step 2: Create `firmware/src/bin/blinky.rs`

New file. Uses `daisy_p.user_led` (PC7, the Seed3 onboard user LED) with a 300 ms on/off cycle. Key design decisions:

- **No `defmt-rtt` or `panic-probe`** — these require a debug probe. Use `panic-halt` instead.
- **No `defmt::info!()` calls** — without RTT transport, they produce dead code. The blinking LED IS the output.
- Keep the existing no-op `defmt::global_logger` to satisfy linker symbols required by `embassy-stm32`'s internal defmt usage.
- Use `#[embassy_executor::main]` async pattern matching daisy-embassy's example.
- Rename current `main.rs` to remove the default binary target conflict, OR configure Cargo so `blinky` is a separate binary. Since `main.rs` currently exists as the default binary, rename it to `main.rs` under a different bin name... actually, Cargo multi-bin convention: each file in `src/bin/` becomes a separate binary named after its filename. So `src/bin/blinky.rs` produces binary `blinky` and `src/bin/main.rs` produces binary `main`. No conflict.

File structure:
```rust
#![no_std]
#![no_main]

use embassy_time::Timer;
use {defmt, panic_halt};

// No-op defmt logger — satisfies linker symbols required by embassy-stm32's
// internal defmt usage. Remove when adding real logging (e.g. defmt-rtt).
#[defmt::global_logger]
struct Logger;

unsafe impl defmt::Logger for Logger {
    fn acquire() {}
    unsafe fn release() {}
    unsafe fn flush() {}
    unsafe fn write(data: &[u8]) {
        let _ = data;
    }
}

#[no_mangle]
unsafe extern "C" fn _defmt_panic() -> ! {
    loop {}
}

#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    let config = daisy_embassy::default_rcc();
    let p = daisy_embassy::hal::init(config);
    let board: daisy_embassy::DaisyBoard<'_> = daisy_embassy::new_daisy_board!(p);
    let mut led = board.user_led;

    loop {
        led.on();
        Timer::after_millis(300).await;
        led.off();
        Timer::after_millis(300).await;
    }
}
```

#### Step 3: Build pipeline — `Makefile` in `firmware/`

Create `firmware/Makefile` with targets:
- `build`: `cargo objcopy --release --features seed3 --bin blinky -- -O binary firmware.bin`
- `flash`: `dfu-util -a 0 -s 0x08000000:leave -D firmware.bin`
- `flash-all`: `$(MAKE) build && $(MAKE) flash`
- `clean`: standard cargo clean

This gives the human a single-command path (`make flash-all`) and documents prerequisites.

Prerequisites to document in comments:
- `rustup target add thumbv7em-none-eabihf`
- `cargo install cargo-binutils`
- `dfu-util` system package

#### Step 4: Verify build succeeds

Run `cargo build --release --features seed3 --bin blinky` to confirm compilation. Then `cargo objcopy --release --features seed3 --bin blinky -- -O binary firmware.bin` to produce the `.bin`. Check file size is well under 128 KB.

Note: If `cargo objcopy` is not installed, the build step will fail gracefully with an error message pointing to `cargo install cargo-binutils`.

#### Step 5: Update `docs/reference/daisy-seed3.md`

Update the "Flashing without a debug probe" section with:
- The exact working command sequence (including `--bin blinky`)
- Note about `:leave` suffix reliability on STM32H7 (may need power cycle)
- Reference to `firmware/Makefile` targets
- Note about `memory.x` FLASH length mismatch (cosmetic, doesn't block blinky)

### Verification

- [ ] `cargo build --release --features seed3 --bin blinky` compiles without errors
- [ ] `cargo objcopy` produces `firmware.bin` < 128 KB
- [ ] `firmware/Makefile` targets exist and are documented
- [ ] `docs/reference/daisy-seed3.md` updated with exact procedure
<!-- PLAN:END -->
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation

### Files created/modified
- **firmware/src/bin/blinky.rs** — new blinky binary, toggles user LED (PC7) at ~1.6 Hz (300 ms on/off). Uses embassy-time for delays, no debug probe required.
- **firmware/Makefile** — build/flash targets: `make build` (cargo objcopy to .bin), `make flash` (dfu-util), `make flash-all` (both), `make check`, `make clean`.
- **firmware/Cargo.toml** — added `embassy-time = "0.5"` dependency (needed for Timer::after_millis()).
- **flake.nix** — fixed fenix toolchain configuration. Previous version listed `f.targets.thumbv7em-none-eabihf.stable.rust-std` as a separate package, which doesn't merge the target stdlib into rustc's sysroot. Changed to `f.combine [f.stable.rustc f.targets.thumbv7em-none-eabihf.stable.rust-std f.stable.llvm-tools]` so cargo finds core/liballoc for thumbv7em-none-eabihf and llvm-objcopy for cargo-binutils.
- **docs/reference/daisy-seed3.md** — expanded flashing section with step-by-step procedure, Makefile targets, manual commands, and notes about :leave reliability, binary size, and memory.x.

### Build verification
- `cargo build --release --features seed3 --bin blinky` compiles cleanly
- `cargo objcopy` produces firmware.bin at **17,614 bytes** (well under 128 KB internal flash limit)
- `make build` works end-to-end via the Makefile
- Existing main.rs binary still compiles (no regression)
<!-- SECTION:NOTES:END -->
