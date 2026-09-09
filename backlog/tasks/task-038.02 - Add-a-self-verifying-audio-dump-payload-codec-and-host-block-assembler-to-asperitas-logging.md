---
id: TASK-038.02
title: >-
  Add a self-verifying audio dump payload codec and host block assembler to
  asperitas-logging
status: Blocked
assignee:
  - '@agent'
created_date: '2026-09-09 11:33'
updated_date: '2026-09-09 16:10'
labels:
  - planned
dependencies:
  - TASK-038.02.01
  - TASK-038.02.02
  - TASK-038.02.03
  - TASK-038.02.04
modified_files:
  - crates/asperitas-logging/src/dump.rs
  - crates/asperitas-logging/src/lib.rs
  - crates/asperitas-logging/src/frame.rs
  - crates/asperitas-logging/tests/console_dump.rs
  - crates/asperitas-logging/examples/dump_reassemble.rs
  - crates/asperitas-logging/Cargo.toml
  - Cargo.lock
  - .github/workflows/ci.yml
parent_task_id: TASK-038
priority: high
type: task
ordinal: 56500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Recorded audio has to leave the device through the console transport TASK-030 built, and that transport is printable-ASCII text with 200-byte bodies: `sanitize_byte` replaces every byte below 0x20, so raw sample bytes cannot travel. The frame grammar itself must not change — `MAX_BODY = 200`, `MAX_FRAME = 228` are compile-time asserted and shared by the encoder, the decoder, `PANIC_FRAME`, and `RecordBufs`, and widening them is a v2 wire change under TASK-030's own versioning rule. So this ticket adds a payload layer inside v1, not a new protocol.

Add a `dump` module to `crates/asperitas-logging` that compiles for both no_std (device producer) and host (test + example), defining two record bodies in the existing house style (`BOOT`/`STATUS` key=value precedent in `console.rs`):

- `AUDIO blk=<4hex> n=<2hex> c=<2hex> d=<base64>` — one chunk of one block: a constant 27-byte header plus a standard-base64 payload of up to 172 characters, carrying **129 raw bytes** in a 227-byte frame. Short keys and fixed-width hex are forced by arithmetic rather than taste: `MAX_BODY` bounds the whole body including keys, so the descriptive form this ticket originally specified (`block_index=/n_of_n=/chunk_i=`) delivered 111 raw bytes at 0.487 useful efficiency, and its claimed 150 was unreachable at any header length. See the budget table in Implementation Notes.
- `AUDEND blk=<4hex> n=<2hex> bytes=<dec> crc16=<4hex>` — closes a block; the CRC covers the **raw** concatenated bytes, which is what makes a lost whole *record* provable rather than merely suspicious. `frame.rs` documents that a record whose leading `~` is lost produces no integrity failure at all, so per-record CRC alone cannot prove absence; block sequence plus the chunk count can. A CRC-16 cannot detect chunk permutation either, so ordering gets its own guarantee and its own test.

Also here, because it is arithmetic rather than firmware: the ring-capacity policy the dump writer uses, expressed as a pure function with a host test, so the invariant that keeps live log traffic lossless is checked in CI instead of discovered at the bench. A dump must never consume the last `MAX_FRAME` bytes of the 2048-byte pipe; when there is less room than that the writer retries instead of writing, which is what lets AC #3 of the parent be a real claim about `dropped_full`. The commit goes through the same `RECORD_BUFS` lock the ordinary log path holds, because log records arrive from IRQ context and would otherwise interleave a partial frame with a dump's. Note that `Pipe::ready_send` does not exist in embassy-sync 0.6.2 and its async writes strand partial buffers behind one shared waker — the writer polls `free_capacity()` and commits with `frame::write_whole`, backing off on a timer in TASK-038.03.

Finally, a host-only reassembler example, so the format is proven by a program before any board exists.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A `dump` module in `crates/asperitas-logging` compiles for both no_std and host, defines the `AUDIO blk=<4hex> n=<2hex> c=<2hex> d=<b64>` and `AUDEND blk= n= bytes=<dec> crc16=<4hex>` bodies inside the existing v1 frame grammar, and leaves `MAX_BODY`, `MAX_FRAME`, `sanitize_byte`, and the CRC polynomial untouched.
- [ ] #2 Base64 encode and decode are implemented without allocation and without `std`, strictly canonical (padding required, non-zero trailing bits rejected), and a host property test proves byte-equality with the `base64` crate as a dev-dependency-only oracle over random inputs plus exhaustive trailing-symbol enumeration; a full-size `AUDIO` record carries exactly 129 raw bytes in 172 base64 characters.
- [ ] #3 A host assembler accepts a block only when all `n` chunks arrived and the CRC-16 of the reassembled raw bytes matches `AUDEND`; on failure it names the missing chunk indices rather than returning partially assembled data, and it separates a repeated identical chunk (idempotent) from a repeated chunk carrying different bytes (conflict — block refused, conflict named).
- [ ] #4 Loss is proven by test, not asserted in prose: deleting any whole record from a synthetic stream is detected by sequence, a corrupted body fails the block CRC, chunks arriving out of order reassemble byte-identically, and a stream stripped of its leading `~` markers produces no completed block at all — consistent with the blind spot documented at the top of `frame.rs`.
- [ ] #5 The ring-capacity policy is a pure function the firmware calls, and a host test proves the invariant that a dump commit never consumes the last `MAX_FRAME` bytes of the pipe — checked over every starting occupancy `0..=LOG_PIPE_SIZE` — so a maximum-size log or status record always has room and `dropped_full` cannot be blamed on the dump.
- [ ] #6 A host test computes useful-bytes-per-wire-byte and wire cost per second of capture from the same constants the encoder uses, pinned at 129 raw / 227 wire bytes = 0.568 useful (745 records/s, ~169 kB/s for mono 16-bit capture at 96,000 B/s), so the arithmetic quoted in documentation cannot drift away from the code.
- [ ] #7 `examples/dump_reassemble.rs` turns a captured console byte stream into raw PCM plus a manifest, exits non-zero on any incomplete or mismatched block, and is exercised in CI from synthetic streams containing truncation, corruption, and lost start markers with no board attached.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Four leaf tasks carry this ticket. Each has its own plan; read this one first because it records the two premises the ticket shipped with that turned out to be false, and the decisions every child inherits.

## Shape

`dump.rs` is written top-down by four sequential leaves rather than one big session: the payload codec, then the record grammar built on it, then the consumer that proves the format self-verifying, then the pipe discipline that keeps the dump from eating the log traffic that observes it. They share files (`src/dump.rs`, `tests/console_dump.rs`), so they are chained by dependency even where their *semantics* are independent — specifically TASK-038.02.04 needs nothing from .01–.03 but must not land concurrently with them.

| Ticket | Ships | Depends on |
|---|---|---|
| .01 | strict canonical base64 codec, proven against `base64 0.23` as a dev-dep oracle, exhaustive trailing-symbol enumeration | — |
| .02 | `AUDIO blk/n/c/d` + `AUDEND` bodies, const-derived `CHUNK_RAW = 129`, pinned golden frames, efficiency arithmetic test | .01 |
| .03 | `BlockAssembler`, adversarial integrity suite, `examples/dump_reassemble.rs` with `--selftest`, one CI line | .02 |
| .04 | `RESERVE`/`dump_fits` predicate, `try_emit_dump` commit path, occupancy-sweep and interleave tests | .03 (file contention only) |

Nothing here needs the board, ears, or an instrument: AC #7's "exercised in CI ... with no board attached" is why the example carries `--selftest`. Measured throughput stays where it always was — TASK-038.05, @human. Every number in these plans is a prediction until that bench session replaces it.

## Decisions, with the reason

**1. The ticket's 150-raw-bytes-per-record premise is impossible and the layout changed.** `MAX_BODY = 200` bounds the whole body including keys. Full budget table, both candidate layouts, and the rejected alternatives are in Implementation Notes; the chosen form is `AUDIO blk=<4hex> n=<2hex> c=<2hex> d=<b64>` carrying **129 raw bytes** at **0.568 useful** (mono 16-bit capture ⇒ 745 records/s, ~169 kB/s). Any header between 25 and 28 bytes yields the same 129, which is why short keys survive and positional-with-no-keys does not: dropping keys entirely buys 4.6% and costs a human reading a cold capture everything the keys say. Hex digits are fixed-width, which is what makes the header constant-length and the geometry derivable.

**2. Geometry is derived, then pinned.** `CHUNK_RAW` comes from const evaluation over the same byte templates the writer emits, with `const _: () = assert!(...)` on 129 / 199 / 227. That converts "documentation drifted from the code" into a compile error, which is the only honest reading of AC #6.

**3. Strictness is a corruption-detection requirement, not taste.** `sanitize_byte` passes printable ASCII through untouched, so a corrupted symbol looks exactly like a legitimate one unless the decoder rejects everything the reference rejects — padding required, no inner `=`, no junk after padding, no whitespace, and non-canonical tails whose error lands entirely in the ignored bits. Those rules were measured against `base64 0.23.1` rather than assumed, and accept/reject agreement with the oracle is achievable exactly. Ascii85/Z85 stay rejected: they contain `~` (the record start marker) and `<>` (reserved host-to-device by TASK-032).

**4. Sequence proves absence; the CRC proves content; neither proves order.** `frame.rs:47-49` documents that a lost leading `~` produces no integrity failure whatsoever, so `n_of_n` plus a block CRC over raw bytes is the only mechanism that can prove a whole record vanished. A 16-bit CRC over ~8 kB cannot detect chunk permutation either, and XMODEM's three receiver classes (complement mismatch, duplicate, out-of-sequence) are all undefined in the original ticket — .03 defines and tests each. Chunks are placed by index, so permutation is structurally impossible; duplicates get compared byte-for-byte and a differing repeat refuses the block.

**5. A dump writer holds `RECORD_BUFS`; there is no `ready_send`.** Both were verified against source. `embassy-sync 0.6.2` exposes only `write`/`write_all`/`try_write`/`free_capacity`, its async writes strand partial buffers across a wrap, and one shared `write_waker` wakes only on the full→non-full transition — so the old note's "ask the pipe for capacity then commit" is unimplementable. Replacement: poll `free_capacity()` and commit via `frame::write_whole` inside the existing lock (.04), retry with a timer backoff in TASK-038.03, never spin. Log records arrive from IRQ context, so any producer outside that lock can interleave a partial frame with a dump's.

**6. Refusals cost nothing; commits cost a sequence number.** `try_emit_dump` decides before `take_seq()`, bumps no loss counter on refusal, and bumps `records_sent` on commit. Otherwise a retry loop manufactures `seq` gaps that read as transport loss, and `dropped_full` — the very counter TASK-038.03's bench criterion reads — stops meaning "we threw a record away".

**7. One owner per primitive.** Widen `frame.rs`'s private `write_hex`/`write_decimal`/`parse_hex`/`parse_decimal` to `pub(crate)` instead of copying them into `dump.rs`; reuse `crc16_ccitt` for the block CRC; keep `MAX_BODY`, `MAX_FRAME`, `sanitize_byte`, the prefix, and the polynomial untouched. Raising `MAX_BODY` remains a v2 decision with regenerated golden CRCs and is not available as a shortcut here.

**8. Deep, not broad, interfaces.** `_record` functions compose body-building with `frame::encode`; the assembler owns AUDIO/AUDEND parsing; the reserve is a constant, not a parameter. Callers should not have to know the buffer dance, the field widths, or the headroom rule.

## Verification path

CI proves all of it with no board: oracle-equivalence and exhaustive tail enumeration (.01), compile-time geometry plus independently-CRC-pinned golden frames and the efficiency arithmetic (.02), deletion/corruption/permutation/duplicate/marker-stripping attacks and the `dump_reassemble --selftest` run added to `.github/workflows/ci.yml` (.03), and the occupancy sweep plus the log-survives-a-saturated-dump interleave (.04). Integration check when all four land: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, the example's `--selftest`, and `cd firmware && cargo build --release --features seed3` — the last is the only compile coverage for anything behind `log-usb`, since root CI never enables that feature.

Then hand to TASK-038.03, which consumes `audio_record`/`audend_record`/`try_emit_dump` and owns the retry loop, and to TASK-038.06, which copies the grammar table from `dump.rs`'s module doc rather than re-inventing it. TASK-038.05 replaces the predicted wire numbers with measured ones.

## Not in any child

- The async wait/backoff loop (TASK-038.03 owns the executor and timer context).
- Block-size selection for the capture ring — recommended at 64 chunks (8,256 raw ≈ 86 ms) in .02's doc comment, decided by TASK-038.03.
- Owning the device serial port (TASK-031's runner), and any change to `MAX_BODY`, framing, or the CRC polynomial.
- A stall counter for the dump itself: belongs with TASK-038.03's starvation counters, not the console counters.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## What planning verified against the code (2026-09-09), and what this ticket got wrong

Two premises in the original ticket were false. Both are corrected below and the ACs rewritten around the corrections. Read these before proposing any shape change.

**1. 150 raw bytes per record is arithmetically impossible.** `MAX_BODY = 200` bounds the *whole* body, keys included (frame.rs:93), so a keyed header spends payload. Budget computed from the real constants (`PREFIX_LEN = 21`, `TRAILER_LEN = 7`), where encoded length is `4*ceil(raw/3)`:

| Body layout | header B | raw B | b64 chars | body | frame | useful | rec/s @96 kB/s | wire |
|---|---|---|---|---|---|---|---|---|
| ticket-literal `AUDIO block_index=<dec> n_of_n=<dec> chunk_i=<dec> b64=` (widths 6/3/3) | 52 | 111 | 148 | 200 | 228 | 0.487 | 865 | 197 kB/s |
| same, u32 worst-case decimal widths | 70 | 96 | 128 | 198 | 226 | 0.425 | 1000 | 226 kB/s |
| **chosen:** `AUDIO blk=<4hex> n=<2hex> c=<2hex> d=<b64>` | 27 | **129** | 172 | 199 | 227 | **0.568** | 745 | 169 kB/s |
| positional, no keys at all | 17 | 135 | 180 | 197 | 225 | 0.600 | 711 | 160 kB/s |

Payload rounds down to whole 4-char groups, so **any header between 25 and 28 bytes yields the same 129 raw bytes**. That is why the chosen layout keeps short key names instead of going positional: dropping the keys entirely buys 6 more raw bytes (4.6%) and costs a human reading a capture cold everything the keys say. Descriptive keys (`block_index=`) cost 18 raw bytes. Hex digits are fixed-width, which is what makes the header constant-length and the geometry derivable at compile time.

**2. `Pipe::ready_send(n)` does not exist.** Verified in the vendored source (`~/.cargo/registry/src/*/embassy-sync-0.6.2/src/pipe.rs`): the writer surface is `write` (:371), `write_all` (:378), `try_write` (:390), `clear/is_full/is_empty/capacity/len/free_capacity` (:422-458). No `ready_send`, no `poll_ready`. Worse for the "wait then commit" plan in the old notes:

- `WriteFuture::poll` (:69-74) is Ready on *any* nonzero write, so awaiting `write(frame)` strands a partial frame across the ring whenever capacity is short of the frame. `write_all` (:378) loops over exactly that, so it is equally unusable.
- `PipeState` holds exactly **one** `write_waker` (:202-206), registered only when the ring is completely full (:318-341), and `write_waker.wake()` fires only on the full→non-full transition (:266-268, :291-293) or `clear()`. `consume()` does not wake it. So there is no partial-watermark wakeup available even in principle.

Replacement: the writer polls `free_capacity()` and commits through `frame::write_whole` under the existing `RECORD_BUFS` lock (see #3), and *waits with a timer*, not with a pipe future. `Timer::after_ms(...)` backoff between refusals is the boring correct choice; the drainer frees up to `DRAIN_BUF_SIZE = 256` bytes per wakeup (usb.rs:268), so a 1 ms poll interval bounds stall time well below anything that matters for a post-run dump. Precedent for a bounded busy-wait without a `Timer` exists at usb.rs:405-411 if a finer deadline is ever needed. The retry loop itself belongs to TASK-038.03, which owns the executor and timer context; this ticket ships the pure predicate plus the commit entry point.

**3. A dump writer must hold `RECORD_BUFS`, not just check capacity.** `emit()` (lib.rs:263-297) takes `seq`, encodes, pre-checks `LOG_PIPE.free_capacity()` and commits, all inside `RECORD_BUFS.lock(...)` deliberately: log records are emitted from arbitrary context including the audio callback, so an IRQ can preempt any producer that is not holding that lock, and two interleaved `write_whole` calls would splice two frames' bytes into the ring. Capacity can only *increase* while the lock is held (the consumer runs with IRQs on), which is what makes the pre-check sound. Consequence: this ticket adds one reserved-capacity commit path in `lib.rs` behind `log-usb` rather than teaching `firmware/` to poke `LOG_PIPE` directly.

**4. Strict base64 semantics are pinned by the oracle, not invented.** Measured against `base64 0.23.1` (`STANDARD`, `default-features = false` builds offline; API is `Engine::encode_slice` returning bytes written, `Engine::decode_slice`/`decode_engine_slice` into a caller buffer): padding is required (`"AB="` → Invalid padding, `"A"` → bad length), `=` inside the string is rejected, trailing junk after padding is rejected, inner whitespace is rejected, and **non-canonical trailing bits are rejected** (`"AB=="` → "Invalid last symbol ... decoded as 0b00000001"). Accept/reject agreement is therefore achievable exactly, and rejecting non-canonical tails is deliberate: otherwise a corrupted final symbol whose error lands in the ignored bits decodes silently. 129 is a multiple of 3, so full-size chunks need no padding and only a block's final chunk exercises `=`.

**5. Field budget guards.** `n` is 2 hex digits, so `MAX_CHUNKS_PER_BLOCK = 256` (a block ≤ 33,024 raw bytes); the encoder must refuse `n > 256` rather than truncate. `blk` is 4 hex digits = 65,536 blocks; a full 32 MiB capture ring at 8 KiB blocks is 4,096 blocks, 16× headroom, and cross-run ordering is the frame `seq`'s job anyway. Recommend 64 chunks (8,256 raw ≈ 86 ms of mono 16-bit) to TASK-038.03.

## Constraints already established (verified, do not re-derive)

- Record grammar: `'~' level SP seq(8 hex) SP t_ms(8 dec) SP body '*' crc(4 hex) CRLF` (frame.rs:20-40, offset table :66-82). `crc16_ccitt` is public (frame.rs:144) and covers `out[1..PREFIX_LEN+body_len]`; reuse it for the block CRC over **raw** bytes.
- `encode(level, seq, now_ms, body, out) -> Encoded{len, truncated}` (frame.rs:185) sanitizes every body byte unconditionally via `sanitize_byte` (:240) — printable ASCII is an identity map, so base64 and hex pass untouched, but there is no raw bypass and none is needed. `Encoded.len` is the exact frame length; body length is `len - PREFIX_LEN - TRAILER_LEN`.
- `write_whole(frame, free_capacity, write) -> bool` (frame.rs:306): all-or-nothing, and in release it spins forever if the sink stalls after the pre-check passed — hence the pre-check inside the lock. `try_write` short-writes at every ring wrap even on an empty ring (measured 112 of 200 into 512 free), which is why `write_whole` exists at all.
- `HEX_DIGITS` (:248), `write_hex(value, dst)` (:252, MS-first, width from `dst.len()`, lowercase, zero-padded) and `write_decimal` (:263) are **module-private**. Either widen them to `pub(crate)` or hand-roll equivalents in `dump.rs`; widening is preferable to duplicating, and both writers already have `debug_assert!`s on width.
- Decoder validates framing and CRC only — it has no verb concept (frame.rs:694-748), so `AUDIO`/`AUDEND` records flow to the host consumer untouched. Level letter must be one of `IWEDT` (:722) and spaces are required at buf[2]/buf[11]/buf[20]. Use `Level::Info` like BOOT/STATUS.
- `LOG_PIPE` is `pub static` (lib.rs:213) with `LOG_PIPE_SIZE = 2048` (lib.rs:195); `emit`, `emit_boot`, `emit_status` are private/`pub(crate)`, so today nothing in `firmware/src/` reaches the producer path at all — it only uses the `log::info!` macros, `usb::init/run`, `led`, `panic_handler`.
- Host tests cannot link `CriticalSectionRawMutex` (undefined symbol at *link* time). The working pattern is a **local** `Pipe<NoopRawMutex, N>` because `NoopRawMutex` is `!Sync` and cannot live in a `static` — see `pipe512()`/`commit()`/`park_cursors_at()` at frame.rs:1257-1288, used by `write_whole_rounds_are_byte_exact_under_randomized_interleaving` (frame.rs:1340, 20,000 rounds). Reuse that shape for the occupancy property test.
- CI (`.github/workflows/ci.yml`, single `check` job in `nix develop .#default`): `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` (twice, second with `asperitas-pod/pod-hw`), `cargo test --workspace` (twice, same), then `cd firmware && cargo build --release --features seed3`. **`log-usb` is never enabled by root-workspace CI**, so anything behind it is compile-covered only by the firmware release build — keep it minimal and pure. No step asserts Cargo.lock is unchanged, and `proptest` proves dev-deps do not leak into `firmware/Cargo.lock` (grep: absent), so adding `base64` as a dev-dep touches only the root lock.
- Test style contract (tests/console_frame.rs:1-22): strategies sized by *count*, lowercase `prop_assert!` messages naming offending values, `// ── Section ──` banners, third-person indicative test names, `///` doc on every helper, exhaustive-over-enumeration justified in-doc. No `ProptestConfig` anywhere in the repo — default 256 cases; the only override mechanism used is a checked-in `<testfile>.proptest-regressions`.
- Example shape (examples/console_decode.rs): `CHUNK_BYTES = 4096`, stdin-or-path via `env::args().nth(1)`, all messages prefixed `console_decode: `, stdout carries payload, stderr carries counters plus the accounting law, and it exits 0 regardless of integrity counters — exit 1 only for input it could not open. `dump_reassemble` deliberately differs (integrity failure ⇒ exit 1) because AC#7 asks it to gate CI; say so in its doc header so the divergence reads as chosen, not accidental.

## Deliberate non-goals

- No change to `MAX_BODY`, `MAX_FRAME`, `sanitize_byte`, the record prefix, or the CRC polynomial. If TASK-038.05 measures the dump as unbearably slow, raising `MAX_BODY` is its own v2 ticket with regenerated golden CRCs — the table above is the argument to make there, not here.
- No serial-port code; the reassembler reads a file or stdin. Owning the device port belongs to TASK-031's runner.
- No COBS/binary framing and no Ascii85/Z85 (they contain `~`, the record start marker, and `<>`, reserved host-to-device by TASK-032).
- No async wait loop here. The retry/backoff loop needs an executor and a timer and belongs to TASK-038.03.

## Parked 2026-09-09: this is an umbrella, not a unit of work

No implementation was attempted here. All seven ACs are carried by the four leaves (.01 codec, .02 grammar, .03 assembler + example + CI, .04 pipe reserve), all `@agent`, all Dev Ready, none started. Evidence from the tree rather than from statuses: `crates/asperitas-logging/src/` contains console.rs, frame.rs, led.rs, lib.rs, panic_handler.rs, usb.rs — there is no `dump.rs`, no `tests/console_dump.rs`, no `examples/dump_reassemble.rs`, and `Cargo.toml` has no base64 dev-dependency. Nothing this ticket could ship exists yet, and writing four tickets' code under one ID would leave the leaves open against a Done parent.

`backlog task list --ready` already excludes this ticket (it depends on .04, transitively on .01-.03) and lists **TASK-038.02.01** as the only ready Dev Ready item. That is the next actionable ticket; select it, not this one.

### AC to leaf map, for whoever closes this umbrella
- #1 dump module on both targets + AUDIO/AUDEND bodies, frame constants untouched -> .01 (module, ungated in `lib.rs`) plus .02 (bodies, const asserts). See .02 AC #1/#2/#3 and the firmware release build in both leaves' verification steps.
- #2 strict canonical base64, oracle property test, 129 raw in 172 chars -> .01 AC #1-#4.
- #3 assembler completes only when all n chunks arrive and the block CRC matches; names missing indices; idempotent repeat vs conflict split -> .03 AC #1-#3.
- #4 loss proven by test (deletion, corruption, reordering, `~` marker stripping) -> .03 AC #4.
- #5 ring-capacity predicate plus exhaustive occupancy sweep `0..=LOG_PIPE_SIZE` -> .04 AC #1, #3, #4, #5.
- #6 efficiency arithmetic derived from the encoder's own constants -> .02 AC #5.
- #7 `examples/dump_reassemble.rs` gating CI via `--selftest` -> .03 AC #5, #6.

Check these boxes only after the owning leaf is Done, using that leaf's test output as the evidence. Do not check any of them from a planning or parking run.

### Commit state

The 2026-09-09 16:01 planning rewrite (both false premises corrected: 150 raw bytes per record is unreachable at `MAX_BODY = 200`; `Pipe::ready_send` is absent in embassy-sync 0.6.2) and the four new leaf files were left uncommitted by the planning run. This commit lands them.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-09 16:01
---
Planning on 2026-09-09 checked both load-bearing premises against the code and both failed, so the acceptance criteria and description were rewritten rather than executed as written.

1. **150 raw bytes per record was unreachable.** `MAX_BODY = 200` bounds the whole body including keys. The specified `block_index=/n_of_n=/chunk_i=` form with realistic decimal widths yields 111 raw at 0.487 useful efficiency; with u32 worst-case widths, 96 at 0.425. Chosen replacement: `AUDIO blk=<4hex> n=<2hex> c=<2hex> d=<b64>` — 27-byte constant header, 129 raw bytes, 0.568 useful, 227-byte frame. Full budget table and rejected alternatives in Implementation Notes.
2. **`Pipe::ready_send(n)` does not exist** in embassy-sync 0.6.2 (verified in vendored source: `write`, `write_all`, `try_write`, `free_capacity` only), its async writes strand partial buffers across a wrap, and one shared `write_waker` wakes only on the full→non-full transition. Replacement design: pure predicate + commit through `frame::write_whole` inside the existing `RECORD_BUFS` lock, timer backoff in TASK-038.03.

Downstream consequence for other tickets: the efficiency figure any document may quote is **0.568, not 0.66**. For mono 16-bit capture at 96,000 B/s that is 745 records/s and ~169 kB/s predicted (not the 640/s and 146 kB/s in the original notes) — roughly 199 s to drain a full 32 MiB ring if the link sustains it. TASK-038.06 should copy the grammar table from `dump.rs`'s module doc and publish the corrected numbers; TASK-038.05 replaces them with measurements. Nothing here changes TASK-038's decision to capture 16-bit mono or to dump after the run rather than live.

Four leaf sub-tickets created (.01 codec, .02 grammar, .03 assembler + example + CI, .04 pipe reserve), all @agent, all planned and Dev Ready, chained by dependency because they share `src/dump.rs` and `tests/console_dump.rs`. They carry complete plans rather than descriptions only because research resolved every open question — the remaining work is writing code, not deciding shape.
---
<!-- COMMENTS:END -->
