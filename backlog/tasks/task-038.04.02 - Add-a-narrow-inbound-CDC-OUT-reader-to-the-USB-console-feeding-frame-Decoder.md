---
id: TASK-038.04.02
title: 'Add a narrow inbound CDC OUT reader to the USB console feeding frame::Decoder'
status: Done
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
SHIPPED by b46c36f. This plan is superseded; the ticket's final summary describes what actually landed.
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
