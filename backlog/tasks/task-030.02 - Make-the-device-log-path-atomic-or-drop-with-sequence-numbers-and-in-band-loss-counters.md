---
id: TASK-030.02
title: >-
  Make the device log path atomic-or-drop with sequence numbers and in-band loss
  counters
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 03:24'
updated_date: '2026-09-09 04:01'
labels:
  - planned
dependencies:
  - TASK-030.01
modified_files:
  - crates/asperitas-logging/src/lib.rs
  - crates/asperitas-logging/src/usb.rs
  - crates/asperitas-logging/src/panic_handler.rs
  - Cargo.lock
  - firmware/Cargo.lock
parent_task_id: TASK-030
priority: high
type: task
ordinal: 49500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: TASK-030. **Read TASK-030's implementation plan first** — §2 pins facts already verified (do not
re-research them), §3 pins console protocol v1, §4 is this ticket, §5 is the counter model. Depends on
TASK-030.01 for the codec; do not reimplement framing here.

Make the device side actually honour the format: every record reaches the wire whole or not at all, under
one lock, carrying a sequence number, with cumulative loss counters the host can read in band.

Three defects to remove, all in `crates/asperitas-logging`:

1. `lib.rs:46-55` treats `Pipe::try_write`'s short write as success — embassy-sync short-writes by design,
   which is precisely the truncated-line symptom. And it short-writes at every **ring wrap**, however empty
   the ring is: `RingBuffer::push_buf` hands back only the contiguous run to the end of the array while
   `free_capacity()` reports total free bytes, so a "check capacity, then write once" fix still truncates
   (parent §2 has the measured case: 200 requested, 112 accepted, 512 reported free). Use `frame::write_whole`
   - pre-check total free space, then loop until the record is fully in - and drop plus count the whole
   record when it cannot fit.
2. `lib.rs:149-151` uses `NoopRawMutex` and stores the pipe as `static mut Option<Pipe>` reached through
   `&raw mut` and a `pipe() -> &'static mut Pipe` accessor, with `#![allow(static_mut_refs)]` at `lib.rs:13`
   papering over it. Switch to `CriticalSectionRawMutex` and store it as a plain immutable
   `static LOG_PIPE: Pipe<CriticalSectionRawMutex, N> = Pipe::new()` — verified locally that this compiles
   and works through `&self` alone, so the aliasing hack and the lint allow can be deleted rather than
   re-invented. Fallback if some trait bound bites: `StaticCell` + `Pipe::split()`. No new dependency:
   `crates/asperitas-logging/Cargo.toml:12` already enables `critical-section-single-core`. Keep every use
   of `CriticalSectionRawMutex` behind `#[cfg(feature = "log-usb")]`; on the host the symbol is undefined
   at link time and the std fallback is not re-entrant.
3. `FORMAT_BUF` (`lib.rs:80`) is shared mutable state with no producer exclusion. Keep the buffer, but
   hold the record lock across format → frame → space check → commit, which is what makes a record
   indivisible. Bound the locked region explicitly (~200 B body cap, ≤229 B frame) and say so in a comment:
   this work happens with interrupts disabled while a 48 kHz audio block arrives every ~667 µs on the same
   single-threaded executor.

Then the reporting half: `BOOT` record at init, `STATUS` record debounced to at most one per second and
only when a counter moved, counters as independent `AtomicU32`s so they can always be incremented without
needing buffer space. Raise `LOG_PIPE_SIZE` 512 → 2048. Fix the drain loop to read more than 64 bytes per
wakeup while still writing at most one endpoint packet per `write_packet` call, and stop busy-`yield_now()`
when the ring is empty. Frame the panic-path message emitted through `usb::emit_blocking` so a `PANIC:`
line arrives as one valid record — without taking the record lock, allocating, or panicking.

AC #6 constrains the design: `firmware/src/bin/*.rs` call sites must not change. Verify with
`git diff --stat firmware/`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The log pipe uses CriticalSectionRawMutex instead of NoopRawMutex, is stored so that no &static mut aliasing trick or static_mut_refs allow is needed, and every use of the mutex stays behind cfg(feature = "log-usb") so host builds still link.
- [ ] #2 A framed record is committed to the pipe whole or not at all: when free capacity is short the record is dropped and counted, no partial record can reach the wire, and bytes already buffered for other records still decode cleanly.
- [ ] #3 Formatting, framing, the space check and the commit happen inside one critical section, so two producers cannot interleave inside a single record; the maximum work done with interrupts disabled is bounded by the 200-byte body cap and stated in a comment.
- [ ] #4 Every record carries a monotonic per-boot sequence number and a milliseconds-since-boot timestamp assigned by the device.
- [ ] #5 A BOOT record is emitted once at init and a STATUS record reports cumulative counters (records sent, records dropped for lack of space, bytes dropped, bodies shortened by the 200-byte cap, endpoint errors) at most once per second and only when a counter changed; counters are incremented without needing buffer space.
- [ ] #6 The panic path emits its message as one valid framed record through emit_blocking, taking no locks and allocating nothing, and a PANIC line remains readable in a plain terminal.
- [ ] #7 The drain loop reads more than one endpoint packet per wakeup while never handing write_packet more than 64 bytes, and does not busy-spin the executor when the ring is empty.
- [ ] #8 No call site in firmware/src/bin/*.rs changes, confirmed by git diff --stat firmware/, and the firmware still cross-compiles with cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --features seed3 --release.
- [ ] #9 LOG_PIPE_SIZE is raised to 2048 and the fmt/clippy/test gates pass.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# Plan — device log path: atomic-or-drop, sequence numbers, in-band loss counters

Read parent TASK-030 §2 (facts already verified — do not re-research), §3 (normative wire spec), §4 (this
ticket), §5 (counter model). Depends on TASK-030.01 for `frame.rs`; **do not reimplement framing here**,
call `frame::encode` / `frame::write_whole`.

Sequence: (1) kill the aliasing hack and swap the mutex, (2) rewrite the commit path, (3) counters + BOOT +
STATUS, (4) panic framing, (5) drain loop. Each step keeps the gates green before the next starts. Steps 1
and 2 are unsafe-code surgery and should be one reviewable commit.

## 1. Storage and mutex (`lib.rs`)

- Replace `static mut LOG_PIPE: Option<Pipe<NoopRawMutex, N>>` (`lib.rs:149-152`) with
  `pub static LOG_PIPE: Pipe<CriticalSectionRawMutex, LOG_PIPE_SIZE> = Pipe::new();` under
  `#[cfg(feature = "log-usb")]`. Verified compilable for `thumbv7em-none-eabihf` through `&self` alone
  (parent §2, `/tmp/chprobe`).
- Delete `pub fn pipe()` (`lib.rs:156-164`), the `&raw mut` pokes at `lib.rs:50` and `usb.rs:175`, and the
  crate-level `#![allow(static_mut_refs)]` (`lib.rs:15`) once nothing needs it — leaving the allow behind
  would hide the *next* real `static mut` misuse. `pipe()` has exactly one caller (`usb.rs:231`); no
  firmware binary uses it, so its removal does not touch AC #8's call-site rule.
- **Every** use of `CriticalSectionRawMutex` stays inside `#[cfg(feature = "log-usb")]`. On the host there
  is no registered `critical-section` impl and the failure is an *undefined symbol at link time*, not a
  compile error (parent §2) — a stray import turns `cargo test --workspace` into a link failure that looks
  unrelated to this change. No new dependency: `cortex-m`'s `critical-section-single-core` feature is
  already enabled (`Cargo.toml:12`).
- Rename `FORMAT_BUF: [u8; 256]` → `FRAME_BUF` and keep exactly one buffer. Build the record where it will
  be copied from: body at `FRAME_BUF[21..]`, sanitised in place, then the fixed-width prefix into
  `[0..21]` and the trailer after it — which is precisely what `frame::encode` does given
  `&mut [u8; MAX_FRAME]`. `21 + 200 + 7 = 228 ≤ 256`; put that arithmetic in the doc comment. A second
  staging buffer plus a copy would double the work inside the critical section for nothing.

## 2. The commit path (`usb_pipe_write`, `lib.rs:46-55`)

One lock per record, in this order:

```
let now_ms = Instant::now().as_millis() as u32;                 // BEFORE the lock
blocking_mutex::lock(&LOCK, || {                                // or CriticalSectionRawMutex::lock
    let seq = CONSOLE_SEQ.fetch_add(1, Relaxed);                // inside: wire order == seq order
    let enc = frame::encode(level, seq, now_ms, body, &mut FRAME_BUF);
    if enc.truncated { CONSOLE_TRUNCATED.fetch_add(1, Relaxed); }
    let f = &FRAME_BUF[..enc.len];
    if !frame::write_whole(f, LOG_PIPE.free_capacity(), |chunk| LOG_PIPE.try_write(chunk).ok()) {
        CONSOLE_DROPPED_FULL += 1; CONSOLE_BYTES_DROPPED += f.len(); return;
    }
    CONSOLE_RECORDS_SENT += 1;
})
```

- Read the clock **outside** the lock: `Instant::now()` inside would nest the time driver's own locking in
  our IRQ-off window for no benefit, and a millisecond of skew is invisible next to a 1 ms field.
- `frame::write_whole` exists because a single `try_write` cannot be trusted: embassy short-writes at every
  ring wrap even when the ring is empty (parent §2, measured: 200 requested, 112 accepted, `free_capacity`
  reported 512). The pre-check plus the bounded loop is the fix; the loop is shared with the host tests so
  the argument is machine-checked rather than asserted here. Do not "simplify" it back to one call.
- The `body` slice handed to `encode` is what `format_log_record` produces today minus the `[LEVEL] `
  prefix and the trailing `\r\n`: level lives in the header and CRLF is the delimiter. Keep the existing
  `Writer` (it already truncates rather than overflowing); note in a comment that the old
  `Ok(_) => true` arm was the bug — `Ok(n < len)` meant "tail discarded".
- Bound and document the locked region in code: ~200 B formatted + ~228 B CRC + ≤2 memcpys, executed with
  interrupts disabled while a 48 kHz audio block arrives every ~667 µs on the same executor. If it ever
  measures too long, the answer is a reserve/commit ring replacing the `Pipe` (parent §4) — record a
  measurement in the notes, never redesign quietly.
- Waking the drain task happens inside the lock via `try_write`'s waker and is fine (PRIMASK-nested PendSV
  fires on exit; the thread-mode executor cannot preempt itself). What is *not* fine: awaiting anything, or
  logging anything, inside the lock.
- Records logged before `usb::init` still go to the `NoOp` backend and vanish uncounted. Leave that; say so
  in a comment so nobody later mistakes it for a counter bug.

## 3. Counters, BOOT, STATUS (`lib.rs` + drain task)

Five `static AtomicU32` in `lib.rs`, incremented without ever needing buffer space (parent §5):
`CONSOLE_SEQ`, `CONSOLE_RECORDS_SENT`, `CONSOLE_DROPPED_FULL`, `CONSOLE_BYTES_DROPPED`,
`CONSOLE_TRUNCATED`, plus `CONSOLE_ENDPOINT_ERRORS` bumped by the drain and panic paths. `Relaxed` is
enough (single core; they exist to be snapshotted without taking the record lock). Expose one read-only
snapshot function so the emitter reads all six coherently-ish from one place instead of six scattered
loads.

- **BOOT**: emitted once, right after `install_logger()` and the `Backend::Usb` switch in `usb::init`
  (`usb.rs:177-181`), so it takes the normal path and gets `seq = 0`:
  `BOOT proto=1 fw=<CARGO_PKG_VERSION> pipe=<LOG_PIPE_SIZE> maxbody=200`. It is how a reader tells a
  reboot from loss; bytes written before the endpoint is up simply wait in the ring.
- **STATUS** from the **drain task**, never from inside the record lock:
  `STATUS proto=1 sent=… dropped_full=… bytes_dropped=… trunc=… ep_err=… seq_next=… pipe_free=…`, at most
  once per second and only when some counter moved since the last emission. Debounce matters: during a
  full-ring condition the status record competes for the very space that is missing, and a drop storm must
  not starve real logs.
- Getting a periodic tick without spinning: the drain loop (§5) sleeps in `pipe.read().await`, so wrap it —
  `embassy_futures::select::select(read_fut, Timer::after_millis(250))` and check the debounce after every
  wakeup. Dropping a pending `read` future when the timer branch wins is safe: `Pipe::read` consumes bytes
  only in the poll that returns `Ready`, so a dropped-but-pending read loses nothing. If that select fights
  you, the acceptable fallback is to emit STATUS only after a successful flush, gated by ≥1 s and a changed
  counter — but say in the notes that a stalled host then sees no STATUS, only seq gaps.

## 4. Panic path (`panic_handler.rs` + `usb::emit_blocking`)

`handle_panic` formats into its own 128 B buffer and calls `usb::emit_blocking(msg)` (`panic_handler.rs:41`,
`usb.rs:281+`). Frame it with the same v1 encoder — same leading `~`, same CRC range — bumping
`CONSOLE_SEQ` via `fetch_add` (uncontended once the executor is halted), and keep `emit_blocking`'s existing
chunking to `MAX_PACKET_SIZE` (64). Constraints, restated because they are absolute: no locks, no allocation, no panic,
no reliance on the ring (the executor is dead — a pipe write here is guaranteed-discarded, which is why this
path bypasses it). Result: `README.md:194-199`'s "steady red plus a `PANIC:` line" procedure still reads as
text and now carries a valid CRC. Verify against `firmware/src/bin/panictest.rs` behaviour by reading the
code path end to end and saying in the notes which lines you traced — the audible/board proof is
TASK-030.04 and TASK-033, not this ticket.

## 5. Drain loop (`usb.rs:213-247`)

- Grow the read buffer to 256 B and keep writing at most one endpoint packet per `write_packet`: chunk the
  read bytes to `MAX_PACKET_SIZE` (64) and skip empty chunks (never send a zero-length packet). The
  `BufferOverflow`-masquerading-as-disconnect trap at `usb.rs:27-33` stays fixed exactly as documented.
- Replace the empty-ring `yield_now()` spin with `pipe.read(&mut buf).await`, which parks until a byte
  exists. That stops the drain task burning executor slots from the audio loop, and is plausibly the poll
  rate TASK-027.02 is measuring — mention it in your notes so that ticket's author knows the baseline moved.
- Disconnect detection stays "a write failed", so a stall in `read()` can outlive a replug until the next
  record arrives; that is acceptable and worth one honest comment. Bytes pulled just before a failed write
  are lost with it (≤ one 256 B read per reconnect) and show up in `ep_err` / seq gaps rather than silently.
- Keep `log::info!("USB connected"/"disconnected")` outside any lock — once the mutex is real, calling the
  logger from inside the critical section would be a nesting accident waiting to bite (cortex-m's
  critical-section is re-entrant, the `std` fallback used by host builds is not).

## 6. Verification

Gates: `cargo fmt --all --check` · `cargo clippy --workspace --all-targets -- -D warnings` ·
same with `--features asperitas-pod/pod-hw` · `cargo test --workspace` ·
`cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --features seed3 --release`.

Then, specifically for this ticket:

- `git diff --stat firmware/` must be **empty** (AC #8). If it isn't, the design drifted toward changing
  call sites; fix the design.
- `grep -n "static mut\|&raw mut\|unsafe" crates/asperitas-logging/src/lib.rs` — report what survives and
  why each remaining `unsafe` is needed. Deleting `#![allow(static_mut_refs)]` should leave `GLOBAL_BACKEND`
  and the CDC raw refs as the only ones, both pre-existing.
- FLASH is capped at 128 K (`firmware/memory.x:7`); if `llvm-size`/`cargo size` is available in the dev
  shell, record the `.text`/`.bss` delta for `podtest` in the notes. RAM is 512 K and irrelevant here.
- Notes must state: the maximum bytes written with interrupts disabled, any place where you had to deviate
  from parent §4, and explicitly that there is **no board proof in this ticket** — legibility, zero bad
  frames and audio safety are TASK-030.04's and TASK-033's to establish. Deviations get written back into
  the parent plan, not buried in a commit message.
<!-- SECTION:PLAN:END -->
