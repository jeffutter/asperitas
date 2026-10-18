//! Helpers shared by every suite that drives the frame codec or the log ring.
//!
//! Included with `#[path]` rather than as a crate, so `asperitas-wire`'s own unit tests,
//! its integration tests and `asperitas-logging`'s integration tests all read one copy.

#![allow(dead_code)]

extern crate std;

use std::{vec, vec::Vec};

use asperitas_wire::frame::Decoder;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::pipe::Pipe;
use proptest::test_runner::{RngAlgorithm, TestRng};

/// A delivered record in a form tests can compare without borrowing the decoder.
pub type Seen = (u8, u32, u32, Vec<u8>);

/// A deterministic RNG for a fixed seed, so randomised rounds replay identically on every run.
pub fn seeded_rng(seed: u64) -> TestRng {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    bytes[8..].copy_from_slice(&(!seed).to_le_bytes());
    TestRng::from_seed(RngAlgorithm::XorShift, &bytes)
}

/// Advance both ring cursors `at` bytes into the backing array, leaving the ring empty by
/// occupancy - the state in which a wrap short-write happens.
pub fn park_cursors_at<const N: usize>(pipe: &Pipe<NoopRawMutex, N>, at: usize) {
    let filler = vec![b'f'; at];
    assert_eq!(pipe.try_write(&filler).ok(), Some(at));
    let mut drained = vec![0u8; at];
    assert_eq!(pipe.try_read(&mut drained).ok(), Some(at));
    assert_eq!(
        pipe.free_capacity(),
        N,
        "ring must report itself empty after parking the cursors"
    );
}

/// Feed `bytes` in pieces of `sizes` (cycling), returning every record the decoder yields.
///
/// Chunking is expressed as a count of chunk sizes rather than as bytes so a test can say "ten
/// pushes" without caring where the boundaries fall.
pub fn decode_in_chunks(bytes: &[u8], sizes: &[usize]) -> Vec<Seen> {
    let mut decoder = Decoder::new();
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let take = sizes[pos % sizes.len()].max(1);
        let end = (pos + take).min(bytes.len());
        let mut off = pos;
        while off < end {
            off += decoder.push(&bytes[off..end]);
            while let Some(r) = decoder.next_record() {
                out.push((r.level, r.seq, r.t_ms, r.body.to_vec()));
            }
        }
        pos = end;
    }
    decoder.finish();
    while let Some(r) = decoder.next_record() {
        out.push((r.level, r.seq, r.t_ms, r.body.to_vec()));
    }
    out
}

/// Decode a whole stream in one push.
pub fn decode_whole(bytes: &[u8]) -> Vec<Seen> {
    decode_in_chunks(bytes, &[usize::MAX / 2])
}
