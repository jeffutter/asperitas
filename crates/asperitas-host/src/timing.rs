//! Device-clock timing of a sweep, read from a recorded console stream.
//!
//! Every frame carries `t_ms`, so the first `CTLR OK SET` reply stamps when the device applied the
//! first knob override. The stream does not stamp the start of the capture: audio-dump frames are
//! emitted after the capture finishes, so their `t_ms` is dump time, and no record carries the
//! device time of the first captured block. An offset in seconds from capture start therefore
//! cannot be derived from the stream alone; [`sweep_offset_s`] says so with
//! [`TimingError::NoCaptureClock`] instead of guessing from dump times. The session sidecar has to
//! carry the host-side pairing of capture start and device time.

use std::fmt;
use std::io::{self, Read};

use asperitas_wire::control::{parse_reply, Reply, Verb};
use asperitas_wire::frame::Decoder;

use crate::stream::pump;

/// Why a sweep offset could not be derived from a stream.
#[derive(Debug)]
pub enum TimingError {
    Io(io::Error),
    /// The stream holds no `CTLR OK SET` reply, so no sweep started in it.
    NoSweep,
    /// A `CTLR OK SET` was found, but nothing in the stream says what device time the capture
    /// began at, so the offset is unknowable from the stream.
    NoCaptureClock {
        /// Device `t_ms` of the first `CTLR OK SET`.
        first_set_t_ms: u32,
    },
}

impl fmt::Display for TimingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimingError::Io(e) => write!(f, "stream read failed: {e}"),
            TimingError::NoSweep => write!(f, "stream contains no CTLR OK SET reply"),
            TimingError::NoCaptureClock { first_set_t_ms } => write!(
                f,
                "first CTLR OK SET at device t_ms {first_set_t_ms}, but the stream carries no \
                 device time for the start of the capture"
            ),
        }
    }
}

impl std::error::Error for TimingError {}

impl From<io::Error> for TimingError {
    fn from(e: io::Error) -> Self {
        TimingError::Io(e)
    }
}

/// Device `t_ms` of the first `CTLR OK SET` reply in a raw console stream, or `None` if there is
/// none. Later `SET` replies and every other verb are ignored. The value is modulo `1e8`, as
/// transmitted.
pub fn first_set_t_ms<R: Read>(input: R) -> io::Result<Option<u32>> {
    let mut decoder = Decoder::new();
    let mut first = None;
    pump(input, &mut decoder, |record| {
        if first.is_none() && parse_reply(record.body) == Ok(Reply::Ok(Verb::Set)) {
            first = Some(record.t_ms);
        }
    })
    .into_result()?;
    Ok(first)
}

/// Seconds from capture start to the first `CTLR OK SET`, if the stream could say. It cannot:
/// see the module docs. Returns [`TimingError::NoSweep`] when there is no `SET` and
/// [`TimingError::NoCaptureClock`] (carrying the `SET` time) when there is.
pub fn sweep_offset_s<R: Read>(input: R) -> Result<f64, TimingError> {
    match first_set_t_ms(input)? {
        None => Err(TimingError::NoSweep),
        Some(first_set_t_ms) => Err(TimingError::NoCaptureClock { first_set_t_ms }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asperitas_wire::control::BodyBuf;
    use asperitas_wire::dump::{audio_record, CHUNK_RAW};
    use asperitas_wire::frame::{encode, MAX_BODY, MAX_FRAME};
    use asperitas_wire::Level;

    fn frame(seq: u32, t_ms: u32, body: &[u8]) -> Vec<u8> {
        let mut buf = [0u8; MAX_FRAME];
        let n = encode(Level::Info, seq, t_ms, body, &mut buf).len;
        buf[..n].to_vec()
    }

    fn reply(seq: u32, t_ms: u32, r: Reply) -> Vec<u8> {
        let mut b: BodyBuf = [0u8; MAX_BODY];
        let n = r.encode(&mut b);
        frame(seq, t_ms, &b[..n])
    }

    /// One AUDIO chunk frame stamped with a dump-time `t_ms`.
    fn audio(seq: u32, t_ms: u32) -> Vec<u8> {
        let raw = [0u8; CHUNK_RAW];
        let mut body = [0u8; MAX_BODY];
        let mut fb = [0u8; MAX_FRAME];
        let e = audio_record(Level::Info, seq, t_ms, 0, 1, 0, &raw, &mut body, &mut fb).unwrap();
        fb[..e.len].to_vec()
    }

    fn stream(parts: &[Vec<u8>]) -> Vec<u8> {
        parts.concat()
    }

    #[test]
    fn first_set_is_found_and_later_set_and_release_are_ignored() {
        let wire = stream(&[
            reply(0, 100, Reply::Ok(Verb::Fx)),
            reply(1, 2_500, Reply::Ok(Verb::Set)),
            reply(2, 3_000, Reply::Ok(Verb::Set)),
            reply(3, 4_000, Reply::Ok(Verb::Release)),
        ]);
        assert_eq!(first_set_t_ms(wire.as_slice()).unwrap(), Some(2_500));
    }

    #[test]
    fn a_stream_without_set_has_none() {
        let wire = stream(&[
            reply(0, 100, Reply::Ok(Verb::Fx)),
            reply(1, 200, Reply::Ok(Verb::Release)),
            frame(2, 300, b"CTLR ERR busy"),
        ]);
        assert_eq!(first_set_t_ms(wire.as_slice()).unwrap(), None);
        assert!(matches!(
            sweep_offset_s(wire.as_slice()),
            Err(TimingError::NoSweep)
        ));
    }

    #[test]
    fn audio_frame_times_do_not_give_a_capture_clock() {
        let wire = stream(&[
            reply(0, 2_500, Reply::Ok(Verb::Set)),
            audio(1, 90_000),
            audio(2, 90_010),
        ]);
        match sweep_offset_s(wire.as_slice()) {
            Err(TimingError::NoCaptureClock { first_set_t_ms }) => {
                assert_eq!(first_set_t_ms, 2_500)
            }
            other => panic!("expected NoCaptureClock, got {other:?}"),
        }
    }
}
