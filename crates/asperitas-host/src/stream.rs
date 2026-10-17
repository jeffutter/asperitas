//! Feeding a byte stream through the framed-console [`Decoder`].
//!
//! Every host tool that reads a console capture does the same thing: read a chunk, hand it to
//! [`Decoder::feed`], and flush at end of stream. Written once here so no tool has its own read
//! loop to get wrong.

use std::io::{self, Read};

use asperitas_wire::frame::{Decoder, Record};

/// Bytes requested per read. Larger than one frame so record boundaries never align by luck.
const READ_BYTES: usize = 4096;

/// What [`pump`] got through.
#[derive(Debug)]
pub struct Pumped {
    /// Bytes the decoder actually took. Bytes still waiting in a chunk when the pump stopped early
    /// were never offered, so they belong to no counter and the byte-accounting law still balances
    /// against this figure.
    pub pushed: u64,
    /// Why the pump stopped before end of stream, if it did. A stream with an error here is not a
    /// complete capture whatever its counters say.
    pub error: Option<io::Error>,
}

impl Pumped {
    /// The pushed byte count, or the error that cut the stream short. For callers that have no use
    /// for a partial result.
    pub fn into_result(self) -> io::Result<u64> {
        match self.error {
            None => Ok(self.pushed),
            Some(e) => Err(e),
        }
    }
}

/// Read `reader` to end of stream, handing every validated record to `on_record` in arrival order.
///
/// The decoder is always left finished, even after a read error: whatever it still held undecided
/// becomes discarded bytes instead of an invisible remainder, so its
/// [`stats`](Decoder::stats) and [`buffered`](Decoder::buffered) describe everything that was read
/// and the caller can check the accounting law against [`Pumped::pushed`]. Records the final flush
/// completes are delivered too.
///
/// [`io::ErrorKind::Interrupted`] is retried. A decoder that stops taking input ends the pump with
/// an error rather than spinning.
pub fn pump<R, F>(mut reader: R, decoder: &mut Decoder, mut on_record: F) -> Pumped
where
    R: Read,
    F: FnMut(&Record<'_>),
{
    let mut chunk = [0u8; READ_BYTES];
    let mut pushed: u64 = 0;
    let mut error = None;

    loop {
        let filled = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                error = Some(e);
                break;
            }
        };
        let taken = decoder.feed(&chunk[..filled], &mut on_record);
        pushed += taken as u64;
        if taken < filled {
            error = Some(io::Error::other("decoder made no progress"));
            break;
        }
    }

    decoder.finish();
    while let Some(record) = decoder.next_record() {
        on_record(&record);
    }
    Pumped { pushed, error }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asperitas_wire::frame::{encode, MAX_FRAME};
    use asperitas_wire::Level;

    fn frames(count: u32) -> Vec<u8> {
        let mut wire = Vec::new();
        for seq in 0..count {
            let mut buf = [0u8; MAX_FRAME];
            let n = encode(
                Level::Info,
                seq,
                seq,
                format!("hello {seq}").as_bytes(),
                &mut buf,
            )
            .len;
            wire.extend_from_slice(&buf[..n]);
        }
        wire
    }

    #[test]
    fn delivers_every_record_in_order_and_counts_bytes() {
        let wire = frames(1000);
        let mut decoder = Decoder::new();
        let mut seqs = Vec::new();
        let pushed = pump(wire.as_slice(), &mut decoder, |r| seqs.push(r.seq))
            .into_result()
            .unwrap();
        assert_eq!(pushed, wire.len() as u64);
        assert_eq!(seqs, (0..1000).collect::<Vec<_>>());
        assert_eq!(decoder.stats().bad_frames, 0);
    }

    #[test]
    fn a_read_error_is_an_error_not_a_short_stream() {
        struct Failing;
        impl Read for Failing {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("unplugged"))
            }
        }
        let mut decoder = Decoder::new();
        assert!(pump(Failing, &mut decoder, |_| {}).into_result().is_err());
    }
}
