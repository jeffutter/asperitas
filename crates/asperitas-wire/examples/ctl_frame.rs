//! `ctl_frame` - print one framed `CTL FX`, `CTL SET` or `CTL REL` host command on stdout, for
//! sending to `rig` by hand.
//!
//! ```sh
//! cargo run -q -p asperitas-wire --example ctl_frame -- 01 0xC000 > fx.frame
//! cargo run -q -p asperitas-wire --example ctl_frame -- set 00 0x8000 > set.frame
//! cargo run -q -p asperitas-wire --example ctl_frame -- rel 00 > rel.frame
//! cat fx.frame > /dev/cu.usbmodem...
//! ```
//!
//! Arguments are `<fx> <value> [seq]`, `set <knob> <value> [seq]` or `rel <knob> [seq]`: `fx` is
//! the effect code (`00` bypass, `01` gain, `02` one-pole low-pass), `knob` the knob index (the
//! rig serves knob 0 only) and `value` the `0..=0xFFFF` knob position, all hex with an optional
//! `0x`; `seq` is decimal and defaults to 0. The frame is built by the same `control::Command::encode`
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

use asperitas_wire::control::{BodyBuf, Command};
use asperitas_wire::frame::{encode, MAX_BODY, MAX_FRAME};
use asperitas_wire::Level;

fn hex(arg: &str) -> Option<u32> {
    u32::from_str_radix(arg.trim_start_matches("0x"), 16).ok()
}

/// The command named by `args`, and the frame sequence number, or `None` for bad arguments.
fn parse(args: &[String]) -> Option<(Command, u32)> {
    let byte = |a: &str| hex(a).filter(|&v| v <= 0xFF).map(|v| v as u8);
    let word = |a: &str| hex(a).filter(|&v| v <= 0xFFFF).map(|v| v as u16);
    let seq = |a: Option<&String>| match a {
        None => Some(0),
        Some(s) => s.parse::<u32>().ok(),
    };
    match args {
        [verb, knob, value, rest @ ..] if verb == "set" && rest.len() <= 1 => Some((
            Command::OverrideSet {
                knob: byte(knob)?,
                value: word(value)?,
            },
            seq(rest.first())?,
        )),
        [verb, knob, rest @ ..] if verb == "rel" && rest.len() <= 1 => Some((
            Command::OverrideRelease { knob: byte(knob)? },
            seq(rest.first())?,
        )),
        [fx, value, rest @ ..] if rest.len() <= 1 => Some((
            Command::ReplayFx {
                fx: byte(fx)?,
                value: word(value)?,
            },
            seq(rest.first())?,
        )),
        _ => None,
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some((command, seq)) = parse(&args) else {
        eprintln!("usage: ctl_frame <fx hex> <value hex> [seq] | set <knob hex> <value hex> [seq] | rel <knob hex> [seq]");
        exit(2);
    };

    let mut body: BodyBuf = [0; MAX_BODY];
    let n = command.encode(&mut body);
    let mut frame = [0u8; MAX_FRAME];
    let encoded = encode(Level::Info, seq, 0, &body[..n], &mut frame);
    if let Err(e) = io::stdout().write_all(&frame[..encoded.len]) {
        eprintln!("ctl_frame: write failed: {e}");
        exit(2);
    }
}
