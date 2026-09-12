---
id: TASK-038.03.02.01
title: >-
  Add rig console verbs and one whole-record emit path to asperitas-logging,
  with host tests
status: Done
assignee:
  - '@agent'
created_date: '2026-09-11 15:53'
updated_date: '2026-09-12 05:19'
labels:
  - task
  - planned
dependencies: []
modified_files:
  - crates/asperitas-logging/src/console.rs
  - crates/asperitas-logging/src/frame.rs
  - crates/asperitas-logging/src/lib.rs
parent_task_id: TASK-038.03.02
priority: high
type: task
ordinal: 83600
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The crate-side half of TASK-038.03.02 — and the only half that can be tested with no board attached.

`RIGCFG`, `RIGGEN`, `CAPSTAT`, `CAPMAX` and `DUMPEND` are wire grammar. The convention at `console.rs:171-178` is that a verb's field set and field order are a contract pinned by a unit test **in that same file**, because the firmware package has no host-test target: bodies written ad hoc in `rig.rs` would ship with no test at all. This leaf lands those five builders, their pinning tests, and the one public entry point `rig.rs` will commit them through.

The verb set is five rather than the four parent plan §2 first sketched, because that sketch could not fit its own byte limit: `CAPSTAT` saturated at 284 bytes against `frame::MAX_BODY` = 200, and `RIGCFG` carrying the generator's free text reached ~291. Parent §2 was corrected on 2026-09-12 with the measured table; read it before writing a builder.

Two smaller things live here for the same reason:

**A bound the sibling leaf's rate gate needs.** Parent plan §8 gates `CAPSTAT` traffic against the dump's own byte budget, and the gate is only honest if the number it divides by belongs to the crate that renders the record — so `CAPSTAT_MAX_BODY` is defined next to its builder and checked by a saturated-render test, not typed into `rig.rs` by hand.

**An incremental CRC.** Parent plan §7 step 3 emits `AUDEND` with a CRC over the block's raw bytes in chunk order, but the dump writer holds one `dump::CHUNK_RAW` slice at a time and never materialises the block. `frame.rs:204` offers only `crc16_ccitt(data) -> u16` over a whole buffer, so chaining chunks is impossible today; this leaf adds the update form and proves it agrees with the whole-buffer form.

Out of scope, owned by TASK-038.03.02.02: `firmware/src/bin/rig.rs` and everything in it, `firmware/Cargo.toml` features, `.github/workflows/ci.yml`, the SDRAM memory-model note in `docs/reference/daisy-seed3.md`, and the stale `steal()` comment in `spin_budget.rs`. No file under `firmware/` changes here.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `crates/asperitas-logging/src/console.rs` gains five body builders beside `status_body` — `rigcfg_body`, `riggen_body`, `capstat_body`, `capmax_body`, `dumpend_body` — each following `status_body`'s shape exactly (`TruncWriter` + `core::write!`, `proto=1` second, every field in one format string, returning `w.filled()`), rendering the corrected field sets and orders in parent plan §2 verbatim, and each pinned by a host unit test in that file naming every field in order. `riggen_body` carries the generator's `describe()` bytes last and publishes `RIGGEN_MAX_GEN_BYTES = 160` as the budget its caller clips to.
- [x] #2 Each of the five builders has a saturated-counters test asserting its worst-case render is shorter than `frame::MAX_BODY`, modelled on `status_body_renders_saturated_counters_as_u32_max` (`console.rs:327`), which exists because a 255-byte body silently truncating at 200 is the failure nobody notices until a host parser rejects it. In addition, a `const fn` worst-case-length helper plus `const _: () = assert!(…)` lines prove each verb's *field table* fits one record at compile time, so a builder drifting from the table fails the build rather than a nightly test run.
- [x] #3 `console.rs` publishes `pub const CAPSTAT_MAX_BODY: usize = crate::frame::MAX_BODY;` beside its builder (an alias, so the two numbers cannot drift; its value stays 200), and the saturated test asserts both `len < CAPSTAT_MAX_BODY` and `CAPSTAT_MAX_BODY <= frame::MAX_BODY`, so TASK-038.03.02.02's rate gate divides by a number this crate owns and a host test checks.
- [x] #4 One public whole-record entry point `pub fn emit_record(level: Level, now_ms: u32, body: &[u8]) -> bool` exists in `lib.rs`, gated `#[cfg(feature = "log-usb")]` like `try_emit_dump`, and reaches the wire through the existing `commit_records` (`lib.rs:368`) — `RECORD_BUFS`, `CONSOLE.take_seq()`, `frame::encode`, `frame::write_whole(framed, LOG_PIPE.free_capacity(), …)`, outcome counted. It is built by splitting `emit` into a private `emit_at(level, now_ms, fill)` that `emit` calls after reading `Instant::now()` outside the lock, so the crate keeps exactly one place that formats, frames and commits an ordinary record and `commit_records(` stays at two call sites (`emit_at`, `try_emit_dump`). Nothing added calls `usb::emit_blocking`.
- [x] #5 `frame.rs` gains `pub fn crc16_ccitt_update(crc: u16, data: &[u8]) -> u16`, and `crc16_ccitt(data)` becomes `crc16_ccitt_update(CRC16_INITIAL, data)` with `CRC16_INITIAL` published, so there is one implementation of the polynomial. A host test chains a real 32 KiB block at `dump::CHUNK_RAW` boundaries and asserts the chained result equals the whole-buffer result, including for the short final chunk. Because `fn crc16_ccitt(` is listed in `tests/commit_path_no_panic.rs`'s `FRAME_FNS`, that test must be updated in the same commit: add `crc16_ccitt_update` to `FRAME_FNS` and repoint the positive-control needle that followed `0x1021` into the function where the polynomial now lives. Editing that test is required here; leaving it untouched would mean hiding the new call from the scanner.
- [x] #6 All gates pass and their outputs go in the finalization notes: `cargo fmt --all --check`; `cargo test --workspace`; `cargo clippy --workspace --all-targets -- -D warnings`; `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps` in default and `--all-features` forms; `cargo build -p asperitas-logging --target thumbv7em-none-eabihf` in both default and `--features log-usb` forms (only the latter type-checks `emit_record`); and both firmware release builds (`--features seed3`, `--no-default-features --features "seed3 log-defmt"`) with `size -B` for `main` and `podtest` recorded before and after, since these are additive exports no binary calls yet.
- [x] #7 `git diff --name-only HEAD` lists nothing outside `crates/asperitas-logging/`.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
### Read first

- **Parent TASK-038.03.02 §2, as corrected on 2026-09-12**, plus its Implementation Notes entry "§2's grammar could not satisfy its own byte limit". The corrected verb table there is the contract for this ticket.
- `crates/asperitas-logging/src/console.rs`: `status_body` (:206) and its two tests (`status_body_pins_...`, `status_body_saturated_counters_fit_one_frame` :327) are the template; body-budget consts :35-44; `TruncWriter` :107; `MAX_STR_BYTES` :23.
- `crates/asperitas-logging/src/lib.rs`: `RECORD_BUFS` :230, `UsbClock` :241, `commit_records` :368, `emit` :420 (note its `fill: impl FnOnce(&mut [u8; console::BODY_WINDOW]) -> usize` and the clock-before-lock comment), `try_emit_dump` :444, `emit_status` :535.
- `crates/asperitas-logging/tests/commit_path_no_panic.rs`: `FRAME_FNS` :279-283, the closures test :345, the positive controls :359-368. Read it before touching either the emit path or the CRC.
- `crates/asperitas-dsp/tests/stimulus_tests.rs:100-107`: the three pinned `describe()` strings, which are `RIGGEN`'s real payload size (83 / 92 / 97 bytes).
- `capture.rs` geometry consts :45-85, and `ring_duration_micros` :233 / `total_capture_bytes` :243 / `unused_headroom_bytes` :252 for what `.02` will pass in.

### Why these field sets differ from parent §2's original draft (do not "restore" them)

Every rig verb is one record, so each verb's *worst-case* render must be under `frame::MAX_BODY` = 200. Measured at `u32::MAX` (ten digits per numeric field):

| verb | as drafted | as shipped |
| --- | --- | --- |
| `CAPSTAT` | **284 B** (thirteen fields) | **187 B** (eight) |
| `RIGCFG` | **≈291 B** (carried `describe()`) | **177 B** (numeric only) + `RIGGEN` ≤ 175 B |
| `CAPMAX` | 133 B | unchanged |
| `DUMPEND` | 155 B | 194 B (gains `refused`, `stall_ms`) |

The rule behind every cut: **a fact appears on the wire once.** `sent` and `bytes_dropped` belong to `STATUS`, which also carries `seq_next` so an interval is still bracketable; `free` is `RING_BLOCKS − (delivered − dumped)` minus the block being filled, from published constants; `refused` and `stall_ms` describe one dump, so they moved to `DUMPEND`; `dumping` became `dumped`, because progress is a count of completed blocks rather than an index. Parent AC #5 still holds: `CAPSTAT` carries dump progress and `dropped_full`.

The generator text got its own verb because a body mixing one unbounded string with ten numbers has no checkable bound: `RIGCFG`'s numeric fields alone leave 22 bytes, less than the default `pulse_train` string (97). If you want a field back, do the arithmetic first. Remaining slack: `CAPSTAT` 13 B, `DUMPEND` 6 B, `RIGCFG` 23 B.

### Ship in `console.rs`

```rust
pub enum MonoLane { Left, Right }                    // renders lane=L|R; keeps garbage off the wire
pub struct RigConfig { pub lane: MonoLane, pub blocks: u32, pub block_bytes: u32,
                       pub bytes_per_s: u32, pub capsec_us: u32, pub window_s: u32,
                       pub cpu_hz: u32, pub icache: bool, pub dcache: bool }
pub fn rigcfg_body(cfg: &RigConfig, out: &mut [u8; BODY_WINDOW]) -> usize

pub const RIGGEN_MAX_GEN_BYTES: usize = 160;         // header (15) + this = 175 < MAX_BODY
pub fn riggen_body(describe: &[u8], out: &mut [u8; BODY_WINDOW]) -> usize   // text last, clipped to the budget

pub struct CaptureStatus { /* delivered, expected, overrun, max_block_us, worst_gap_us,
                              audio_exit, dumped, dropped_full: all u32 */ }
pub fn capstat_body(st: &CaptureStatus, out: &mut [u8; BODY_WINDOW]) -> usize

pub struct RingCapacity { /* total_bytes, ring_bytes, seconds_max, us_max, unused_headroom_bytes */ }
pub fn capmax_body(cap: &RingCapacity, out: &mut [u8; BODY_WINDOW]) -> usize

pub struct DumpSummary { /* blocks, chunks, bytes, elapsed_ms, refused, stall_ms,
                            sent, dropped_full, bytes_dropped */ }
pub fn dumpend_body(d: &DumpSummary, out: &mut [u8; BODY_WINDOW]) -> usize

pub const CAPSTAT_MAX_BODY: usize = crate::frame::MAX_BODY;   // AC #3; never retype 200
```

Structs, not positional arguments: `status_body(&ConsoleCounters)` is the precedent, and eight same-typed `u32`s at a call site invite transposition. No lifetime parameter anywhere, because the only borrowed data (the generator text) now lives in its own verb. Numeric fields stay `u32` for wire uniformity even where the source is wider (`total_bytes` tops out at 67 108 864, `us_max` at 349 525 333); say so in the struct docs so nobody reads `u32` as a claim about ring size.

Next to the builders, publish the bound at compile time, not only in a test: a `const fn saturated_len(prefix: &str, fields: &[(&str, usize)]) -> usize` summing `1 + name.len() + 1 + digits` per field, then five `const _: () = assert!(X_WORST < crate::frame::MAX_BODY);` lines and one for `RIGGEN`'s header plus budget. Precedent for arithmetic-as-proof in this repo: `dump.rs:146 check_decimal_fits`. The runtime saturated tests then prove the *builder* still matches the table; only the const asserts prove the table fits, which is the half a drifted format string breaks.

Tests, all in `console.rs`'s module, following `status_body_*`: one exact-string pin per verb naming every field in order, one saturated render per verb (`< frame::MAX_BODY`; for `capstat` also `< CAPSTAT_MAX_BODY` and `CAPSTAT_MAX_BODY <= frame::MAX_BODY`), a `riggen` test showing a full 160-byte payload frames untruncated through `frame::encode` and one showing a longer input clips without panicking, and extend `boot_and_status_bodies_survive_the_wire_codec` (:361) to all five new bodies. Use dsp's pinned sine string as the realistic `describe()` literal.

### Emit path (AC #4): split `emit`, do not add a second funnel

`emit` already does everything except choose `now_ms` itself.

- Private `fn emit_at(level: Level, now_ms: u32, fill: impl FnOnce(&mut [u8; console::BODY_WINDOW]) -> usize) -> bool` takes today's body verbatim, clock comment and all, and returns `commit_records`' bool.
- `emit(level, fill)` becomes `emit_at(level, embassy_time::Instant::now().as_millis() as u32, fill)`. The `Instant::now()` read stays outside the lock, exactly as the comment at :421-426 demands.
- `#[cfg(feature = "log-usb")] pub fn emit_record(level: Level, now_ms: u32, body: &[u8]) -> bool` = `emit_at(level, now_ms, |w| { let n = body.len().min(console::BODY_WINDOW); w[..n].copy_from_slice(&body[..n]); n })`. Silent clamping matches `status_body`'s truncation convention, and `frame::encode` counts it in `trunc`; no rig builder can reach it, since all five are provably under `MAX_BODY`.

Why the copy rather than a third `commit_records` closure: it keeps exactly one place in the crate that formats, frames and commits an ordinary record, keeps `commit_records(` call sites at two (`emit_at` and `try_emit_dump`) so `commit_path_no_panic`'s closure assertions keep their meaning, and costs one ≤256-byte memcpy per rig record against a CRC that runs ~1 760 shift-xor steps per chunk. `try_emit_dump` keeps its own closure because its `dump_fits` headroom rule is different by design. Update the `lib.rs` docs to name `emit_record` beside `try_emit_dump`, and `console.rs`'s module line claiming "the two record bodies whose bytes are a wire contract (BOOT, STATUS)".

### CRC (AC #5)

`crc16_ccitt(data)` becomes `crc16_ccitt_update(CRC16_INITIAL, data)`, with `pub const CRC16_INITIAL: u16 = 0xffff;` published (style precedent: `dump.rs`'s `SEQ_UNDEFINED`). Chaining is sound because this is CRC-16/CCITT-FALSE (poly `0x1021`, init `0xffff`, no reflection, no final xor, check `0x29b1`): running the register forward over concatenated bytes equals running it over each piece in turn. Put that reasoning in the doc comment, since reflection and absent-final-xor are precisely what make it true.

Test home: `tests/capture_geometry.rs` beside `a_real_255_chunk_block_costs_what_the_module_claims` (:189), which already walks a real `[0xA5; 32768]` block at `CHUNK_RAW` boundaries (254 full chunks plus a 2-byte tail) and calls `crc16_ccitt` today. Convert it to the chained form and assert equality with the whole-block call; add an all-split-points sweep on a small buffer, not on 32 KiB. Leave `crc16_ccitt_matches_reference` (:238) untouched as the oracle.

### `commit_path_no_panic.rs` must change (AC #4's "test unchanged" is impossible)

`fn crc16_ccitt(` sits in `FRAME_FNS` (:279-283), i.e. it is scanned as something reachable under the record lock. Delegating to `crc16_ccitt_update` therefore makes `no_unscanned_callees_reaches_the_record_lock` fail (:248-251). Two honest edits:

1. Add `"fn crc16_ccitt_update("` to `FRAME_FNS` (→ `[&str; 8]`), with a comment saying it is scanned because the `AUDEND` CRC reaches it through `crc16_ccitt`.
2. Repair the positive controls (:359-368). Each pairs a function name with a needle inside it, and `("fn crc16_ccitt(", "0x1021")` goes stale the moment the polynomial moves. Point that pair at `fn crc16_ccitt_update(`, where `0x1021` now lives, and give `crc16_ccitt` a needle it still contains, such as `CRC16_INITIAL`. A control that passes vacuously is worse than none: that test exists to prove the scanner is not blind.

Do not dodge the finding by hiding the call behind an impl method or a `Self::` alias; the file's whole claim is that everything reachable from the lock has been read. Two mechanics worth knowing: `masked()` blanks comments, so prose mentioning forbidden tokens is safe, and `"fn crc16_ccitt("` is not a substring of `"fn crc16_ccitt_update("`, so region matching stays correct. `emit_at`'s closure still contains `fill(`, so the existing positive control for the emit producer keeps working and `emit_record` adds no closure.

### Disposition of `stash@{0}` (review, then drop)

`git stash show -p 'stash@{0}'` ("wip-038.03.02-uncommitted", 687 insertions across `console.rs`, `frame.rs`, `lib.rs`, `spin_budget.rs`, `tests/commit_path_no_panic.rs`) came from the two attempts that died at the parent. Per hunk:

- **Reuse nearly as-is:** the `frame.rs` CRC restructure and its prose; the `CAPSTAT_MAX_BODY` alias with its const assert; `TruncWriter` usage patterns.
- **Rewrite:** its `CAPSTAT`/`RIGCFG` field sets. It cut the right *kind* of fields but silently, and its arithmetic does not reproduce (it claims 226 B for a shape that renders 219 B here), and it kept `describe()` inside `RIGCFG`, where its own assertion (`n + 10 < MAX_BODY`) is violated by the default `pulse_train` string. Adopt the cuts above deliberately, one verb at a time, with the numbers in the commit message.
- **Drop:** its `spin_budget.rs` hunk (owned by `.02`) and any rename it invented without a measured reason.

Drop the stash once your commits land. Do not archive it; an archived stub sharing IDs with real tasks shadows them.

### Gates, in commit order

The parent died twice at the 40-minute deadline with zero commits, so land three green commits rather than one big one, and ping over intercom before each long-running command.

1. Verbs + budget consts + const asserts + pin/saturated/wire tests → `cargo fmt --all --check`, `cargo test -p asperitas-logging --features log-usb console`. Commit.
2. CRC split + chaining tests + the two `commit_path_no_panic` edits → `cargo test -p asperitas-logging --features log-usb`. Commit.
3. `emit_at` / `emit_record` + doc updates → `cargo test -p asperitas-logging --all-features`, then the cross builds. Commit.

Final gate list; paste outputs into the finalization notes:

- `cargo fmt --all --check`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` (this crate is lint-gated; `firmware/Cargo.toml` has no `[lints]` table, so nothing you want enforced travels there)
- `cargo clippy -p asperitas-logging --all-targets --all-features -- -D warnings -D dead_code -D unused_variables`
- `RUSTDOCFLAGS="-D warnings" cargo doc -p asperitas-logging --no-deps --all-features`
- `cargo build -p asperitas-logging --target thumbv7em-none-eabihf` **and** the same with `--features log-usb` (both confirmed working today, ~9 s warm; the default-feature form alone never type-checks `emit_record`)
- `cd firmware && cargo build --release --features seed3`, then `size -B target/thumbv7em-none-eabihf/release/{main,podtest}` before and after. Record the `size -B` output, not binary file sizes. Baselines on disk now: `main` 3 431 784 B, `podtest` 3 197 120 B.
- `grep -rn "usb::emit_blocking" crates/asperitas-logging/src` must print nothing.

### Hand-off to `.02` (put these in your finalization notes)

- The signatures exactly as shipped, and `CAPSTAT_MAX_BODY`'s value, which your §8 rate gate divides by.
- `RIGGEN_MAX_GEN_BYTES` is the number `rig.rs` debug-asserts `describe()`'s returned length against. dsp's own budget test only guarantees `< 200`, so do not rely on it.
- `capture::ring_duration_micros()` returns `u64` and `total_capture_bytes()` returns `usize`, while the builders take `u32`. Narrow at the call site and say in a comment why the value cannot overflow.
- `icache`/`dcache` stayed in `RIGCFG`, per parent §2. The stash moved them to `CAPMAX` for space; we did not need to.

### Out of scope

`firmware/src/bin/rig.rs`, `firmware/Cargo.toml`, CI, `spin_budget.rs`, and any `asperitas-dsp` change (AC #7 keeps the diff inside `crates/asperitas-logging/`). In particular, do not shorten `describe()` in dsp to buy room: measure first, and if the budget is genuinely wrong, bring the number back to this plan.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Landed 2026-09-12 in three commits: `951b660` (verbs), `22714b0` (CRC chain), `f0b4e18` (`emit_record`). Diff touches six files, all under `crates/asperitas-logging/`.

### Shipped surface (hand-off to TASK-038.03.02.02)

Builders, all `pub`, all in `console.rs`, all taking `(…, out: &mut [u8; BODY_WINDOW])` and returning `usize`:

```
rigcfg_body(cfg: &RigConfig, out)   -> usize   worst 177 B
riggen_body(describe: &[u8], out)   -> usize   worst 175 B, budget RIGGEN_MAX_GEN_BYTES = 160
capstat_body(st: &CaptureStatus, out) -> usize worst 187 B, budget CAPSTAT_MAX_BODY = 200
capmax_body(cap: &RingCapacity, out) -> usize  worst 133 B
dumpend_body(d: &DumpSummary, out)  -> usize   worst 194 B
```

Struct-first / `out`-last, matching `boot_body` rather than `status_body`'s older argument order, because five new builders set the convention going forward and `status_body` is one call site from being restyled. Payload structs are `pub` with `pub` fields (`RigConfig`, `CaptureStatus`, `RingCapacity`, `DumpSummary`) so `rig.rs` can build them as literals; `MonoLane` is a two-variant `pub enum` and its `.letter()` stays crate-private, since the verb renders it.

`lib.rs` gains `pub fn emit_record(level: Level, now_ms: u32, body: &[u8]) -> bool`, `log-usb`-gated, alongside `emit_at(level, now_ms, fill)` which now carries everything `emit` used to do except reading `Instant::now()`. `commit_records(` has exactly two call sites (`lib.rs:491` via `emit_at`, `lib.rs:648` from `try_emit_dump`).

`frame.rs` gains `pub const CRC16_INITIAL: u16 = 0xFFFF` and `pub fn crc16_ccitt_update(crc: u16, data: &[u8]) -> u16`; `crc16_ccitt(data)` is now `crc16_ccitt_update(CRC16_INITIAL, data)`. Chaining is sound here specifically because this CRC has no refin/refout/xorout, so the register after *n* bytes is the whole-buffer state at *n*. Proven by `chaining_the_crc_at_any_split_matches_one_shot` (`tests/capture_geometry.rs:237`): every split point of the `0x31…0x39` check vector, then a real 32 KiB block accumulated at `dump::CHUNK_RAW` boundaries including the short tail.

### Two things in the plan that turned out not to be true

**`capture::total_capture_bytes()` does not exist.** The hand-off bullet above says it returns `usize`; `capture.rs` publishes no such function. Its actual byte-producing functions are `wire_bytes_per_block()`, `tail_frame_bytes()`, `audend_frame_bytes()` and `ring_duration_micros() -> u64`. So `RIGCFG.blocks`/`block_bytes` and `CAPMAX.total_bytes`/`ring_bytes` must be computed in `rig.rs` from the SDRAM allocation it actually got, not read off `capture`. Do not go looking for the missing function.

**A runtime assertion on `CAPSTAT_MAX_BODY <= frame::MAX_BODY` cannot fail**, being a comparison of two constants, and `clippy::assertions_on_constants` says so. That half moved to a `const _: () = assert!(…)`; the test keeps the assertion that *is* contingent, namely the saturated render's length.

Also corrected while here: two comments still described the AUDEND path as going through `emit()` after TASK-038.03.01.02 moved it to `emit_at` (`frame.rs:466`, top of `usb.rs`). Both inside this crate, so AC #7 still holds.

### Gates

`cargo fmt --all --check` clean. `cargo test --workspace`: 277 tests, 17 suites, 0 failures. `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean, plus the stricter `-p asperitas-logging --all-targets --all-features -- -D warnings -D dead_code -D unused_variables`. `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps` clean in default and `--all-features` forms. `cargo build -p asperitas-logging --target thumbv7em-none-eabihf` clean with and without `--features log-usb`; only the latter type-checks `emit_record`. `grep -rn "usb::emit_blocking("` over `src/` prints nothing; the three remaining mentions of that name are prose in doc comments written before TASK-038.03.01.02 and predate this ticket.

Doc links needed care: `[`emit_record`]` and `[`try_emit_dump`]` in the crate header resolve only with `log-usb`, and a link from a `pub fn` to the private `emit_at` trips `private_intra-doc-links`, so those four references are code text. Same reason the saturated-test names are code text: rustdoc does not see `#[test]` functions.

The scanner edit AC #5 demanded is not decorative: `FRAME_FNS` went to 8 entries with `fn crc16_ccitt_update(` added and the positive-control needle repointed (`0x1021` now sits only in `crc16_ccitt_update`, `CRC16_INITIAL` only in `crc16_ccitt`). Verified non-vacuous by removing the new entry and watching two tests fail before restoring it.

### Size, before and after

Measured against `def7177` in a throwaway worktree, `size -B` on the ELF sections (the plan's "baselines on disk" were `ls` byte counts, which are not comparable). Baseline / after, `text`:

| config | main | podtest |
|---|---|---|
| `--features seed3` | 88173 -> 88269 (+96) | 72133 -> 72221 (+88) |
| `--no-default-features --features "seed3 log-defmt"` | 47616 -> 47624 (+8) | 31460 -> 31468 (+8) |

`data` and `bss` unchanged in all four. Nothing calls the new exports, so the +96/+88 in the USB form is the `log-usb` commit path this crate compiles for `emit_record`; the uniform +8 in the defmt form is the CRC split, the one change reachable from a binary that cannot see USB.

Gotcha worth recording: both feature forms build into the same `target/thumbv7em-none-eabihf/release/`, so the second build overwrites `main` and `podtest`. Measure between the builds, not after both - the first pass of this produced a baseline that was silently the defmt binary twice.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-12 04:02
---
Re-planned 2026-09-12. Parent plan §2 could not satisfy its own byte limit: saturated CAPSTAT renders 284 B and RIGCFG-with-describe() ~291 B against frame::MAX_BODY = 200, so AC #1's 'render §2 verbatim' and AC #2's saturated < 200 test were jointly unsatisfiable. §2 is corrected upstream with measured numbers; this ticket now ships five verbs (RIGGEN carries the generator text behind a named 160-byte budget), a compile-time field-table bound, and the emit_at funnel. Also recorded here: AC #4 originally demanded tests/commit_path_no_panic.rs pass unchanged, which the CRC split makes impossible by construction, since fn crc16_ccitt( is itself in that test's FRAME_FNS scan list.
---
<!-- COMMENTS:END -->
