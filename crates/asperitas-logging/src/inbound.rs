//! Host-to-device console records: the byte-level half of the CDC OUT reader.
//!
//! The device has always allocated a CDC OUT endpoint and never read it. The first thing that
//! needs it is excerpt install (TASK-038.04), which streams ordinary v1 `~` frames from the host
//! with a plain `cat stream.bin > /dev/cu.usbmodem…`. This module turns those raw bytes into
//! whole, CRC-checked records; `usb.rs` owns the endpoint and the channel the binary reads.
//!
//! # Why this module is not behind `log-usb`
//!
//! Same reason as [`crate::frame`] and [`crate::console`]: everything here is arithmetic on a
//! [`Decoder`] and a few atomics, so the default-feature `cargo test --workspace` that CI runs
//! can prove the lossless feed loop on the host. Only the endpoint read and the channel live in
//! the gated `usb.rs`.
//!
//! # Lossless by construction
//!
//! [`InboundFeed::feed`] takes bytes and a sink. The sink either accepts a record or refuses
//! it; a refused record stays held in the feed, and `feed` stops taking bytes until a later
//! call delivers it. Nothing is dropped to make room, so the only thing a slow consumer can do
//! is slow the reader down — and on the device that means the read stops being re-armed, the
//! endpoint NAKs, and USB flow control pushes the stall back to the host writer. The async
//! caller's loop is:
//!
//! ```text
//! let mut off = 0;
//! loop {
//!     off += feed.feed(&packet[off..], |r| channel.try_send(*r).is_ok());
//!     match feed.take_pending() {
//!         Some(r) => channel.send(r).await,   // backpressure: suspend, then retry the rest
//!         None => break,                      // every byte of `packet` taken
//!     }
//! }
//! ```
//!
//! # Scope: narrow and install-oriented, on purpose
//!
//! This is the minimum inbound path excerpt install needs: frames in, records out, counters
//! for what was rejected. It parses no commands and assigns no meaning to a body — the
//! consumer (`excerpt::parse_record` today) does that. TASK-032 (host control) can reuse all of
//! it unchanged: [`InboundFeed`], [`InboundRecord`], [`INBOUND`]'s counters and `usb.rs`'s
//! channel are transport, not install policy. What TASK-032 would add is a dispatcher that
//! routes records by verb to more than one consumer, which is deliberately absent here because
//! with a single consumer it would be a configuration parameter nobody uses.
//!
//! # Counters
//!
//! [`INBOUND`] is separate from [`crate::console::CONSOLE`] and nothing here changes `STATUS`:
//! its field set is a contract TASK-031 parses, and inbound traffic only exists in binaries
//! that opt in. A consumer that wants these numbers on the wire reports them in its own
//! records. Like the console counters they are cumulative since boot and saturate rather than
//! wrap; see `console`'s module docs for why.

use core::sync::atomic::{AtomicU32, Ordering};

use crate::frame::{Decoder, Record, Stats, MAX_BODY};

// ---------------------------------------------------------------------------
// Owned record
// ---------------------------------------------------------------------------

/// One validated host-to-device record, owned so it can cross a channel.
///
/// [`Record`] borrows the decoder's queue and dies at the next push, which is why the feed
/// copies each one out. 212 bytes; `Copy` because a channel moves it by value anyway.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InboundRecord {
    /// Wire letter: `I`, `W`, `E`, `D` or `T`.
    pub level: u8,
    /// Sequence number as the host transmitted it.
    pub seq: u32,
    /// Milliseconds field as the host transmitted it.
    pub t_ms: u32,
    len: u8,
    body: [u8; MAX_BODY],
}

// A body length must fit `len`; MAX_BODY is 200 today. Fails the build, not a capture.
const _: () = assert!(MAX_BODY <= u8::MAX as usize);

impl InboundRecord {
    /// Copy a decoder record out of the decoder's queue.
    pub fn from_record(record: &Record<'_>) -> Self {
        let mut body = [0u8; MAX_BODY];
        // The decoder never delivers more than MAX_BODY bytes of body.
        let n = record.body.len().min(MAX_BODY);
        body[..n].copy_from_slice(&record.body[..n]);
        Self {
            level: record.level,
            seq: record.seq,
            t_ms: record.t_ms,
            len: n as u8,
            body,
        }
    }

    /// The record body exactly as it arrived (already sanitised by the wire format).
    pub fn body(&self) -> &[u8] {
        &self.body[..self.len as usize]
    }
}

// ---------------------------------------------------------------------------
// Counters
// ---------------------------------------------------------------------------

/// One read of every inbound counter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InboundCounters {
    /// Records that validated on framing and CRC. Counted at validation, so a record the
    /// sink refused is counted while it is held; it is never dropped, so it will arrive.
    pub records: u32,
    /// Candidate start markers examined and rejected.
    pub bad_frames: u32,
    /// Rejections after which a later start marker was found.
    pub resyncs: u32,
    /// Bytes that never reached a validated record: junk between records, rejected
    /// candidates, and the half record a dropped link leaves behind.
    pub discarded_bytes: u32,
    /// Endpoint reads that failed, i.e. the host went away mid-session.
    pub ep_errors: u32,
}

/// Cumulative inbound counters. `Relaxed` throughout, for the same reason as
/// [`crate::console::ConsoleStats`]: single core, snapshotted, ordering nothing.
#[derive(Debug, Default)]
pub struct InboundStats {
    records: AtomicU32,
    bad_frames: AtomicU32,
    resyncs: AtomicU32,
    discarded_bytes: AtomicU32,
    ep_errors: AtomicU32,
}

/// The device's inbound counters, written by `usb.rs`'s reader.
pub static INBOUND: InboundStats = InboundStats::new();

impl InboundStats {
    /// Zeroed, as at boot.
    pub const fn new() -> Self {
        Self {
            records: AtomicU32::new(0),
            bad_frames: AtomicU32::new(0),
            resyncs: AtomicU32::new(0),
            discarded_bytes: AtomicU32::new(0),
            ep_errors: AtomicU32::new(0),
        }
    }

    /// An endpoint read failed.
    pub fn endpoint_error(&self) {
        add(&self.ep_errors, 1);
    }

    /// Every counter in one read.
    pub fn snapshot(&self) -> InboundCounters {
        InboundCounters {
            records: self.records.load(Ordering::Relaxed),
            bad_frames: self.bad_frames.load(Ordering::Relaxed),
            resyncs: self.resyncs.load(Ordering::Relaxed),
            discarded_bytes: self.discarded_bytes.load(Ordering::Relaxed),
            ep_errors: self.ep_errors.load(Ordering::Relaxed),
        }
    }
}

/// Add `n`, saturating at `u32::MAX`.
fn add(counter: &AtomicU32, n: u64) {
    let n = u32::try_from(n).unwrap_or(u32::MAX);
    if n == 0 {
        return;
    }
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
        Some(v.saturating_add(n))
    });
}

// ---------------------------------------------------------------------------
// Feed
// ---------------------------------------------------------------------------

/// Raw inbound bytes in, validated records out to a sink, with nothing lost to backpressure.
///
/// Wraps a [`Decoder`] (about 1.9 KB inline) plus one held record. Counters are published to
/// the [`InboundStats`] it was built with as they move, so a snapshot taken mid-session is
/// current rather than waiting for a disconnect.
pub struct InboundFeed<'s> {
    decoder: Decoder,
    /// A record the sink refused, delivered before anything else.
    pending: Option<InboundRecord>,
    /// Decoder counters already added to `stats`, so each publish adds only the delta.
    published: Stats,
    stats: &'s InboundStats,
}

impl<'s> InboundFeed<'s> {
    /// A fresh feed reporting into `stats` (on the device, [`INBOUND`]).
    pub const fn new(stats: &'s InboundStats) -> Self {
        Self {
            decoder: Decoder::new(),
            pending: None,
            published: Stats {
                records: 0,
                bad_frames: 0,
                resyncs: 0,
                discarded_bytes: 0,
            },
            stats,
        }
    }

    /// Offer `bytes`, handing every record they complete to `sink`, and return how many bytes
    /// were taken.
    ///
    /// `sink` returns `false` to refuse a record (a full channel). The feed then keeps
    /// that record, stops taking bytes, and returns a short count — or a full count if the
    /// refusal came after the last byte. Either way [`take_pending`](Self::take_pending) then
    /// returns `Some`, and the caller delivers it (awaiting space) before offering the
    /// untaken remainder again. When `take_pending` returns `None`, every byte was taken and
    /// every record they completed reached the sink.
    pub fn feed(&mut self, bytes: &[u8], mut sink: impl FnMut(&InboundRecord) -> bool) -> usize {
        let mut off = 0;
        loop {
            if !self.drain(&mut sink) {
                break;
            }
            if off == bytes.len() {
                break;
            }
            // `push` stops early only when the decoder's own queue is full, which the drain
            // above just emptied; the loop then drains and pushes again.
            off += self.decoder.push(&bytes[off..]);
        }
        self.publish();
        off
    }

    /// The record the sink last refused, if any. The caller must deliver it before the next
    /// [`feed`](Self::feed); dropping it loses a record
    /// the `records` counter already reports as arrived.
    pub fn take_pending(&mut self) -> Option<InboundRecord> {
        self.pending.take()
    }

    /// Start a new session: abandon the undecided window and begin with a fresh decoder.
    ///
    /// Called on every (re)connection, so a half record left by a dropped link is counted as
    /// discarded and cannot splice onto the next session's bytes. `finish()` first so those
    /// bytes reach `discarded_bytes` before the decoder holding them is replaced.
    ///
    /// Whole records survive: a held record and anything still in the decoder's queue are
    /// carried over and delivered by the next `feed`. In the device loop both are always
    /// empty here, because a session only ends at an endpoint read, and a read is only issued
    /// once the previous packet was fully delivered.
    pub fn reset(&mut self) {
        self.decoder.finish();
        self.publish();
        if self.decoder.queued() == 0 {
            self.decoder = Decoder::new();
            self.published = Stats::default();
        }
        // Otherwise keep the decoder: finish() already emptied its window, so no half
        // record can splice, and replacing it would destroy validated records.
    }

    /// Bytes offered but not yet decided. Zero after [`reset`](Self::reset).
    pub fn buffered(&self) -> usize {
        self.decoder.buffered()
    }

    /// Deliver the held record and then the decoder's queue. Returns `false` if the sink
    /// refused one, which is then held.
    fn drain(&mut self, sink: &mut impl FnMut(&InboundRecord) -> bool) -> bool {
        if let Some(record) = &self.pending {
            if !sink(record) {
                return false;
            }
            self.pending = None;
        }
        while let Some(record) = self.decoder.next_record() {
            let owned = InboundRecord::from_record(&record);
            if !sink(&owned) {
                self.pending = Some(owned);
                return false;
            }
        }
        true
    }

    /// Add the decoder's counter movement since the last publish.
    fn publish(&mut self) {
        let now = self.decoder.stats();
        add(&self.stats.records, now.records - self.published.records);
        add(
            &self.stats.bad_frames,
            now.bad_frames - self.published.bad_frames,
        );
        add(&self.stats.resyncs, now.resyncs - self.published.resyncs);
        add(
            &self.stats.discarded_bytes,
            now.discarded_bytes - self.published.discarded_bytes,
        );
        self.published = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{encode, MAX_FRAME};
    use log::Level;

    fn frame(seq: u32, body: &[u8]) -> ([u8; MAX_FRAME], usize) {
        let mut buf = [0u8; MAX_FRAME];
        let len = encode(Level::Info, seq, 7, body, &mut buf).len;
        (buf, len)
    }

    #[test]
    fn reset_mid_record_discards_the_half_and_the_next_session_starts_clean() {
        let stats = InboundStats::new();
        let mut feed = InboundFeed::new(&stats);
        let (a, a_len) = frame(1, b"first");
        let mut got = [0u32; 4];
        let mut n = 0;

        // Half a record, then the link drops.
        let half = a_len / 2;
        assert_eq!(feed.feed(&a[..half], |_| panic!("no record yet")), half);
        feed.reset();
        assert_eq!(feed.buffered(), 0);
        assert_eq!(stats.snapshot().discarded_bytes, half as u32);

        // The tail of that record arrives on the new session: it must not complete anything.
        let mut sink = |r: &InboundRecord| {
            got[n] = r.seq;
            n += 1;
            true
        };
        feed.feed(&a[half..a_len], &mut sink);
        let (b, b_len) = frame(2, b"second");
        feed.feed(&b[..b_len], &mut sink);
        assert_eq!(&got[..n], &[2]);
        assert_eq!(stats.snapshot().records, 1);
    }

    #[test]
    fn reset_keeps_a_queued_whole_record() {
        let stats = InboundStats::new();
        let mut feed = InboundFeed::new(&stats);
        let (a, a_len) = frame(5, b"kept");
        // Refuse everything, so the record is held.
        assert_eq!(feed.feed(&a[..a_len], |_| false), a_len);
        let held = feed.take_pending().expect("refused record is held");
        // Put it back the way a caller that forgot to deliver it would leave things.
        feed.pending = Some(held);
        feed.reset();
        let mut seen = None;
        feed.feed(&[], |r| {
            seen = Some(r.seq);
            true
        });
        assert_eq!(seen, Some(5));
    }
}
