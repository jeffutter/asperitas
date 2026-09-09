---
id: TASK-038.02.01
title: >-
  Add a strict canonical base64 codec to asperitas-logging with a
  reference-oracle property test
status: Done
assignee:
  - '@agent'
created_date: '2026-09-09 15:50'
updated_date: '2026-09-09 16:37'
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
- [x] #1 A `dump` module exists in `crates/asperitas-logging`, is ungated in `lib.rs` so `cargo test --workspace` builds it on host under the crate's unconditional `#![no_std]`, and exposes `encoded_len`, `max_raw_for`, `encode`, `decode` over caller-provided buffers with no allocation.
- [x] #2 A proptest asserts our encoder is byte-identical to `base64 0.23`'s `STANDARD::encode_slice`, and that decode agrees with the oracle on accept/reject and on decoded bytes, over random inputs from 0 to `CHUNK_RAW + 7` bytes. `base64` appears only in `[dev-dependencies]` and `firmware/Cargo.lock` is unchanged.
- [x] #3 Trailing-symbol handling is proven exhaustively, not sampled: all 64 possible final symbols for a 1-byte tail and all 64 for a 2-byte tail are checked against the oracle, so non-canonical input (ignored bits nonzero) is rejected exactly as the oracle rejects it.
- [x] #4 Pinned golden vectors cover RFC 4648 §10 in full, 129 bytes of `0x00`, 129 bytes of `0xFF`, and a vector exercising all 64 alphabet characters plus `+`/`/` boundaries; malformed-input tests cover length-not-a-multiple-of-4, `=` inside the string, junk after padding, inner whitespace, and too-small output buffers — each as an error, never a panic.
- [x] #5 `cargo fmt --all --check`, `cargo clippy -p asperitas-logging --all-targets -- -D warnings`, `cargo test -p asperitas-logging`, and `cd firmware && cargo build --release --features seed3` all pass.
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Shipped 2026-09-09

`crates/asperitas-logging/src/dump.rs` (new, ungated in `lib.rs` next to `frame`/`console`) holds an
allocation-free, `core`-only base64 codec: `encoded_len`, `max_raw_for`, `encode`, `decode`, plus
`EncodeError`/`DecodeError` whose variants carry the offending offset so a caller can say *where* a
capture broke. Decoding is driven by a `const fn`-built `[i8; 256]` inverse alphabet (`-1` invalid,
`-2` pad), which makes validity fall out of the lookup instead of a chain of range comparisons.
Padding legality is decided in exactly one place, `tail_shape`, so validation and decoding cannot
drift apart on the rules that matter.

`base64 = "0.23"` is a dev-dependency only, with the reason written above the section it belongs to.
`Cargo.lock` gains `base64 0.23.1`; `firmware/Cargo.lock` is untouched (`git diff --stat` empty).

### Measured oracle behaviour, confirmed rather than assumed
Every rule in the module doc was probed against `base64 0.23.1 STANDARD` before being asserted: a
throwaway harness printed both verdicts for 30 hand-picked strings and all 30 agreed (offsets too,
except where our stricter length rule legitimately fires first). The harness is gone; the table it
covered now lives in `agrees_with_the_reference_on_tricky_strings`. Two findings were worth
recording because they are easy to get wrong:

- `"AB=="` is refused by the reference as *Invalid last symbol*, not silently decoded — matching our
  `TrailingBits`. This is why strictness buys anything: `sanitize_byte` leaves printable ASCII alone,
  so a bit-flip in a payload's final symbol would otherwise decode cleanly into wrong audio.
- A length violation usually masks the interesting error (`"QUJDRA==X"` fails on length, not on junk
  after padding). The malformed-class table therefore isolates each class with strings that are a
  whole number of groups long.

### One expectation corrected while writing the tests
A one-byte tail keeps six of twelve bits, so four symbols of the alphabet are canonical (`A`, `Q`,
`g`, `w`) — not sixteen. The exhaustive sweep caught this by counting 4 where the comment predicted
16; the two-byte tail really is 16 of 64. Both counts are now asserted from the bit arithmetic the
test names.

### Verification
- `cargo test -p asperitas-logging`: 31 unit + 13 dump + 33 frame + 1 doctest, all passing. No
  `.proptest-regressions` file appeared, because nothing ever failed a shrink.
- `cargo fmt --all --check`, `cargo clippy -p asperitas-logging --all-targets -- -D warnings`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and the `asperitas-pod/pod-hw` variant:
  clean.
- `cargo test --workspace` and `cd firmware && cargo build --release --features seed3`: both exit 0.
  The firmware build is the real no_std evidence — `dump` is ungated, so it compiles into the device
  image even though no firmware code calls it yet.

### Deliberate shape choices
- Error *variants* are ours and are not expected to mirror the reference's wording; only the
  accept/reject decision and the accepted bytes are comparable, and that is what the properties
  assert.
- `encoded_len` / `max_raw_for` clamp to `usize::MAX` instead of wrapping. Callers size buffers with
  them, and a wrapped value would hand back a too-small buffer while reporting success.
- An undersized output buffer is refused before any byte is written (both directions, tested).
  Mid-string errors may leave earlier groups already decoded — documented rather than papered over
  with a rollback nobody needs, since every caller passes a fresh chunk buffer.
- No `CHUNK_RAW`, no `AUDIO`/`AUDEND`, nothing behind `log-usb`: those belong to .02 and .04. The
  129-byte full-payload assertions here use the literal with a comment naming where the constant
  will live.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Strict canonical base64 codec shipped in crates/asperitas-logging/src/dump.rs (no_std, no allocation, caller-provided buffers) with a 13-test suite in tests/console_dump.rs proving byte-identical encoding and accept/reject equivalence with base64 0.23.1 over random input, mutated input, an exhaustive sweep of all 64 final symbols for each tail shape, RFC 4648 vectors, and pinned full-payload saturation patterns. base64 is a dev-dependency only; firmware/Cargo.lock unchanged. fmt, clippy (crate, workspace, pod-hw), cargo test --workspace, and the seed3 firmware release build all pass.
<!-- SECTION:FINAL_SUMMARY:END -->
