---
id: TASK-030.01
title: Add a host-testable self-verifying console frame codec with property tests
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 03:24'
updated_date: '2026-09-09 04:04'
labels:
  - planned
dependencies: []
modified_files:
  - crates/asperitas-logging/src/frame.rs
  - crates/asperitas-logging/Cargo.toml
  - crates/asperitas-logging/examples/console_decode.rs
parent_task_id: TASK-030
priority: high
type: task
ordinal: 48500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: TASK-030. **Read TASK-030's implementation plan first** — §2 pins facts already verified (so do not
re-research them) and §3 pins console protocol v1 byte for byte. This ticket implements §3's codec and
nothing else.

Deliver a pure, host-testable module `crates/asperitas-logging/src/frame.rs` that turns a log record into
one framed line and turns an arbitrary byte stream back into validated records. It must compile with the
crate's **default** features — no `log-usb`, no `embassy-*`, no `cortex-m`, no hardware types — because
the root workspace builds and tests this crate on the host (`members = ["crates/*"]`, `firmware/` excluded),
and CI's `cargo test --workspace` is what will run these tests. The precedent to copy is
`crates/asperitas-pod`: pure logic ungated, hardware behind a feature.

The encoder takes `(level, seq, now_ms, body)` from the caller rather than reading clocks or counters
itself, so the device path (TASK-030.02) owns sequencing and timing and the codec stays deterministic.
The decoder is incremental: `push(&mut self, bytes: &[u8])` may be called with any chunking, including a
record split across ten calls, and yields only records whose framing *and* CRC both validate, while
accumulating statistics (records decoded, integrity failures, resynchronisations, bytes discarded).
Corruption must cost one record, never the stream: on failure, discard one candidate start and retry at
the next `~`.

Also deliver `examples/console_decode.rs`, a std-only filter that reads raw capture bytes from a file or
stdin, prints each decoded record, and ends with the summary counts. That example is how a human checks a
capture before TASK-031's rig runner exists, and TASK-031 should reuse the decoder rather than write its
own.

Property tests are the point of this ticket, not a formality: `proptest 1.11.0` is already used by
`crates/asperitas-dsp/tests/property_tests.rs`. The set must include forged-record detection — truncation
at every offset, single-byte mutation in every field position, two producers' output interleaved into one
stream, records concatenated with their delimiter removed — and must prove the decoder never emits a
record whose text differs from what was encoded.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 crates/asperitas-logging/src/frame.rs compiles with the crate default features (no log-usb) and references no hardware, embassy, or cortex-m type, so cargo test -p asperitas-logging exercises it on the host.
- [ ] #2 The encoder turns (level, seq, now_ms, body) into exactly one record in the TASK-030 §3 v1 grammar, appending CRC-16/CCITT-FALSE over the documented byte range; a unit test pins the 123456789 -> 0x29b1 check vector.
- [ ] #3 Bodies are sanitised so CR, LF, other control bytes and 0x7F cannot forge a delimiter, and are capped at 200 bytes: an over-long body ships shortened with a valid CRC and sets the truncated flag the caller counts as `trunc`, never as a dropped record and never as an unterminated record.
- [ ] #4 The decoder accepts arbitrary chunk boundaries across calls and yields only records whose framing and CRC both validate, exposing counts of decoded records, integrity failures, resynchronisations, and discarded bytes.
- [ ] #5 proptest coverage includes round-trip identity for arbitrary bodies, never panicking on arbitrary input bytes, truncation at every offset, single-byte mutation in every field position, records concatenated with the delimiter removed, and two producers interleaved into one stream: no forged record is ever accepted as valid.
- [ ] #6 examples/console_decode.rs reads raw capture bytes from a file or stdin and prints decoded records plus summary counts, working with no board attached.
- [ ] #7 cargo fmt --all --check, both clippy invocations with -D warnings, and cargo test --workspace pass.
- [ ] #8 frame::write_whole is the single whole-record-or-nothing commit helper shared by the device and the tests: it refuses a frame larger than the reported free capacity without writing any bytes, and a host test drives it against a real embassy-sync Pipe<NoopRawMutex, 512> across the ring-wrap condition (fill 400, drain 400, commit 200) asserting byte-exact concatenation of committed frames over randomized producer/consumer rounds.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# Plan — console protocol v1 codec (`frame.rs`), host-tested

Parent TASK-030 §3 is the normative wire spec; §2 lists facts already verified so nobody re-researches
them. This ticket builds **only** the pure codec plus its tests and the decode example. No device code,
no `embassy-*`, no `cortex-m`, no hardware type may appear in `frame.rs` — AC #1 exists because CI runs
`cargo test --workspace` on the host with this crate's *default* features (`log-usb` off).

Work order is red-green: constants + CRC first, encoder second, decoder third, properties last. Each step
lands with its tests passing before the next starts.

## 1. Files and manifest

- New `crates/asperitas-logging/src/frame.rs`, declared `pub mod frame;` in `lib.rs` **outside** every
  `#[cfg]` (it must build with default features).
- New `crates/asperitas-logging/examples/console_decode.rs`.
- `crates/asperitas-logging/Cargo.toml`: add
  `[dev-dependencies] proptest = "1"` (matching `crates/asperitas-dsp/Cargo.toml:14`) and
  `embassy-sync = "0.6"` — dev-only, host-only, and *independent of the optional `log-usb` dependency*,
  which is what lets a host test drive a real embassy ring buffer (§5). Copy `Cargo.lock` changes as
  cargo writes them; `firmware/Cargo.lock` should not move (dev-deps don't reach the firmware workspace —
  confirm with `git diff --stat`).
- Do **not** add `crc` or `cobs`: parent §2 says both are absent from the lockfiles and CRC is ~15 lines.

## 2. Public surface (pin these names; TASK-030.02 consumes them verbatim)

```rust
pub const MAX_BODY: usize = 200;          // body cap, per parent §3
pub const PREFIX_LEN: usize = 21;         // '~' level SP seq(8) SP t_ms(8) SP
pub const TRAILER_LEN: usize = 7;         // '*' crc(4) CR LF
pub const MAX_FRAME: usize = PREFIX_LEN + MAX_BODY + TRAILER_LEN;   // == 228, assert it

/// One encoded record. `truncated` is true when the body did not fit MAX_BODY
/// and was shortened — the caller counts that as `trunc`, not as a dropped record.
pub struct Encoded { pub len: usize, pub truncated: bool }

pub fn encode(level: log::Level, seq: u32, now_ms: u32, body: &[u8],
              out: &mut [u8; MAX_FRAME]) -> Encoded;

pub fn crc16_ccitt(data: &[u8]) -> u16;

/// Whole-record-or-nothing commit into ANY byte sink. Lives here rather than in
/// the device path so the host can exercise the real algorithm (§5).
pub fn write_whole(frame: &[u8], free_capacity: usize,
                   write: impl FnMut(&[u8]) -> Option<usize>) -> bool;
```

`encode` takes `out` from the caller instead of owning a static: the device passes its one guarded static
buffer (TASK-030.02 §"one buffer"), tests pass stack arrays, and `frame.rs` needs no `unsafe`, no statics,
and no interior mutability. `now_ms` is `u32` milliseconds since boot.

`write_whole` has exactly two outcomes and no third: `false` means *refused, zero bytes written*; `true`
means every byte is in the sink. A mid-loop stall (`Ok(0)` or `Some(0)`, or `None` after progress) is not a
runtime case to handle — it is impossible given the pre-check and the caller's lock (§parent 4), so make it
a `debug_assert!` and let release builds finish the loop. Keep the function pure over its closure: no
statics, no `embassy` types in its signature, so the host test can supply a real `Pipe` through the closure
while the device supplies its own.

Level mapping: `Error→'E'`, `Warn→'W'`, `Info→'I'`, `Debug→'D'`, `Trace→'T'`. Hex digits lowercase;
`t_ms` printed as 8 zero-padded decimal digits of `now_ms % 100_000_000` — pin that modulo in code and in
a comment, because a raw `u32` of milliseconds exceeds 8 digits after ~100 000 s and would break the fixed
width silently. Document that neither field can reveal a wrap on its own; continuity comes from `BOOT`
plus `seq`.

Sanitise while copying the body: any byte `< 0x20` or `== 0x7F` becomes `'_'`; bytes `>= 0x80` pass through
untouched so UTF-8 survives. Cap at `MAX_BODY` **before** computing the CRC.

CRC covers exactly `out[1..PREFIX_LEN + body_len]` — everything after the leading `~` up to but not
including `*`. Table-less bit loop (poly `0x1021`, init `0xFFFF`, non-reflected, `xorout` 0): ~15 lines,
no flash cost worth counting, and identical source on both ends of the link. Unit-test the check vector
`b"123456789"` ⇒ `0x29b1`.

## 3. Decoder

`Decoder` is incremental and never panics on any input. Keep the storage inline so it needs no allocator:

```rust
pub struct Decoder { /* buf: [u8; MAX_FRAME], len, pending record copy, stats */ }
pub struct Record<'a> { pub level: u8, pub seq: u32, pub t_ms: u32, pub body: &'a [u8] }
pub struct Stats { pub records: u64, pub bad_frames: u64, pub resyncs: u64, pub discarded_bytes: u64 }

impl Decoder {
    pub const fn new() -> Self;
    pub fn push(&mut self, bytes: &[u8]);            // feed any chunking
    pub fn next_record(&mut self) -> Option<Record<'_>>;  // one outstanding record at a time
    pub fn stats(&self) -> Stats;
}
```

`next_record` copies the validated body into the decoder's own record slot and returns a borrow of it, so
callers use one record before asking for the next. That is deliberate: it avoids returning a
self-referential struct and avoids making callers supply a buffer, and both the example and the tests want
exactly this shape.

### The parse rule that makes resynchronisation provable

A well-formed body cannot contain CR or LF (§3 sanitisation), and the fixed-width prefix contains no CRLF
either. Therefore **the first CRLF after a candidate start terminates that candidate**, and a parse that
fails against that candidate can never succeed later — the start is disqualified permanently, forever. Same
for a candidate that reaches `MAX_FRAME` without a CRLF: no valid record is longer. Consequences to state
in a code comment, because they are what make the decoder O(n) rather than quadratic:

1. Accumulate until a CRLF arrives at index ≥ `PREFIX_LEN + TRAILER_LEN - 1`… simply: attempt the parse as
   soon as a CRLF shows up at index `i`, and separately give up when `len == MAX_FRAME`.
2. On failure, count one `bad_frames`, then advance the start to the **next `~` inside the examined span**
   and retry it — each such start is also permanently disqualified by rule above. If the span holds no
   further `~`, discard the whole span (`discarded_bytes += span`) and wait for a fresh `~`.
3. Never repair, never guess a shorter body, never accept a record whose CRC does not match. Corruption
   costs the hit record, not the capture.
4. A successful parse emits the record and memmoves the remainder of the window to the front.

Field parsing after a candidate start: fixed offsets, so `body = out[PREFIX_LEN..i]` where `i` is the CRLF
index; verify `out[i]=='\r'`, `out[i+1]=='\n'`, `out[i+2-7]=='*'` i.e. the `*` sits exactly 6 bytes before
end-of-CRLF... derive it from `i` explicitly and reject anything that does not line up, plus non-hex CRC
digits and a wrong CRC. `records` counts only frames that pass framing **and** CRC.

## 4. `examples/console_decode.rs`

std-only binary, no board: read a file given as `$1` or stdin if absent, feed it through `Decoder` in
fixed chunks (say 4 KiB — deliberately larger than `MAX_FRAME` so chunk boundaries never coincide with
record boundaries by luck), print each `Record` as `~L seq t_ms body *crc` reconstructed from fields, and
finish with the four `Stats` counters on stderr. Exit status 0 always — this tool reports, it does not
judge; TASK-031 will decide what constitutes a failed capture. It must be clippy-clean under
`--all-targets`, which CI applies.

## 5. Tests — the point of the ticket

Put unit tests in `frame.rs` (`#[cfg(test)]`) and the property suite in
`crates/asperitas-logging/tests/console_frame.rs`, following the idiom already used at
`crates/asperitas-dsp/tests/property_tests.rs:30-41`. Name every adversarial case `rejects_<violation>`.

Required cases, mapped to the ACs:

- **Golden vectors:** CRC check vector; one hand-written expected encoding, byte-for-byte, including the
  exact 21-byte prefix widths.
- **Round-trip identity** (proptest, arbitrary bodies incl. control bytes and high bytes): encode →
  decode → same level, seq, t_ms, and body equal to the *sanitised, capped* input; `records == 1`,
  `bad_frames == 0`.
- **Never panics** (proptest): arbitrary `Vec<u8>` fed in arbitrary chunkings yields only typed results.
- **Truncation at every offset:** take one known-good encoding and feed `bytes[..k]` for every `k`; assert
  no record is emitted for any `k < len`, and exactly one for `k == len`.
- **Single-byte mutation in every field position and every bit position**: assert either rejection or an
  integrity failure — a mutated frame must never decode as a *valid* record with the original CRC passing.
  (A mutation can coincidentally produce a different valid CRC; assert the weaker, honest property
  `decoded.seq/body == input` fails, and record the ~1/65536 escape rate in a comment.)
- **Delimiter forgery:** bodies containing `\r\n`, `~`, `*`, all 32 low control bytes, `0x7F`, lone `\r` —
  assert the stream still decodes to exactly the intended records and the forger is neutralised by `_`.
- **Concatenated records with the delimiter removed** between them, and records split across ten `push`
  calls, decode identically to the same bytes delivered whole.
- **Two producers interleaved:** two encoders writing alternating records into one stream; assert the
  decoder emits both sequences intact in byte order, and that a *byte-level* interleave (a producer's
  output spliced into the middle of another's) is reported as failures, never as two plausible records.
- **Garbage prefix / embedded `~`:** leading junk, and a body containing a literal `~` after sanitisation
  is impossible — assert a stray `~` in the middle of a corrupt region only causes resync, never a
  false-valid record.
- **`write_whole` never partially commits** (AC #2's mechanism, tested against the real library): drive it
  with a closure over a real `embassy_sync::pipe::Pipe<NoopRawMutex, 512>` — dev-dependency, host target,
  no `log-usb` needed. Reproduce the wrap-boundary condition from parent §2 (write 400, drain 400, then
  commit a 200-byte frame) and assert full acceptance; then run randomized producer/consumer rounds
  asserting the concatenation of committed frames equals the bytes read back, exactly as
  `/tmp/pipecheck2` did. Include the negative case: a frame larger than `free_capacity` is refused with
  nothing written.

## 6. Gates

`cargo fmt --all --check` · `cargo clippy --workspace --all-targets -- -D warnings` ·
`cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings` ·
`cargo test --workspace` · firmware cross-compile unchanged (`cargo build --manifest-path
firmware/Cargo.toml --target thumbv7em-none-eabihf --features seed3 --release`) — this ticket must not
break it even though it does not touch the device path.

Compiling is not evidence: the deliverable is a property suite that refuses forged records. Record in the
ticket notes the proptest case counts and any case you had to weaken, and why.
<!-- SECTION:PLAN:END -->
