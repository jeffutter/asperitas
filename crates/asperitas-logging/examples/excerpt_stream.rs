//! `excerpt_stream` - turn a WAV file into the framed `EXCSTART`/`EXCDATA`/`EXCEND` install stream
//! for one QSPI excerpt slot, on stdout.
//!
//! ```sh
//! cargo run -p asperitas-logging --example excerpt_stream -- clip.wav 3 guitar_a > stream.bin
//! cat stream.bin > /dev/cu.usbmodem…
//! # or straight to the device:
//! cargo run -p asperitas-logging --example excerpt_stream -- clip.wav 3 guitar_a > /dev/cu.usbmodem…
//! ```
//!
//! Arguments are `<wav> <slot> <name> [--seq0 N]`. The WAV must be PCM mono 48 kHz 16-bit;
//! anything else is refused rather than converted (see `excerpt::parse_wav`). `slot` is decimal,
//! `0..14`. `name` is 1 to 16 characters of `[a-z0-9_]`. `--seq0` sets the first frame's sequence
//! number (default 0); every frame's `t_ms` is 0.
//!
//! # What this tool does not decide
//!
//! Whether the install worked. The pipe has no return path and this tool waits for nothing: it
//! writes the frames and exits 0. The device reads every PCM byte back from flash after `EXCEND`
//! and reports `EXCOK` or `EXCFAIL` on the console, and that readback is the only verdict. A clean
//! exit here means only that the stream was written.
//!
//! The frames come from `excerpt::install_frames`, the same function the host tests pin against
//! `tests/golden/excerpt_stream.bin`, so this wrapper cannot emit a wire format the device was not
//! tested against. To check it end to end:
//!
//! ```sh
//! cargo run -p asperitas-logging --example excerpt_stream -- \
//!     crates/asperitas-logging/tests/golden/excerpt_ramp.wav 3 ramp1000 \
//!   | cmp - crates/asperitas-logging/tests/golden/excerpt_stream.bin
//! ```
//!
//! Exit codes: `0` stream written, `2` bad arguments, unreadable or refused WAV, or a write error.

use std::env;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::process::exit;

use asperitas_logging::excerpt::{install_frames, parse_wav, Tag};

fn main() {
    if let Err(message) = run(env::args().skip(1).collect()) {
        eprintln!("excerpt_stream: {message}");
        exit(2);
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    const USAGE: &str = "usage: excerpt_stream <wav> <slot> <name> [--seq0 N]";
    let mut positional = Vec::new();
    let mut seq0 = 0u32;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--seq0" {
            let value = args.next().ok_or(USAGE)?;
            seq0 = value
                .parse()
                .map_err(|_| format!("--seq0 {value:?} is not a u32"))?;
        } else {
            positional.push(arg);
        }
    }
    let [path, slot, name] = <[String; 3]>::try_from(positional).map_err(|_| USAGE)?;

    let slot: u8 = slot
        .parse()
        .map_err(|_| format!("slot {slot:?} is not a number"))?;
    let tag = Tag::new(name.as_bytes()).map_err(|err| format!("name {name:?}: {err:?}"))?;
    let raw = fs::read(&path).map_err(|err| format!("{path}: {err}"))?;
    let pcm = parse_wav(&raw).map_err(|err| format!("{path}: {err:?}"))?;

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let mut write_error = None;
    install_frames(slot, &tag, pcm, seq0, 0, |frame| {
        if write_error.is_none() {
            write_error = out.write_all(frame).err();
        }
    })
    .map_err(|err| format!("{path}: {err:?}"))?;
    if let Some(err) = write_error {
        return Err(format!("stdout: {err}"));
    }
    out.flush().map_err(|err| format!("stdout: {err}"))
}
