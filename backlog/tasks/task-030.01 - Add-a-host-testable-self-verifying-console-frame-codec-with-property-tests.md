---
id: TASK-030.01
title: Add a host-testable self-verifying console frame codec with property tests
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-09 03:24'
updated_date: '2026-09-09 05:43'
labels:
  - planned
dependencies:
  - TASK-030.01.01
  - TASK-030.01.02
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
# Plan — codec decomposition, execution order, and integration verification

Parent **TASK-030** §2 (verified facts) and §3 (console protocol v1) are normative; read them first. This
ticket was left `Dev Ready` by an earlier planning pass with a plan that was wrong in four load-bearing
places, listed below so nobody re-imports them. The detail now lives in two agent-owned children.

## Why it is split, and where the seam is

No human step exists here: everything runs on the host, and the only hardware-facing consequence (the locked
region grows by one CRC computation) is measured by the already-`@human` TASK-030.04.

The seam is the one the codebase already draws — producer vs consumer, exactly as `crates/asperitas-pod/src/encoder.rs`
separates quadrature encode from decode:

| child | owns | unblocks |
|---|---|---|
| **TASK-030.01.01** | constants, `crc16_ccitt`, sanitise/cap, `encode`, `write_whole`, `[dev-dependencies]`, inline unit tests | TASK-030.02 (its AC #3 names only these four items) |
| **TASK-030.01.02** | `Decoder`, `Record`, `Stats`, `examples/console_decode.rs`, `tests/console_frame.rs` | TASK-031, TASK-032, TASK-033 readers |

Strictly ordered: `.01.02` needs `.01.01`'s `encode` to generate test input, so `.01.02` declares the
dependency rather than carrying a duplicated encoder fixture. Nothing else runs in parallel.

## Corrections to the earlier version of this plan

All four were found by building a model of the protocol and running mutation experiments against it; each is
now written into the child that has to obey it.

1. **Every checksum in the old plan was fabricated**, including the illustrative `*2f9e` in TASK-030 §3's
   example line. Re-derived with the pinned parameters (`poly 0x1021`, `init 0xFFFF`, non-reflected,
   `xorout 0`, check `0x29b1`) and verified independently: `~I 00000042 00004567 ENC +1*9c17\r\n` (34 B),
   `~D 00000000 00000000 *91d4\r\n` (28 B), max body ⇒ `*6c90`. Golden vectors live in `.01.01` §4.
2. **The decoder index arithmetic was inconsistent.** Pin it to one derivation from `j` = CR's absolute index:
   `'*'` at `j-5`, CRC digits `j-4..j`, body `start+21 .. j-5`, CRC covers `start+1 .. j-5`, minimum relative
   `j-start` is 26, and the identity `MIN_CR_OFFSET + MAX_BODY + 2 == MAX_FRAME == 228`. The old guard
   (`i + TRAILER_LEN > MAX_FRAME`) rejected **every max-size record**; the old body slice also mis-decoded any
   body containing `'*'`, because it located `'*'` by searching forward instead of deriving it from `j`.
3. **"A corrupt stream cannot produce a spurious `~`" is false, and the escape hedge was misestimated.** A
   tilde-free payload is not guaranteed by sanitisation — `~`, `*`, `|` are printable and survive it. Framing
   strength comes from the grammar plus the CRC and, above all, the no-CR/LF invariant. Weight-1 corruption
   was then checked exhaustively (10 455 / 58 140 / 7 140 single-byte mutations across three frame shapes):
   **zero** accepted, so the "~1/65536" guess is replaced by proof for weight-1 and an honest measured floor
   (>99 %) for weight ≥ 2, where linear-CRC cancellation genuinely exists (~1/232 in principle).
4. **Stats were under-specified, which is what would have made the capture summaries untrustworthy.** Two
   rules close it: every decision may use only bytes already offered (the old "resync at the next `~`" rule
   looked ahead across the whole stream, so statistics changed with chunk boundaries), and `finish()` must be
   called before reading `Stats`, otherwise a capture ending in a half-record or unframed text reports
   `discarded_bytes = 0` — precisely the lie TASK-030 exists to prevent. The accounting law
   `bytes_pushed == consumed + discarded_bytes + buffered()` holds after every push; canonical per-case
   numbers are tabulated in `.01.02` §4.

Two further limits belong in the module docs, not just tests: a record whose leading `~` vanished yields
**no** `bad_frames` (only `seq` gaps or a `STATUS` counter reveal it), and a byte-level splice between two
producers legitimately yields two good records plus one integrity failure — the guarantee is that no record
is ever *invented*, not that nothing decodes.

## Execution order

**This ticket holds no implementation of its own.** An executor that picks it up must not write `frame.rs`
here: it must carry `.01.01` and then `.01.02` to Done, and close this ticket only once both are, since its
acceptance criteria are exactly those two children plus the gates below.

1. **TASK-030.01.01** — red-green: constants + CRC (one-line test), then `encode` against the golden frames,
   then `write_whole` against a real `Pipe<NoopRawMutex, 512>` driven across the ring wrap.
2. **TASK-030.01.02** — port the pinned parse rules verbatim from its §2, get the canonical table green, then
   add the property suite, then the example.

## Integration verification (what closes this ticket)

- `cargo test -p asperitas-logging` runs both halves' tests under **default features** (`log-usb` off), which
  is how CI invokes it; `frame.rs` names no cortex-m/embassy type outside `[dev-dependencies]`.
- `cargo clippy --workspace --all-targets -- -D warnings` twice and `cargo test --workspace` twice
  (`.github/workflows/ci.yml:19-38`), plus the firmware cross-compile, with **no diff in
  `firmware/Cargo.lock`** (parent §4 risk 6).
- `cargo run -p asperitas-logging --example console_decode < raw_capture > decoded.txt` prints only validated
  records and a zero-loss-looking summary only when the counters say so; feed it a capture you corrupted on
  purpose and confirm the counters move.
- `backlog doctor` clean, and TASK-030.02 able to start on `MAX_*`, `Encoded`, `encode`, `write_whole` alone.

Compiling is not evidence. The evidence is: five byte-exact golden frames, a truncation test at every offset,
exhaustive single-byte mutation that never decodes, chunk-independence, the canonical counter table, and a
`write_whole` suite that drives a real embassy ring buffer across the wrap.
<!-- SECTION:PLAN:END -->
