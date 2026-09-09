---
id: TASK-030.02
title: >-
  Make the device log path atomic-or-drop with sequence numbers and in-band loss
  counters
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-09 03:24'
updated_date: '2026-09-09 10:01'
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
TASK-030.01 for the codec; do not reimplement framing here, and do not modify `frame.rs`.

Make the device side actually honour the format: every record reaches the wire whole or not at all, under
one lock, carrying a sequence number, with cumulative loss counters the host can read in band. Two
transport fixes belong with it because they surface only once you look at the USB layer: a bulk transaction
whose final packet is exactly 64 bytes is withheld by the host until something shorter follows (so a capture
loses its tail), and the drain task currently busy-spins whenever the ring is empty.

Four defects to remove, all in `crates/asperitas-logging`:

1. `usb_pipe_write` (`lib.rs:47-55`) treats `Pipe::try_write`'s short write as success — embassy
   short-writes by design, which is precisely the truncated-line symptom. And it short-writes at every
   **ring wrap**, however empty the ring is: `RingBuffer::push_buf` hands back only the contiguous run to
   the end of the array while `free_capacity()` reports total free bytes, so a "check capacity, then write
   once" fix still truncates (parent §2 has the measured case: 200 requested, 112 accepted, 512 reported
   free; the same code ships in 0.6.2 and 0.8.0, so no upgrade removes this). Use `frame::write_whole` —
   pre-check total free space inside the lock, then loop until the record is fully in — and drop plus count
   the whole record when it cannot fit.
2. `lib.rs:158-162` stores the pipe as `static mut Option<Pipe<NoopRawMutex, N>>` reached through `&raw mut`
   via a `pipe() -> &'static mut Pipe` accessor (`lib.rs:164-175`), with `#![allow(static_mut_refs)]`
   (`lib.rs:15`) papering over it. Switch to `CriticalSectionRawMutex` and a plain immutable
   `static LOG_PIPE: Pipe<CriticalSectionRawMutex, N> = Pipe::new()` — verified to compile and work through
   `&self` alone (parent §2), so the aliasing hack dies rather than being worked around. Keep every use of
   the mutex behind `#[cfg(feature = "log-usb")]`: on the host there is no registered impl and the failure
   is an undefined symbol at link time, and the `std` fallback is not re-entrant. Nesting our lock around
   `Pipe::try_write` is documented-safe on target. No new dependency.
3. The format buffer (`FORMAT_BUF`, `lib.rs:80`) is shared mutable state with no producer exclusion. Keep
   one format buffer and one frame buffer (amendment A1: `frame::encode` takes `body` and `out` as disjoint
   borrows, so building the record in place is not expressible against the shipped codec), held in a single
   static reachable only under the record lock, which spans format → encode → space check → commit and is
   what makes a record indivisible. Bound the locked region explicitly (≤256 B body, ≤228 B frame) and say
   so in a comment: this work happens with interrupts disabled while a 48 kHz audio block arrives every
   ~667 µs on the same single-threaded executor.
4. The drain task (`usb.rs:218-246`) reads only 64 bytes per wakeup, busy-`yield_now()`s when the ring is
   empty, and never terminates a bulk transaction with a short packet.

Then the reporting half: `BOOT` record at init (after the backend switch, or `NoOp` swallows it), `STATUS`
debounced to at most one per second and only when a counter moved, counters as independent `AtomicU32`s that
saturate so they can always be incremented and always be differenced by the host. Raise `LOG_PIPE_SIZE`
512 → 2048. Frame the panic-path message emitted through `usb::emit_blocking` so a `PANIC:` line arrives as
one valid record — without taking the lock, allocating, or adding a 228-byte stack local.

AC #8 constrains the design: `firmware/src/bin/*.rs` call sites must not change. Verify with
`git diff --stat firmware/`. That rule is also why STATUS rides the existing drain task rather than a newly
spawned task.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The log pipe uses CriticalSectionRawMutex instead of NoopRawMutex, is stored so that no &static mut aliasing trick or static_mut_refs allow is needed, and every use of the mutex stays behind cfg(feature = "log-usb") so host builds still link.
- [x] #2 A framed record is committed to the pipe whole or not at all: when free capacity is short the record is dropped and counted, no partial record can reach the wire, and bytes already buffered for other records still decode cleanly.
- [x] #3 Formatting, framing, the space check and the commit happen inside one critical section, so two producers cannot interleave inside a single record; the maximum work done with interrupts disabled is bounded by the 200-byte body cap and stated in a comment.
- [x] #4 Every record carries a monotonic per-boot sequence number and a milliseconds-since-boot timestamp assigned by the device.
- [x] #5 A BOOT record is emitted once at init and a STATUS record reports cumulative counters (records sent, records dropped for lack of space, bytes dropped, bodies shortened by the 200-byte cap, endpoint errors) at most once per second and only when a counter changed; counters are incremented without needing buffer space.
- [x] #6 The panic path emits its message as one valid framed record through emit_blocking, taking no locks and allocating nothing, and a PANIC line remains readable in a plain terminal.
- [x] #7 The drain loop reads more than one endpoint packet per wakeup while never handing write_packet more than 64 bytes, and does not busy-spin the executor when the ring is empty.
- [x] #8 No call site in firmware/src/bin/*.rs changes, confirmed by git diff --stat firmware/, and the firmware still cross-compiles with cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --features seed3 --release.
- [x] #9 LOG_PIPE_SIZE is raised to 2048 and the fmt/clippy/test gates pass.
- [x] #10 Every bulk transaction handed to the endpoint ends short: when the last packet written was exactly 64 bytes and no further bytes remain to send, the drain task sends a zero-length packet before parking, and emit_blocking does the same when its framed message length is an exact multiple of 64.
- [x] #11 Loss counters saturate at u32::MAX rather than wrapping, so successive STATUS records can be differenced on the host without wrap arithmetic, while CONSOLE_SEQ keeps wrapping mod 2^32 by design to match the 8-hex-digit seq field.
- [x] #12 The STATUS body layout and the STATUS debounce decision are pure functions compiled for the host and covered by unit tests that fail if the field set or the >=1s-and-changed rule changes.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# Plan v2 — device log path: atomic-or-drop, sequence numbers, in-band loss counters

This supersedes the pass-2 plan. Three corrections to parent §4 came out of verifying it against the
codec that TASK-030.01 actually shipped and against vendored embassy sources; they are listed as
**A1/A2/A3** at the bottom and were posted as an amendment comment on TASK-030.

Read first: parent **TASK-030 §2** (facts already verified — do not re-research), **§3** (normative wire
spec), **§4** (this ticket), **§5** (counter model). The codec is `crates/asperitas-logging/src/frame.rs`
(TASK-030.01, Done): call `frame::encode` / `frame::write_whole`, **do not modify frame.rs**.

Versions that matter (both lockfiles agree): embassy-sync **0.6.2** for this crate (`Cargo.toml:20` pins
`"0.6"`; the lock also carries 0.8.0 for embassy-usb/-stm32, and 0.8's `Pipe` gains nothing relevant —
`push_buf` is byte-identical and there is still no synchronous all-or-nothing API, so nobody should propose
an upgrade as a fix), embassy-usb **0.6.0**, embassy-futures 0.1.2, embassy-time 0.5.1,
critical-section 1.2.0 via `cortex-m`'s `critical-section-single-core` (`Cargo.toml:12`). No new dependency.

Line numbers below were re-derived against the current tree (pass 2's had drifted by up to 11 lines).

Sequence — each step leaves the gates green before the next starts. Steps 1+2 are the unsafe-code surgery
and should be one reviewable commit.

## 1. Storage, mutex, buffers (`lib.rs`)

Replace `pub static mut LOG_PIPE: Option<Pipe<NoopRawMutex, _>>` (`lib.rs:158-162`), the `pub fn pipe()`
accessor (`lib.rs:164-175`) and both `&raw mut` pokes (`lib.rs:50`, `usb.rs:174-176`) with one immutable
static:

```rust
#[cfg(feature = "log-usb")]
pub static LOG_PIPE: Pipe<CriticalSectionRawMutex, LOG_PIPE_SIZE> = Pipe::new();
```

Delete the accessor outright — its only caller is the drain task (`usb.rs:231`) and no firmware binary
names it, so AC #8's call-site rule is unaffected. **Every** mention of `CriticalSectionRawMutex` stays
under `#[cfg(feature = "log-usb")]`: on the host no `critical-section` impl is registered and the failure
is an *undefined symbol at link time*, which looks nothing like this change in a `cargo test --workspace`
log. Host tests keep their own local `NoopRawMutex` pipes (`frame.rs:1251-1256` records why) and must not
reference `LOG_PIPE`.

Nesting is legal and load-bearing here: `critical-section 1.2.0`'s `acquire` docs state "Nesting critical
sections is allowed. The inner critical sections are mostly no-ops since they're already protected by the
outer one", and cortex-m's impl saves/restores PRIMASK. So our outer lock → `Pipe::try_write` (which takes
its own) is fine **on target**; the `std` fallback is not re-entrant, which is the second reason host builds
must never reach this code.

`#![allow(static_mut_refs)]` (`lib.rs:15`): attempt to delete it. After this ticket the pipe no longer
needs it, but four pre-existing statics do — `GLOBAL_BACKEND` (`lib.rs:58`), `CDC_REF`/`USB_DEV_REF`
(`usb.rs:76`/`:80`), `BOOT_LED_REF` (`led.rs:95`). Try `core::ptr::addr_of_mut!` at those sites (the
sanctioned non-lint form). If the lint goes quiet, delete the crate-level allow. If not, demote the allow
onto just those items with a comment naming them. What must *not* happen is leaving a crate-wide blanket
that hides the next real `static mut` misuse — that half of AC #1 is the point, not the cosmetics.

**Buffers: two, not one (amendment A1).** `frame::encode(body: &[u8], out: &mut [u8; MAX_FRAME])` takes two
disjoint borrows, so parent §4's "format the body at `FRAME_BUF[21..]`, sanitise in place, then write the
prefix backwards into `[0..21]`" is not expressible against the codec as built. Neither is rewriting
`encode` for in-place use worth it: `encode` *always* copies + sanitises from a separate source
(`frame.rs:193-196`), so the honest cost of a separate body buffer is one extra ≤256-byte copy — well under
a microsecond at 480 MHz, against a ~667 µs audio block period. Keep both buffers in one guarded static:

```rust
#[cfg(feature = "log-usb")]
struct Bufs { body: [u8; 256], frame: [u8; MAX_FRAME] }

#[cfg(feature = "log-usb")]
static LOG_BUFS: embassy_sync::blocking_mutex::Mutex<
    CriticalSectionRawMutex,
    core::cell::UnsafeCell<Bufs>,
> = embassy_sync::blocking_mutex::Mutex::new(UnsafeCell::new(Bufs { body: [0; 256], frame: [0; MAX_FRAME] }));
```

`Mutex::new` is `const` and `Mutex<R, T>: Sync` when `R: Sync, T: Send`, so this is a plain `static` — no
`static mut`, no hand-rolled `Sync` newtype. Note `blocking_mutex::Mutex::lock` hands out `&T`, not
`&mut T` (`embassy-sync-0.6.2/src/blocking_mutex/mod.rs:44-50`), which is why the payload is an
`UnsafeCell`: inside the lock, `LOG_BUFS.lock(|c| { let b = unsafe { &mut *c.get() }; … })`. One `unsafe`
block, one sentence of justification: the sole route to `Bufs` is this mutex, the core is single-core, and
the reference never escapes the closure.

Why `body` is 256 bytes and not `MAX_BODY`: `Encoded::truncated` is computed from the **input** length
(`body.len() > MAX_BODY`, `frame.rs:216`). A formatter capped at exactly 200 would produce `len == 200` and
`truncated == false` forever, so the `trunc` counter could never fire. Keep today's 256-byte window
(`FORMAT_BUF`, `lib.rs:80`) and let `encode` do the 200-byte cap and set the flag. Rename the pair to say
what they are; delete `FORMAT_BUF`'s `static mut`.

Keep `Backend::{NoOp, Usb}` / `GLOBAL_BACKEND` dispatch (`lib.rs:29-44`, `:58`) exactly as it is. It is how
pre-`usb::init` records get discarded, and AC #3's claim is about the region inside the lock, not the
`match` above it.

## 2. The commit path (replace `usb_pipe_write`, `lib.rs:46-55`)

```
let now_ms = Instant::now().as_millis() as u32;              // BEFORE the lock
LOG_BUFS.lock(|cell| {                                       // one IRQ-off region per record
    let b = unsafe { &mut *cell.get() };
    let seq  = CONSOLE_SEQ.fetch_add(1, Relaxed);            // inside: wire order == seq order
    let n    = format_body(record, &mut b.body);             // message only, no [LEVEL], no CRLF
    let enc  = frame::encode(record.level(), seq, now_ms, &b.body[..n], &mut b.frame);
    if enc.truncated { bump(CONSOLE_TRUNCATED); }
    let f = &b.frame[..enc.len];
    if !frame::write_whole(f, LOG_PIPE.free_capacity(), |c| LOG_PIPE.try_write(c).ok()) {
        bump(CONSOLE_DROPPED_FULL); bump_by(CONSOLE_BYTES_DROPPED, f.len()); return;
    }
    bump(CONSOLE_RECORDS_SENT);
})
```

- `format_body` is today's `format_log_record` (`lib.rs:83-113`) minus the `[{}] {}` level prefix and the
  trailing `\r\n`: level lives in the frame header, CRLF is the delimiter. Reuse the existing `Writer`
  verbatim — it already truncates instead of overflowing. Note in a comment that the arm being deleted,
  `Ok(_) => true` (`lib.rs:52`), meant "tail discarded" and *was* the bug.
- **The space check must be inside the same critical section as the loop.** `write_whole` pre-checks
  `frame.len() > free_capacity` and then `debug_assert!`s on any stall (`frame.rs:311-331`); checked
  outside the lock, a full ring becomes a debug-build panic instead of a clean drop. Inside the lock it is
  sound: the consumer only ever *increases* free capacity, and advancing the read cursor never shrinks the
  writer's contiguous run, so pre-check-passes ⇒ progress every round, and two rounds always suffice after
  crossing the wrap. Do not "simplify" the loop back to one `try_write` — embassy short-writes at every
  ring wrap even when the ring is empty (parent §2, measured: 200 requested, 112 accepted, 512 free), and
  that behaviour is identical in 0.6.2 and 0.8.0.
- Counters: `dropped_full` **and** `bytes_dropped` on refusal (ring untouched, record gone whole);
  `trunc` when `enc.truncated`; `records_sent` on commit. Never `dropped_full` for truncation (§3's
  amendment — the two counters mean different things to a reader).
- Bound the locked region **in a comment** (AC #3): ≤256 B formatted, CRC-16 over ≤220 bytes × 8 bit
  iterations ≈ 1 760 shift-and-conditional-xor steps, plus two memcpys (≤256 B, ≤228 B), executed with
  PRIMASK set while a 48 kHz block arrives every ~667 µs on the same single-threaded executor. State the
  byte and iteration counts, not invented microseconds — see step 6 for why this crate cannot measure one.
  The table-less CRC dominates the region by a wide margin; if it ever proves too long the answer is a
  reserve/commit ring replacing the `Pipe` (parent §4), recorded as a follow-up ticket, never a quiet
  redesign.
- Reading the clock before the lock keeps the timer driver's own locking out of our window; 1 ms of skew
  is invisible in a 1 ms field. Everything that must agree with wire order — seq, frame, space check,
  commit — is inside.
- Waking the drain task happens inside the lock via `try_write`'s waker and is fine (PendSV fires on exit;
  a thread-mode executor cannot preempt itself). What is not fine: awaiting anything, or logging anything,
  inside the lock.
- Records logged before `usb::init` still go to `NoOp` and vanish **uncounted**. Leave that; comment it so
  nobody later files it as a counter bug.

## 3. Counters, BOOT, STATUS

Seven `static AtomicU32` in `lib.rs`, `Ordering::Relaxed` throughout (single core; they exist to be
snapshotted without touching the record lock): `CONSOLE_SEQ`, `CONSOLE_RECORDS_SENT`,
`CONSOLE_DROPPED_FULL`, `CONSOLE_BYTES_DROPPED`, `CONSOLE_TRUNCATED`, `CONSOLE_ENDPOINT_ERRORS`, and one
for the panic path's seq participation (same `CONSOLE_SEQ` — see step 4, so six plus the shared seq).
Increment them through two helpers so the semantics are stated once:

- `bump(&AtomicU32)` — **saturating**: `fetch_update(Relaxed, Relaxed, |v| v.checked_add(1))`, i.e. stop at
  `u32::MAX`. Reason (AC #11): STATUS counters are cumulative-since-boot and the host differences
  successive readings to get rates; a wrapped `bytes_dropped` (24 days of sustained dropping at ring
  capacity — reachable on a rig left running) turns into a huge negative rate and looks like a decoder bug.
- `CONSOLE_SEQ` alone keeps plain `fetch_add`, i.e. wraps mod 2^32 by design, matching the 8-hex-digit
  `seq` field. A saturated seq would emit a duplicate sequence number; a wrapped one is recoverable by the
  reader with modular subtraction (`seq_now − seq_prev` as u32; a jump ≥ 2^31 means restart or corruption,
  not loss). Say both halves in the comment — the asymmetry is deliberate, not sloppiness.

Expose one `pub fn console_counters() -> ConsoleCounters` snapshot so the emitter reads all fields from one
place instead of six scattered loads.

**Two pieces must be pure and host-compiled (AC #12)** — no `critical-section`, no `Instant`, no atomics —
because the wire field set is a contract that TASK-030.03 documents and TASK-031 parses:

- `fn status_body(out: &mut [u8; 256], snap: &ConsoleCounters, pipe_free: usize) -> usize`, writing
  `STATUS proto=1 sent=… dropped_full=… bytes_dropped=… trunc=… ep_err=… seq_next=… pipe_free=…` in exactly
  that order. Unit tests pin the field names and order, and assert a saturated counter renders `4294967295`.
- `struct StatusGate { last_emit_ms: u32, last_seen: Option<ConsoleCounters> }` with
  `fn due(&mut self, now_ms: u32, snap: &ConsoleCounters) -> bool` implementing "≥1000 ms since the last
  emission **and** some counter moved". Tests cover: first emission, sub-second suppression, unchanged
  counters suppressed, change after 1 s emits, and the `now_ms` wrap at 2^32 ms behaving sanely.

**BOOT** — emitted once in `usb::init`, *after* `install_logger()` **and** after the
`GLOBAL_BACKEND = Backend::Usb` assignment (`usb.rs:178-182`): before that switch the backend is `NoOp` and
silently swallows it, which is the one way this record can be lost. Body:
`BOOT proto=1 fw=<CARGO_PKG_VERSION> pipe=<LOG_PIPE_SIZE> maxbody=200`. It rides the normal commit path, so
it consumes `seq 0` and is subject to drops like anything else — bytes written before the endpoint is up
just wait in the ring.

**STATUS** — emitted from the drain task, never from inside the record lock, at the point in step 5 where
the ring has just drained. Debounce matters beyond politeness: during a full-ring condition the status
record competes for the very space that is missing, and a drop storm must not starve real logs. Emitting
only after a successful flush also means STATUS never needs a timer, never needs a second task (spawning
one would force a firmware call-site change and violate AC #8), and satisfies AC #5 as written — a quiet
device has no changed counters, so it owes no STATUS. Honest limits for the notes: a STATUS record lost to
a full ring is indistinguishable from nothing having happened except through seq gaps, and a host that
stops reading sees neither STATUS nor the reason.

## 4. Panic path (`panic_handler.rs` + `usb::emit_blocking`)

`handle_panic` formats into its own 128 B buffer (`panic_handler.rs:44`) and calls
`usb::emit_blocking(msg)` (`panic_handler.rs:45`). Frame it with the same v1 encoder — same leading `~`,
same CRC range — so a `PANIC:` line arrives as one valid record and `README.md:194-199`'s procedure keeps
meaning what it says.

- Build the frame in `LOG_BUFS`'s `frame` buffer **without taking the lock**, and bump `CONSOLE_SEQ` with
  a plain `fetch_add`. Constraints restated because they are absolute: no locks (the executor may be dead
  mid-lock and a lock here could be held forever), no allocation, no panicking, no reliance on the ring — a
  `Pipe` write on this path is guaranteed-discarded, which is why `emit_blocking` bypasses it.
- Why the unlocked buffer is acceptable: the executor is halted, so nothing else is formatting. The one
  theoretical overlap — a panic raised *inside* the critical section — cannot happen in release (nothing in
  the region panics; `frame.rs`'s only traps are `debug_assert!`s), and its consequence would be a garbled
  final line, not memory unsafety. One comment saying exactly that.
- Do **not** put a `[u8; MAX_FRAME]` (228 B) local on the panic stack: the handler already keeps its frame
  small precisely because it runs on a stack that may be nearly exhausted.
- Keep `emit_blocking`'s existing chunking to `MAX_PACKET_SIZE` and its busy-poll/select-with-deadline
  shape; keep timers out of it (`usb.rs:300-306` explains why). Add the trailing-ZLP rule from step 5: if
  the framed length is an exact multiple of 64, follow with `write_packet(&[])`, or the most important
  record in the capture can sit in the host's driver buffer forever.
- Verify by reading `firmware/src/bin/panictest.rs` end to end and listing in the notes which lines you
  traced. Board proof is TASK-030.04 / TASK-033, not this ticket.

## 5. Drain loop (`usb.rs:218-246`) and the short-packet rule (amendment A2)

Pass 2 said "never hand `write_packet` a zero-length chunk". For stream-like bulk IN that advice is wrong,
and growing the read to 256 B makes the bad case routine. Verbatim, `embassy-usb-0.6.0/src/class/cdc_acm.rs:88-93`:
*"If you write a packet that is exactly `max_packet_size` bytes long, it won't be processed by the host
operating system until a subsequent shorter packet is sent. A zero-length packet (ZLP) can be sent if there
is no other data to send. This is because USB bulk transactions must be terminated with a short packet,
even if the bulk endpoint is used for stream-like data."* Nothing in `CdcAcmClass::write_packet`
(`cdc_acm.rs:332-334`) does this for us; embassy's own classes do it by hand (`hid.rs:332-339`,
`cdc_ncm/mod.rs:423-426`), and `write_packet(&[])` really does emit a ZLP on this hardware
(`embassy-usb-synopsys-otg-0.3.3/src/lib.rs:1313` accepts length 0, `:1374-1397` arms a 1-packet 0-byte
transfer). Left unfixed, the symptom is a capture whose **tail is missing** — the exact class of loss this
ticket exists to eliminate, and one the framing would faithfully report as a seq gap while the cause sat
in our own write pattern.

Invariant to implement: **never park while the last packet handed to the endpoint was exactly 64 bytes.**

```
let mut buf = [0u8; 256];
let mut last_was_full = false;
loop {
    cdc.wait_connection().await;                 // log::info!("USB connected") — outside any lock
    loop {
        match LOG_PIPE.try_read(&mut buf) {
            Ok(0) | Err(Empty) => {
                if status_gate.due(now_ms, &console_counters()) { emit_status(); continue; }
                if last_was_full && cdc.write_packet(&[]).await.is_err() { bump(ep_err); break; }
                last_was_full = false;
                LOG_PIPE.read(&mut buf).await;   // park until a byte exists; cancel-safe
            }
            Ok(n) => {
                for chunk in buf[..n].chunks(MAX_PACKET_SIZE as usize) {
                    if cdc.write_packet(chunk).await.is_err() { bump(ep_err); break outer-inner; }
                    last_was_full = chunk.len() == MAX_PACKET_SIZE as usize;
                }
            }
        }
    }
    log::info!("USB disconnected");              // outside any lock
}
```

- Order inside the empty branch is deliberate: consider STATUS first (it enqueues and we loop to flush
  it), and only ZLP when we are genuinely about to park. Sending a ZLP then appending more is legal but
  wasteful.
- `try_read` returning `n < 256` is normal (ring wrap, `ring_buffer.rs:44-58`) and is *not* a loss signal;
  only a `chunk.len() == 64` matters, and only for the flag.
- `Pipe::read`'s future consumes bytes only in the poll that returns `Ready` (`pipe.rs:269-290`), so
  parking here cannot eat a record even if the future is later dropped. Replacing the `yield_now()` spin
  (`usb.rs:232-234`) removes the thing currently stealing executor slots from the audio loop — say so in
  the notes, because it plausibly moves the baseline TASK-027.02 measures.
- Disconnect detection stays "a write failed". Bytes pulled just before a failed write are lost with it
  (≤ one 256 B read per reconnect) and surface in `ep_err` / seq gaps rather than silently. A stall in
  `read()` can outlive a replug until the next record arrives — acceptable, worth one honest comment.
- Delete the dead `pub struct Disconnected` and `impl From<EndpointError> for Disconnected`
  (`usb.rs:42-53`): nothing constructs or consumes them, and they are the mechanism by which
  `BufferOverflow` would be mislabelled as a disconnect if a future call site used `?`. Keep the
  `MAX_PACKET_SIZE` doc comment (`usb.rs:26-33`) — the oversize-write lesson still holds and the new
  chunking preserves it by construction.

## 6. Why there is no microsecond measurement in this ticket

The obvious instrument — timestamp the locked region with `Instant` and report a max in STATUS — has no
resolution here. The time driver runs at **32 768 Hz** (`tick-hz-32_768`, selected by daisy-embassy's
`embassy-time` features; `firmware/Cargo.toml:29` asks for embassy-time bare and inherits it), i.e. one
tick ≈ 30.5 µs, which is coarser than the ~20–40 µs window worth measuring: a max-ticks field would sit at
0 and occasionally read 1, which is fake precision on a safety-relevant number. Cortex-m 0.7 ships no
DWT/CYCCNT driver (its `src/` has `itm`, no `dwt`), so cycle counting means hand-writing the M7 LAR/PRAR
unlock and trusting the trace enable to come up. Shipping a counter that reads zero is worse than shipping
none. Measure it properly once the probe lands (TASK-036/037, DWT CYCCNT) or with a GPIO toggle and a logic
analyser on the bench; TASK-030.04's ears are the interim control and step 2's static bound is what this
ticket commits to. (If a future pass does add the measurement, it belongs in STATUS as `lock_max_ticks`
with the 30.5 µs tick named beside it, never as `_us`.)

## 7. Verification

Gates: `cargo fmt --all --check` · `cargo clippy --workspace --all-targets -- -D warnings` · same with
`--features asperitas-pod/pod-hw` · `cargo test --workspace` ·
`cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --features seed3 --release`.

Ticket-specific evidence:

- `git diff --stat firmware/` must be **empty** (AC #8). If it isn't, the design drifted toward changing
  call sites; fix the design, not the criterion.
- `grep -n "static mut\|&raw mut\|unsafe" crates/asperitas-logging/src/{lib.rs,usb.rs}` — report what
  survives and the one-line justification for each. Expected survivors: the `UnsafeCell` deref under the
  lock, the CDC/LED raw refs, and whatever `GLOBAL_BACKEND` needs.
- New host tests must actually fail when the behaviour breaks: mutate `status_body`'s field order or the
  saturation helper and watch a test go red. If neither does, the test is decoration.
- FLASH is capped at 128 K (`firmware/memory.x:7`); record the `.text`/`.bss` delta for `podtest` if
  `cargo size`/`llvm-size` is available. RAM is 512 K and irrelevant.
- Notes must state: the static bound on the locked region, every place this plan deviated from parent §4
  (with the deviation written back into the parent, not buried in a commit message), and explicitly that
  there is **no board proof here** — legibility, zero bad frames, the missing-tail question and audio
  safety belong to TASK-030.04 and TASK-033.

Modified files: `crates/asperitas-logging/src/lib.rs`, `usb.rs`, `panic_handler.rs` (+ its tests),
`Cargo.lock`, `firmware/Cargo.lock`.

---

## Amendments to parent §4 (posted to TASK-030)

**A1 — "one buffer, not two" is not implementable.** `frame::encode` takes `body: &[u8]` and
`out: &mut [u8; MAX_FRAME]` as disjoint borrows, and `encode` always copies + sanitises from the source
anyway (`frame.rs:193-196`). Two buffers it is, costing one extra ≤256 B copy inside the lock (~0.1 µs at
480 MHz vs a 667 µs block period). Rewriting `encode` for in-place use would reopen a finished,
property-tested module for a rounding error.

**A2 — "never hand `write_packet` a zero-length chunk" is inverted for the final packet.** Bulk
transactions must end short; a trailing exactly-64-byte packet is withheld by the host until something
shorter follows. Evidence quoted in step 5. The rule becomes: never park with a full-size packet outstanding,
and mirror it in `emit_blocking`. Needs board confirmation — TASK-030.04 gained an AC for the capture tail.

**A3 — pass 2's coordinates had drifted** (LOG_PIPE `149-151`→`158-162`, `pipe()` `156-164`→`164-175`,
drain loop `213-247`→`218-246`, `emit_blocking` call `panic_handler.rs:41`→`:45`, BufferOverflow trap
`usb.rs:27-33`→the dead `From` impl at `:42-53` plus the doc comment). Fixed inline above.

Cross-ticket edits made by this planning pass: TASK-030.03 gained an AC to document the short-packet/ZLP
rule for reader authors; TASK-030.04 gained an AC to check that a captured file ends on a complete record.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## What landed

**New `crates/asperitas-logging/src/console.rs` (ungated on purpose):** `ConsoleStats` +
`ConsoleCounters` snapshot (`take_seq` wraps mod 2^32; every other counter saturates via one shared
`checked_add` path), `boot_body`, `status_body`, `StatusGate`, and 10 host unit tests. Ungated because the
root workspace never enables `log-usb`, so gating it would mean the wire contract TASK-030.03 documents and
TASK-031 parses had no CI coverage at all.

**`lib.rs`:** `LOG_PIPE` is now `pub static Pipe<CriticalSectionRawMutex, 2048> = Pipe::new()` — the
`Option`, both `&raw mut` pokes and the `pipe()` accessor are deleted (`Pipe` methods take `&self`, and
`Pipe::new()` is `const`, so the aliasing hack bought nothing). One `RECORD_BUFS`
`Mutex<CriticalSectionRawMutex, UnsafeCell<RecordBufs>>` holds `{ body: [u8;256], frame: [u8;MAX_FRAME] }`
(A1 honoured: two buffers, because `frame::encode` takes disjoint borrows). New `emit(level, fill)` does
seq → format → encode → capacity pre-check → `frame::write_whole` → count inside **one** critical section,
with the IRQ-off bound stated as bytes/CRC-iterations in its doc comment. Refusal bumps `dropped_full` +
`bytes_dropped` and leaves the ring untouched; commit bumps `records_sent`; a cap hit bumps `trunc` only.
The clock is read *before* the lock; everything that must agree with wire order is inside.

Crate-level `#![allow(static_mut_refs)]` is **gone**, with no replacement allow anywhere: the five remaining
`static mut`s (`GLOBAL_BACKEND`, `CDC_REF`, `USB_DEV_REF`, `BOOT_LED_REF`, `PANIC_FRAME`) are reached
through `core::ptr::addr_of_mut!` (+ `read_volatile` for the pointer loads). Verified lint-silent under
`-D warnings`, and separately confirmed by experiment that `addr_of_mut!` + raw deref does not trip the lint
on rustc 1.97.

**`usb.rs`:** drain task now reads `DRAIN_BUF_SIZE = 256` per wakeup, chunks every write to
`MAX_PACKET_SIZE`, sends a ZLP before parking whenever the last packet was exactly 64 bytes (A2), and parks
on `Pipe::read` instead of `yield_now()`-spinning. Debounced STATUS rides this loop and is considered only
where the ring has just drained. `init()` no longer constructs a pipe; it installs the logger, calls
`set_backend_usb()`, then `emit_boot()` in that order. Dead `Disconnected` struct + `From<EndpointError>`
removed (they were the mechanism by which `BufferOverflow` could be mislabelled as a disconnect). New
`emit_panic_record(body)` frames into a panic-dedicated `PANIC_FRAME` static — no lock, no allocation, no
stack frame buffer — and `emit_blocking` gained the same trailing-ZLP rule for lengths that are exact
multiples of 64.

**`panic_handler.rs`:** body loses its trailing CRLF (the frame owns the delimiter; a CR/LF inside a body
would only be sanitised to `_`) and reuses `TruncWriter`. **`led.rs`:** `pub fn get_mut() -> &'static mut
BootLed` (zero callers) replaced by private `with_led(impl FnOnce(&mut BootLed) -> R) -> Option<R>`, which
ends the borrow before returning — `blink_task` used to hold a `&mut BootLed` across `.await` while
`set_global_state` (other tasks, and the panic handler) drove the same pins.

## Evidence per acceptance criterion

- #1/#3 — host build links with zero `critical-section` references outside `log-usb` (grep clean); clippy
  gates green with default features and with `asperitas-pod/pod-hw`.
- #2 — the whole-or-nothing mechanism is already pinned by TASK-030.01 host tests against a real
  `embassy_sync::pipe::Pipe` (`a_single_try_write_short_writes_at_the_ring_wrap_even_when_empty`,
  `write_whole_commits_a_full_record_across_the_ring_wrap`,
  `write_whole_refuses_a_frame_that_does_not_fit_without_writing_anything`). The device call site uses that
  exact pattern with the pre-check moved inside the lock, which is also what stops the stall
  `debug_assert` in `write_whole` from becoming a debug-build panic on a full ring.
- #4/#5/#11/#12 — `cargo test -p asperitas-logging`: 10 console tests. Field names/order pinned verbatim;
  saturated counters render `4294967295`; worst-case STATUS fits under the 200-byte cap; saturation rather
  than wrap; first/sub-second/unchanged/changed-after-1s pacing; one attempt per second even when every
  attempt is dropped; sane behaviour across the 2^32 ms clock wrap; BOOT/STATUS bodies decoded back out of
  the real v1 encoder + `Decoder` with `bad_frames == 0`.
- #6 — traced `firmware/src/bin/panictest.rs` end to end: `#[panic_handler]` wrapper (l. 63-66) →
  `handle_panic` → `led::set_global_state` → `format_panic_message` → `usb::emit_panic_record` →
  `emit_blocking` (INITIALIZED guard, `select(device_fut, write_fut)`, `Instant` deadline). Board proof is
  TASK-030.04 / TASK-033.
- #7/#10 — code paths above; `git diff --stat firmware/` empty; thumbv7em release cross-compile clean. The
  `rust-lld: cannot find entry symbol _start` warning is pre-existing — reproduced identically at HEAD with
  these changes stashed.
- #8/#9 — LOG_PIPE_SIZE 2048; fmt/clippy/test/cross-compile all green.

## Honest limits and knock-on effects

- No microsecond measurement of the locked region ships here, per plan step 6: the time driver ticks at
  32 768 Hz (~30.5 us/tick), coarser than the window worth measuring. If a probe ever lands, add
  `lock_max_ticks` to STATUS beside the tick size, never a `_us` field.
- Removing the empty-ring busy-spin plausibly **moves the baseline TASK-027.02 measures** — its poll-rate
  numbers were taken while the drain task was stealing executor slots. Worth saying before anyone reads a
  change there as an audio regression.
- A STATUS record lost to a full ring is indistinguishable from nothing having happened except through the
  `seq` gap it leaves; the gate deliberately spends one attempt per second so a drop storm cannot turn
  STATUS into a retry loop. A host that stops reading sees neither STATUS nor its cause.
- Records logged before `usb::init` still vanish uncounted (commented in `Backend::write`) — deliberate, not
  a counter bug.
- Follow-up filed as **TASK-030.05**: `firmware/src/bin/panictest.rs` expected-output table still shows the
  unframed `PANIC:` line. Blocked from fixing here by this ticket AC #8 (nothing under `firmware/` may
  change).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Device log path is now atomic-or-drop with per-boot sequence numbers and in-band loss counters: LOG_PIPE is a const static Pipe<CriticalSectionRawMutex, 2048>, one critical section spans format -> frame -> capacity pre-check -> whole-record commit -> count, short writes are refused rather than split, and every drop reason is counted by code that needs no buffer space. BOOT goes out at init and debounced STATUS carries seq/sent/dropped_full/bytes_dropped/trunc/ep_err, so the first byte of a capture already explains the gap behind it. The panic path emits one valid framed record through emit_blocking taking no locks and allocating nothing, drain reads 256 B per wakeup while never handing the endpoint more than 64 B, and every bulk transaction now ends short (ZLP before parking), which was silently withholding each capture's tail. Crate-wide #![allow(static_mut_refs)] deleted; firmware call sites unchanged and cross-compilation clean.
<!-- SECTION:FINAL_SUMMARY:END -->
