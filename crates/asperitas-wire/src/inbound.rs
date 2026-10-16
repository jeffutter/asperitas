//! The owned form of a validated host-to-device record.
//!
//! [`crate::frame::Record`] borrows the decoder's queue and dies at the next push, so anything
//! that moves a record across a channel copies it out first. The feed loop that does the
//! copying, and the counters it keeps, live in `asperitas-wire`; the type is here because
//! the control grammar ([`crate::control::dispatch`]) reads it.

use crate::frame::{Record, MAX_BODY};

/// One validated host-to-device record, owned so it can cross a channel.
///
/// 212 bytes; `Copy` because a channel moves it by value anyway.
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
