---
id: TASK-030.01.02
title: >-
  Add the incremental console-frame decoder, the console_decode example, and the
  adversarial property suite
status: Backlog
assignee:
  - '@agent'
created_date: '2026-09-09 05:37'
updated_date: '2026-09-09 05:38'
labels:
  - task
  - planned
dependencies:
  - TASK-030.01.01
modified_files:
  - crates/asperitas-logging/src/frame.rs
  - crates/asperitas-logging/tests/console_frame.rs
  - crates/asperitas-logging/examples/console_decode.rs
  - crates/asperitas-logging/Cargo.toml
parent_task_id: TASK-030.01
priority: high
ordinal: 53500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent **TASK-030.01**; grandparent **TASK-030 §3** is the normative wire spec. Depends on
**TASK-030.01.01**, which lands the constants, `crc16_ccitt`, `encode`, sanitisation rules and the
`[dev-dependencies]` block; this child adds the consumer half of the same module.

Deliver the incremental decoder (`Decoder`, `Record`, `Stats`) in `crates/asperitas-logging/src/frame.rs`,
`examples/console_decode.rs` (a std-only filter a human can point at a raw capture before TASK-031's rig
runner exists), and the adversarial property suite in
`crates/asperitas-logging/tests/console_frame.rs`. The suite is the deliverable, not a formality: nothing
else in this repo proves that a corrupted capture cannot be mistaken for clean data, and TASK-031/032 link
against this decoder rather than writing their own.

The whole module stays pure and host-only: no allocator, no `unsafe`, no embassy/cortex-m type outside
`[dev-dependencies]`, default features only. The reference behaviour was modelled and verified during
planning (`/tmp/refmodel.py`, ephemeral — every one of its checks is reproduced by the Rust tests below); the
rules an implementer must match are pinned in the plan, including the canonical statistics table the tests
assert exactly.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `Decoder`, `Record` and `Stats` exist with exactly the names and signatures in the plan (`new`/`push`/`next_record`/`finish`/`buffered`/`stats`), use no allocator, no `unsafe`, and never hold more than `MAX_FRAME` undecided bytes.
- [ ] #2 Records **and** `Stats` are independent of chunk boundaries: the same byte stream split at random boundaries yields identical output (proptest over random chunkings).
- [ ] #3 The canonical statistics table in the plan is asserted exactly, one `#[test]` per row, including the two rows that encode known limitations (a record that lost its leading `~` reports zero `bad_frames`; a byte-level producer splice recovers two legitimate records plus one integrity failure).
- [ ] #4 The accounting law `bytes_pushed == consumed + discarded_bytes + buffered()` holds after every push and after `finish()`, and `finish()` is documented as required before reading `stats()` so trailing unframed text cannot be reported as zero loss.
- [ ] #5 The adversarial suite covers every case listed in the plan: truncation at every offset; exhaustive single-byte mutation over three frame shapes asserting nothing decodes at all; a weight >= 2 detection floor above 99 % with the linear-CRC escape rate stated honestly; delimiter forgery in bodies; missing delimiter between records; interleaved producers expected in stream order; byte-level splice; garbage prefix; embedded `~`, `*`, `|`; over-long body; and a never-invents-a-record check against known encodings.
- [ ] #6 `examples/console_decode.rs` reads a file or stdin in 4 KiB chunks, prints validated records verbatim to stdout and the four counters plus a seq-gap summary to stderr, always exits 0, and is clippy-clean under `--all-targets`.
- [ ] #7 Any shrunk proptest case is committed as `crates/asperitas-logging/proptest-regressions/console_frame.txt`; fmt, both clippy invocations with `-D warnings`, both `cargo test --workspace` invocations and the firmware cross-compile pass; the final summary states proptest case counts, any weakening applied and why, and the measured weight-2 detection rate.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# Plan — incremental decoder, decode example, adversarial property suite

Parent TASK-030 §3 is normative; TASK-030.01.01 has already landed `MAX_*`, `MIN_CR_OFFSET`,
`crc16_ccitt`, `Encoded`, `encode` and `write_whole`. Read that child's §2 derivation table first — every
offset here comes from it, and getting it wrong is the single most likely way to fail this ticket.

## 1. Public surface (TASK-031 and TASK-032 consume these names verbatim)

```rust
pub struct Record<'a> { pub level: u8, pub seq: u32, pub t_ms: u32, pub body: &'a [u8] }

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stats {
    pub records: u64,          // validated on framing AND CRC
    pub bad_frames: u64,       // candidate starts examined and rejected
    pub resyncs: u64,          // rejections after which a later start marker was located
    pub discarded_bytes: u64,  // bytes dropped that never reached a validated record
}

impl Decoder {
    pub const fn new() -> Self;
    pub fn push(&mut self, bytes: &[u8]);                 // any chunking, including mid-record
    pub fn next_record(&mut self) -> Option<Record<'_>>;  // one outstanding record at a time
    pub fn finish(&mut self);                             // abandon the pending window (see §3)
    pub fn buffered(&self) -> usize;                      // bytes held and not yet decided
    pub fn stats(&self) -> Stats;
}
```

Storage is inline (`buf: [u8; MAX_FRAME]`, `len`, a copied-out record slot, the counters) so the type needs
no allocator and can be constructed `const`. `next_record` returns a borrow of the decoder's own record slot:
callers use one record before asking for the next. That is deliberate — it avoids a self-referential return
type and avoids making every caller supply a buffer, and both the example and the tests want exactly this
shape. Document it on the type so nobody "improves" it into returning `Vec`s.

## 2. Parse rules that make resynchronisation provable

Two states only. **Every decision uses only bytes already offered**, which is what makes the output
independent of chunking — the property that a previous planning-pass formulation failed (its resync rule
looked ahead for "the next `~`" across the whole stream, so a split delivered different statistics than the
same bytes delivered whole).

```
push(bytes):
  while bytes remain:
      copy min(remaining, capacity - len) into buf        # never overwrite; buf holds ≤ MAX_FRAME
      scan_loop()

scan_loop():
  loop:
    if not in_candidate:                                  # ---------- SCAN for a start marker
        i = index of first b'~' in buf[len..] relative to current window
        if found:
            discarded_bytes += bytes skipped before it    # inter-record junk, counted once
            drop those bytes; in_candidate = true
            if last_rejection_was_followed_by_no_start_marker_yet: resyncs += 1
            continue
        else:
            if len >= MAX_FRAME:                          # no start marker can be missed: a valid
                discard the whole window                  # record needs '~', and none appeared
            return                                        # keep whatever is left, need more bytes

    match examine(buf[0 .. len]):                         # ---------- CANDIDATE
        NeedMore   -> return                              # undecided; keep everything
        Rejected   -> bad_frames += 1
                      discarded_bytes += 1                # the forfeited '~' itself
                      advance STRICTLY past that byte     # never guess a shorter body
                      in_candidate = false; arm resync; continue
        Accepted(rec, consumed) ->
                      copy fields + body into the record slot; records += 1
                      drop `consumed` bytes; in_candidate = false; continue

examine(buf):                                             # all offsets derived from j = index of CR
    q = first CRLF at or after index 1                    # requires buf[q+1] present
    if none, or LF not yet buffered:
        return Rejected if len >= MAX_FRAME else NeedMore # over-length is the ONLY give-up condition
    r = q                                                 # == j - start, see .01 §2 table
    reject unless MIN_CR_OFFSET <= r and r - MIN_CR_OFFSET <= MAX_BODY
    reject unless buf[q-5] == b'*'                        # '*' located FROM j, never by forward search
    reject unless buf[q-4 .. q] are 4 lowercase hex digits
    reject unless buf[1] in I W E D T and buf[2], buf[11], buf[20] are SP
    reject unless buf[3..11] hex and buf[12..20] decimal
    reject unless crc16_ccitt(buf[1 .. q-5]) == parsed hex
    accept with body = buf[PREFIX_LEN .. q-5], consumed = r + 2
```

Why this terminates and cannot wedge: a well-formed body contains no CR or LF (§3 sanitisation) and neither
does the fixed-width prefix, so **the first complete CRLF after a candidate start must be that record's
terminator** — a candidate that fails against it can never succeed later and is disqualified permanently.
Same for a window that reaches `MAX_FRAME` with no CRLF: nothing valid is longer. So each rejected byte is
dropped once, each `examine` inspects ≤ `MAX_FRAME` bytes, and cost per input byte is bounded independently
of history. Say exactly that in a comment — *not* plain "O(n)", which this is not in the worst case (many
`~`s inside one corrupt span each get their own ≤228-byte examination).

This is a refinement of parent §3's wording ("discard one candidate start and retry at the next `~`"): same
contract, but implemented as "advance strictly past the disqualified byte", which keeps the decision local
and therefore chunk-independent. Real parsers do exactly this — `rp-pps`'s assembler ("a mid-line `$`
resynchronises; an overflowing buffer is dropped"), `nmea0183-parser` (zero-alloc, framing separated from
content validation), `pamoja-mavlink` ("a stray byte or mangled frame makes the parser resynchronize on the
next start marker rather than wedge").

**The named hazard this defends against:** ArduPilot's C MAVLink parser desynchronised permanently when a
bad-CRC message ended in a byte equal to the STX magic, because resuming at "the next plausible-looking byte"
can land *inside* the next real frame
(<https://github.com/ArduPilot/pymavlink/issues/881>). Our defence is that resync advances strictly past the
disqualified start and never guesses, plus records are emitted only from fully validated frames.

## 3. Counters, and the accounting law that makes them trustworthy

Define them so each answers one question, then assert the law:

```
bytes_pushed  ==  Σ consumed (over emitted records)  +  discarded_bytes  +  buffered()
```

- `discarded_bytes` counts every byte dropped that did not reach a validated record: inter-record junk, the
  forfeited `~` of each rejected start, and windows abandoned because no start marker appeared. Bytes of a
  rejected candidate that are still buffered are *not* counted yet; they are counted when they are actually
  dropped, which is why the law holds after **every** push, not just at the end.
- `finish()` exists because the law alone is not enough for a capture summary: a stream that ends with a
  half-record or with unframed terminal text leaves those bytes in `buffered()` — and a summary printing
  `discarded_bytes = 0` there would be exactly the lie this whole ticket exists to prevent. `finish()` drops
  the pending window into `discarded_bytes`; `stats()` is only complete after it. Say that on the method.
- `resyncs` increments when a start marker is located after a rejection (not at rejection time), so it stays
  chunk-independent. `resyncs ≤ bad_frames`, differing only when corruption runs to the end of the capture.
- A record whose leading `~` was lost in transit produces **no** `bad_frames` at all — the decoder never saw
  a candidate; the bytes surface as `discarded_bytes`, and only a `seq` gap or a `STATUS` counter reveals it.
  Put that limitation in the doc comment: CRC verifies integrity, never absence. Continuity is `seq`/`BOOT`'s
  job (prior art: Serial Studio's "Checksums and CRC" on precisely this blind spot; QP/Spy's HDLC-style
  separate continuity and integrity fields; Fuchsia RFC-0079, which reports *bytes dropped* alongside log
  data — the same shape as our `STATUS dropped_full=`/`bytes_dropped=`).

## 4. Canonical statistics table — assert these exact numbers

Feed each stream whole, call `finish()`, compare all four counters. Every row was produced by the verified
reference model; if an implementation disagrees, the implementation is wrong until proven otherwise.

| stream | pushed | records | bad_frames | resyncs | discarded_bytes |
|---|---|---|---|---|---|
| 3 clean frames | 90 | 3 | 0 | 0 | 0 |
| `garbage\xff\x00` + 1 frame | 50 | 1 | 0 | 0 | 9 |
| 4 frames, one CRLF removed | 118 | 3 | 1 | 1 | 28 |
| CRC digits replaced by `ffff` | 42 | 0 | 1 | 0 | 42 |
| CRLF injected mid-record | 68 | 1 | 1 | 1 | 27 |
| two lone LFs, then a frame | 43 | 1 | 0 | 0 | 2 |
| frame + first 10 bytes of another | 51 | 1 | 0 | 0 | 10 |
| body holding `~`, `*`, `|`, `0x01`, `0x7F` | 43 | 1 | 0 | 0 | 0 |
| record that lost its `~`, then a frame | 72 | 1 | 0 | 0 | 42 |
| max frame (200-byte body) | 228 | 1 | 0 | 0 | 0 |
| plain terminal text, no frames at all | 35 | 0 | 0 | 0 | 35 |
| over-long body (201 raw bytes) | 229 | 0 | 1 | 0 | 229 |
| interleaved producers a0 b0 a1 b1 | 120 | 4 | 0 | 0 | 0 |
| producer B's bytes spliced inside producer A's record | 90 | 2 | 1 | 1 | 30 |

One row deserves a comment in the test: the last one recovers **two** legitimate records plus one integrity
failure — the spliced-in frame and the following frame are intact and contiguous, so emitting them is right;
what must never happen is inventing a record out of the splice. That is the honest reading of parent AC #5's
"never mistaken for a valid record", and it is why the suite asserts membership rather than "everything
failed".

## 5. Property suite — `crates/asperitas-logging/tests/console_frame.rs`

Idiom to copy: `crates/asperitas-dsp/tests/property_tests.rs` — `//!` header with a run hint, `// --- … ---`
section banners, helper fns returning `impl Strategy<Value = …>` with doc comments, `prop_assert!` with a
lowercase message naming index and values, early `return Ok(())` for degenerate cases. Name every adversarial
case `rejects_<violation>`. Size strategies **by count of chunks** (`vec(any::<u8>(), 1..40)` then chunk by a
separate count vector), not by bytes.

Commit `crates/asperitas-logging/proptest-regressions/console_frame.txt` when a case shrinks — officially
recommended and it keeps counterexamples replaying in CI
(<https://proptest-rs.github.io/proptest/proptest/failure-persistence.html>). Nothing in this repo commits
regressions yet, so create the directory deliberately and mention it in the final summary.

Required properties:

1. **`round_trips_arbitrary_bodies`** — arbitrary `Vec<u8>` (control bytes and high bytes included): encode →
   decode whole → one record, level/seq/t_ms equal and body equal to the **sanitised, capped** input;
   `records == 1`, `bad_frames == 0`, `discarded_bytes == 0`.
2. **`never_panics_on_arbitrary_bytes`** — random bytes in random chunkings yield only typed results; no
   panic, no unbounded buffering (`buffered() <= MAX_FRAME` after every push).
3. **`is_independent_of_chunk_boundaries`** — the same stream split at random boundaries gives identical
   records *and* identical `Stats` (this caught the planning-pass regression; keep it).
4. **`accounts_for_every_byte`** — the §3 law asserted after every push and after `finish()`, over mutated
   streams.
5. **`rejects_truncation_at_every_offset`** — one known-good encoding fed as `bytes[..k]` for **every** `k`:
   no record for any `k < len`, exactly one at `k == len`. Deterministic loop, not proptest.
6. **`rejects_every_single_byte_mutation`** — exhaustive over every position × all 256 values, on three frame
   shapes (28 B empty-body, 34 B normal, 228 B max): assert the mutated stream **never decodes at all** —
   zero records, not merely "different fields". Verified exhaustively during planning: 10 455 / 58 140 /
   7 140 mutations, zero accepted as the original *or* as any other record. No weakening is needed for
   weight-1; the grammar's rigidity plus the CRC closes it structurally. Do not soften this to the
   "~1/65536 escape rate" the earlier version of this plan hedged with — that figure was guessed.
7. **`detects_most_multi_byte_corruption`** — weight ≥ 2, payload-only substitutions: report the measured
   detection rate and `prop_assert!` a floor of > 99 % rejected. Measured ≈ 1/6 000–1/8 000 accepted as the
   original (0.012 %–0.025 % over 20 k–500 k trials). Explain in a comment why a floor rather than a proof:
   CRC-16 is linear, so double-substitutions at certain separations cancel algebraically — ~280 collision
   classes `(separation d ∈ 1..33, byte pair)` ⇒ ≈1/232 undetectable in principle — and anyone who can edit
   the four hex digits bypasses it by construction. State plainly that CRC verifies **integrity only**: a
   wholly missing record, or a writer who controls the checksum, is invisible to it.
8. **`neutralises_delimiter_forgery`** — bodies containing `\r\n`, `~`, `*`, `|`, all 32 low control bytes,
   `0x7F`, a lone `\r`: the stream decodes to exactly the intended records with the forger replaced by `_`.
   Note in the test name/comment that `~`, `*`, `|` **survive** sanitisation — the earlier claim that a body
   could not contain `~` was false, and the invariant the parse proof needs is only "no CR or LF".
9. **`rejects_missing_delimiter_between_records`** and **`reassembles_records_split_across_ten_pushes`** —
   both decode identically to the same bytes delivered whole.
10. **`emits_both_producer_streams_in_byte_order`** — two encoders writing alternating records. Expected
    order is `a0, b0, a1, b1, …` (stream order), **not** all of A then all of B; a planning-pass harness got
    this backwards and reported a false failure. Then the separate byte-level splice case above — keep them
    as two named tests, since only byte-level splicing can produce `bad_frames`.
11. **`never_invents_a_record`** — build streams by concatenating known frames, mutate up to two bytes, and
    assert every emitted record is byte-identical to one of the known encodings (compare the raw frame slice,
    not just fields). Verified over 300 mutated multi-producer streams during planning: zero inventions.
12. **Canonical table** (§4) as one `#[test]` per row.

Keep the CRC honest in the tests: compute expected values with `crc16_ccitt` where the point of the test is
framing, and use the pinned literals where the point of the test is the CRC itself. A test that recomputes
the checksum both ways proves nothing.

## 6. `examples/console_decode.rs`

std-only binary (the crate is `#![no_std]` but examples are ordinary bins — no conflict, and
`panic_handler.rs` exports `handle_panic()` rather than a `#[panic_handler]` item, so nothing collides).
Read the file named by `$1`, or stdin when absent, in fixed 4 KiB chunks — deliberately larger than
`MAX_FRAME` so record boundaries never coincide with chunk boundaries by luck. Print each decoded record's
**raw validated bytes verbatim** to stdout (greppable, lossless for UTF-8 bodies, and it does not require
re-deriving the checksum), then `finish()` and print the four counters plus a seq-continuity summary to
stderr: total records, `seq_gaps`, and the first gap's endpoints. Exit 0 always — this tool reports, it does
not judge; TASK-031 decides what constitutes a failed capture. Keep the seq-gap logic here rather than in the
library (a gap immediately after a `BOOT` record is a restart, not loss, and that judgement belongs to the
reader), and note that TASK-031 may lift it.

No flags, no configuration parameters: one behaviour, chosen here. Must be clippy-clean under
`--all-targets`, which CI applies twice.

## 7. Gates (identical to CI, `.github/workflows/ci.yml:19-38`)

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings
cargo test --workspace
cargo test --workspace --features asperitas-pod/pod-hw
cd firmware && cargo build --release --features seed3
```

Compiling is not evidence: record in the final summary the proptest case counts you ran, any case you had to
weaken and why, and the measured weight-2 detection rate. If a canonical-table row will not match, stop and
re-read §2/§3 — the last planning pass lost time to exactly that (an off-by-one guard of
`r + TRAILER_LEN > MAX_FRAME` silently rejected every max-size record; the correct guards are
`r >= MIN_CR_OFFSET` and `r - MIN_CR_OFFSET <= MAX_BODY`).
<!-- SECTION:PLAN:END -->
