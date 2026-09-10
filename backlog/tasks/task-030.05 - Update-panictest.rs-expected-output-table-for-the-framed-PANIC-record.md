---
id: TASK-030.05
title: Update panictest.rs expected-output table for the framed PANIC record
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-09 09:55'
updated_date: '2026-09-10 06:20'
labels:
  - planned
dependencies:
  - TASK-030.02
parent_task_id: TASK-030
priority: medium
type: task
ordinal: 54500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-030.02 wrapped the panic message in a self-verifying frame (TASK-030 §3 grammar), so the last line a developer sees on a raw terminal now carries the ~E prefix, seq, t_ms and *crc trailer. panictest.rs is the binary whose whole purpose is checking that line against a known-good expectation, and its header still describes the unframed form — the next person to run it will read '~E 0000000c 0000001512 PANIC: ...' as a defect.

Blocked from doing this inside TASK-030.02 by that ticket's AC #8, which forbids any change under firmware/ (verified with git diff --stat firmware/). Doc-comment-only edit; no code or call site changes. The board confirmation that the framed PANIC line arrives and decodes is TASK-030.04 / TASK-033, not this ticket.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The 'Reading the output' table in firmware/src/bin/panictest.rs shows the panicked stage's serial line as a console v1 frame (~E <seq> <t_ms> PANIC: <msg> at src/bin/panictest.rs:L:C*<crc> CRLF) rather than the bare PANIC: text, and names usb::emit_panic_record as what pushes it.
- [ ] #2 cargo fmt --all --check from the root AND inside firmware/ (the root command excludes firmware/ entirely), cargo clippy --workspace --all-targets -- -D warnings, and `cd firmware && cargo build --release --features seed3` (the exact ci.yml:44 command) all pass. Do not substitute `cargo build --manifest-path firmware/Cargo.toml ...`: measured this pass, that form exits 0 while never loading firmware/.cargo/config.toml, so it links without -Tlink.x and leaves an ELF with start address 0x0 and no .vector_table - a green that proves nothing. Only doc comments in the binary change: `git diff -U0 firmware/ | grep -E '^[+-][^+-]' | sed -E 's/^.//' | grep -vE '^//'` prints nothing.
- [ ] #3 All three rows of the table (Boot, Countdown, Panicked) show console v1 records byte-faithful to the shipped encoder apart from <...> placeholders, with the '*' flush against the body, and the framing preamble states that ~<level> <seq> <t_ms> <body>*<crc> plus CRLF is the form and points at docs/reference/daisy-seed3.md Console protocol v1.
- [ ] #4 The bullets after the table name both halves honestly - usb::emit_panic_record frames the record without the record lock, usb::emit_blocking drives the endpoint with the ring bypassed - written as code text rather than intra-doc links (this crate's `usb` resolves to the HAL module via the use at panictest.rs:57, and `pub mod usb` sits behind feature = log-usb, so either link form would break under RUSTDOCFLAGS=-D warnings); give the seq arithmetic (BOOT 0, USB connected 1, countdown 2-11, panic normally 0000000c) without pinning a constant; explain the doubled `panictest:` prefix; and record the 128-byte PANIC_MSG_BUF headroom trap where a truncated body still carries a valid CRC and trips no counter.
- [ ] #5 README.md's transcription of the panic line drops its stray space before *<crc> so README, docs/reference/daisy-seed3.md Console protocol v1, and this table agree; no other README wording changes. `grep -n 'L:C \*<crc>' README.md` is empty and the flush form hits once.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# TASK-030.05 — plan

Doc-comment-only correction: `firmware/src/bin/panictest.rs`'s "Reading the output" table still
describes the pre-framing wire, so the next person to run this binary reads the framed `~E …` line
as a defect. One ~30-line comment block plus one stray character in `README.md`. **No sub-tickets**
(see §6). No hardware work here — board proof belongs to TASK-030.04 / TASK-033.

## 1. Facts established by this planning pass — do not re-research

Every item below was read out of the code or measured, not inferred.

**The exact bytes of each record.**

- Panic body is assembled in `crates/asperitas-logging/src/panic_handler.rs`: `"PANIC: "` at
  `:50-51`, then `{info}` (`LocationFormatter` prints `panicked at {file}:{line}:{col}`), then the
  crate-name backfill prepends `panictest: ` because the payload contains no `::`
  (`:56-66`). Result today:
  `PANIC: panictest: deliberate panic, exercising the LED + serial panic path at src/bin/panictest.rs:161:13`
  — 105 bytes, frame 133 bytes with CRLF. The doubled `panictest:` is real behaviour, not a typo.
- Location string really is `src/bin/panictest.rs` (confirmed in the built ELF's `.rodata`); no
  `--remap-path-prefix` anywhere in `flake.nix` or the Makefiles, so that form is stable.
- Column is the `panic!` token's own column: 8-space indent ⇒ **col 13**, reproduced on host with
  rustc 1.97.1. Line number moves with any edit above it, including this ticket's edit — so the
  table keeps `L:C` and must not pin `161:13`. Nothing in the repo pins it today
  (`grep -rn "161:13" backlog/ docs/ README.md` → empty).
- Countdown bodies: `panictest: panicking in N...` (`panictest.rs:158`, `info!` ⇒ level letter `I`).
- BOOT body: `BOOT proto=1 fw={ver} pipe={LOG_PIPE_SIZE} maxbody={MAX_BODY}`
  (`console.rs:179-193`), emitted at `Level::Info` from `lib.rs:387-396`.
- Frame grammar `record := '~' level SP seq SP t_ms SP body '*' crc CRLF`
  (`docs/reference/daisy-seed3.md:181`); `PREFIX_LEN` 21, `TRAILER_LEN` 7, overhead 28,
  `MAX_BODY` 200, `MAX_FRAME` 228, `BODY_WINDOW` 220 (`frame.rs:161,169,175,178,186`).
  **The `*` is flush against the body — no space** — proven by the golden vectors
  `~I 00000042 00004567 ENC +1*9c17\r\n`, `~D 00000000 00000000 *91d4\r\n`,
  `…knob r2=298 ✓*b321\r\n` (`frame.rs:887-917`).

**`seq` arithmetic for a `log-usb` panictest run** (default features = `seed3` + `log-usb`,
`firmware/Cargo.toml`): BOOT consumes `seq 0` (`usb.rs:223` → `lib.rs:387`), the drain task's own
`log::info!("USB connected")` takes `1` (`usb.rs:260`), the ten countdown lines take `2`–`11`, so
the panic record is normally `0000000c`. A `STATUS` record (`usb.rs:277`, gated by
`status_gate.due()`) shifts it. Hence: prose arithmetic, never a pinned constant.

**Who does what:** `usb::emit_panic_record` (`usb.rs:341`, `pub`) frames the body — `frame::encode`
with `take_seq()` + `now_ms()`, no record lock — then calls `usb::emit_blocking` (`usb.rs:376`)
which drives the endpoint directly with the ring bypassed. Module diagram at `usb.rs:9-13`. Say
both halves; naming only `emit_blocking` (the current text, `panictest.rs:44`) hides the framing.

**Truncation trap worth recording:** `PANIC_MSG_BUF` is 128 (`panic_handler.rs:22`) against a
105-byte body today, i.e. 23 bytes of headroom. Overflow goes through `TruncWriter`
(`console.rs:39-50`) and yields a **valid CRC**; `trunc_total` counts frames dropped by
`frame::encode` over `MAX_BODY` 200, which this path can never reach
(`usb.rs:341-352` has no length check). Silent data loss, invisible on the wire.

**Gates that actually exist** (root `Cargo.toml` has `exclude = ["firmware"]`):

- Root `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`
  cover `crates/*` only — they never see `panictest.rs` (`ci.yml:24-36`, `lefthook.yml:7-25`).
- The only gate that compiles it: `cd firmware && cargo build --release --features seed3`
  (`ci.yml:44`, and `lefthook.yml:29-30` on push).
- Firmware clippy runs nowhere but `firmware/Makefile:179`. No `fmt` or `doc` target exists there.
- Nothing runs `cargo doc` or rustdoc lints in CI or hooks; no `RUSTDOCFLAGS` anywhere.

**Measured: AC #2's original build command is a false green.** Run from the repo root,
`cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --features seed3
--release` **exits 0 but never loads `firmware/.cargo/config.toml`** (cargo discovers
`.cargo/config.toml` from the CWD, not from `--manifest-path`). Evidence: in a `-vv` log that
command produced **zero** occurrences of `link-arg=-Tlink.x` (vs. two when run from inside
`firmware/`), rebuilt 112 units, and left an ELF whose `objdump -f` header is
`start address: 0x00000000` with no `.vector_table` / `.text` — unflashable junk that "passed".
The CI-equivalent command produces the real artifact: `start address: 0x08000299`,
`.vector_table` @ `0x08000000`. AC #2 has been amended accordingly.

**Doc-link hazard — use plain backticks, not `[`…`]` links.** In this crate `usb` resolves through
`use daisy_embassy::hal::{…, usb}` (`panictest.rs:57`) to the HAL's usb module, so
`[`usb::emit_panic_record`]` breaks under `RUSTDOCFLAGS="-D warnings"`; and `pub mod usb` is behind
`feature = "log-usb"` (`lib.rs:223-224`), so even the fully-qualified form only resolves in a
doc build that enables it. House style agrees: firmware binaries contain exactly one intra-doc link
([`PANIC_DELAY_SECS`], a local const) and zero cross-crate links — everything else is bare
backticks (`panictest.rs:9-10,35-36,44`). TASK-043 tracks the 7 warnings this habit avoids.

## 2. The edit — replace `firmware/src/bin/panictest.rs:30-49`

Keep lines 1-29 and everything from `#![no_std]` onward untouched. Replacement (≤100 columns;
rustfmt does not reflow comments, so wrap them by hand):

```
//! # Reading the output
//!
//! Every serial line below is a console v1 record — `~<level> <seq> <t_ms> <body>*<crc>` then
//! CRLF — as specified in `docs/reference/daisy-seed3.md` §Console protocol v1 and pinned by the
//! `encode_golden_*` tests in `frame.rs`. The `*` sits flush against the body: there is no space
//! before the checksum. `<seq>` and `<t_ms>` vary between runs; nothing else does. The `BOOT`
//! line is queued before the countdown but only leaves the device once a host attaches, so
//! attaching mid-countdown shows it late rather than not at all.
//!
//! | Stage     | LED          | Serial                                                              |
//! |-----------|--------------|---------------------------------------------------------------------|
//! | Boot      | steady red   | `~I <seq> <t_ms> BOOT proto=1 fw=<ver> pipe=<n> maxbody=<n>*<crc>`  |
//! | Countdown | steady green | `~I <seq> <t_ms> panictest: panicking in N...*<crc>`, ×N            |
//! | Panicked  | steady red   | `~E <seq> <t_ms> PANIC: <msg> at src/bin/panictest.rs:L:C*<crc>`    |
//!
//! All three stages must appear. Specifically:
//!
//! - The countdown lines prove the ordinary pipe → drain-loop → endpoint path works; if the LED
//!   counts down but no text arrives, the fault is there. They are framed on the normal commit
//!   path, like every other record.
//! - The `PANIC:` line proves the *panic* path works, which is a different mechanism: the
//!   executor is dead by then, so `usb::emit_panic_record` frames it without taking the record
//!   lock and hands the finished bytes to `usb::emit_blocking`, which drives the endpoint itself
//!   and bypasses the ring entirely. Countdown text without a `PANIC:` line is precisely the
//!   failure this binary exists to catch.
//! - The line must end cleanly at the source location, with no trailing NUL garbage. `L:C` is the
//!   `panic!`'s own position — the column is where the `panic!` token starts — so read it off the
//!   wire instead of asserting a number any edit to this file moves.
//! - Expect `panictest:` twice, as in `PANIC: panictest: deliberate panic, …`. The handler prepends
//!   the module path whenever the message contains no `::`, so the doubling is it working, not a
//!   defect.
//! - `seq` counts every record since boot, including ones nobody logged: `BOOT` takes `0`, the
//!   drain task's own `USB connected` takes `1`, the ten countdown lines take `2`–`11`, so the
//!   `PANIC:` line normally arrives as `0000000c`. A single `STATUS` record in that window shifts
//!   it, so treat that as arithmetic rather than an expected constant.
//! - Green → red is the LED half of the same signal, and is the only half that works with no host
//!   attached.
//! - Keep the message short: `PANIC_MSG_BUF` is 128 bytes and this one renders 105, so growing it
//!   by more than 23 bytes truncates the body silently — a truncated panic still carries a valid
//!   CRC and trips no counter, so nothing on the wire admits the loss.
```

Deliberate choices inside it:

- **Placeholders, not a pinned line.** `t_ms` is inside the CRC-covered range, so any table row
  with a real CRC would also have to fix `t_ms` and would be wrong for every actual run. A fully
  synthetic example invites reading it as a bench capture; `daisy-seed3.md:198-205` gets away with
  it only because it labels those bytes as the encoder's own.
- **All three rows framed, not just the panic row.** Fixing only the `Panicked` row leaves the same
  misreading one row away, and `—` for Boot became false once `BOOT` existed.
- **No intra-doc links** (§1). Names stay code text: `` `usb::emit_panic_record` ``,
  `` `usb::emit_blocking` ``.

## 3. The README fix — one character, same commit

`README.md:203` transcribes the panic line as `…at src/bin/panictest.rs:L:C *<crc>` — a space
before `*` that the wire does not have, inconsistent with its own line 202 (countdown, correctly
flush) and with `daisy-seed3.md:181,193`. Delete the space. Touch nothing else in README: line 203
already defers to this bin's table ("the stage table in `src/bin/panictest.rs` is the copy to
read"), so fixing one and not the other re-creates the disagreement this ticket exists to kill.

It rides in this commit rather than becoming a sibling ticket because: it is one character; it is
outside `firmware/`, so AC #2's comment-only diff guard is unaffected; and splitting a consistency
fix across two tickets lets the two copies drift apart again between merges. `daisy-seed3.md`
needs no change — its examples are already flush.

## 4. Verification (run in this order)

1. `cd firmware && cargo fmt --all --check` — required: the root `cargo fmt --all` excludes
   `firmware/` entirely.
2. `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` from the
   root — unchanged by this diff, run to satisfy AC #2 as written.
3. `cd firmware && cargo build --release --features seed3` — the exact `ci.yml:44` command. Then
   confirm it linked for real:
   `objdump -f firmware/target/thumbv7em-none-eabihf/release/panictest | head -3` must show a
   `0x0800….xxxx` start address, and `objdump -h … | head -8` must list `.vector_table` at
   `08000000`. Do **not** substitute the `--manifest-path` form (§1).
4. `cd firmware && make clippy BINARY=panictest FEATURES=seed3` — leave `NO_DEFAULT` unset so
   `log-usb` stays on as in CI. Only place firmware clippy ever runs.
5. Comment-only proof: `git diff -U0 firmware/ | grep -E '^[+-][^+-]' | sed -E 's/^.//' | grep -vE '^//'`
   must print nothing.
6. README agreement: `grep -n 'L:C\*<crc>' README.md` hits once; `grep -n 'L:C \*<crc>' README.md`
   is empty.
7. Optional, cheap insurance that no broken doc link was introduced:
   `cd firmware && cargo doc --no-deps --bin panictest --features seed3` — remember this is in no
   CI job or hook, so it is not evidence of anything except point 2 in §1.

## 5. Out of scope — do not widen

- Confirming on the board that the framed `PANIC:` line arrives and decodes: TASK-030.04 / TASK-033
  (`@human`). This ticket only makes the written expectation match the shipped encoder.
- Any host-side snapshot/approval test that would assert this table's text: none exists today
  (`crates/asperitas-logging/tests/console_frame.rs` asserts decoded bodies; the `encode_golden_*`
  tests in `frame.rs:887-917` are the CI-running byte-exact copy of this grammar). Adding an
  insta-style harness is its own ticket's job; a `#[test]` here would also violate AC #2's
  comment-only rule and would not run in CI anyway.
- Changing `PANIC_MSG_BUF`, adding a length assertion to `emit_panic_record`, or otherwise touching
  code: forbidden here. Record the trap (§1, bullet 7) and move on.
- Rustdoc link hygiene in `asperitas-logging`: TASK-043.

## 6. Why no sub-tickets, and why nothing is `@human`

One ~30-line comment block plus one character in README: under 20 meaningful lines of content,
single logical change, tightly coupled (the two copies must land together), no independent
increment to ship. Splitting it would buy two tickets that each break the other's consistency
check. No step needs the device, ears, an instrument, or an owner decision — the physical
confirmation of the framed panic line is already owned by TASK-030.04 and TASK-033, both `@human`.

## 7. When done

Record in implementation notes: the final byte count of the new panic body (should be unchanged at
105 — the message itself is not edited), the observed `objdump -f` start address, and the result of
the comment-only diff check in §4.5. Mark TASK-030.05 Done; TASK-030 stays `@human` until
TASK-030.04 closes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Two findings from planning, both measured rather than inferred:

1. **AC #2's original build command is a false green.** `cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --features seed3 --release`, run from the repo root, exits 0 but never loads `firmware/.cargo/config.toml` — cargo discovers `.cargo/config.toml` from the CWD, not from `--manifest-path`. A `-vv` log shows zero `link-arg=-Tlink.x` occurrences for that form (two when run inside `firmware/`), and the ELF it leaves has `start address: 0x00000000` with no `.vector_table` or `.text`. The CI-equivalent `cd firmware && cargo build --release --features seed3` yields `start address: 0x08000299` with `.vector_table` @ `08000000`. AC #2 now names the working command.
2. **AC #1 as first written asked for a wire-wrong string** — `…panictest.rs:L:C *<crc>`, with a space before the checksum. The shipped encoder puts `*` flush against the body (`frame.rs:887-917` golden vectors, `daisy-seed3.md:181` grammar), and `README.md:203` carries the same stray space, which is why AC #5 exists. AC #1 has been corrected to the flush form.

Byte-exact facts the plan relies on: panic body 105 B -> frame 133 B (not a multiple of 64, so no ZLP today); `PANIC_MSG_BUF` 128 B, so 23 B of headroom before silent truncation that still passes CRC; seq arithmetic BOOT 0 / "USB connected" 1 / countdown 2-11 / panic normally `0000000c`; location really `src/bin/panictest.rs:161:13` at planning time, column 13 = the `panic!` token's own column, and this ticket's own edit moves the line number, which is why the table keeps `L:C`.
<!-- SECTION:NOTES:END -->
