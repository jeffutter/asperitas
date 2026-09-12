---
id: TASK-055
title: >-
  Correct the six places where the defmt-over-RTT record is wrong, and pin the
  frame-size margin with a compile-time assert
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-12 07:16'
updated_date: '2026-09-12 19:18'
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
- [x] #1 defmt_log.rs's margin argument cites the default-build write path it actually relies on, verified line by line against the vendored defmt-rtt 1.3.0, and no longer attributes the oversized-frame refusal to a cfg-gated arm our build does not compile.
- [x] #2 The body-window arithmetic is enforced by a compile-time assert. Prove it bites: widen console::BODY_WINDOW past the bound, show the named command fail, revert and show it pass. Name the gate that catches it.
- [x] #3 No stale firmware.bin byte count survives anywhere in the tree; each remaining number names the feature configuration and date it was measured, including firmware/Makefile:84.'s "~32 KB" claim.
- [x] #4 firmware/Cargo.toml's defmt-graph comment matches Cargo.lock (0.3.100 still resolved via embassy-net-driver and stm32-metapac, as a shim over 1.1.1), and one line beside the defmt-rtt dependency records that disable-blocking-mode, drop-on-contention and DEFMT_RTT_BUFFER_SIZE are left at their defaults on purpose, with the reason.
- [x] #5 docs/reference/daisy-seed3.md's loss table gains a fourth regime for the host detaching mid-run, quoting defmt-rtt's own crate documentation rather than paraphrasing, with the existing three rows and the loss-ledger section left consistent with it.
- [x] #6 The cache paragraph states SEGGER's alignment-and-size requirement, records both measured addresses and that neither is cache-line aligned, notes that defmt-rtt exposes only two features, names the MPU-window and upstream-patch routes, includes the NOLOAD-outside-DTCM bus-fault-before-main trap, and states that implementing either route is deliberately unscheduled pending TASK-038.'s caching decision.
- [x] #7 A short passage names the defmt frame sources outside asperitas-logging that are present in the linked RTT image, gives the cargo nm command that proves it, cites the sai error-conversion site, and neither claims they fire from the audio callback nor proposes a DEFMT_LOG filter.
- [x] #8 Host gates green in nix develop: fmt, the four clippy invocations, both RUSTDOCFLAGS=-D warnings doc runs, cargo test --workspace, and both firmware cross-compiles from ci.yml.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Shape of this ticket

Seven evidence fixes plus one `const`. Three files carry everything: `crates/asperitas-logging/src/defmt_log.rs`,
`firmware/Cargo.toml`, `docs/reference/daisy-seed3.md` (plus two number fixes in `console.rs`-adjacent code and
`capture.rs`). No board, no ears, no owner decision: every criterion is satisfiable by reading sources, running
host gates, cross-compiling, or inspecting an ELF. Assignee stays `@agent` and nothing gets split.

**No sub-tickets, deliberately.** Items #3, #5, #6 and #7 all edit `docs/reference/daisy-seed3.md`; splitting them
buys no independently shippable increment and guarantees conflicts in one file. The whole ticket is one commit's
worth of prose plus one `const`, and it tells one story.

## What has moved since this ticket was written (re-measured 2026-09-12)

Every line reference in the description and the old plan has drifted, and TASK-054 closed two of the named
targets. Work from this table, not from the numbers in the description:

| The ticket points at | What is actually there now |
|---|---|
| `defmt_log.rs:84-97` margin passage | `defmt_log.rs:91-106`; `const MAX_FRAME_BODY` is `:106` |
| `daisy-seed3.md:414-422` loss table | `:445-460` (table rows `:451-453`); `:414-420` is the `DEFMT_LOG` gate |
| `daisy-seed3.md:454-477` cache paragraph | `:485-508` |
| `daisy-seed3.md:585-600` loss ledger | `:745-760`, "What each channel loses" |
| `firmware/Cargo.toml:54-57` defmt-graph comment | `:95-98` (`defmt = "1"` at `:99`, `defmt-rtt` at `:102`) |
| `firmware/Makefile:84` "a correct ~32 KB binary" | **Gone.** TASK-054 (`6fbd6bc`) replaced it with the measured table at `Makefile:89-97` |
| `firmware/Cargo.toml:38` "88101 bytes" | **Gone**, same commit |

So AC #3's two named targets are already fixed. Its real content is the stragglers in step 3.

Implementation Notes asked whether anything still claims the `DEFMT_LOG` size ladder is monotonic. It does not:
`defmt_log.rs:54` states outright that `info` came out *below* the error-only baseline, and `:56-61` disclaims
cross-profile transferability. Do not re-open that.

## Facts confirmed locally today (quote what you see, not what is written here)

Vendored crates: `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/{defmt-rtt-1.3.0,defmt-1.1.1,defmt-0.3.100}`.

* Default-build ring path: `write_all`'s loop `channel.rs:38-42`; `blocking_write` returns 0 when `available == 0`
  (`channel.rs:57-59`); `write_impl` `channel.rs:71-87` **clamps** (`let len = bytes.len().min(available);`) and
  never refuses anything; `nonblocking_write` `channel.rs:64-69` truncates at `BUF_SIZE`; `available_buffer_size`
  `channel.rs:158-164` caps at `BUF_SIZE - 1`. The oversized-frame refusal `bytes.len() >= BUF_SIZE` at
  `channel.rs:93` lives inside `#[cfg(feature = "drop-on-contention")] impl Channel` (block starts `:90`), which
  our build does not compile. For that mode's semantics cite the crate docs at `defmt-rtt/src/lib.rs:42-52`.
* `Channel::flush` `channel.rs:139-149` returns early when the host is *not* connected, else busy-waits on
  `read() != write()`. `host_is_connected` `channel.rs:151-154` decides attachment **purely from the mode bits**
  (`flags & MODE_MASK == MODE_BLOCK_IF_FULL`), with the comment "we assume that a host is connected if we are in
  blocking-mode. this is what probe-run does."
* `build.rs:4,11,23`: `BUF_SIZE` defaults to 1024, overridable by `DEFMT_RTT_BUFFER_SIZE`, with
  `rerun-if-env-changed` wired in defmt-rtt's own build script. Nothing in this workspace reads that variable.
* defmt 1.1.1 hands the logger **encoded** bytes, incrementally: `encoding/mod.rs:58` documents that the write
  closure "may be called zero, one, or multiple times"; rzcobs is the default (`encoding/mod.rs:4-5` selects
  `raw.rs` only under feature `encoding-raw`, which nothing here enables); rzcobs costs roughly one extra byte per
  seven payload bytes plus a frame separator (`encoding/rzcobs.rs:27-53`). A `{}` str arg carries a fixed 4-byte LE
  length prefix, not a varint (`export/mod.rs` `str()` calls `usize(&s.len())`, and `integers.rs` `usize()` writes
  `(*b as u32).to_le_bytes()`).
* defmt-rtt's default-mode logger therefore calls `write_all` **once per encoder callback**, not once per frame:
  `lib.rs:166-186` is `critical_section::acquire()` → reentrancy panic → `encoder.start_frame(|b| { _SEGGER_RTT
  .up_channel.write_all(b); })`, with the same shape in its `write()`. Each chunk is a handful of bytes. Read this
  before writing step 1: it changes the argument, not just the citation.
* `defmt 0.3.100` is resolved *and* built, not merely locked:
  `cd firmware && nix develop ..#default --command cargo tree -e features -i defmt@0.3.100 --features seed3,log-defmt`
  shows the edge `daisy-embassy → embassy-stm32 feature "defmt" → stm32-metapac feature "defmt" → defmt 0.3.100`,
  plus a second root through `embassy-net-driver` (via `embassy-usb`). `Cargo.lock:279-286` shows the shim's only
  dependency is `defmt 1.1.1`. Both parents are at their latest published versions (embassy-net-driver 0.2.0,
  stm32-metapac 21.0.0), so no version bump removes it today.
* Gate that compiles `defmt_log.rs` on the host: `ci.yml:37-38`,
  `cargo clippy -p asperitas-logging --features log-defmt --lib -- -D warnings`. Verified green locally, and it
  genuinely re-checks the crate (`touch` on `defmt_log.rs` → "Checking asperitas-logging", ~0.6 s warm).
* `asperitas-logging` depends on `defmt` but **not** on `defmt-rtt` (`crates/asperitas-logging/Cargo.toml:15-19,28`;
  `defmt-rtt` sits in the binary: `firmware/Cargo.toml:31,102`). Step 2's assert cannot name defmt-rtt's `BUF_SIZE`.

## Steps

### 1. Rewrite the margin argument (AC #1) - `defmt_log.rs:91-106`

Do not just swap the citation. Two things there are wrong:

* (a) it attributes an oversized-frame refusal to `channel.rs:93`, which is cfg'd out of our build;
* (b) it concludes the margin "keeps that path unreachable rather than merely unlikely". Because the ring receives
  many small writes (see the encoder facts above), no write ever approaches `BUF_SIZE`, so frame size cannot gate
  whether a write fits at all. The spin depends on ring occupancy versus host drain rate, full stop.

Write the honest version: the spin is `write_all` looping through `blocking_write` while `available == 0`
(`channel.rs:38-42, 57-59`), which means "host attached and not draining". What the body window buys is a bound on
how much ring one log line can consume, hence how fast a stalled host fills 1023 bytes and how long interrupts stay
off inside defmt-rtt's critical section; and a frame larger than the usable ring could not be delivered intact
anyway, since non-blocking mode truncates (`channel.rs:64-69`). Keep the arithmetic and keep the conclusion (a
256-byte window against 1023 usable bytes); drop the guarantee that the code does not provide.

Fix in passing: `defmt_log.rs:50` cites `env_filter.rs:34`. In defmt-macros 1.1.1
`LEVEL_WHEN_NOTHING_IS_SPECIFIED: LogLevelOrOff = Some(Level::Error)` is at **:35**, and `:34` is the easily
confused sibling `LEVEL_WHEN_LEVEL_IS_NOT_SPECIFIED = Some(Level::Trace)`. The same wrong citation is at
`daisy-seed3.md:416`; fix it there too (step 7 touches that passage).

### 2. Pin the arithmetic with a compile-time assert (AC #2)

Because of the dependency boundary above, declare the ring size locally. Suggested shape next to `MAX_FRAME_BODY`
(`defmt_log.rs:106`), values verified above:

```rust
/// defmt-rtt 1.3.0 `build.rs` default; nothing here reads `DEFMT_RTT_BUFFER_SIZE`, so if anyone
/// ever sets it smaller this constant has to move with it (`firmware/build.rs:34` shows the
/// `rerun-if-env-changed` pattern if that deserves a gate rather than a comment).
const RTT_BUF_SIZE: usize = 1024;
const RTT_RING_USABLE: usize = RTT_BUF_SIZE - 1; // channel.rs:158-164
const FRAME_OVERHEAD_BYTES: usize = 8; // header byte + format-index varint + 4-byte str length
/// rzcobs expands roughly one byte per seven, plus a frame separator.
const WORST_ENCODED_FRAME: usize = (MAX_FRAME_BODY + FRAME_OVERHEAD_BYTES) * 8 / 7 + 2;

const _: () = assert!(
    WORST_ENCODED_FRAME < RTT_RING_USABLE,
    "worst-case defmt frame must fit defmt-rtt's usable ring"
);
```

House style to match: `capture.rs:254-311` (numbered comment naming the slack, lowercase message that names the
invariant rather than the numbers), `console.rs:438-441` and `firmware/src/bin/rig.rs:134-146`, which record the two
clippy traps in this area (`assertions_on_constants` for runtime asserts on constants;
`absurd_extreme_comparisons` when comparing against 1). One more constraint measured: the message must be a plain
literal - a formatted `assert!` message does not compile in const context (`E0015`, "cannot call non-const
formatting macro in constants"), so spell any numbers into the string.

Proof protocol, and paste both halves into the ticket notes:

1. Temporarily set `console.rs:45` `BODY_WINDOW = 2000`.
2. `nix develop .#default --command cargo clippy -p asperitas-logging --features log-defmt --lib -- -D warnings`
   and confirm the error text is *your* message string (not some other gate tripping on the wider window).
3. Revert, re-run, show green. Also run the default-features clippy (`--workspace --all-targets`) and say in the
   notes which invocation catches it: `mod defmt_log` is `#[cfg(feature = "log-defmt")]` (`lib.rs:230-231`), so the
   `log-defmt` run is the only host gate that sees it.

### 3. Sweep the sizes (AC #3)

Already handled by TASK-054: `Makefile:84-97` and `[profile.release]`'s table in `firmware/Cargo.toml`. Live
stragglers:

* `docs/reference/daisy-seed3.md:144` - "**Binary size check:** `ls -la firmware.bin` should show < 128 KB (blinky
  is ~18 KB)." Measure it fresh: `make build FEATURES="seed3" BINARY=blinky` and
  `NO_DEFAULT=1 FEATURES="seed3" BINARY=blinky`. An earlier pass today got 65,638 and 21,202 bytes - neither is
  ~18 KB, and the line names no configuration. Re-measure at your commit and name config + date, or delete the
  parenthetical and keep only the 128 KB budget.
* `crates/asperitas-logging/src/capture.rs:111` ("69 KB is free") and `:286` ("~69 KB free"). Unattributed, and its
  premise is discredited: `task-038.03` ties that figure to ".bss 86.13%", which `task-038.03.02.02:88` records as
  never having existed (measured `main`: text 88181 / data 1428 / bss 8224). Prefer stating the invariant (block
  indices must fit a `u16`) without inventing headroom; only keep a number you can attribute to a named binary,
  config and date.
* `defmt_log.rs:51-61` names artifact and DWARF level but no date. Add one; `6fbd6bc` (2026-09-12) is where those
  numbers came from.
* Re-run the sweep after editing: `grep -rnE "[0-9]{4,6} bytes|~[0-9]+ ?KB|KB binary"` excluding `target/`,
  `.git/`, `backlog/`. Classify each hit: measurement (needs config + date) or budget/limit (leave). Budgets to
  leave alone include `firmware/memory.x:3,9`, `README.md:312`, `Makefile:170,239`, `daisy-seed3.md:477,718,737`.
  Numbers inside `backlog/` are dated records, not doc claims: leave them.

### 4. Correct the manifest (AC #4) - `firmware/Cargo.toml:95-98`

The current comment ends "...one major is a correctness requirement here, not tidiness", which the lock reads as a
contradiction. Replace with the accurate story: our direct dependency is 1.x; the lock still resolves
`defmt 0.3.100` (`Cargo.lock:279`) because daisy-embassy enables embassy-stm32's `defmt` feature, which enables
stm32-metapac's, and embassy-net-driver asks for 0.3 as well. That crate is a shim whose sole dependency is
`defmt 1.1.1` (`Cargo.lock:279-285`, sole dependency entry at `:284`), so the wire format stays single-major.
Quote the shim's own self-description verbatim rather than paraphrasing (`defmt-0.3.100/src/lib.rs:8-10`: "This is a
defmt-0.3 compatbility [sic] crate. It depends upon `defmt-1.0` and re-exports the items that were available in
`defmt-0.3`. This allows you to mix defmt-0.3 and defmt-1.0 within the same compilation."). Record that both parents are at their latest published
versions, so this is "waiting on upstream", not "tidied up". Give the reproducing command from the facts section.

Add one line beside the `defmt-rtt` dependency (`:102`) recording that its two features and its build knob are
deliberately at defaults, each with its reason: `disable-blocking-mode` is what makes an attached host lossless
(discussed as insurance at `daisy-seed3.md:467-473`); `drop-on-contention` is ARM-only (`compile_error!` at
`defmt-rtt/src/lib.rs:56-57`) and drops frames by design; a smaller `DEFMT_RTT_BUFFER_SIZE` would shrink the margin
step 2 pins. Right now a reader cannot tell decided from overlooked.

### 5. Fourth loss regime (AC #5) - `daisy-seed3.md:445-460`

Add a fourth row: host detaches mid-run. Quote defmt-rtt's crate docs verbatim (`defmt-rtt/src/lib.rs:15-21`, fetch
them, do not copy this ticket's wording): "`probe-rs` puts RTT into blocking-mode, to avoid losing data. / As an
effect this implementation may block forever if `probe-rs` disconnects at runtime. This is because the RTT buffer
will fill up and writing will eventually halt the program execution. / `defmt::flush` would also block forever in
that case."

Mechanism sentence to go with it: `host_is_connected()` (`channel.rs:151-154`) infers attachment from the mode bits
alone, so a host that set `BLOCK_IF_FULL` and then vanished leaves the target writing as though someone were still
draining; `Channel::flush` (`channel.rs:139-149`) only escapes early when the mode says non-blocking. Same freeze as
row three, a far more ordinary trigger: someone closes the terminal.

Honesty limit, and state it: whether probe-rs restores the mode flags on detach is unverified upstream. Do not
assert either direction; name TASK-037 as the bench session that would measure it. Then make `:755-756` in "What
each channel loses" consistent with four rows - it already hedges, and probably needs only "while the host keeps
reading *and stays attached*". Leave rows one to three intact.

### 6. Rewrite the cache paragraph (AC #6) - `daisy-seed3.md:485-508`

Must contain:

* SEGGER's Cortex-M requirements, quoted from kb.segger.com/RTT "Cortex-M specifics": control block and all RTT
  buffers "must start cache line aligned"; sizes "must be the multiple of a cache line"; with multiple cache levels
  "take the cache with the largest line size as the reference point"; "It is **user application's responsibility**
  to call a cache clean + invalidate on the RTT control block + all RTT buffers after segment init is complete but
  before RTT is used for the first time"; plus the two the current paragraph omits, because they constrain any fix:
  the control block, buffers and pointers to their names "must be linked with virtual address == physical address",
  and "The application must provide a uncached address alias to the memory where the control block + buffers are
  located".
* Both addresses as measured on the `log-defmt` `main` image at this commit: `_SEGGER_RTT` `0x24000008` (`.data`, 8
  bytes into its line) and `defmt_rtt::BUFFER` `0x240010e4` (4 bytes into its line), neither 32-byte aligned. Other
  binaries differ (`rig` `0x24001234`, `panictest` `0x240004ac`), so label these "measured here, at this commit" and
  give the command from step 7. Re-measure rather than trusting these.
* That defmt-rtt ships exactly two features and offers no alignment or section control - and, as the mechanism an
  upstream patch would ride on, that `BUFFER` is emitted into `.uninit.defmt-rtt.BUFFER` and `NAME` into
  `.data.defmt-rtt.NAME` (`defmt-rtt/src/lib.rs:130-141`), which is selectable from a linker script.
* The design wrinkle that argues against the naive fix: control block and buffer sit ~4 KB apart with unrelated
  `.data`/`.bss` interleaved, so one non-cacheable window covering both also makes those neighbours uncached, and
  partial-line sharing defeats invalidate-by-address on M7. Two credible routes remain: an MPU non-cacheable window
  over the start of AXI SRAM, or an upstream align/section patch.
* The trap: relocating into a `NOLOAD` section placed naively outside DTCM drags `__ebss` across an unmapped gap and
  bus-faults before `main`, passing simulation and failing on silicon (daisy-rs records exactly this on the same
  H750; the reference is on this ticket).
* Discoverability, verified rather than cited: run `nix develop .#default --command probe-rs chip info
  STM32H750IBKx` (the doc already runs it at `:394-396`) and record whether DTCM `0x20000000..0x20020000` appears in
  its RAM list. Do not cite probe-rs YAML paths you have not opened. Naming nuance worth one clause:
  `STM32H750IBKx` is a package variant of the chip entry `STM32H750IB`.
* The scheduling statement, explicit: implementing either route is **not** scheduled and deliberately not filed.
  Caching is off today (the three-place verification at `:501-507` stands); the day TASK-038's SDRAM/DSP work wants
  it on, that ticket plans this then.

Also correct the current closing sentence, "it is linker work, not a config bit": against SEGGER's list, linker work
is necessary but not sufficient - the barrier call and the uncached alias are application work too.

### 7. Name the foreign frames (AC #7) - a short passage in `daisy-seed3.md`

Put it beside the `DEFMT_LOG` gate (`:414-420`) or just after the loss table. Content:

* Selecting `log-defmt` switches defmt on across daisy-embassy's whole stack (step 4's `cargo tree` is the receipt),
  so driver frames are in the image whether we write them or not. `crates/asperitas-logging/src/lib.rs:18-22` already
  says RTT is not idle in that configuration; this passage supplies the evidence.
* The proof command, with the flags called out as load-bearing:
  `cd firmware && CARGO_TARGET_DIR=<throwaway> cargo nm --release --no-default-features --features "seed3 log-defmt" --bin main -- | grep -c '"package"'`
  (an earlier pass counted 82 frame symbols, 75 tagged `defmt_error`; recount at your commit). Warn in the text that
  `cargo nm`/`cargo objdump` re-run the build: omit those flags and it silently overwrites
  `target/.../release/main` with the console image, which links the `#[cfg(not(feature = "log-defmt"))]` no-op
  logger and contains zero `SEGGER` bytes - symptoms that read exactly like "RTT is missing from the firmware". The
  two cheap discriminators are the `"package"` count and a `SEGGER` byte scan of the ELF.
* One named site: `{"package":"embassy-stm32","tag":"defmt_error","data":"Ringbuffer broken invariants detected!",...}`,
  from `embassy-stm32-0.6.0/src/sai/mod.rs:33` inside `impl From<ringbuffer::Error> for Error` (only for
  `ringbuffer::Error::DmaUnsynced`, returning `Self::Overrun`), reached from `daisy-embassy/src/audio.rs:166-180`'s
  `start_callback` `codec.read(...)/codec.write(...)` loop. That is **task context, not the audio callback** - the
  callback signature is infallible - and the passage must not blur the two.
* ERROR is the level that survives an unset `DEFMT_LOG` (use the corrected `:35` citation from step 1), and propose
  no filter: `DEFMT_LOG=off,crate=off` is recorded in TASK-036.03's notes as a ~300-error failure mode, and that
  experiment belongs to whoever actually needs it.

### 8. Gates (AC #8)

Exactly as `ci.yml` enumerates them, under `nix develop .#default`: `cargo fmt --all --check`; `cargo clippy
--workspace --all-targets -- -D warnings`; `cargo clippy -p asperitas-logging --features log-usb --lib -- -D
warnings`; the same with `--features log-defmt`; `cargo clippy --workspace --all-targets --features
asperitas-pod/pod-hw -- -D warnings`; `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` twice (default
and `--all-features`); `cargo test --workspace` and again with `pod-hw`; `cargo run -p asperitas-logging --example
dump_reassemble -- --selftest`; then `cd firmware && cargo build --release --features seed3` and `cargo build
--release --no-default-features --features "seed3 log-defmt"`. Record the outputs in the ticket notes, including the
red/green pair from step 2 and whatever addresses and counts steps 6 and 7 measured.

## Not in scope

Implementing the MPU window or any linker surgery for `_SEGGER_RTT` (documented, deliberately unfiled); enabling
`disable-blocking-mode` or `drop-on-contention`; anything needing a probe (TASK-037); the DWARF level (TASK-054); the
probe CLI surface (TASK-053); populating `RigConfig`'s `icache`/`dcache` fields, which belong with the rig binary in
TASK-038.03.02.02 and are noted here only so the omission is on record.

## Promotion gate - now satisfied

The old plan held this at `To Do` until TASK-053 and TASK-054 were Done, because all three touch the same Makefile
comment block, the same `[profile.release]` comment and the same documentation section. Both are Done (`93c5721`,
`6fbd6bc`), and this plan's reference table is written against their output, so the ticket goes to Dev Ready.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
**Two of your six are already closed by TASK-054 (2026-09-12), so don't redo them.**

1. The `firmware/Makefile` "giving a correct ~32 KB binary for DFU flashing" claim is gone: it now
   carries the measured 88,741 bytes (`FEATURES="seed3"`) and 48,320 bytes (`FEATURES="seed3
   log-defmt" NO_DEFAULT=1`) against the 131,072-byte budget, with a note that the figures belong
   to the profile they were taken at.
2. `crates/asperitas-logging/src/defmt_log.rs`'s `DEFMT_LOG` size ladder was re-measured at the new
   `[profile.release] debug = 2` and rewritten to stop implying monotonicity.

**A finding worth your attention while you are in that module.** At `debug = 2` the `info` build is
47,712 bytes, *below* the unset/`error` baseline of 48,320; at `line-tables-only` it was 48,304
*above* 48,084. Each reproduced on a second clean build into a fresh target dir. Nobody has
explained it, and TASK-054 left it alone as out of scope. If any record in `defmt_log.rs` still
asserts that adding levels only ever grows the image, that is the sentence to fix.

### Executed 2026-09-12 (@ralph)

**AC #1/#2 - the margin argument and its gate.** Rewritten against the vendored sources at
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/{defmt-rtt-1.3.0,defmt-1.1.1,defmt-macros-1.1.1}`.
The load-bearing fact the old comment missed: with the default rzcobs encoding every encoder
callback carries **one byte** (`defmt-1.1.1/src/encoding/rzcobs.rs:57`, `let mut write_byte = move
|b: u8| write(&[b]);`), and `defmt-rtt`'s logger forwards each callback straight to
`Channel::write_all` (`src/lib.rs:166-186`). So no whole-frame write ever approaches `BUF_SIZE` and
frame size cannot gate whether a write fits; occupancy versus host drain rate does, via
`write_all`'s loop (`channel.rs:38-42`) over `blocking_write` returning 0 at `available == 0`
(`channel.rs:57-59`), all inside `critical_section::acquire()`/`release()` (`src/lib.rs:168`, `:239`).
The oversized-frame refusal is `channel.rs:93`, inside `#[cfg(feature = "drop-on-contention")] impl
Channel` starting `:90`, which our build does not compile; default `write_impl` (`:71-87`) clamps at
`bytes.len().min(available)`. Kept the conclusion (256-byte window against 1023 usable), dropped the
guarantee. Also fixed `env_filter.rs:34` -> `:35` in both places (`defmt_log.rs`, `daisy-seed3.md`);
`:34` is the sibling `LEVEL_WHEN_LEVEL_IS_NOT_SPECIFIED = Some(Level::Trace)`.

Gate added: `RTT_BUF_SIZE` / `RTT_RING_USABLE` / `FRAME_OVERHEAD_BYTES` / `WORST_ENCODED_FRAME` plus
`const _: () = assert!(WORST_ENCODED_FRAME < RTT_RING_USABLE, "...")`. The message is a plain literal;
a formatted message is E0015 in const context.

Proof it bites, `BODY_WINDOW` set to 2000:

```text
error[E0080]: evaluation panicked: worst-case defmt frame exceeds defmt-rtt's usable ring
   --> crates/asperitas-logging/src/defmt_log.rs:122:15
```

from `nix develop .#default --command cargo clippy -p asperitas-logging --features log-defmt --lib --
-D warnings`; reverted to 256 and re-run, green. Which gate catches it: only that `log-defmt`
invocation. Measured, not assumed - with `BODY_WINDOW` still at 2000, `cargo clippy --workspace
--all-targets -- -D warnings` finished clean, because `mod defmt_log` is
`#[cfg(feature = "log-defmt")]`.

**AC #3 - sizes.** Re-measured today with `make build` at `[profile.release] debug = 2`:
`blinky.bin` 65,638 (`FEATURES="seed3"`), 25,176 (`NO_DEFAULT=1 FEATURES="seed3 log-defmt"`), 21,202
(`NO_DEFAULT=1 FEATURES="seed3"`). None is "~18 KB", and the line also named a `firmware.bin` that the
Makefile no longer produces, so `daisy-seed3.md:144` now quotes all three with their configs and the
128 KB budget. `capture.rs:111`/`:286` lost their "~69 KB free" headroom claims rather than getting a
new date: that figure traces to the 86.13 % `.bss` baseline TASK-038.03.02.02 records as never having
been measured. Remaining sweep hits are dated measurements, budgets (`memory.x`, Makefile, README) or
test fixtures.

**AC #4 - manifest.** `cargo tree -e features -i defmt@0.3.100 --features seed3,log-defmt` shows two
roots: `embassy-stm32 feature "defmt" -> stm32-metapac feature "defmt" -> defmt 0.3.100`, and
`embassy-net-driver` (via `embassy-usb`) asking 0.3 directly. `cargo info` on 2026-09-12 reports
embassy-net-driver 0.2.0 and stm32-metapac 21.0.0 as latest, so the shim is upstream's to drop. Added
the deliberate-defaults line beside `defmt-rtt`.

**AC #5/#6/#7 - documentation.** Fourth loss row plus `defmt-rtt/src/lib.rs:15-21` quoted verbatim,
mechanism attributed to `host_is_connected()` reading only the mode bits (`channel.rs:151-154`), and an
explicit refusal to claim whether probe-rs restores the flags on detach (TASK-037 measures it). Cache
section rebuilt around SEGGER's *Cortex-M specifics* bullets fetched from kb.segger.com/RTT today.

Measured on the `log-defmt` release `main` ELF today, with `cargo nm ... -- --print-size`:
`_SEGGER_RTT` `0x24000008` size `0x30` (48 B) in `.data` (starts `0x24000000`), and
`defmt_rtt::BUFFER` `0x240010e4` size `0x400` (1 KiB) in `.uninit`; `.bss` starts `0x240005d0`, hence
the ~4 KB gap with unrelated data between the two objects. Precision the plan did not have: the ring's
1 KiB *is* a multiple of a cache line, so only its start is misaligned (4 B in), whereas the control
block fails both (48 B, straddling the line at `0x24000020`). `probe-rs chip info STM32H750IBKx` run
today lists `RAM: 0x20000000..0x20020000 (128.0 KiB)`, so a DTCM relocation stays discoverable.

One correction to the plan's AC #7 recipe: the `"package"` symbol count is NOT a console-versus-RTT
discriminator. Measured: 82 frame symbols (75 `defmt_error`) in the RTT image, 99 (92 `defmt_error`) in
the console image - the console build compiles driver frames too, it just lacks the consolidated
`.defmt` section and any `SEGGER` magic. The doc now says the byte scan
(`strings -a <ELF> | grep -c SEGGER`) is what tells them apart, and records both counts. Verified the
named site independently: `embassy-stm32-0.6.0/src/sai/mod.rs:33` inside
`impl From<ringbuffer::Error> for Error` (only `DmaUnsynced`, returns `Self::Overrun`), reached from
`daisy-embassy ca9bcc9 src/audio.rs:166-178`, whose callback is `FnMut(&[u32], &mut [u32])` and so
cannot be the audio-callback path.

**AC #8 - gates.** All twelve ci.yml invocations run under `nix develop .#default`, all pass: fmt;
clippy workspace/all-targets; clippy `log-usb`; clippy `log-defmt`; clippy with
`asperitas-pod/pod-hw`; `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` default and
`--all-features`; `cargo test --workspace` (34 suites ok) and again with `pod-hw`;
`dump_reassemble --selftest`; `cargo build --release --features seed3`; `cargo build --release
--no-default-features --features "seed3 log-defmt"`.

No follow-up tickets filed: the two open questions this ticket surfaced (does probe-rs restore the RTT
mode flags on detach; when caching gets enabled) are already owned by TASK-037 and TASK-038, and both
are now named in the doc where someone will hit them.

Follow-up filed while sweeping: TASK-057. The build/flash quickstart at docs/reference/daisy-seed3.md:95-108 still names firmware.bin, which the Makefile stopped producing when images became $(BINARY).bin, and its manual cargo objcopy line omits the --only-section list the Makefile calls load-bearing (without it -O binary spans FLASH to RAM, ~469 MB). Outside this ticket's byte-count scope, so it got its own ticket rather than a drive-by edit here.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-12 18:40
---
Planning re-verified every claim against the tree and upstream sources on 2026-09-12; the plan replaces the line references in the description, which have all drifted, and records three findings the description does not have. (1) AC #1's passage needs new reasoning, not a new citation: defmt-rtt's default-mode logger calls write_all once per encoder callback (defmt-rtt/src/lib.rs:166-186 over defmt 1.1.1's streaming Encoder), so no whole-frame write ever approaches BUF_SIZE and the body window cannot make the spin unreachable as defmt_log.rs:101-104 claims. (2) AC #3's two named targets were already fixed by TASK-054 (6fbd6bc); the live stragglers are daisy-seed3.md:144 'blinky is ~18 KB' and capture.rs:111/:286 '~69 KB free', whose .bss premise task-038.03.02.02:88 records as never having existed. (3) defmt 0.3.100 is built and linked, not merely resolved: cargo tree -e features shows daisy-embassy -> embassy-stm32 feature "defmt" -> stm32-metapac feature "defmt", which ties AC #4's manifest fix to the same mechanism that puts foreign frames in the image (AC #7). Also found a seventh wrong citation of the same class: env_filter.rs:34 should be :35, at defmt_log.rs:50 and daisy-seed3.md:416. No sub-tickets: items #3, #5, #6, #7 all edit docs/reference/daisy-seed3.md, so splitting would create conflicts for no independent increment. No acceptance criterion needs hands, so nothing was split off as @human.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Six wrong statements in the defmt-over-RTT record corrected and the frame-size margin pinned, all
without a board. `defmt_log.rs` now argues the margin from the write path our build actually compiles
(one byte per rzcobs callback into `Channel::write_all`, the spin caused by ring occupancy rather than
frame size) and drops the cfg-gated `write_impl` refusal it used to cite; a new
`assert!(WORST_ENCODED_FRAME < RTT_RING_USABLE)` pins the arithmetic, proven red at
`BODY_WINDOW = 2000` and green at 256, and caught only by the `log-defmt` clippy gate. Stale sizes are
gone: blinky re-measured today (65,638 / 25,176 / 21,202 bytes) with configs named, and capture.rs's
unmeasurable "~69 KB free" headroom replaced by the u16 invariant it actually enforces.
`firmware/Cargo.toml` now says what the lock says - defmt 0.3.100 is still resolved and built via
embassy-stm32 -> stm32-metapac and via embassy-net-driver, as a shim over 1.1.1 that upstream has not
dropped - and records why defmt-rtt's two features and its buffer knob stay at defaults on purpose.
The reference doc gains a fourth loss regime for a host detaching mid-run, in defmt-rtt's own words; a
cache section rebuilt on SEGGER's Cortex-M requirements against both measured addresses, neither of
them cache-line aligned; and a passage naming the driver frames present in the linked image with the
nm command that proves it, correcting the plan's claim that the frame-symbol count distinguishes the
console build from the RTT one. All twelve ci.yml gates green.
<!-- SECTION:FINAL_SUMMARY:END -->
