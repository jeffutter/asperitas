---
id: TASK-038.04.02
title: 'Add a narrow inbound CDC OUT reader to the USB console feeding frame::Decoder'
status: In Progress
assignee:
  - '@ralph'
created_date: '2026-10-08 15:28'
updated_date: '2026-10-08 16:15'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-038.04
priority: high
ordinal: 132800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Scope: crates/asperitas-logging/src/usb.rs allocates the OUT endpoint (EP_OUT_BUFFER 256 B) but nothing reads it on device. Add an inbound task that reads packets promptly (so USB flow control, not loss, paces the host redirect), feeds frame::Decoder, and hands complete records to a bounded channel the binary consumes. Narrow and install-oriented, but shape it so TASK-032 can reuse it; record that decision. Must not starve or be starved by the drain loop, count endpoint errors and decoder drops in the existing console counters, and must not touch the audio executor. Acceptance: firmware builds for rig and the other log-usb binaries, counters visible in STATUS, decoder-feed logic that is separable from embassy has host tests. No hardware claim.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 New ungated module crates/asperitas-logging/src/inbound.rs: an InboundFeed that pushes raw bytes into frame::Decoder using the drain-then-retry loop and hands each validated record to a caller-supplied sink, never losing a record when the sink applies backpressure; host tests cover chunking at every split, a record larger than one packet, garbage between records, and backpressure
- [x] #2 InboundCounters (records, bad_frames, resyncs, discarded_bytes, ep_errors) are separate from ConsoleStats; STATUS body and its pinned test are unchanged
- [x] #3 usb::run joins a third inbound future that waits for connection, reads EP OUT packets promptly, feeds InboundFeed, and delivers records through a bounded embassy_sync Channel exposed as a documented public receiver; a full channel suspends the read so USB NAKs the host instead of dropping bytes
- [x] #4 Reconnect resets the decoder (finish() then new) so a half record from a dropped link cannot join the next session's bytes
- [x] #5 rig, main and other log-usb binaries still build for the thumb target; no new dependency; the audio executor is untouched; module docs record that this path is narrow and install-oriented and what TASK-032 would reuse
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 6a99698

Facts found: usb.rs allocates EP_OUT_BUFFER (256 B) but nothing reads it. cdc() returns a &'static mut CdcAcmClass used by run() (write_packet) and by emit_blocking on the panic path, so keep the whole class and call cdc().read_packet() from a third future rather than split(), which would break the panic path. run() is embassy_futures::join(usb_fut, drain_fut); add inbound_fut as a join3. frame::Decoder is ~2 KiB (8 slots x 200 B) and push() returns bytes taken, stopping when its delivery queue is full - the doc comment gives the lossless drain-then-retry loop to copy. Dump.rs documents '<>' as reserved host-to-device, so no framing change is needed: install records are ordinary '~' frames.

Steps:
1. inbound.rs (ungated, like frame/dump): InboundCounters with atomics and a snapshot, mirroring ConsoleStats style; InboundFeed { decoder } with async-free fn feed(&mut self, bytes, sink: impl FnMut(&Record)->Poll-like) ... Keep it simple: provide fn push_some(&mut self, bytes)->usize and fn next(&mut self)->Option<Record>, so the async caller does 'push; drain each record into channel.send().await; retry remainder'. A sync-only core keeps host tests trivial. Counters updated from decoder.stats() deltas after finish() and on each drain.
2. usb.rs: static INBOUND: Channel<CriticalSectionRawMutex, InboundRecord, 4> where InboundRecord { seq:u32, len:u8, body:[u8;frame::MAX_BODY] }; pub fn inbound() -> Receiver<'static,...>. inbound_fut: loop { cdc().wait_connection().await; fresh InboundFeed; loop { read_packet(&mut pkt)->Err => ep_errors++, break; feed; for each record send().await } } . wait_connection per iteration handles reconnects. Because the drain loop also calls wait_connection, check it is cancel-safe/shareable (it takes &mut self; two borrows via cdc() alias as the existing pattern already does for write_packet vs usb_dev) - note this in a comment.
3. Do not touch STATUS; install progress and these counters are reported by the rig ticket (.05) in its own records. Reason: STATUS field order is a documented contract TASK-031 parses.
4. Tests: host tests in inbound.rs and tests/inbound_feed.rs using frame::encode to build records: byte-at-a-time, random splits (proptest), 3 records back to back, garbage prefix, sink-full retry, reconnect reset.
5. Verify: cargo test -p asperitas-logging; cargo build (firmware workspace) for rig and main with log-usb.

Risks: two futures holding cdc() mutable aliases; deadlock if the inbound channel fills while no consumer exists (nothing consumes until .05) - drop-free backpressure then stalls host writes, which is acceptable but must be documented, and binaries that do not consume should simply not enable the reader (gate the join arm behind an explicit usb::enable_inbound() call or a flag so main/blinky behave exactly as today). Prefer the explicit enable.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Nothing of this existed at a16f5d2 (EP OUT allocated, never read).

Implementation:
- crates/asperitas-logging/src/inbound.rs (ungated): InboundRecord (owned 212 B copy of frame::Record), InboundStats/InboundCounters (records, bad_frames, resyncs, discarded_bytes, ep_errors; saturating, Relaxed) with static INBOUND, and InboundFeed. feed(bytes, sink: FnMut(&InboundRecord)->bool) runs drain-then-retry; a refused record is held, feed stops taking bytes, take_pending() hands it to the async caller to send().await. records is counted at validation (decoder stats delta), so a held record is counted once. reset() = finish() then a fresh Decoder (keeps the old one only if validated records are still queued, so nothing whole is destroyed).
- Sink returns bool rather than Result<(), InboundRecord>: the Result form trips clippy::result_large_err at every caller (212 B Err).
- usb.rs: deviated from the plan's 'keep the whole class and call cdc().read_packet()' - that would hold two live &mut to one CdcAcmClass across awaits. Instead the class is split() at init into cdc_acm::Sender (drain task + panic path; has write_packet and wait_connection, so emit_blocking is unchanged in behaviour) and cdc_acm::Receiver (inbound reader only). run() is now join3(usb_dev.run, drain, inbound_reader). inbound_reader returns immediately unless enable_inbound() was called before run() starts, so main/blinky/etc never read EP OUT and behave as before. enable_inbound() returns usb::InboundReceiver over a static Channel<CriticalSectionRawMutex, InboundRecord, INBOUND_DEPTH=4>. A full channel suspends before the next read_packet, so the endpoint NAKs.
- Found and fixed a latent frame::Decoder bug the feed tests exposed: when push() filled the 8-slot queue it stopped scanning but had already taken every byte, so whole records left in the window sat undelivered until more bytes arrived - the documented drain loop then lost the tail of a stream (last records of an install). next_record() now resumes scan() when the queue is empty. Regression test frame::tests::records_left_in_the_window_by_a_full_queue_drain_without_more_bytes fails without the fix. Added Decoder::queued().
- STATUS untouched (AC #2); description's 'counters visible in STATUS' is superseded by AC #2 - a consumer (TASK-038.04.05) reports INBOUND itself.
- Tests: tests/inbound_feed.rs (every 2-way split, byte-at-a-time, 200-byte body across 64-byte packets, garbage between records with exact discarded_bytes, channel cap 1 with 20 records, refusal after last byte, reconnect mid-record, ep_errors, proptest over chunking x capacity 1..4) plus unit tests in inbound.rs.
- Verified: cargo test -p asperitas-logging; workspace clippy --all-targets; clippy log-usb and log-defmt; firmware cargo build --release --features seed3 (all bins incl. rig, main) and clippy --bins; rig stim-ess build. No hardware claim: EP OUT NAK under backpressure is the expected embassy-usb-synopsys behaviour, unmeasured until TASK-038.09.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added the device's first host-to-device path. inbound.rs (ungated, host-tested) turns raw CDC OUT bytes into whole CRC-checked InboundRecords through frame::Decoder with lossless backpressure, and counts rejects in a new INBOUND counter set separate from STATUS. usb.rs splits the CDC class into Sender/Receiver halves and joins a third inbound_reader future that, only after usb::enable_inbound(), reads EP OUT, feeds InboundFeed, and delivers into a bounded channel (depth 4) whose full state suspends the read so the host is NAKed. Each connection resets the decoder so a half record cannot splice across sessions. Also fixed a latent Decoder bug where records stranded in the window by a full queue were not delivered until more bytes arrived. All firmware bins build; no hardware claim.
<!-- SECTION:FINAL_SUMMARY:END -->
