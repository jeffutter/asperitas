//! Items every firmware binary needs the linker to see exactly once: the Rust panic handler, the
//! defmt panic handler and the defmt global logger. Each binary links this crate with
//! `use asperitas_firmware as _;`, which is what pulls these symbols into its image even though
//! the binary names nothing from here.
//!
//! `ledtest` is the one binary that does not link this crate. It exists to rule out the LED
//! singleton and the USB pipe, so it carries its own panic handler that does nothing but stop.
#![no_std]

pub mod audio;
pub mod host_control;

/// Hand every panic to the shared handler, which latches the LED and flushes what it can over
/// the console.
#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    asperitas_logging::panic_handler::handle_panic(info)
}

// `_defmt_panic` is its own symbol (defmt `export` module) that `defmt-rtt` does not provide, so
// this stays compiled in under `log-defmt` too; dropping it breaks the link of every
// `defmt::assert!` in embassy-stm32. It is unrelated to the `#[panic_handler]` above. No `bkpt()`:
// with no probe attached BKPT escalates to a HardFault instead of halting, which would turn a
// diagnostic into a silent lockup.
#[defmt::panic_handler]
fn defmt_panic_handler() -> ! {
    loop {
        cortex_m::asm::nop();
    }
}

// The defmt logger supplies `_defmt_write`, `_defmt_acquire`, `_defmt_release` and `_defmt_flush`,
// which every defmt frame inside embassy-stm32 and daisy-embassy resolves against. Exactly one of
// the two sources below is compiled in. With a probe wired, `defmt-rtt` fills its RTT ring and
// probe-rs reads it; without one nobody scans RAM, so the stub discards every byte.
#[cfg(feature = "log-defmt")]
use defmt_rtt as _;

#[cfg(not(feature = "log-defmt"))]
asperitas_logging::noop_defmt_logger!();
