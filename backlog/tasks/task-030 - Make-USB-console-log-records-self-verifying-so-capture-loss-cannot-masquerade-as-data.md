---
id: TASK-030
title: >-
  Make USB console log records self-verifying so capture loss cannot masquerade
  as data
status: Blocked
assignee:
  - '@human'
created_date: '2026-09-09 01:23'
updated_date: '2026-09-09 05:42'
labels:
  - planned
dependencies:
  - TASK-030.01
  - TASK-030.02
  - TASK-030.03
  - TASK-030.04
documentation:
  - docs/reference/daisy-seed3.md
  - TASK-018.04
  - TASK-027
modified_files:
  - crates/asperitas-logging/src/lib.rs
  - crates/asperitas-logging/src/usb.rs
priority: high
type: feature
ordinal: 40000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Hardware captures are this project's only verification mechanism for control-surface behaviour, and the transport corrupts roughly 8.8% of them (1226 of 13968 lines directly counted in TASK-018.04's notes). That defect nearly produced a false finding: a line reading r2=298 was a record cut mid-number, and it was close to being reported as an ADC glitch.

Two faults in asperitas-logging explain it. First, the pipe write path treats a partial write as success — when the 512-byte buffer is nearly full, a record is written only as far as space remains and the remainder is discarded, which is exactly the observed symptom of lines truncated mid-token. Second, the pipe's mutex is the no-op kind, providing no mutual exclusion, so concurrent producers share the format buffer and the queue's internal counters unprotected.

The deeper problem is that loss is undetectable by the reader. Nothing on the wire distinguishes a complete record from a truncated one, so a consumer parsing this stream cannot tell clean data from garbage. Every planned automated capture is built on these bytes, so integrity has to become something the reader can check rather than something it hopes for. Lost data must surface as an explicit count instead of looking like plausible readings.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Log records are self-delimiting and carry their own integrity check, so a reader can distinguish a complete record from a damaged one.
- [ ] #2 A record that does not fit the buffer is dropped whole, never partially; bytes buffered for other records remain valid.
- [ ] #3 Concurrent producers cannot interleave inside a single record.
- [ ] #4 Totals for dropped records and failed frames are reported to the host in-band, so a capture declares its own loss instead of appearing clean.
- [ ] #5 Host-side tests cover truncation at every boundary between frames, corruption inside a frame, and interleaved multi-producer emission — each is detected and never mistaken for a valid record.
- [ ] #6 Existing log call sites in firmware/src/bin/*.rs need no changes.
- [ ] #7 docs/reference/daisy-seed3.md's probe-free debug channel section describes the framing and the loss counters.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# TASK-030 — orchestration plan + normative wire spec

This ticket is an **umbrella**: no code lands here. All work is in children `TASK-030.01`–`TASK-030.04`.
**This plan is normative.** Child planners must implement console protocol **v1** exactly as pinned
below, or amend it deliberately and visibly in their own plan (and say so here). Two executors
implementing opposite ends of the link from two different readings of this file must agree byte for byte.

## 1. Why, and what this ticket is not

Three faults, only two of which are bugs in the usual sense:

- `crates/asperitas-logging/src/lib.rs:46-55` maps `Pipe::try_write`'s `Ok(n)` to unconditional success.
  embassy-sync 0.6.2 short-writes **by design** — `let n = available.len().min(buf.len())`
  (`~/.cargo/registry/src/index.crates.io-*/embassy-sync-0.6.2/src/pipe.rs:398-401`). A record straddling
  the end of the 512-byte ring silently loses its tail. That is the observed shape: knob lines missing
  their `r1=/r2=` fields, `ENC` cut after the token, button lines losing `BTN1`/`BTN2`
  (`backlog/tasks/task-018.04 - Verify-every-Pod-control-on-hardware.md:87-92`).
- `lib.rs:149-151` picks `NoopRawMutex`, and `lib.rs:80` `FORMAT_BUF` is a shared `static mut` scratch
  buffer, so nothing excludes concurrent producers even in principle (AC #3).
- Deeper: CDC-ACM hands the host a byte stream with no message boundaries, so **nothing on the wire
  distinguishes complete from damaged**. Loss cannot be reported if it cannot be seen.

Evidence baseline this must beat: 1226 of 13968 lines (~8.8%) garbled or missing over 240 s
(`task-018.04…:83`), including two button presses that vanished immediately after a truncated line
(`:73`), and one truncated knob line that nearly got filed as an ADC glitch.

Not in scope (owned elsewhere): the host reader tool (**TASK-031**), inbound control commands riding the
same frames (**TASK-032**), hardware proof of the loss numbers (**TASK-033**, with a cheap early smoke
check split off as **TASK-030.04**), and probe/RTT as an alternative channel (**TASK-036**).

## 2. Facts established this planning pass (children: do not re-research these)

**Verified by experiment**, not inference: `static PIPE: Pipe<CriticalSectionRawMutex, N> = Pipe::new();`
compiles and works through `&self` alone (`free_capacity`, `try_write`, `try_read`) — `Pipe` is `Sync`
because its internals are `UnsafeCell` + `blocking_mutex::Mutex`. So the `Option<Pipe>` storage, the
`&raw mut` pokes, the `pipe() -> &'static mut Pipe` accessor (`lib.rs:149-161`) and the crate-level
`#![allow(static_mut_refs)]` (`lib.rs:15`) can all be **deleted**, not worked around. Probe kept at `/tmp/pipecheck`.
The same probe (`/tmp/chprobe`, built `--target thumbv7em-none-eabihf`) also confirms a plain
`static LOG_Q: Channel<CriticalSectionRawMutex, Frame, 16>` works the same way, so switching to a
record-oriented queue stays available as a fallback (§4) rather than being blocked on statics.

**The defect nobody had named, found and reproduced this pass.** `Pipe::try_write` does not merely
short-write when the ring is nearly full — it short-writes at every **ring wrap**, however empty the ring
is. `RingBuffer::push_buf` returns only the contiguous run up to the end of the backing array
(`embassy-sync-0.6.2/src/ring_buffer.rs:18-31`), while `Pipe::free_capacity()` reports *total* free bytes
(`pipe.rs:456`). Measured on the host against the real 0.6.2 `Pipe<NoopRawMutex, 512>` (`/tmp/pipecheck2`):
after 400 bytes are written and drained, the ring is empty (`free_capacity() == 512`) yet a 200-byte write
accepts **112**. Consequence for the design: a "reserve-check then write once" fix — which is what this
plan said before this pass — still truncates records, exactly as badly as today's code. It also explains
the observed rate better than buffer-fullness ever did: truncation happens once per wrap, i.e. roughly
every `512 / mean record length` records ≈ every 10 records for ~50 B lines ≈ 10%, against the 8.78%
tally in TASK-018.04. Buffer pressure is therefore a secondary mechanism.

- **No new dependency for the mutex.** `crates/asperitas-logging/Cargo.toml:12` already carries
  `cortex-m = { version = "0.7", features = ["critical-section-single-core"] }`, which registers the
  `critical-section` impl for ARM targets. `critical-section 1.2.0` is in both lockfiles already.
- **Host link hazard.** With no impl registered, `critical_section::acquire()` resolves to
  `_critical_section_1_0_acquire`, an *undefined symbol at link time* — not a compile error. Every use of
  `CriticalSectionRawMutex` must stay behind `#[cfg(feature = "log-usb")]`, which never builds for host.
  Second hazard on the same path: the `critical-section` `std` fallback is **not re-entrant**, while
  cortex-m's is (PRIMASK save/restore). Therefore: never call into the `Pipe` from inside a critical
  section in any code path that could be built for host. On target, outer-lock → `pipe.try_write` (which
  takes its own lock) nests safely.
- **Host tests will run in CI unchanged.** Root `Cargo.toml` is `members = ["crates/*"]`, `exclude = ["firmware"]`;
  `firmware/` is its own workspace with its own lockfile. `cargo test --workspace` (CI step 4,
  `.github/workflows/ci.yml:27-36`, and `lefthook.yml` pre-push) already builds and tests
  `asperitas-logging` on the host with default features, i.e. *without* `log-usb`. `proptest 1.11.0` is
  already in use — copy the idiom in `crates/asperitas-dsp/tests/property_tests.rs:30-41`.
- **`crc` and `cobs` appear in neither lockfile.** Do not add them: CRC-16/CCITT-FALSE is ~15 lines and
  AC #5's properties are exactly the coverage a hand-rolled one needs.
- **Nobody parses the stream yet.** Exhaustive search found no host script or tool that opens the serial
  device. The only format commitments are prose: `README.md:163-200` (human `screen` instructions,
  including panictest's "countdown text with no `PANIC:` line is a real failure") and
  `firmware/src/bin/podtest.rs:277` ("field order is fixed and space-separated so host tooling can parse
  it"). Both survive v1 untouched, because v1 stays printable ASCII — see §3.
- **Producers:** 19 `info!` call sites in `firmware/src/bin/*.rs` (13 of them in `podtest.rs`), plus the
  drain task's own `usb.rs:228`/`:244`. The 48 kHz audio callback (`main.rs:236-255`) does **not** log,
  no ISR logs, and everything runs on one single-threaded embassy executor
  (`embassy-executor 0.10.0`, `platform-cortex-m` + `executor-thread`, no `executor-interrupt`). So AC #3
  is currently about cooperative interleaving across await points plus insurance for the day someone adds
  an ISR log.
- **RAM is not a constraint:** `firmware/memory.x:19` declares 512 KB at `0x24000000`.
- **Timing budget:** `BLOCK_LENGTH = 32` at 48 kHz ⇒ one audio block per ~667 µs, and the audio loop
  shares the single executor with the drain task. TASK-026's finding is that stalling the executor shows
  up as ticker catch-up and SAI risk. Consequence: the locked region must be bounded and small, and the
  drain loop must never busy-spin.
- **Throughput headroom is large.** Current worst case is ~100 records/s × ~50 B ≈ 5 KB/s against CDC-FS
  throughput orders of magnitude higher. Framing overhead is not a reason to make any decision here.

## 3. Console protocol v1 — normative

**Decision: printable-ASCII framed lines, not binary COBS/SLIP frames.** Binary framing is the usual
embedded choice and was considered (COBS + `0x00` delimiter self-synchronises beautifully, and
`log-event-pack`/defmt are close precedents). It is rejected here for one project-specific reason: this
project verifies itself by a human reading a capture. `screen` output and `grep` on a raw capture file
must keep working, and `README.md:191-199` documents a verification procedure that is explicitly about
reading text. v1 buys integrity and resynchronisation while keeping every record human-legible and
greppable. Cost: 28 fixed bytes per record (21-byte prefix + 7-byte trailer) instead of ~4, against
enormous headroom. Rejected
alternatives, for the record: length-prefix framing (cannot resynchronise after a loss), defmt binary
(formatting change at every call site, violates AC #6, and unreadable without a decoder).

### Grammar

```
record   := '~' level SP seq SP t_ms SP body '*' crc CRLF
level    := one of  I W E D T                 (log::Level; Info -> 'I')
seq      := 8 lowercase hex digits            (u32, monotonic within a boot, 0 for the first record)
t_ms     := 8 decimal digits, zero-padded     (milliseconds since boot; wraps at ~27.8 h)
body     := 0..=200 bytes, each byte >= 0x20 and != 0x7F   (see sanitisation)
crc      := 4 lowercase hex digits            (CRC-16/CCITT-FALSE, see below)
SP       := 0x20 ; CRLF := 0x0D 0x0A
```

Example: `~I 00000042 00004567 ENC +1 *2f9e\r\n`

(The CRC above is illustrative. TASK-030.03 AC #1 asks for a *real* captured line in the docs, so nobody
has to trust a hand-computed checksum — including the one above.)

- **CRC range:** the ASCII bytes of `level SP seq SP t_ms SP body` — everything after `~` up to but not
  including the `*`. Parameters pinned explicitly: poly `0x1021`, init `0xFFFF`, `refin=false`,
  `refout=false`, `xorout=0x0000`. Golden test vector: the 9-byte ASCII string `123456789` ⇒ `0x29b1`.
- **Sanitisation (this is what makes CRLF a true delimiter):** any body byte `< 0x20` or `== 0x7F` is
  replaced by `'_'`. Bytes `>= 0x80` pass through untouched (UTF-8 messages stay intact). Bodies longer
  than 200 bytes are truncated at 200 **before** the CRC is computed, so the record still ships and still
  validates.
  **Amendment (this planning pass):** an over-long body is *not* counted as a dropped record. It gets its
  own counter, `trunc`, because `dropped_full` means "you never got this record at all" and folding
  truncation into it would make the counter lie about something the reader cannot tell apart. Truncation
  is near-impossible in practice here — every literal in `firmware/src/bin/*.rs` is under 40 characters —
  so `trunc > 0` is still worth surfacing: it names a class of silent loss that would otherwise look like
  a call site writing a short message.
- **Why the parse is unambiguous:** bodies contain no CR/LF, so the first CRLF after a `~` ends the
  record; the fixed-width prefix means spaces inside the body cannot confuse field splitting; a stray
  `~` or `*` inside a body can only produce a CRC mismatch, never a false-valid record.
- **Resynchronisation contract for readers:** scan for `~`; try to parse a record there; on any failure
  (missing `*`, non-hex CRC, CRC mismatch, missing CRLF, over-length) discard **one** candidate start and
  retry at the next `~`. Never attempt to repair a record. Corruption costs the hit record, not the stream.
- **Direction markers reserved:** `~` is device→host only. TASK-032's host→device commands must use a
  different leading byte (`>` recommended) with the identical field and CRC rules, so one parser handles
  both directions without ambiguity. Record this in the docs (TASK-030.03).
- **Versioning:** the leading `~` *is* version 1. No per-record version field. The BOOT record carries
  `proto=1` so a reader can handshake before trusting anything.

### Reserved body prefixes (in-band meta-records)

```
BOOT proto=1 fw=<CARGO_PKG_VERSION> pipe=<ring bytes> maxbody=200
STATUS proto=1 sent=<n> dropped_full=<n> bytes_dropped=<n> trunc=<n> ep_err=<n> seq_next=<n> pipe_free=<n>
```

`BOOT` is emitted once at init. It is how a restart is distinguished from loss: CRC and seq detect
damage, never absence, so a reader seeing a seq gap must be able to tell "board rebooted" from "bytes
lost". A second `BOOT` mid-capture, or a seq regression, is the answer. Deliberately **no** boot-id in
v1: it would need `.noinit` persistence or a peripheral read for no gain over the banner's presence, and
reset *reason* belongs to TASK-032's command set.

## 4. Device write path (TASK-030.02)

One lock, one commit, per record:

```
now_ms = Instant::now()                        // BEFORE the lock: see the note below
CriticalSectionRawMutex::lock(|| {
    seq  = CONSOLE_SEQ.fetch_add(1)            // assigned inside the lock, so wire order == seq order
    format body into FRAME_BUF[21..]           // existing Writer, lib.rs:82-108
    sanitize + cap body at 200                 // set Encoded::truncated, bump `trunc`
    write prefix '~' level ' ' seq ' ' now_ms ' ' and trailer '*' crc CRLF   // all done by frame::encode
    let f = &FRAME_BUF[..enc.len];
    if !frame::write_whole(f, LOG_PIPE.free_capacity(), |c| LOG_PIPE.try_write(c).ok()) {
        bump dropped_full + bytes_dropped; return;                       // dropped WHOLE, ring untouched
    }
    bump records_sent
})
```

- **The commit must loop, and the pre-check alone is not enough.** Per §2, `try_write` stops at the ring
  wrap even when the ring is empty, so a single call truncates mid-record no matter how much space is
  free. The loop terminates because each round either writes ≥1 byte or reports `Full`, and after the
  first round crosses the wrap the contiguous run equals total free space; with `free_capacity() >= n`
  checked under the same lock, two rounds always suffice. The consumer can only *increase* free space,
  and moving the read cursor forward never shrinks the writer's contiguous run, so nothing outside the
  lock can starve the loop. This is not paper reasoning: `/tmp/pipecheck2` runs exactly this commit
  against the real `Pipe<NoopRawMutex, 512>` for 20 000 randomized producer/consumer rounds and every
  committed frame arrives byte-exact and in order, with no partial ever observed. TASK-030.01 turns that
  probe into a permanent test.
- **`write_whole` has two outcomes and no third.** It returns `false` having written *nothing* when the
  frame cannot fit, and `true` only when every byte is in the ring. Mid-loop starvation is not a runtime
  case to handle — it is a `debug_assert!` over an impossibility proved above. Should an embassy change
  ever make it possible anyway, the fragment left behind is caught by the reader's CRC and counted as an
  integrity failure instead of masquerading as data, which is precisely what AC #1 buys.
- Replace `Pipe<NoopRawMutex, _>` with `Pipe<CriticalSectionRawMutex, _>`, stored as a plain
  `static LOG_PIPE: Pipe<...> = Pipe::new()` (§2), deleting the `Option`/`&raw mut`/`pipe()` aliasing
  hack and the `static_mut_refs` allow.
- **Raise `LOG_PIPE_SIZE` 512 → 2048.** Whole-record-or-nothing turns a nearly-full ring into drops, and
  2 KB out of 512 KB is free. We will learn the true drop rate from the new counters rather than guess.
- Max frame is **228 B** (28 fixed + 200 body), so the ring always holds at least 8 maximum-size records;
  the locked region is bounded by construction (~200 B of formatting + ~228 B of CRC + ≤2 memcpys).
  Document that bound in the code. If that IRQ-off window ever measures too long against the 667 µs audio
  block, the fix is a reserve/commit ring (printk-style) replacing the `Pipe` — record the measurement in
  the child's notes, do not silently redesign.
- **One buffer, not two.** Rename `FORMAT_BUF: [u8; 256]` to `FRAME_BUF` and build the record where it
  will be copied from: format the body at offset 21, sanitise it in place, then write the fixed-width
  prefix backwards into `[0..21]` and the trailer after it. The prefix widths are fixed, so none of it
  needs the body's length first, and 21 + 200 + 7 = 228 ≤ 256 keeps the arithmetic provable by looking.
  A separate body buffer plus a second copy would double the work inside the critical section for no
  gain. The buffer stays a single static, now guarded by the same lock as the commit — that is the whole
  of AC #3: format, frame, check and commit are one indivisible region.
- **Read the clock before taking the lock.** `embassy_time::Instant::now()` inside the critical section
  would nest a driver's own locking inside our IRQ-off window for no benefit; a millisecond of skew
  between reading the clock and stamping the record is irrelevant next to a 1 ms field. Everything else
  that must agree with wire order — `CONSOLE_SEQ`, the frame, the space check, the commit — stays inside.
- **Waking the drain task happens inside the lock and that is fine.** `try_write` calls the reader's
  `WakerRegistration::wake()`, which on `platform-cortex-m` pends an exception; with PRIMASK set it fires
  on exit, and the thread-mode executor cannot preempt itself either way. What is *not* fine is awaiting
  anything inside the lock, or logging from inside it — `usb.rs`'s own `log::info!` calls stay outside.
- Counters live in independent `static AtomicU32` (`Ordering::Relaxed` is enough — single core, and they
  exist to be snapshotted by the emitter without taking the record lock): `CONSOLE_SEQ`,
  `CONSOLE_RECORDS_SENT`, `CONSOLE_DROPPED_FULL`, `CONSOLE_BYTES_DROPPED`, `CONSOLE_TRUNCATED`,
  `CONSOLE_ENDPOINT_ERRORS`.
  They must be incrementable **without needing buffer space**, otherwise the report of a drop can itself
  be the thing that fails.
- **STATUS emission:** from the drain task, at most once per second, and only when a counter changed
  since the last emission. Hard debounce matters: during a full-ring condition the status record competes
  for the very space that is missing, and a drop storm would then starve real logs. Seq gaps carry the
  truth between emissions.
- **Panic path:** `panic_handler.rs` formats into its own 128 B buffer and calls `usb::emit_blocking`.
  Give the panic record the same v1 framing (build it with the shared `CONSOLE_SEQ`, uncontended once the
  executor is halted) and chunk the framed bytes to `MAX_PACKET_SIZE` (64) as `emit_blocking` already
  does. Must not take the record lock, must not allocate, must not panic. Result: a `PANIC:` line still
  reads as text, now with a valid CRC, so `README.md:194-199`'s procedure keeps meaning what it says.
- **Drain loop** (`usb.rs:213-247`): read up to 256 B per wakeup and `write_packet` in ≤64 B chunks (the
  endpoint rejects oversize writes as `BufferOverflow`, which the current code misreads as a disconnect —
  the comment at `usb.rs:27-33` records that lesson; keep the invariant, just stop reading only 64 B).
  Replace the empty-ring `yield_now()` spin with `pipe.read(&mut buf).await`, which sleeps until there is
  a byte: that removes the only thing currently stealing executor time from the audio loop, and it is
  likely the poll-rate ceiling TASK-027.02 is chasing. Never hand `write_packet` a zero-length chunk.
  Bytes pulled from the ring just before a disconnect are lost with the failed write — at most one 256 B
  read per reconnect — and the STATUS counters make that visible rather than silent.
- **If the loop ever has to go:** `Channel<CriticalSectionRawMutex, Frame, N>` (§2 probe) makes atomicity
  structural instead of argued, hands the drain loop whole records, and costs one ~228 B stack local per
  `log!` call plus a copy — which is why the byte `Pipe` wins today. Do not switch on taste; switch on a
  measured failure of the argument above, and say so in the ticket.
- AC #6 is satisfied by construction: the facade signature and the `info!` macro surface do not change.
  Verify with `git diff --stat firmware/` showing no call-site edits.

## 5. Drop accounting (AC #4)

| Counter | Incremented when | Reported |
|---|---|---|
| `dropped_full` | a framed record did not fit the ring | STATUS, and implied by seq gaps |
| `bytes_dropped` | ditto, accumulated frame length | STATUS |
| `trunc` | a body exceeded the 200-byte cap; the record still shipped, shortened (§3) | STATUS |
| `ep_err` | `write_packet` returned `Err`, in drain or panic path | STATUS |
| `sent` | a frame was committed to the ring | STATUS (cross-check against host's decoded count) |
| host-side bad frames | reader's own decode statistics | rig artifact (TASK-031) |

Both sides are needed and neither is sufficient: the device knows what it lost before the wire, the host
knows what it lost on the wire. TASK-031's artifact reports both; TASK-033's bar is zero host-side
failures against the 8.8% baseline.

## 6. Sub-ticket map and order

| Child | Owns | ACs served | Planned? |
|---|---|---|---|
| `TASK-030.01` | `frame.rs`: pure codec, encoder, incremental decoder, proptest suite, `examples/console_decode.rs` | #1, #5 | yes — planned this pass |
| `TASK-030.02` | device path: real mutex, atomic-or-drop, seq/counters, BOOT+STATUS, panic framing, drain fix | #2, #3, #4, #6 | yes — planned this pass |
| `TASK-030.03` | `docs/reference/daisy-seed3.md` §Debugging without a probe + `README.md` | #7 | yes (mechanical, derivable from §3) |
| `TASK-030.04` | `@human` bench smoke check: legibility, zero bad frames, no audio regression | final evidence for #1–#4 | n/a (human) |

Execution order `01 → 02 → {03, 04}`. Dependencies are set accordingly (parent depends on all four;
`02` on `01`; `03`/`04` on `02`).

`.01` and `.02` were left unplanned by the first planning pass and are planned in the second one, because
the parent already pins the wire format normatively: splitting that spec across two planner sessions that
cannot see each other is how an encoder and a decoder drift apart, which is the failure this ticket
exists to prevent. Their plans restate §3/§4 only where an implementer needs it spelled out; **this file
stays the normative copy**, and any amendment a child discovers belongs here as well as there.

Why `030.01` is separate: the codec is the only part that must be trusted and the only part that is pure.
Splitting it means its property tests gate everything else, and TASK-031/032 get a decoder they can link
against rather than reimplement. Why `030.04` exists even though TASK-033 is the real hardware proof:
TASK-033 additionally waits on TASK-032's control channel. If v1 is unreadable in a terminal or costs the
audio loop, we want to know after one flash, not after the rig runner and the command protocol are built
on top of it.

## 7. Gates (every child, no exceptions)

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings
cargo test --workspace
cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --features seed3 --release
```

Compiling is not evidence. For this ticket the evidence is: property tests that refuse forged records,
and the numbers recorded by `TASK-030.04`.

## 8. Risks carried

1. **Locked region vs audio deadline.** Bounded at ~200 B of work per record; if TASK-030.04 hears
   problems, the answer is a reserve/commit ring, recorded as a follow-up ticket, not a quiet redesign.
2. **`static Pipe` might not satisfy some trait** in a way the probe missed (probe used host + std
   critical-section). Now checked directly for both candidates: `static Pipe<CriticalSectionRawMutex, N>`
   and `static Channel<CriticalSectionRawMutex, Frame, 16>` compile clean for `thumbv7em-none-eabihf`
   through `&self` alone (`/tmp/chprobe`). If anything still bites, `StaticCell<Pipe<...>>` + `split()`
   (`pipe.rs:353`, `Writer`/`Reader` are `Copy`) is the fallback. Either way the `&'static mut` aliasing
   hack dies.
3. **Format churn cost.** If TASK-031/032 find v1's fields insufficient (e.g. sub-millisecond `t_ms`),
   changing the grammar invalidates captures. Mitigation: `~` is v1 and the BOOT banner carries `proto=1`;
   a v2 changes the leading byte, so old captures stay decodable.
4. **Two tickets edit `lib.rs`** (this one and TASK-036's RTT feature). Sequence TASK-036 after this one
   or expect a conflict in the backend enum.
5. **The 8.8% figure is a hand tally** whose raw capture no longer exists on disk
   (`crates/asperitas-pod/src/encoder.rs:654`). It is a baseline, not a reproducible measurement — which
   is itself part of the argument for TASK-031.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Parked by an execution run that was assigned this ticket while it sat in Dev Ready as @agent.

Evidence for the park, from the files rather than ticket statuses:
- This umbrella's ACs #1-#7 are all satisfied by children (.01 codec, .02 device path, .03 docs); the plan at section 6 says 'no code lands here'. An executor picking up 030 has no file to edit.
- TASK-030.04 is @human (board, terminal, ears) and is one of the four recorded dependencies, so 030 cannot reach Done without a person. Per the parent-inherits-strictest-assignee rule the assignee was wrong: fixed from @agent to @human here, which also stops another agent run being handed a ticket it can only spin on. That is the same shape as the TASK-004 stall recorded in CLAUDE.md.
- Dependencies were already recorded (030.01-.04), so `backlog task list -s Blocked --ready` will surface this correctly once they close.

Next actionable step, in order: TASK-030.01 (ready now, `backlog task list --ready`), then .02, then .03. When .01-.03 are Done, flip 030 back to To Do for the human: only .04 remains and its acceptance criteria are HUMAN-prefixed.

Also committed here: planning pass 2's edits to this file and the four child ticket files were still untracked/uncommitted in the working tree (auto_commit is off), so nothing had them.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-09 04:05
---
Planning pass 2: all four children now carry plans and the 'planned' label (.01 codec, .02 device path, .03 docs, .04 human bench check), so nothing here needs another /backlog-planner run before execution. Found and fixed a defect in pass 1's pinned design: Pipe::try_write short-writes at every ring wrap even when the ring is empty (RingBuffer::push_buf returns only the contiguous run; free_capacity reports total), so 'check capacity then write once' would still truncate records. Measured on host against real embassy-sync 0.6.2 Pipe<_,512>: ring empty, free_capacity()==512, a 200-byte write accepted 112. That mechanism also fits the observed loss rate better than buffer pressure - one truncation per wrap is ~1 record in 10 for ~50 B lines over a 512 B ring, against the 8.78% tally in TASK-018.04. Section 2 now records the experiment (/tmp/pipecheck2, 20k randomized rounds of the corrected commit helper, all frames byte-exact), section 4 pins pre-check + bounded write loop as the fix with Channel<CriticalSectionRawMutex,Frame,N> kept as the documented fallback, and max frame is corrected to 228 B (pass 1 said 229). Spec amendment: bodies shortened by the 200-byte cap get their own 'trunc' counter instead of being counted as dropped records, so dropped_full keeps meaning exactly one thing.
---

created: 2026-09-09 05:42
---
Amendment from planning TASK-030.01 (every number below was re-derived and checked by mutation experiments; details in TASK-030.01.01/.02):

1. §3's illustrative `*2f9e` is wrong. Correct frames under the pinned parameters (`crc16_ccitt(b"123456789") == 0x29b1`): `~I 00000042 00004567 ENC +1*9c17\r\n` (34 B), empty body `~D 00000000 00000000 *91d4\r\n` (28 B), max-size body ends `*6c90`. TASK-030.03 must use these in docs/reference/console-protocol.md, not the ones currently in §3.
2. §3's sentence "a corrupt stream cannot produce a spurious `~` because payloads are tilde-free by construction" is false: sanitisation maps only bytes < 0x20 and 0x7F, so `~`, `*`, `|` survive inside bodies. Framing strength comes from the rigid grammar plus the CRC and, above all, the no-CR/LF invariant. Measured: exhaustive single-byte mutation over three frame shapes (~75 000 cases) accepts **zero**; weight >= 2 payload mutations reject > 99 % (about 1/232 cancellation classes exist in principle because CRC-16 is linear).
3. Two limits §3/§5 should state, because they change what a capture summary can claim: a record whose leading `~` was lost produces **no** `bad_frames` at all (only a `seq` gap or a `STATUS` counter reveals it), and a byte-level splice between two producers legitimately yields two valid records plus one integrity failure. The guarantee is that no record is ever invented, not that nothing decodes.
---
<!-- COMMENTS:END -->
