//! Host suite for `asperitas_logging::inbound::InboundFeed`: the lossless feed loop the CDC OUT
//! reader runs, driven here without embassy.
//!
//! Run with: `cargo test -p asperitas-logging --test inbound_feed`
//!
//! Every test drives the feed through `deliver`, a copy of the device loop in `usb.rs` with
//! the channel replaced by a bounded `VecDeque` that a test drains on its own schedule. That
//! is what makes backpressure observable: a full queue refuses, and the loop must hand the
//! held record over before offering more bytes.

use std::collections::VecDeque;

use asperitas_logging::frame::{encode, MAX_BODY, MAX_FRAME};
use asperitas_logging::inbound::{InboundCounters, InboundFeed, InboundRecord, InboundStats};
use asperitas_logging::Level;
use proptest::prelude::*;

/// A bounded stand-in for the embassy channel.
struct Bounded {
    queue: VecDeque<InboundRecord>,
    cap: usize,
    out: Vec<(u32, Vec<u8>)>,
    refusals: usize,
}

impl Bounded {
    fn new(cap: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            cap,
            out: Vec::new(),
            refusals: 0,
        }
    }

    fn try_send(&mut self, r: &InboundRecord) -> bool {
        if self.queue.len() == self.cap {
            self.refusals += 1;
            false
        } else {
            self.queue.push_back(*r);
            true
        }
    }

    /// What `send().await` amounts to: the consumer frees space, then the record goes in.
    fn send_blocking(&mut self, r: InboundRecord) {
        self.drain_one();
        assert!(self.try_send(&r), "space was just made");
    }

    fn drain_one(&mut self) {
        if let Some(r) = self.queue.pop_front() {
            self.out.push((r.seq, r.body().to_vec()));
        }
    }

    fn drain_all(&mut self) {
        while !self.queue.is_empty() {
            self.drain_one();
        }
    }
}

/// The device loop for one packet, verbatim in shape.
fn deliver(feed: &mut InboundFeed<'_>, chan: &mut Bounded, packet: &[u8]) {
    let mut off = 0;
    loop {
        off += feed.feed(&packet[off..], |r| chan.try_send(r));
        match feed.take_pending() {
            Some(r) => chan.send_blocking(r),
            None => break,
        }
    }
    assert_eq!(off, packet.len(), "a packet is always taken whole");
}

fn frame(seq: u32, body: &[u8]) -> Vec<u8> {
    let mut buf = [0u8; MAX_FRAME];
    let len = encode(Level::Info, seq, 1000 + seq, body, &mut buf).len;
    buf[..len].to_vec()
}

fn body_for(seq: u32, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| b'A' + ((seq as usize + i) % 26) as u8)
        .collect()
}

fn run(stream: &[u8], cuts: &[usize], cap: usize) -> (Vec<(u32, Vec<u8>)>, InboundCounters, usize) {
    let stats = InboundStats::new();
    let mut feed = InboundFeed::new(&stats);
    let mut chan = Bounded::new(cap);
    let mut start = 0;
    for &cut in cuts.iter().chain(std::iter::once(&stream.len())) {
        let cut = cut.clamp(start, stream.len());
        deliver(&mut feed, &mut chan, &stream[start..cut]);
        start = cut;
    }
    chan.drain_all();
    (chan.out, stats.snapshot(), chan.refusals)
}

#[test]
fn every_two_way_split_yields_the_same_records() {
    let mut stream = frame(1, b"EXCSTART a");
    stream.extend(frame(2, &body_for(2, MAX_BODY)));
    stream.extend(frame(3, b"EXCEND"));
    let (whole, _, _) = run(&stream, &[], 8);
    assert_eq!(whole.iter().map(|r| r.0).collect::<Vec<_>>(), [1, 2, 3]);
    for split in 0..=stream.len() {
        let (got, counters, _) = run(&stream, &[split], 8);
        assert_eq!(got, whole, "split at {split}");
        assert_eq!(counters.records, 3);
        assert_eq!(counters.discarded_bytes, 0);
    }
}

#[test]
fn byte_at_a_time_matches_whole() {
    let mut stream = Vec::new();
    for seq in 0..5 {
        stream.extend(frame(seq, &body_for(seq, 40 + seq as usize * 30)));
    }
    let cuts: Vec<usize> = (1..stream.len()).collect();
    let (got, counters, _) = run(&stream, &cuts, 2);
    assert_eq!(got.len(), 5);
    assert_eq!(counters.records, 5);
}

#[test]
fn a_record_larger_than_one_packet_spans_64_byte_reads() {
    let body = body_for(9, MAX_BODY);
    let stream = frame(9, &body);
    assert!(stream.len() > 3 * 64, "a max frame spans four packets");
    let cuts: Vec<usize> = (64..stream.len()).step_by(64).collect();
    let (got, _, _) = run(&stream, &cuts, 4);
    assert_eq!(got, vec![(9, body)]);
}

#[test]
fn garbage_between_records_is_counted_not_delivered() {
    let mut stream = b"hello\r\n".to_vec();
    stream.extend(frame(1, b"one"));
    stream.extend(b"~not a frame\r\n");
    stream.extend(frame(2, b"two"));
    let (got, counters, _) = run(&stream, &[], 8);
    assert_eq!(
        got,
        vec![(1, b"one".to_vec()), (2, b"two".to_vec())],
        "both real records survive the junk"
    );
    assert_eq!(counters.records, 2);
    assert!(counters.bad_frames >= 1);
    assert_eq!(counters.discarded_bytes as usize, 7 + 14);
}

#[test]
fn a_full_sink_loses_nothing_and_keeps_order() {
    // Twenty short records in one packet against a channel of one: the sink refuses on
    // almost every record, so the held-record path carries the whole stream.
    let mut stream = Vec::new();
    for seq in 0..20 {
        stream.extend(frame(seq, b"x"));
    }
    let (got, counters, refusals) = run(&stream, &[], 1);
    assert_eq!(
        got.iter().map(|r| r.0).collect::<Vec<_>>(),
        (0..20).collect::<Vec<_>>()
    );
    assert_eq!(counters.records, 20);
    assert!(
        refusals >= 18,
        "backpressure was actually exercised ({refusals})"
    );
}

#[test]
fn a_refusal_after_the_last_byte_still_hands_the_record_back() {
    let stats = InboundStats::new();
    let mut feed = InboundFeed::new(&stats);
    let stream = frame(4, b"late");
    assert_eq!(feed.feed(&stream, |_| false), stream.len());
    let held = feed
        .take_pending()
        .expect("refused record is held, not dropped");
    assert_eq!((held.seq, held.body()), (4, &b"late"[..]));
    assert_eq!(
        stats.snapshot().records,
        1,
        "counted at validation; held, never dropped"
    );
}

#[test]
fn reconnect_drops_the_half_record_and_counts_it() {
    let stats = InboundStats::new();
    let mut feed = InboundFeed::new(&stats);
    let mut chan = Bounded::new(4);
    let a = frame(1, b"split across a disconnect");
    let half = a.len() / 2;
    deliver(&mut feed, &mut chan, &a[..half]);
    feed.reset();
    deliver(&mut feed, &mut chan, &a[half..]);
    deliver(&mut feed, &mut chan, &frame(2, b"after"));
    chan.drain_all();
    assert_eq!(chan.out, vec![(2, b"after".to_vec())]);
    let c = stats.snapshot();
    assert_eq!(c.records, 1);
    assert_eq!(
        c.discarded_bytes as usize,
        a.len(),
        "both halves are discarded"
    );
}

#[test]
fn endpoint_errors_are_their_own_counter() {
    let stats = InboundStats::new();
    stats.endpoint_error();
    stats.endpoint_error();
    assert_eq!(
        stats.snapshot(),
        InboundCounters {
            ep_errors: 2,
            ..InboundCounters::default()
        }
    );
}

proptest! {
    #[test]
    fn any_chunking_and_any_capacity_deliver_every_record(
        lens in prop::collection::vec(0usize..=MAX_BODY, 1..8),
        mut cuts in prop::collection::vec(0usize..2000, 0..20),
        cap in 1usize..5,
    ) {
        let mut stream = Vec::new();
        let mut want = Vec::new();
        for (i, &len) in lens.iter().enumerate() {
            let body = body_for(i as u32, len);
            stream.extend(frame(i as u32, &body));
            want.push((i as u32, body));
        }
        cuts.sort_unstable();
        let (got, counters, _) = run(&stream, &cuts, cap);
        prop_assert_eq!(got, want);
        prop_assert_eq!(counters.records as usize, lens.len());
        prop_assert_eq!(counters.discarded_bytes, 0);
    }
}
