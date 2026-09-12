---
id: TASK-055
title: >-
  Correct the six places where the defmt-over-RTT record is wrong, and pin the
  frame-size margin with a compile-time assert
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-12 07:16'
updated_date: '2026-09-12 07:33'
labels:
  - planned
dependencies:
  - TASK-053
  - TASK-054
references:
  - 'https://kb.segger.com/RTT'
  - 'https://docs.rs/defmt-rtt/latest/defmt_rtt/'
  - >-
    https://raw.githubusercontent.com/twitzelbos/daisy-rs/main/docs/memory-placement.md
documentation:
  - crates/asperitas-logging/src/defmt_log.rs
  - docs/reference/daisy-seed3.md
priority: medium
type: task
ordinal: 86700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-036 documented the RTT channel honestly, and most of it holds. Six things in the record are wrong, missing, or unenforced, all found by re-reading the tree against upstream sources rather than by trusting the tickets that wrote them. None needs a board. All of them are the kind of thing that costs someone hours later, because they sit in the sentences people trust when the hardware starts misbehaving.

1. **The frame-size margin cites code that is not in our build.** `crates/asperitas-logging/src/defmt_log.rs:84-97` argues the body window cannot overflow defmt-rtt's ring and cites "`write_impl` refuses to write a chunk of `BUF_SIZE` or more (channel.rs:93)". In defmt-rtt 1.3.0 that arm exists only under the non-default `drop-on-contention` feature (`channel.rs:90-95`); our default build's `write_impl` is at `channel.rs:71` and has no such refusal. The real spin is `write_all`'s `while !bytes.is_empty()` loop (`channel.rs:38-42`) calling `blocking_write` while `available == 0` (`channel.rs:46-59`) - which is "host attached and not draining", not "frame oversized". The conclusion (keep the margin) is right; the mechanism and the citation are not. The arithmetic itself lives only as prose plus a `const`: nothing stops `console::BODY_WINDOW` growing past the ring and no gate would notice.
2. **A claim in the manifest that the lock contradicts.** `firmware/Cargo.toml:54-57` presents the defmt 0.3-to-1 migration as tidied up, but `firmware/Cargo.lock:279` still resolves `defmt 0.3.100`, pulled by `embassy-net-driver 0.2.0` (`Cargo.lock:421`) and `stm32-metapac 21.0.0` (`Cargo.lock:1113`). Harmless - 0.3.100 is a shim over 1.1.1 - but the sentence reads like a completed cleanup.
3. **Binary sizes disagree three ways** and nobody regenerates them: `firmware/Cargo.toml:38` says 88101 bytes, the working artifact is 88613, and `firmware/Makefile:84` promises "a correct ~32 KB binary". (TASK-054 owns the Cargo.toml instance; this ticket owns the Makefile one and the sweep for stragglers.)
4. **The loss model has a hole where the host goes away mid-run.** `docs/reference/daisy-seed3.md:414-422` covers three regimes - no host, attached-and-keeping-up, attached-and-stalled - and never says what happens when probe-rs detaches *while the target is running*. defmt-rtt's own crate docs warn this implementation "may block forever if probe-rs disconnects at runtime", and `defmt::flush` blocks likewise. Given row three already ends with the target spinning with interrupts off, this is the same failure with a different trigger and a much more ordinary cause: someone closes the terminal.
5. **The cache paragraph understates the work.** `daisy-seed3.md:454-477` measures `_SEGGER_RTT` at 0x24000008 and calls relocation "linker work, not a config bit". Re-measured here against the linked image: `_SEGGER_RTT` at `0x24000008`, defmt-rtt's `BUFFER` at `0x240010e4` - neither is 32-byte aligned, and SEGGER's own requirement is that the control block *and every buffer* be cache-line aligned *and* sized to a multiple of the largest cache line, with a barrier for Cortex-M7 reordering. defmt-rtt exposes no alignment or section-control feature (1.3.0 ships exactly two: `disable-blocking-mode`, `drop-on-contention`), so the honest statement is that the fix needs an MPU non-cacheable window or an upstream patch, not a linker tweak. Two adjacent facts belong beside it: relocating into a `NOLOAD` section placed naively outside DTCM drags `__ebss` across an unmapped gap and bus-faults before `main`, passing simulation and failing on silicon (daisy-rs records exactly this on the same H750), and probe-rs's own `STM32H750IBKx` description does list DTCM `0x20000000..0x20020000`, so a relocated block stays discoverable without scanning.
6. **"Nothing logs except the facade" is not true of the linked image.** With `log-defmt` selected, daisy-embassy turns defmt on across its dependency stack, so driver frames exist whether we write them or not. Verified in the built ELF's symbol table: `{"package":"embassy-stm32","tag":"defmt_error","data":"Ringbuffer broken invariants detected!",...}` is present, emitted from `embassy-stm32-0.6.0/src/sai/mod.rs:33` inside `impl From<ringbuffer::Error> for Error`, which fires on the SAI overrun path reached from `daisy-embassy/src/audio.rs`'s `start_callback` read/write loop. It runs in task context, not inside our audio callback, but it is an ERROR-level frame - the one level that survives an unset `DEFMT_LOG` - written inside defmt-rtt's critical section, from code we do not own, at the moment audio is already going wrong.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 defmt_log.rs's margin argument cites the default-build write path it actually relies on, verified line by line against the vendored defmt-rtt 1.3.0, and no longer attributes the oversized-frame refusal to a cfg-gated arm our build does not compile.
- [ ] #2 The body-window arithmetic is enforced by a compile-time assert. Prove it bites: widen console::BODY_WINDOW past the bound, show the named command fail, revert and show it pass. Name the gate that catches it.
- [ ] #3 No stale firmware.bin byte count survives anywhere in the tree; each remaining number names the feature configuration and date it was measured, including firmware/Makefile:84.'s "~32 KB" claim.
- [ ] #4 firmware/Cargo.toml's defmt-graph comment matches Cargo.lock (0.3.100 still resolved via embassy-net-driver and stm32-metapac, as a shim over 1.1.1), and one line beside the defmt-rtt dependency records that disable-blocking-mode, drop-on-contention and DEFMT_RTT_BUFFER_SIZE are left at their defaults on purpose, with the reason.
- [ ] #5 docs/reference/daisy-seed3.md's loss table gains a fourth regime for the host detaching mid-run, quoting defmt-rtt's own crate documentation rather than paraphrasing, with the existing three rows and the loss-ledger section left consistent with it.
- [ ] #6 The cache paragraph states SEGGER's alignment-and-size requirement, records both measured addresses and that neither is cache-line aligned, notes that defmt-rtt exposes only two features, names the MPU-window and upstream-patch routes, includes the NOLOAD-outside-DTCM bus-fault-before-main trap, and states that implementing either route is deliberately unscheduled pending TASK-038.'s caching decision.
- [ ] #7 A short passage names the defmt frame sources outside asperitas-logging that are present in the linked RTT image, gives the cargo nm command that proves it, cites the sai error-conversion site, and neither claims they fire from the audio callback nor proposes a DEFMT_LOG filter.
- [ ] #8 Host gates green in nix develop: fmt, the four clippy invocations, both RUSTDOCFLAGS=-D warnings doc runs, cargo test --workspace, and both firmware cross-compiles from ci.yml.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Shape of this ticket

Evidence hygiene in six spots, plus one `const`. Two files carry code-visible changes
(`crates/asperitas-logging/src/defmt_log.rs`, `firmware/Cargo.toml`); the rest is
`docs/reference/daisy-seed3.md`. It depends on TASK-053 and TASK-054 on purpose: those two edit the
same Makefile comment block, the same `[profile.release]` comment, and the same documentation section,
and the sweep in step 3 only makes sense once their numbers are final.

## Facts re-measured locally for this ticket

* `cargo nm` over the linked `log-defmt` release image: `_SEGGER_RTT` at `0x24000008` (`D`, i.e. `.data`),
  `defmt_rtt`'s `BUFFER` at `0x240010e4` (`B`). Neither is 32-byte aligned; SEGGER's Cortex-M7 rule needs
  alignment *and* a size that is a multiple of the largest cache line, on the control block and every
  buffer.
* Same symbol dump shows an upstream frame compiled into our image:
  `{"package":"embassy-stm32","tag":"defmt_error","data":"Ringbuffer broken invariants detected!",...}`.
  Source: `embassy-stm32-0.6.0/src/sai/mod.rs:33`, inside `impl From<ringbuffer::Error> for Error`,
  reached from `daisy-embassy/src/audio.rs`'s `start_callback` loop when `codec.read`/`codec.write`
  fail. Re-check with:
  `CARGO_TARGET_DIR=<dir> cargo nm --release --no-default-features --features "seed3 log-defmt" --bin main -- | grep -i defmt_`
* defmt-rtt 1.3.0 exposes exactly two Cargo features, `disable-blocking-mode` and `drop-on-contention`;
  neither is enabled here and nothing records that as a decision.
* `firmware/Cargo.lock`: `defmt 0.3.100` at :279, required by `embassy-net-driver 0.2.0` (:421) and
  `stm32-metapac 21.0.0` (:1113).

## Steps

1. **Fix the margin argument where it lives.** In `defmt_log.rs:84-97`, replace the `channel.rs:93`
   attribution with the default-build path actually in force: `write_all`'s `while !bytes.is_empty()`
   (`channel.rs:38-42`) spinning through `blocking_write` while `available == 0` (`channel.rs:46-59`).
   Re-open the vendored crate (`~/.cargo/registry/src/*/defmt-rtt-1.3.0/src/channel.rs`) and quote the
   lines you cite; if they have moved under a version bump, cite what you see, not what is written here.
2. **Turn the arithmetic into something a compiler checks.** Add a `const _: () = assert!(...)` next to
   `MAX_FRAME_BODY` (`defmt_log.rs:98`) bounding body-plus-overhead below defmt-rtt's usable ring
   (`BUF_SIZE - 1`, default 1024 from its `build.rs`), naming the overhead assumption in the message.
   Prove the assert bites: raise `console::BODY_WINDOW` past the bound, watch the `--features log-defmt`
   clippy gate fail on it, revert. That gate exists since TASK-036.05 and compiles this file for the host,
   so a `const` assert there is genuinely unattended-enforced - say which command caught it.
3. **Sweep the stale sizes.** Own `firmware/Makefile:84` ("a correct ~32 KB binary") and grep the whole
   tree for other byte counts attached to a claim (`grep -rnE "[0-9]{4,6} bytes|KB binary"`), refreshing
   or deleting each. Where a number stays, attach the feature configuration it was measured under; a
   bare number in a doc is a future lie. Leave the `[profile.release]` comment to TASK-054.
4. **Correct the manifest's defmt-graph claim** (`firmware/Cargo.toml:54-57`) against the lock facts
   above, and add one line beside the `defmt-rtt` dependency recording that `disable-blocking-mode`,
   `drop-on-contention` and `DEFMT_RTT_BUFFER_SIZE` are deliberately left at their defaults, with the
   reason. Right now a reader cannot tell whether that was decided or overlooked.
5. **Add the fourth loss regime** to `docs/reference/daisy-seed3.md:414-422`: host detaches mid-run.
   Quote defmt-rtt's crate-level docs rather than paraphrasing ("may block forever if `probe-rs`
   disconnects at runtime", plus the equivalent statement about `defmt::flush`) - fetch them, do not
   copy this ticket's wording blindly - and say plainly that "lossless" is a property of an attached,
   draining host, not of the transport. Keep the existing three rows intact and keep the loss ledger
   section at :585-600 consistent with the new row.
6. **Rewrite the cache paragraph** (`daisy-seed3.md:454-477`) around the measurements above: the
   alignment-and-size requirement, both addresses with their misalignment named, the fact that
   defmt-rtt offers no way to express it, the two credible routes (an MPU non-cacheable window over the
   AXI SRAM start, or an upstream align/section patch), the `NOLOAD`-outside-DTCM trap that drags
   `__ebss` across an unmapped gap and bus-faults before `main` while passing simulation, and the fact
   that probe-rs's `STM32H750IBKx` entry already describes DTCM `0x20000000..0x20020000` so a relocated
   block stays discoverable without `--scan-region`. Say explicitly that implementing either route is
   **not** scheduled and is deliberately not filed: caching is off today, verified in three places, and
   the day TASK-038's SDRAM work wants it on, that ticket plans this then.
7. **Name the foreign frames.** One short passage recording that selecting `log-defmt` pulls defmt into
   the whole daisy-embassy stack, with the `cargo nm` command above, the `sai/mod.rs:33` site, that ERROR
   is the level which survives an unset `DEFMT_LOG`, and that these frames are written inside
   defmt-rtt's critical section from task context. Do not claim they fire from the audio callback - the
   callback signature in `daisy-embassy/src/audio.rs` is infallible and our closure logs nothing - and do
   not propose filtering them: `DEFMT_LOG=off,crate=off` is recorded in TASK-036.03's notes as a ~300
   error failure mode, and that experiment belongs to whoever actually needs it.
8. **Gates:** fmt, four clippy invocations including `--features log-defmt`, both doc runs,
   `cargo test --workspace`, both firmware cross-compiles.

## Not in scope

Implementing the MPU window or any linker surgery for `_SEGGER_RTT` (documented, deliberately unfiled);
enabling `disable-blocking-mode` or `drop-on-contention`; anything needing a probe (TASK-037); the DWARF
level (TASK-054); the probe CLI surface (TASK-053); populating `RigConfig`'s `icache`/`dcache` fields,
which belong with the rig binary in TASK-038.03.02 and are noted here only so the omission is on record.

## Promotion gate for this ticket

It stays `To Do` until TASK-053 and TASK-054 are Done, and deliberately so: all three edit the same
Makefile comment block, the same `[profile.release]` comment and the same documentation section, and the
size sweep in step 3 is meaningless while the other two are moving numbers. Because Execute takes the first
`Dev Ready` ticket without re-checking dependencies, promoting this one early would have an agent resolve
figures that are about to change again. Flip it to `Dev Ready` when both parents close.
<!-- SECTION:PLAN:END -->
