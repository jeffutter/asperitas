---
id: TASK-030.01.01
title: >-
  Implement frame.rs encoder side: CRC-16/CCITT-FALSE, sanitising encoder,
  whole-record-or-nothing write_whole
status: Done
assignee:
  - '@agent'
created_date: '2026-09-09 05:36'
updated_date: '2026-09-09 06:13'
labels:
  - task
  - planned
dependencies: []
modified_files:
  - crates/asperitas-logging/src/frame.rs
  - crates/asperitas-logging/src/lib.rs
  - crates/asperitas-logging/Cargo.toml
parent_task_id: TASK-030.01
priority: high
ordinal: 52500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent **TASK-030.01**; grandparent **TASK-030 §3** is the normative wire spec (read §2 and §3 first — §2 lists facts already verified so nobody re-researches them).

Build only the **producer half** of `crates/asperitas-logging/src/frame.rs`: the constants, CRC-16/CCITT-FALSE, body sanitisation and capping, `encode`, and `write_whole`, plus inline `#[cfg(test)]` unit tests and the `[dev-dependencies]` block both halves of the codec need. The incremental decoder, `examples/console_decode.rs` and the adversarial property suite belong to **TASK-030.01.02**; the device log path is **TASK-030.02**, which consumes exactly `MAX_*`, `Encoded`, `encode` and `write_whole` and nothing else from this module.

Everything must build and test on the host with the crate's **default** features (`log-usb` off), because CI's `cargo test --workspace` runs it that way and `cortex-m = "0.7"` sits there unconditionally (`crates/asperitas-logging/Cargo.toml:12`) yet still compiles for host — verified: `cargo test -p asperitas-logging` passes today with zero tests. No cortex-m, embassy, `static mut`, or `unsafe` may appear in any non-dev path of `frame.rs`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `frame.rs` is declared `pub mod frame;` outside every `#[cfg]`, compiles under the crate's default features (`log-usb` off), and names no cortex-m/embassy/hardware type in any non-dev path, so `cargo test -p asperitas-logging` runs its unit tests on the host.
- [x] #2 `crc16_ccitt(b"123456789") == 0x29b1`, with poly 0x1021 / init 0xFFFF / refin=false / refout=false / xorout=0 pinned in the doc comment alongside the alias CRC-16/IBM-3740 and the note that the CCITT name is a misnomer trap.
- [x] #3 encode() reproduces all five golden frames from the plan byte-for-byte (check vector, 34 B `ENC +1`, 28 B empty body, 43 B t_ms-modulo + UTF-8 case, 228 B max body), and the const assertions `MAX_FRAME == 228` and `MIN_CR_OFFSET + MAX_BODY + 2 == MAX_FRAME` hold.
- [x] #4 Bodies are sanitised (< 0x20 and 0x7F become `_`, bytes >= 0x80 untouched) and capped at MAX_BODY **before** the CRC, so a truncated record still validates its own checksum; `Encoded::truncated` is true only when the input exceeded the cap; printable punctuation including `~`, `*`, `|` survives.
- [x] #5 `write_whole` returns false having called the sink zero times when free_capacity < frame len, and true only after every byte reached the sink; a host test drives a real `Pipe<NoopRawMutex, 512>` across the ring wrap (fill 400, drain 400, commit a 200-byte frame accepted whole) plus >= 20 000 randomized producer/consumer rounds whose committed bytes match the bytes read back exactly.
- [x] #6 fmt, both clippy invocations with `-D warnings`, both `cargo test --workspace` invocations, and the firmware cross-compile all pass, and `firmware/Cargo.lock` shows no diff.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# Plan — encoder half of `frame.rs` (console protocol v1)

TASK-030 §3 is normative; this plan only spells out what an implementer needs to write bytes. Red-green:
constants + CRC first (its test is one line), then `encode`, then `write_whole`. Each step lands with its
tests passing before the next starts.

## 1. Files and manifest

- New `crates/asperitas-logging/src/frame.rs`; declare `pub mod frame;` in `src/lib.rs` **outside every
  `#[cfg]`**. It will be the crate's first ungated module — `usb`, `led`, `panic_handler` are all behind
  `log-usb` (`lib.rs:125-135`), which is exactly why AC #1 exists.
- `crates/asperitas-logging/Cargo.toml`: add a `[dev-dependencies]` block (none exists today):

  ```toml
  [dev-dependencies]
  proptest = "1"
  embassy-sync = "0.6"
  ```

  Both are already in the root `Cargo.lock` (`proptest 1.11.0`, `embassy-sync 0.6.2`) so this pulls no new
  crates. Dev-only, host-only: the optional `log-usb` dependency stays untouched, and dev-deps never reach
  the `firmware/` workspace, so `firmware/Cargo.lock` must show no diff (confirm with `git diff --stat`).
  Do **not** add `crc` or `cobs` — parent §2 pins both as absent and CRC is ~15 lines.
- `--modified-file` for this ticket: `crates/asperitas-logging/src/frame.rs`,
  `crates/asperitas-logging/src/lib.rs`, `crates/asperitas-logging/Cargo.toml`.

## 2. Constants, and the arithmetic that everything else hangs off

```rust
pub const MAX_BODY: usize = 200;                    // TASK-030 §3
pub const PREFIX_LEN: usize = 21;                   // '~' level SP seq(8) SP t_ms(8) SP
pub const TRAILER_LEN: usize = 7;                   // '*' crc(4) CR LF
pub const MAX_FRAME: usize = PREFIX_LEN + MAX_BODY + TRAILER_LEN;   // 228
/// Relative offset of CR for the shortest legal record (empty body).
pub const MIN_CR_OFFSET: usize = PREFIX_LEN + TRAILER_LEN - 2;      // 26
const _: () = assert!(MAX_FRAME == 228);
const _: () = assert!(MIN_CR_OFFSET + MAX_BODY + 2 == MAX_FRAME);   // 26 + 200 + 2 == 228
```

**Derivation table — derive every decoder and encoder offset from `j`, the absolute index of CR.** The
planning pass before this one wrote these offsets inconsistently (it used `body = out[PREFIX_LEN..i]` with
`i` the CRLF index, and a minimum length of 27); the table below was verified against a model of the whole
protocol, and the second identity is what makes the max-size record decodable at all.

| quantity | expression | empty body | body = `ENC +1` | body = 200 B |
|---|---|---|---|---|
| total frame length | `j - start + 2` | 28 | 34 | 228 |
| `j - start` (relative CR) | `MIN_CR_OFFSET + body_len` | 26 | 32 | 226 |
| `'*'` | `j - 5` | 21 | 27 | 221 |
| CRC digits | `j-4 .. j` | | | |
| body | `start+PREFIX_LEN .. j-5` | ∅ | `ENC +1` | 200 × `a` |
| CRC-covered range | `start+1 .. j-5`, length `j-start-6` | 20 | 26 | 220 |

Consequences worth a comment in the code: overhead is 28 bytes (parent §3's "28 fixed bytes"); the longest
record is exactly `MAX_FRAME`, so a candidate window of `MAX_FRAME` bytes with no CRLF cannot be a record;
and `'*'` must be located **from `j`**, never by searching forward from the start — a body may legitimately
contain `'*'` (sanitisation only neutralises `< 0x20` and `0x7F`), and a planning-pass harness that used a
forward search mis-decoded exactly those records.

## 3. CRC-16/CCITT-FALSE

```rust
pub fn crc16_ccitt(data: &[u8]) -> u16
```

Table-less bit loop, `poly 0x1021`, `init 0xFFFF`, `refin = false`, `refout = false`, `xorout = 0x0000`.
Pin those five parameters in the doc comment **with the catalogue alias** `CRC-16/IBM-3740` and a note that
the name is a known trap — "CRC-16/CCITT" is commonly misidentified; the true CCITT/V.41 form is reflected
(KERMIT, check `0x2189`) and XMODEM is the init-0 variant. Anyone renaming or "fixing" this function needs
the parameters, not the label. Check vector `b"123456789"` ⇒ `0x29b1`
(<https://reveng.sourceforge.io/crc-catalogue/16.htm>).

Keep it a bit loop rather than a 256-entry table: identical source on both ends of the link, no data
segment, and the cost is ≤ 220 × 16 = 3 520 iterations per record inside the device's critical section.
Do not convert that to "µs of the 667 µs audio block" in a comment — measure it in TASK-030.04 instead
(parent §8 risk 1).

## 4. Encoder

```rust
pub struct Encoded { pub len: usize, pub truncated: bool }

pub fn encode(level: log::Level, seq: u32, now_ms: u32, body: &[u8],
              out: &mut [u8; MAX_FRAME]) -> Encoded;
```

`out` comes from the caller so `frame.rs` owns no static and needs no `unsafe`: the device passes its one
lock-guarded buffer (TASK-030.02 §"one buffer"), tests pass stack arrays. `log::Level` is already re-exported
by the crate (`lib.rs:18`), so no new import surface.

Layout: write the sanitised body at `out[PREFIX_LEN..]`, then the fixed-width prefix into `out[..21]`, then
`*` + 4 lowercase hex CRC digits + `\r\n`. Because the prefix widths are fixed, none of it needs the body
length first. CRC covers `out[1..PREFIX_LEN + body_len]` — i.e. everything after `~` up to but not including
`*`; compute it **after** capping.

Field rules, each pinned by a test:

- Level: `Error→b'E'`, `Warn→b'W'`, `Info→b'I'`, `Debug→b'D'`, `Trace→b'T'`.
- `seq`: 8 lowercase hex digits, zero-padded (`{:08x}`).
- `now_ms`: 8 decimal digits of `now_ms % 100_000_000`. Pin the modulo in code and prose: raw milliseconds
  exceed 8 digits after ~100 000 s and would silently widen the prefix and break every offset above. Neither
  field can reveal a wrap on its own — continuity comes from `BOOT` plus `seq` (parent §3).
- Sanitise while copying: `b < 0x20 || b == 0x7F` ⇒ `b'_'`; `b >= 0x80` passes through untouched so UTF-8
  survives. Cap at `MAX_BODY` **before** the CRC; set `truncated` when the input was longer. Per parent §3's
  amendment the caller counts that as `trunc`, never as a dropped record.
- Why sanitise rather than escape: byte-stuffing is how you make a reserved delimiter unambiguous for a
  receiver that lost bytes (COBS, Cheshire & Baker; SLIP RFC 1055 / PPP RFC 1662 lineage), and COBS buys
  instant resynchronisation at the price of a capture nobody can read — the opposite of what this project
  needs (parent §3 rejects binary framing for exactly that reason). Substituting at the producer is also the
  recognised fix for log injection / CWE-117. **NMEA-0183 is the closest published analogue** to v1 and
  independently validates the shape: printable ASCII sentence, start char, `*` + checksum over everything
  between the delimiters, mandatory CRLF, and a hard maximum sentence length as part of the grammar rather
  than a convention (<https://www.plaisance-pratique.com/IMG/pdf/NMEA0183-2.pdf>).

### Golden frames — assert these byte-for-byte

All five were computed with the pinned parameters; do not copy any checksum from prose, including the
illustrative `*2f9e` in parent §3's example line, which is **bogus**.

| call | expected bytes | len |
|---|---|---|
| `crc16_ccitt(b"123456789")` | `0x29b1` | — |
| `encode(Info, 0x42, 4567, b"ENC +1")` | `~I 00000042 00004567 ENC +1*9c17\r\n` | 34 |
| `encode(Debug, 0, 0, b"")` | `~D 00000000 00000000 *91d4\r\n` | 28 |
| `encode(Warn, 0xdead_beef, 0x9999_9999, "knob r2=298 ✓".as_bytes())` | `~W deadbeef 76980377 knob r2=298 ✓*b321\r\n` | 43 |
| `encode(Trace, 0, 0, &[b'a'; MAX_BODY])` | …`*6c90\r\n` | 228 |

The third row is the 28-byte overhead proof; the fourth exercises the `t_ms` modulo
(`0x9999_9999 = 2 576 980 377 → 76980377`) *and* UTF-8 pass-through in one vector; the fifth is the
`MAX_FRAME` boundary. Give each case its own `#[test]` so a failure names the drifted field, following the
`golden_cases!` rationale in `crates/asperitas-cli/tests/golden_tests.rs:95-107`.

Also assert the sanitiser directly: a body of `b"cr\r\nlf\x00\x7f~*|"` becomes `cr__lf__~*|` — note `~`,
`*`, `|` **survive**. That is deliberate (§3 sanitises only control bytes) and it is why the decoder's
strength comes from the CRC plus the no-CR/LF invariant, not from a tilde-free payload. TASK-030.01.02's
tests depend on this fact, so leave it stated here rather than "fixing" it.

## 5. `write_whole` — the whole-record-or-nothing commit

```rust
pub fn write_whole(frame: &[u8], free_capacity: usize,
                   write: impl FnMut(&[u8]) -> Option<usize>) -> bool
```

Generic over the sink so the host can exercise the real algorithm: the device supplies
`|c| LOG_PIPE.try_write(c).ok()`, the test supplies a closure over a real `Pipe`. No statics, no embassy
types in the signature.

Exactly two outcomes, no third: `false` means **refused, zero bytes written**; `true` means every byte is in
the sink. Pre-check `frame.len() <= free_capacity`, then loop `write` until the whole frame is in. The loop
is required, not defensive: `RingBuffer::push_buf` returns only the contiguous run to the end of the backing
array (`embassy-sync-0.6.2/src/ring_buffer.rs:19-31`) while `free_capacity()` reports total free
(`pipe.rs:456`), so `try_write` short-writes at **every ring wrap even when the ring is empty** — measured:
empty ring, `free_capacity() == 512`, a 200-byte write accepted 112 (parent §2). Two rounds always suffice
because after crossing the wrap the contiguous run equals total free and the consumer can only increase
free space. A mid-loop stall (`Some(0)` / `None` after progress) is therefore impossible given the pre-check
plus the caller's lock: make it a `debug_assert!` and let release builds finish the loop. If an embassy
change ever made it possible, the leftover fragment is caught by the reader's CRC and counted as an
integrity failure rather than masquerading as data — which is precisely what v1 buys.

Reserve/commit-with-discard is standard vocabulary in serious ring buffers — Linux's ring buffer and BPF
`reserve → commit|discard`, printk `prb_reserve/prb_commit`, bitdrift's reserve/commit buffer
(<https://docs.kernel.org/trace/ring-buffer-design.html>) — so keep the two-outcome contract exactly that
shape; do not add a third return value or a partial-write count.

## 6. Tests (inline `#[cfg(test)] mod tests`)

Style to match: descriptive sentence names and a rationale comment beside anything non-obvious
(`crates/asperitas-pod/src/encoder.rs:405-425`). No `ProptestConfig` idiom exists in this repo yet —
`crates/asperitas-dsp/tests/property_tests.rs` uses plain `proptest! {}` with defaults; that matters to
TASK-030.01.02, not here.

1. Golden frames and the check vector, one `#[test]` each (table above).
2. `sanitiser_replaces_control_bytes_but_keeps_printable_punctuation` — includes `~`, `*`, `|`, high bytes.
3. `over_long_body_is_capped_before_the_crc_and_reports_truncation` — decode the returned bytes by hand in
   the test (slice `out[..enc.len]`, take `out[len-6..len-2]` as the hex field, recompute `crc16_ccitt` over
   `out[1..len-6]`) and assert the CRC matches the shipped record. Assert `truncated == true` and
   `len == MAX_FRAME`, and that a 200-byte body gives `truncated == false`.
4. `write_whole_refuses_a_frame_that_does_not_fit_without_writing_anything` — counter closure asserts zero
   calls.
5. `write_whole_commits_across_the_ring_wrap` (AC #5): `static PIPE: Pipe<NoopRawMutex, 512>` — plain
   `static`, no `StaticCell`, no `&raw mut`: parent §2 verified `Pipe` is `Sync` and works through `&self`
   alone. Write 400, drain 400, then `write_whole(a_200_byte_frame, pipe.free_capacity(), …)` must return
   `true` and all 200 bytes must read back contiguously. **Use `NoopRawMutex`, never
   `CriticalSectionRawMutex`, in host tests**: with no `critical-section` impl registered for host the
   acquire symbol is an undefined-symbol link error, and the `std` fallback is not re-entrant (parent §2).
6. `write_whole_rounds_are_byte_exact_under_randomized_interleaving` — port `/tmp/pipecheck2/src/main.rs`
   (~20 lines; it re-ran green during this planning pass: `commits=19799 drops=201
   bytes_expected=2461609 bytes_got=2461609`): randomized producer/consumer rounds, asserting the
   concatenation of committed frames equals the bytes read back, exactly, in order, with no partial ever
   observed. Use `proptest` or a seeded xorshift — either is fine, but the assertion is on **bytes**, not on
   counts.

## 7. Gates (identical to CI, `.github/workflows/ci.yml:19-38`)

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings
cargo test --workspace
cargo test --workspace --features asperitas-pod/pod-hw
cd firmware && cargo build --release --features seed3
```

Compiling is not evidence. The evidence for this child is the five golden frames, the truncation test whose
CRC still validates, and a `write_whole` suite that drives a real embassy ring buffer across the wrap.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in crates/asperitas-logging/src/frame.rs (producer half) + pub mod frame in lib.rs, outside every cfg; [dev-dependencies] proptest/embassy-sync added to Cargo.toml (root Cargo.lock gained one dependency-list line; firmware/Cargo.lock shows no diff).

Evidence (cargo test -p asperitas-logging, default features, log-usb off): 15 unit tests green.
- crc16_ccitt(b"123456789") == 0x29b1 and crc("") == 0xFFFF.
- All five golden frames byte-exact: ~I 00000042 00004567 ENC +1*9c17 (34 B), ~D 00000000 00000000 *91d4 (28 B), ~W deadbeef 76980377 knob r2=298 U+2713*b321 (43 B, pins t_ms modulo AND UTF-8 pass-through), max body -> *6c90 at exactly 228 B, plus a per-level letter test covering E/W/I/D/T. Re-derived independently against /tmp/refmodel.py before writing the literals; none copied from prose.
- Sanitiser: b"cr\r\nlf\x00\x7f~*|" -> cr__lf__~*| (~ * | survive); full sweep asserts 0x20..0x7E and 0x80..0xFF untouched, 0x00..0x1F and 0x7F -> '_'.
- Capping: 300-byte body -> len == MAX_FRAME, truncated == true, and the shipped CRC recomputed over out[1..len-7] matches the transmitted digits (cap happens before the CRC); exactly MAX_BODY gives truncated == false.
- write_whole: refusal path asserts zero sink calls; success path driven one byte per call; ring-wrap test uses a real embassy-sync Pipe<NoopRawMutex, 512> (fill 400, drain 400, free_capacity()==512, commit a 200-byte encoded frame whole, read back byte-exact). A separate test pins the embassy behaviour the loop depends on: raw try_write of 200 into that 'empty' ring accepts 112. Randomized rounds: 20 000 producer/consumer rounds, commits and drops both exercised, committed bytes == bytes read back exactly.

Two deviations from the plan, both verified rather than assumed:
1. Plan section 6 item 5 asked for `static PIPE: Pipe<NoopRawMutex, 512>`. That does not compile: blocking_mutex::Mutex is Sync only when R: Sync, and NoopRawMutex is !Sync (PhantomData<*mut ()>). The pipe is therefore a test-local borrowed for the test's duration. On target the static form works because CriticalSectionRawMutex has an explicit unsafe impl Sync (and it cannot link on host anyway).
2. Claimed with status only, assignee left @agent, because CLAUDE.md's @agent/@human convention overrides the skill's '-a @ralph'; backlog/unblocked-todo.sh filters on @agent.

Gates: cargo fmt --all --check clean; clippy --workspace --all-targets -D warnings clean (both with and without asperitas-pod/pod-hw); cargo test --workspace green; firmware release cross-compile with seed3 finished; firmware/Cargo.lock unchanged.
<!-- SECTION:NOTES:END -->
