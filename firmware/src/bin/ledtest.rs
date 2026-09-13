//! LED bring-up bisect — the minimum firmware that can prove the board runs.
//!
//! This binary exists to answer one question when the board appears dead:
//! *is anything executing at all?* It deliberately depends on nothing but
//! clock init, board init, and three GPIO pins. No USB, no `asperitas-logging`,
//! no LED singleton, no polarity constant, no async beyond the executor and a
//! timer. If this blinks, the fault is downstream in whatever the real binary
//! does next; if it doesn't, the fault is in clocks, pins, or the board macro.
//!
//! # Reading the output
//!
//! The sequence drives each channel active-low in turn, then all-off, then
//! all-on. It is written so that *some* visible change happens regardless of
//! LED polarity, which keeps it useful as a bisect tool — it needs no polarity
//! assumption of its own to prove the core is running.
//!
//! Polarity itself is settled: the LEDs are **active-low**, identified with this
//! very sequence on 2026-08-01 (see `docs/reference/daisy-pod.md`), so in practice
//! you should now see the left-hand column. A right-hand-column reading would mean
//! the board differs from the one that was characterised.
//!
//! | Step        | If LEDs are active-low | If LEDs are active-high |
//! |-------------|------------------------|-------------------------|
//! | 1           | red                    | cyan                    |
//! | 2           | green                  | magenta                 |
//! | 3           | blue                   | yellow                  |
//! | 4           | off (dark)             | white                   |
//! | 5           | white                  | off (dark)              |
//!
//! Any motion at all means the core is running. A steady dark LED through all
//! five steps means it is not.
//!
//! Flash with: `make flash-all BINARY=ledtest`, then tap RESET.

#![no_std]
#![no_main]

include!(concat!(env!("OUT_DIR"), "/asp_pro.inc")); // cfg provenance note, see build.rs

use daisy_embassy::hal::gpio::{Level, Output, Speed};
use embassy_time::Timer;

// Self-contained panic handler — deliberately does NOT use
// `asperitas_logging::panic_handler`. That one touches the LED singleton and
// the USB pipe, which are exactly the subsystems this binary is trying to rule
// out. A panic here should simply stop, visibly, with the LED left as-is.
#[panic_handler]
fn panic_handler(_info: &core::panic::PanicInfo) -> ! {
    loop {
        cortex_m::asm::nop();
    }
}

// Provide the _defmt_panic symbol required by embassy-stm32's internal defmt
// usage. Not the Rust panic handler. No `bkpt()` here: with no debug probe
// attached, BKPT escalates to a HardFault rather than halting, which would
// turn a diagnostic into a silent lockup.
// Stays compiled in under `log-defmt` too: `_defmt_panic` is its own symbol (defmt
// src/export/mod.rs) that defmt-rtt does *not* provide, so dropping this would break linking of
// every `defmt::assert!` in embassy-stm32. It is unrelated to the `#[panic_handler]` above.
#[defmt::panic_handler]
fn defmt_panic_handler() -> ! {
    loop {
        cortex_m::asm::nop();
    }
}

// The defmt logger. Exactly one of these two is compiled in, and both are load-bearing: they
// supply `_defmt_write`, `_defmt_acquire`, `_defmt_release` and `_defmt_flush`, the symbols every
// defmt frame inside embassy-stm32 and daisy-embassy resolves against. Drop either half and the
// link fails on binaries that contain no defmt call of their own.
//
// With a probe wired, `defmt-rtt` fills its RTT ring and probe-rs reads it. Without one there is
// nobody to scan RAM, so the stub discards every byte — silent, but still required.
#[cfg(feature = "log-defmt")]
use defmt_rtt as _;

// No-op defmt logger.
//
// NOTE: This block must live in each binary crate, not in a shared lib.
// `#[defmt::global_logger]` is a proc-macro that emits linker symbols only
// when expanded inside the final binary crate; placing it in a lib crate
// causes dead-code elimination to drop the struct (and its generated
// symbols) because nothing references `Logger` by name.
#[cfg(not(feature = "log-defmt"))]
#[defmt::global_logger]
struct Logger;

#[cfg(not(feature = "log-defmt"))]
unsafe impl defmt::Logger for Logger {
    fn acquire() {}
    unsafe fn release() {}
    unsafe fn flush() {}
    unsafe fn write(data: &[u8]) {
        let _ = data;
    }
}

#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    let config = daisy_embassy::default_rcc();
    let p = daisy_embassy::hal::init(config);
    let board = daisy_embassy::new_daisy_board!(p);

    // Pod LED 1 — D20/D19/D18 = PC1/PA6/PA7. Driven directly rather than
    // through `asperitas_logging::led` so that no shared state or polarity
    // assumption sits between this code and the pins.
    let mut red = Output::new(board.pins.d20, Level::High, Speed::Low);
    let mut green = Output::new(board.pins.d19, Level::High, Speed::Low);
    let mut blue = Output::new(board.pins.d18, Level::High, Speed::Low);

    // Each entry is (red, green, blue) as raw pin levels. Low is "on" if the
    // LEDs are active-low. Both all-high and all-low appear in the sequence so
    // the pattern is unmistakable under either polarity.
    let steps = [
        (Level::Low, Level::High, Level::High),
        (Level::High, Level::Low, Level::High),
        (Level::High, Level::High, Level::Low),
        (Level::High, Level::High, Level::High),
        (Level::Low, Level::Low, Level::Low),
    ];

    loop {
        for (r, g, b) in steps {
            red.set_level(r);
            green.set_level(g);
            blue.set_level(b);
            Timer::after_millis(500).await;
        }
    }
}
