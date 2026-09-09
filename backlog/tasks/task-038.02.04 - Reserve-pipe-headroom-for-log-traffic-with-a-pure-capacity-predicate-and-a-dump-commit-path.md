---
id: TASK-038.02.04
title: >-
  Reserve pipe headroom for log traffic with a pure capacity predicate and a
  dump commit path
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-09 15:51'
updated_date: '2026-09-09 15:57'
labels:
  - planned
dependencies:
  - TASK-038.02.03
modified_files:
  - crates/asperitas-logging/src/dump.rs
  - crates/asperitas-logging/src/lib.rs
  - crates/asperitas-logging/tests/console_dump.rs
parent_task_id: TASK-038.02
priority: high
type: task
ordinal: 65500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A dump that can fill the 2,048-byte log pipe would make the parent's "drop counter stayed at zero during a dump" criterion meaningless, because it would be measuring the traffic the dump crowded out. This ticket makes the headroom rule a pure function proven on host instead of a convention in a comment.

The rule: a dump commit never takes the last `MAX_FRAME` bytes of the pipe, so a maximum-size log or status record always fits. Note that `Pipe::ready_send` does not exist in embassy-sync 0.6.2 and the async writes strand partial buffers behind a single shared `write_waker` — TASK-038.02's Implementation Notes record the verified surface and the replacement design (poll `free_capacity()` under the existing `RECORD_BUFS` lock, commit with `frame::write_whole`, back off with a timer). The retry loop itself belongs to TASK-038.03; this ticket ships the predicate and the one commit path that may touch the pipe.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `dump` exposes one `pub const RESERVE: usize = MAX_FRAME` and one pure `no_std` predicate deciding whether a record of a given body length may be committed against a given `free_capacity`, such that committing leaves at least `RESERVE` bytes free. No capacity knob is introduced.
- [ ] #2 `lib.rs` gains exactly one `log-usb` entry point that commits a pre-built dump body: it reads `LOG_PIPE.free_capacity()` and commits through `frame::write_whole` inside the existing `RECORD_BUFS` lock, takes a sequence number only on the attempt that succeeds, bumps `records_sent` on commit and no loss counters on refusal, and leaves ordinary `emit()` behaviour byte-identical.
- [ ] #3 An exhaustive host test walks every starting occupancy `0..=LOG_PIPE_SIZE` using a local `Pipe<NoopRawMutex, LOG_PIPE_SIZE>` in the shape of `src/frame.rs:1257-1288`, and asserts the predicate agrees with actual acceptance and that every accepted commit leaves at least `MAX_FRAME` free.
- [ ] #4 A randomized interleaving test drives max-size log records and dump chunks against the same pipe with a draining consumer and asserts no log record is ever refused while at least `MAX_FRAME` bytes were free — i.e. the drop counter a bench session reads cannot be blamed on the dump.
- [ ] #5 The refusal path is shown not to consume sequence numbers by construction (the decision is a pure function called before `take_seq`, and the predicate is tested exhaustively over free capacity `0..=LOG_PIPE_SIZE` crossed with body lengths `0, 1, 199, 200`); the doc comment states plainly what CI can and cannot reach, since anything behind `log-usb` is compile-covered only by the firmware release build.
- [ ] #6 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cd firmware && cargo build --release --features seed3` all pass.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Last child of TASK-038.02. Read that ticket's Implementation Notes item 2 (`ready_send` does not exist) and item 3 (a dump writer must hold `RECORD_BUFS`) before touching `lib.rs`; both were verified against vendored source and against `emit()` itself.

## Pure policy (`dump.rs`, no_std, no new feature flags)
```rust
pub const RESERVE: usize = MAX_FRAME;                                  // 228: one maximum record, always survivable
pub const fn dump_fits(body_len: usize, free_capacity: usize) -> bool {
    PREFIX_LEN + body_len + TRAILER_LEN <= free_capacity.saturating_sub(RESERVE)
}
```
No capacity parameter anywhere: the reserve is a protocol property, not a tunable. Doc comment states the invariant as a post-condition — *after any accepted dump commit at least `MAX_FRAME` bytes remain free* — and why `saturating_sub` keeps the function total at `free_capacity < RESERVE` (it returns false, it never underflows).

## The one commit path (`lib.rs`, behind `log-usb`)
```rust
#[cfg(feature = "log-usb")]
pub fn try_emit_dump(body: &[u8]) -> bool
```
Shape, mirroring `emit()` (lib.rs:263-297) exactly where it matters:
1. Read `embassy_time::Instant::now().as_millis() as u32` **before** taking the lock, as `emit` does.
2. `RECORD_BUFS.lock(...)`: compute `blen = body.len().min(MAX_BODY)`, ask `dump::dump_fits(blen, LOG_PIPE.free_capacity())`, and on refusal `return false` **without calling `take_seq()` and without touching any counter**.
3. Only then `let seq = console::CONSOLE.take_seq();`, `frame::encode(Level::Info, seq, now_ms, body, &mut bufs.frame)`, `frame::write_whole(&bufs.frame[..encoded.len], LOG_PIPE.free_capacity(), |c| LOG_PIPE.try_write(c).ok())`, `console::CONSOLE.record_committed()`, `true`.

Why each detail is load-bearing, in the doc comment:
- The lock is not optional. Log records are emitted from arbitrary context including the audio callback, so an IRQ can preempt a producer that does not hold `RECORD_BUFS`; two interleaved `write_whole` calls splice two frames into the ring. Capacity can only grow while the lock is held because the consumer runs with interrupts enabled — that is what makes the pre-check sound, and it is why `write_whole`'s release-mode spin-on-stall cannot fire here.
- Sequence numbers are consumed only on commit, so a retry loop cannot manufacture `seq` gaps that would read as loss. Committed dump records *do* consume `seq` and *do* bump `records_sent`: they really occupy the wire, and `seq` continuity must keep meaning loss. Say that plainly; TASK-038.06 documents it.
- Refusals bump neither `dropped_full` nor `bytes_dropped`. Those counters mean "we had to throw a record away", which a retry is not. If the dump ever needs its own stall counter, that belongs to TASK-038.03's starvation counters, not here.
- Do **not** use `Pipe::write` or `Pipe::write_all`: either strands a partial frame across the ring whenever capacity is short of the frame, and there is a single shared `write_waker` woken only on the full→non-full transition. The retry loop with a timer backoff lives in TASK-038.03, which owns the executor; this function stays synchronous and non-blocking so it cannot deadlock a caller.
- Leave `emit()` alone apart from anything genuinely shared. Its behaviour and counters must stay byte-identical; `tests/console_frame.rs` and `console.rs`'s pinned wire literals are the guard.

## Host tests (`tests/console_dump.rs`)
`CriticalSectionRawMutex` fails at *link* time on host, so `try_emit_dump` itself is untestable here — state that limitation in the test module doc instead of faking around it. Test the predicate exhaustively and the pipe mechanics with the crate's existing stand-in, `pipe512()`/`commit()`/`park_cursors_at()` at `src/frame.rs:1257-1288` (a **local** `Pipe<NoopRawMutex, N>`, never a `static`, because `NoopRawMutex` is `!Sync`).
1. `predicate_agrees_with_the_ring_at_every_occupancy` — for every starting occupancy `0..=LOG_PIPE_SIZE`: park the cursors, evaluate `dump_fits(199, pipe.free_capacity())`, attempt the real `write_whole` commit, and assert acceptance matches the predicate exactly, and that after acceptance `pipe.free_capacity() >= MAX_FRAME`. Sweep body lengths `{0, 1, 199, 200}` to cover the boundary where a max-size body plus reserve exceeds the ring.
2. `a_dump_can_never_take_the_last_max_frame` — exhaustive over `free_capacity in 0..=LOG_PIPE_SIZE × body_len in 0..=MAX_BODY`: whenever `dump_fits` is true, `free_capacity - frame_len >= MAX_FRAME`; whenever false, refusing costs nothing. This is AC #5 stated as arithmetic.
3. `log_records_survive_a_saturated_dump` — randomized interleave (XorShift LCG copied from `frame.rs:1398`, ~20,000 rounds like `write_whole_rounds_are_byte_exact_under_randomized_interleaving`): alternate max-size log commits (which need only `frame_len <= free`) and dump commits (which need the reserve), draining up to `DRAIN_BUF_SIZE = 256` bytes per round, and assert **no log commit was ever refused while at least `MAX_FRAME` bytes were free**. That is exactly the claim TASK-038.03's bench criterion rests on, expressed as something CI can check.
4. `empty_ring_always_admits_a_dump` — `dump_fits(199, LOG_PIPE_SIZE)` is true, i.e. a dump always makes progress the moment the drainer catches up (otherwise the reserve rule would be a deadlock dressed as a safety property).

## Verification
`cargo fmt --all --check` · `cargo clippy --workspace --all-targets -- -D warnings` · `cargo test --workspace` · `cd firmware && cargo build --release --features seed3` (this is the only compile coverage for the `log-usb` path — root CI never enables the feature, so a type error here surfaces only in the firmware build). Confirm `git diff` touches only `dump.rs`, `lib.rs`, and the test file.

## Handoff note for TASK-038.03
It owns the retry loop: call `try_emit_dump`, and on `false` await `embassy_time::Timer::after_ms(...)` (1 ms bounds stall well below anything that matters for a post-run dump, since the drainer frees up to 256 B per wakeup) rather than spinning. Keep the dump task off the audio `InterruptExecutor`, and give stalls their own counter there, not in these counters.
<!-- SECTION:PLAN:END -->
