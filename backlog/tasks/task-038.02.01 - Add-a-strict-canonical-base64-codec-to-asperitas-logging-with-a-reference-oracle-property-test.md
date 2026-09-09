---
id: TASK-038.02.01
title: >-
  Add a strict canonical base64 codec to asperitas-logging with a
  reference-oracle property test
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-09 15:50'
updated_date: '2026-09-09 15:53'
labels:
  - planned
dependencies: []
modified_files:
  - crates/asperitas-logging/src/dump.rs
  - crates/asperitas-logging/tests/console_dump.rs
  - crates/asperitas-logging/Cargo.toml
  - Cargo.lock
parent_task_id: TASK-038.02
priority: high
type: task
ordinal: 62500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The payload layer needs a binary-to-text codec that is strict enough that silent corruption cannot survive it, small enough to live in firmware, and provably correct against a reference implementation before any board exists. This ticket ships that codec alone: no record grammar, no assembler, no pipe interaction.

Hand-write encode/decode in a new `src/dump.rs` (no allocation, no `std`, caller-provided buffers) matching standard base64 with `=` padding, and prove it against `base64 0.23` used as a **dev-dependency-only** oracle. Strictness is not a style choice here: `sanitize_byte` passes printable ASCII through untouched, so a corrupted symbol is indistinguishable from a legitimate one unless the decoder rejects everything the reference rejects — including non-canonical tails whose error lands entirely in the ignored bits.

See TASK-038.02's Implementation Notes for the measured oracle behaviour and the byte-budget table this codec feeds (129 raw bytes per 172-character payload).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A `dump` module exists in `crates/asperitas-logging`, is ungated in `lib.rs` so `cargo test --workspace` builds it on host under the crate's unconditional `#![no_std]`, and exposes `encoded_len`, `max_raw_for`, `encode`, `decode` over caller-provided buffers with no allocation.
- [ ] #2 A proptest asserts our encoder is byte-identical to `base64 0.23`'s `STANDARD::encode_slice`, and that decode agrees with the oracle on accept/reject and on decoded bytes, over random inputs from 0 to `CHUNK_RAW + 7` bytes. `base64` appears only in `[dev-dependencies]` and `firmware/Cargo.lock` is unchanged.
- [ ] #3 Trailing-symbol handling is proven exhaustively, not sampled: all 64 possible final symbols for a 1-byte tail and all 64 for a 2-byte tail are checked against the oracle, so non-canonical input (ignored bits nonzero) is rejected exactly as the oracle rejects it.
- [ ] #4 Pinned golden vectors cover RFC 4648 §10 in full, 129 bytes of `0x00`, 129 bytes of `0xFF`, and a vector exercising all 64 alphabet characters plus `+`/`/` boundaries; malformed-input tests cover length-not-a-multiple-of-4, `=` inside the string, junk after padding, inner whitespace, and too-small output buffers — each as an error, never a panic.
- [ ] #5 `cargo fmt --all --check`, `cargo clippy -p asperitas-logging --all-targets -- -D warnings`, `cargo test -p asperitas-logging`, and `cd firmware && cargo build --release --features seed3` all pass.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Single file plus its tests. Read TASK-038.02's Implementation Notes first — items 4 and 5 are this ticket's contract.

## Files
- `crates/asperitas-logging/src/dump.rs` (new) — module doc stating why base64 rather than Ascii85/Z85/hex (the alphabet survives `sanitize_byte` at frame.rs:240 intact; Ascii85 contains `~`, the record start marker, and `<>`, reserved host-to-device by TASK-032), and that strictness is a corruption-detection requirement, not taste.
- `crates/asperitas-logging/src/lib.rs` — add `pub mod dump;` next to the ungated `pub mod frame;` (:164) / `pub mod console;` (:170). Ungated on purpose: `cargo test --workspace` must build it on host under the crate's unconditional `#![no_std]` (lib.rs:26).
- `crates/asperitas-logging/Cargo.toml` — `base64 = "0.23"` in `[dev-dependencies]`, with a comment in the shape of the existing proptest/embassy-sync note: oracle only, never reaches `firmware/`.
- `crates/asperitas-logging/tests/console_dump.rs` (new) — style follows `tests/console_frame.rs`: `// ── Section ──` banners, count-sized strategies, lowercase `prop_assert!` messages naming offending values, `///` on every helper. No `ProptestConfig` (repo convention is default 256 cases).

## API (pure `core`, no allocation)
```rust
pub const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
pub const fn encoded_len(raw: usize) -> usize;   // (raw + 2) / 3 * 4
pub const fn max_raw_for(b64_chars: usize) -> usize; // b64_chars / 4 * 3
pub fn encode(raw: &[u8], out: &mut [u8]) -> Result<usize, EncodeError>;
pub fn decode(b64: &[u8], out: &mut [u8]) -> Result<usize, DecodeError>;
pub enum EncodeError { OutputTooSmall { need: usize } }
pub enum DecodeError { Length(usize), Char { at: usize, byte: u8 }, Padding { at: usize }, TrailingBits { at: usize }, OutputTooSmall { need: usize } }
```
Implementation notes: build the inverse alphabet once with a `const fn` table (`[i8; 256]`, `-1` invalid, `-2` pad, else 0..=63) so decoding is table-driven rather than a chain of range comparisons; encode in 3-byte groups accumulating a 24-bit value and emitting 4 symbols, then one tail step. Never index a caller-supplied byte into a 64-entry table without the validity check — an unvalidated index is how a corrupted byte becomes a silent wrong value.

## Strictness rules (measured against base64 0.23.1 `STANDARD`, do not deviate)
- Input length must be a multiple of 4; empty input decodes to `Ok(0)`; `"A"` is a length error.
- Padding required: `"AB="` is rejected. `=` may appear only in the final group, only in the last two positions, at most two of them; `"QU=JDRA="` is rejected.
- Non-alphabet bytes anywhere (including `\n`) are `Char` errors — the transport never inserts whitespace inside a body.
- Junk after padding (`"QUJDRA==X"`) is rejected.
- **Ignored bits must be zero**: for `xy==` the low 4 bits of `y`; for `xyz=` the low 2 bits of `z`. The oracle calls this "Invalid last symbol"; reject as `TrailingBits`.

## Tests
1. `encodes_exactly_like_the_reference` — proptest over `prop::collection::vec(any::<u8>(), 0..=136)` (spans 0 through `CHUNK_RAW + 7`, i.e. past one full payload): assert our `encode` output equals `STANDARD.encode_slice` byte-for-byte and that `encoded_len` matches.
2. `decodes_exactly_like_the_reference` — same inputs round-tripped, decoded into an identical-size buffer, asserting both the decoded bytes and the accept/reject decision against `STANDARD.decode_slice`. Comparing decisions and bytes (not error text) is the achievable contract.
3. `rejects_or_accepts_mutation_exactly_like_the_reference` — encode, then flip 1–3 positions drawn from a delimiter-heavy alphabet (`prop_oneof!` over `'='`, `'+'`, `'/'`, `'A'`, `'z'`, `'\n'`, `any::<u8>()`), mirroring `console_frame.rs:739-749`; assert agreement on accept/reject and on bytes when both accept.
4. `every_final_symbol_of_a_one_byte_tail_agrees_with_the_reference` and `..._two_byte_tail...` — exhaustive over all 64 symbols (accept iff the ignored bits are zero). Justify exhaustive-over-enumeration in-doc as `console_frame.rs` does.
5. Golden vectors as literals: RFC 4648 §10 (`""`, `f`→`Zg==`, `fo`→`Zm8=`, `foo`→`Zm9v`, `foob`→`Zm9vYg==`, `fooba`→`Zm9vYmE=`, `foobar`→`Zm9vYmFy`), 129×`0x00`, 129×`0xFF`, and one vector covering all 64 characters (needs ≥48 raw bytes; use a 48-byte counter pattern and assert the ciphertext contains `+` and `/`). Pin the expected strings literally; derive nothing from our own encoder.
6. Error-shape tests for each malformed class in the strictness list above, and for too-small output buffers on both directions — each asserts an error value, never a panic.

## Verification
`cargo fmt --all --check` · `cargo clippy -p asperitas-logging --all-targets -- -D warnings` · `cargo test -p asperitas-logging` · `cd firmware && cargo build --release --features seed3` · `git diff --stat firmware/Cargo.lock` must be empty (root `Cargo.lock` gains base64; the firmware workspace lock must not). If proptest shrinks a failure, commit the generated `tests/console_dump.proptest-regressions`; do not fabricate entries.

## Out of scope here
No `AUDIO`/`AUDEND` bodies, no `CHUNK_RAW`, no assembler, nothing behind `log-usb`. Keep `dump.rs` free of `unsafe`, allocation, and `frame` internals beyond the public constants.
<!-- SECTION:PLAN:END -->
