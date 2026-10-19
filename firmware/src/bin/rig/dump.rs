//! Dump writer: ships the captured blocks over the framed console.

#[cfg(feature = "log-usb")]
use crate::emit_console;
#[cfg(feature = "log-usb")]
use crate::geometry::DUMP_RETRY_MS;
use crate::ring::Ring;
#[cfg(feature = "log-usb")]
use crate::ring::{advance, DUMPED};
#[cfg(feature = "log-usb")]
use asperitas_logging::error;
use asperitas_logging::info;
#[cfg(feature = "log-usb")]
use asperitas_wire::capture::{self, BlockState};
#[cfg(feature = "log-usb")]
use asperitas_wire::console;
#[cfg(feature = "log-usb")]
use core::sync::atomic::Ordering;

/// Running totals for one dump, which become its `DUMPEND`.
#[cfg(feature = "log-usb")]
#[derive(Default)]
struct DumpTally {
    chunks: u32,
    bytes: u32,
    refused: u32,
    stall_ms: u32,
}

/// Commit one dump body, waiting out refusals.
///
/// `try_emit_dump` is the only door, and behind it `dump::dump_fits` keeps one maximum frame of
/// the pipe free for log and `STATUS` records, so a saturated dump cannot crowd them out (the
/// behaviour `tests/console_dump.rs::log_records_survive_a_saturated_dump` protects). A refusal
/// means the pipe is full of earlier chunks; the await between retries is load-bearing, because
/// the USB drain task that frees that space runs on this same thread executor and gets nowhere
/// while this loop holds it.
#[cfg(feature = "log-usb")]
async fn send_dump_body(body: &[u8], tally: &mut DumpTally) {
    let mut stalled_since: Option<embassy_time::Instant> = None;
    while !asperitas_logging::try_emit_dump(body) {
        tally.refused += 1;
        stalled_since.get_or_insert_with(embassy_time::Instant::now);
        embassy_time::Timer::after_millis(DUMP_RETRY_MS).await;
    }
    if let Some(since) = stalled_since {
        tally.stall_ms = tally.stall_ms.max(since.elapsed().as_millis() as u32);
    }
}

/// Ship blocks `first..end` (sequence numbers since boot) in ring order, then one `DUMPEND`.
///
/// Each block is claimed `Full -> Dumping`, sliced into `CHUNK_RAW` chunks with its CRC folded in
/// as each chunk is sliced - one pass over the bytes, not a second one that someone later
/// "optimises" by skipping - closed with `AUDEND`, and handed back `Free`. The block number on the
/// wire is the sequence number, so a host can tell this run's blocks apart from a previous one's.
#[cfg(feature = "log-usb")]
pub(crate) async fn dump_ring(ring: Ring, first: u32, end: u32) {
    use asperitas_wire::{dump, frame};

    let chunks = capture::chunks_per_block() as u16;
    let mut tally = DumpTally::default();
    let mut body = [0u8; frame::MAX_BODY];
    let mut blocks = 0u32;
    let started = embassy_time::Instant::now();

    info!("rig: dumping {} blocks", end - first);
    for seq in first..end {
        let index = seq as usize % capture::RING_BLOCKS;
        if !advance(index, BlockState::Full, BlockState::Dumping) {
            error!("rig: block {seq} was not Full at dump time; skipping it");
            continue;
        }
        // Safety: this block is `Dumping`, which only the writer can be.
        let block = unsafe { ring.block(index) };
        let mut crc = frame::CRC16_INITIAL;
        for (c, raw) in block.chunks(dump::CHUNK_RAW).enumerate() {
            crc = frame::crc16_ccitt_update(crc, raw);
            let n = match dump::audio_body(seq, chunks, c as u16, raw, &mut body) {
                Ok(n) => n,
                Err(e) => {
                    // Unreachable while `capture`'s geometry gates hold; refusing beats shipping a
                    // block the host would reassemble wrong.
                    error!(
                        "rig: AUDIO body for block {seq} chunk {c} refused: {e:?}; dump abandoned"
                    );
                    return;
                }
            };
            send_dump_body(&body[..n], &mut tally).await;
            tally.chunks += 1;
            tally.bytes += raw.len() as u32;
        }
        let n = match dump::audend_body(
            seq,
            chunks,
            capture::RING_BLOCK_BYTES as u32,
            crc,
            &mut body,
        ) {
            Ok(n) => n,
            Err(e) => {
                error!("rig: AUDEND body for block {seq} refused: {e:?}; dump abandoned");
                return;
            }
        };
        send_dump_body(&body[..n], &mut tally).await;
        advance(index, BlockState::Dumping, BlockState::Free);
        DUMPED.fetch_add(1, Ordering::Relaxed);
        blocks += 1;
    }

    let console_now = console::CONSOLE.snapshot();
    let summary = console::DumpSummary {
        blocks,
        chunks: tally.chunks,
        bytes: tally.bytes,
        elapsed_ms: started.elapsed().as_millis() as u32,
        refused: tally.refused,
        stall_ms: tally.stall_ms,
        sent: console_now.records_sent,
        dropped_full: console_now.dropped_full,
        bytes_dropped: console_now.bytes_dropped,
    };
    let mut record = [0u8; console::BODY_WINDOW];
    let n = console::dumpend_body(&summary, &mut record);
    emit_console(&record[..n]);
}

/// The RTT-only image has no framed console to dump over, so the capture stays in SDRAM.
///
/// Said once rather than pretended: a writer that "sent" every chunk into the discard shim would
/// end in a `DUMPEND` describing a transfer that never happened.
#[cfg(not(feature = "log-usb"))]
pub(crate) async fn dump_ring(_ring: Ring, first: u32, end: u32) {
    info!(
        "rig: {} blocks captured; this image has no framed console, so nothing is dumped",
        end - first
    );
}
