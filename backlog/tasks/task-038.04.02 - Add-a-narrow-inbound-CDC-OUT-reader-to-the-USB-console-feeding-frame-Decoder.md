---
id: TASK-038.04.02
title: 'Add a narrow inbound CDC OUT reader to the USB console feeding frame::Decoder'
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-10-08 15:28'
updated_date: '2026-10-08 15:54'
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
- [ ] #1 New ungated module crates/asperitas-logging/src/inbound.rs: an InboundFeed that pushes raw bytes into frame::Decoder using the drain-then-retry loop and hands each validated record to a caller-supplied sink, never losing a record when the sink applies backpressure; host tests cover chunking at every split, a record larger than one packet, garbage between records, and backpressure
- [ ] #2 InboundCounters (records, bad_frames, resyncs, discarded_bytes, ep_errors) are separate from ConsoleStats; STATUS body and its pinned test are unchanged
- [ ] #3 usb::run joins a third inbound future that waits for connection, reads EP OUT packets promptly, feeds InboundFeed, and delivers records through a bounded embassy_sync Channel exposed as a documented public receiver; a full channel suspends the read so USB NAKs the host instead of dropping bytes
- [ ] #4 Reconnect resets the decoder (finish() then new) so a half record from a dropped link cannot join the next session's bytes
- [ ] #5 rig, main and other log-usb binaries still build for the thumb target; no new dependency; the audio executor is untouched; module docs record that this path is narrow and install-oriented and what TASK-032 would reuse
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
