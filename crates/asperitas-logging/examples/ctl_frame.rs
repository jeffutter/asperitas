//! `ctl_frame` - print one framed `CTL FX` host command on stdout, for sending to `rig` by hand.
//!
//! ```sh
//! cargo run -q -p asperitas-logging --example ctl_frame -- 01 0xC000 > fx.bin
//! cat fx.bin > /dev/cu.usbmodem...
//! ```
//!
//! Arguments are `<fx> <value> [seq]`: `fx` is the effect code (`00` bypass, `01` gain, `02`
//! one-pole low-pass) and `value` the `0..=0xFFFF` knob position, both hex with an optional `0x`;
//! `seq` is decimal and defaults to 0. The frame is built by the same `control::Command::encode`
//! and `frame::encode` the device parses, so this cannot emit a body the firmware was not tested
//! against. It sends nothing and waits for nothing: the `CTLR OK FX` reply is the only verdict,
//! and it arrives on the console the caller is already reading.
//!
//! Timing is the caller's problem. `rig` holds the replay pass until the capture's first callback,
//! `ARM_DELAY_MS` after audio starts, and refuses `FX` with `CTLR ERR busy` once the capture is
//! armed. Send the frame right after the console attaches.
//!
//! Exit codes: `0` frame written, `2` bad arguments or a write error.

use std::env;
use std::io::{self, Write};
use std::process::exit;

use asperitas_logging::control::{BodyBuf, Command};
use asperitas_logging::frame::{encode, MAX_BODY, MAX_FRAME};
use asperitas_logging::Level;

fn hex(arg: &str) -> Option<u32> {
    u32::from_str_radix(arg.trim_start_matches("0x"), 16).ok()
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let parsed = match args.as_slice() {
        [fx, value] => hex(fx).zip(hex(value)).map(|(f, v)| (f, v, 0)),
        [fx, value, seq] => hex(fx)
            .zip(hex(value))
            .zip(seq.parse::<u32>().ok())
            .map(|((f, v), s)| (f, v, s)),
        _ => None,
    };
    let Some((fx, value, seq)) = parsed.filter(|&(f, v, _)| f <= 0xFF && v <= 0xFFFF) else {
        eprintln!("usage: ctl_frame <fx hex 00..ff> <value hex 0000..ffff> [seq]");
        exit(2);
    };

    let mut body: BodyBuf = [0; MAX_BODY];
    let n = Command::ReplayFx {
        fx: fx as u8,
        value: value as u16,
    }
    .encode(&mut body);
    let mut frame = [0u8; MAX_FRAME];
    let encoded = encode(Level::Info, seq, 0, &body[..n], &mut frame);
    if let Err(e) = io::stdout().write_all(&frame[..encoded.len]) {
        eprintln!("ctl_frame: write failed: {e}");
        exit(2);
    }
}
