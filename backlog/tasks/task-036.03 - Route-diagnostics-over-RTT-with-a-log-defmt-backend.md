---
id: TASK-036.03
title: Route diagnostics over RTT with a log-defmt backend
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-09 21:42'
updated_date: '2026-09-10 04:33'
labels:
  - planned
dependencies:
  - TASK-036.01
  - TASK-036.02
references:
  - 'https://defmt.ferrous-systems.com/features/filtering'
documentation:
  - docs/reference/rust-daisy-stack.md
modified_files:
  - crates/asperitas-logging/src/lib.rs
  - crates/asperitas-logging/src/defmt_log.rs
  - crates/asperitas-logging/src/panic_handler.rs
  - crates/asperitas-logging/Cargo.toml
  - firmware/Cargo.toml
  - firmware/build.rs
  - firmware/Makefile
  - firmware/src/bin/main.rs
  - firmware/src/bin/blinky.rs
  - firmware/src/bin/ledtest.rs
  - firmware/src/bin/panictest.rs
  - firmware/src/bin/podtest.rs
parent_task_id: TASK-036
priority: high
type: task
ordinal: 69500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The third backend. With a probe wired, defmt over RTT gives diagnostics that share nothing with the USB stack: no endpoint, no pipe, no drain task, no host-side serial reader that can fall behind. It is also the only channel that speaks during the pre-USB window, which is where the RAM-length hard fault lived (docs/reference/daisy-seed3.md:137-144).

Scope: a `log-defmt` feature on asperitas-logging that renders a `log::Record` body through the existing TruncWriter and emits one defmt frame per record; firmware features that make the transport selectable (`default = ["log-usb"]`, so CI's `cargo build --release --features seed3` is untouched); unification onto defmt 1.x; and turning the five verbatim no-op `#[defmt::global_logger]` stubs into a cfg-gated pair.

Two honest qualifications belong in the ticket rather than in a footnote, because they are first-order for a device with a ~667 us audio block deadline. First, "lossless" is conditional on the host being attached: defmt-rtt initialises its up-channel in MODE_NON_BLOCKING_TRIM (defmt-rtt-1.3.0/src/lib.rs:113) and probe-rs flips it to block-if-full on attach; unattached, records are dropped or truncated. Second, once attached, probe-rs documents that block-if-full "can cause the application to freeze if the buffer becomes full and is not read by the host" — and defmt-rtt writes every frame inside `critical_section::acquire()`. That is acceptable only because nothing logs from the audio callback today, and the plan says so out loud.

The framed USB console stays the capture transport: TASK-031, TASK-032 and TASK-038 all parse its sequence numbers and loss counters, and defmt cannot carry base64 audio dumps. This ticket does not replace it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 asperitas-logging exposes a `log-defmt` feature; Backend gains a Defmt arm and set_backend_defmt(), and a log record reaches defmt through one bridge function that renders the body into a fixed window with the existing TruncWriter and emits exactly one defmt frame per record.
- [x] #2 All four firmware configurations compile for thumbv7em-none-eabihf: FEATURES="seed3" (USB console, unchanged), FEATURES="seed3 log-defmt" (RTT only), FEATURES="seed3 log-usb log-defmt" (both), and --no-default-features --features seed3 (neither).
- [x] #3 firmware depends on defmt = "1" rather than the 0.3 compatibility shim, and firmware/Cargo.lock resolves a single defmt major (1.1.x) shared with daisy-embassy and embassy-stm32; the lock file is expected to change for this ticket.
      DEVIATION, deliberate: the second half of this criterion is unreachable from this repo. stm32-metapac 21.0.0 pins `defmt = "0.3.0"` behind a feature embassy-stm32 turns on, so the lock keeps two majors. What the criterion is for — one encoder owning the wire format — is verified instead: 0.3.100 is an empty shim whose sole dependency is defmt 1, and probe-rs decodes the built ELF without a version error. Judge whether the residual deserves its own ticket; see notes.
- [x] #4 Each of the five binaries carries `use defmt_rtt as _` under #[cfg(feature = "log-defmt")] and the existing no-op logger under #[cfg(not(...))], so the link symbols _defmt_write/_defmt_acquire/_defmt_release/_defmt_flush resolve in every configuration; the separate #[defmt::panic_handler] stub stays in all configurations because defmt-rtt does not provide _defmt_panic.
- [x] #5 With log-defmt selected, handle_panic delivers the formatted panic message over RTT in addition to the LED state, at code level; the nop halt loop is unchanged and no second panic handler is introduced.
- [x] #6 cargo clippy --release --features "seed3 log-defmt" -- -D warnings is clean, and the host gates (cargo test --workspace, cargo clippy --workspace --all-targets -- -D warnings) stay green.
- [x] #7 The RTT buffer arithmetic is recorded rather than assumed: defmt-rtt 1.3.0 defaults BUF_SIZE to 1024 bytes via DEFMT_RTT_BUFFER_SIZE, a single frame larger than BUF_SIZE - 1 is dropped even in blocking mode, and the maximum rendered frame is shown to fit with a comment stating the numbers.
- [x] #8 make build output size is reported before and after for both feature sets, and firmware.bin stays under the 128 KB DFU ceiling.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Approach

Third backend arm, one bridge function, four configurations that must compile. Everything here is
verifiable with cargo; the probe itself is TASK-037. Order matters: feature topology first (until
it changes, no feature expression can turn the console off), then the bridge, then per-binary
logger selection, then the panic path.

## Step 1 — make the transport selectable (`firmware/Cargo.toml`)

`asperitas-logging` is currently a hard dependency with `features = ["log-usb"]` (L25), so the
console cannot be deselected. Change to:

    asperitas-logging = { path = "../crates/asperitas-logging", default-features = false }

    [features]
    default   = ["log-usb"]            # keeps CI's `--features seed3` build identical
    seed3     = []
    slow-boot = []
    log-usb   = ["asperitas-logging/log-usb"]
    log-defmt = ["asperitas-logging/log-defmt", "dep:defmt-rtt"]

Putting `log-usb` in `default` is what preserves acceptance criterion #2 and both existing gates
(`.github/workflows/ci.yml:40`, `lefthook.yml:27-30`): `--features seed3` adds to defaults rather
than replacing them. Only `--no-default-features` drops the console.

Unify defmt while touching this file:

    defmt     = "1"                                 # was "0.3"
    defmt-rtt = { version = "1", optional = true }

`defmt 0.3.100` in `firmware/Cargo.lock:276-284` is a compatibility shim whose only dependency is
defmt 1.1.1, so the bump changes no source text — `#[defmt::global_logger]` and
`#[defmt::panic_handler]` are still exported by defmt 1.1.1 (lib.rs:321 and :222). What it buys is
a single defmt major in the graph, which matters because the wire format is versioned: mixing
majors between the firmware encoder and probe-rs's decoder is a hard decode error, and probe-rs
reads the string pool out of the ELF that was flashed. Expect `firmware/Cargo.lock` to change —
unlike most tickets here, a lock diff is correct for this one. Root `Cargo.lock` will also gain
defmt entries as locked-but-not-compiled dependencies of asperitas-logging; that is what an
optional dependency does, and it costs nothing.

## Step 2 — the backend (`crates/asperitas-logging`)

`Cargo.toml`: `defmt = { version = "1", optional = true }`, `log-defmt = ["dep:defmt"]`.

`lib.rs` mirrors the shape already at :40-90 rather than inventing a registry:

    pub(crate) enum Backend {
        NoOp,
        #[cfg(feature = "log-usb")]   Usb,
        #[cfg(feature = "log-defmt")] Defmt,
    }

plus `set_backend_defmt()` beside `set_backend_usb()` (:84-90) and a `Defmt` arm in
`Backend::write` that calls the bridge. `pub fn init()` exists only under
`not(feature = "log-usb")` today (:467-473); make it exist whenever `log-usb` is off, selecting
`Defmt` when `log-defmt` is on and `NoOp` otherwise, so a defmt-only binary has one obvious entry
point.

Move `format_body` (lib.rs:322-329, currently `#[cfg(feature = "log-usb")]`) out from under
`log-usb` and share it. It is pure rendering, and one rendering rule for two transports is the
whole point of having a facade.

The bridge — new `src/defmt_log.rs`, cfg-gated internally like the other modules:

    pub(crate) fn emit(record: &log::Record) {
        use core::fmt::Write;
        let mut buf = [0u8; crate::console::BODY_WINDOW];   // 256, the window the USB path uses
        let body_len = crate::format_body(record, &mut buf); // same rule as the console
        let body = core::str::from_utf8(&buf[..body_len]).unwrap_or("<invalid utf8>");
        match record.level() {
            Level::Error => defmt::error!("{}", body),
            Level::Warn  => defmt::warn!("{}", body),
            Level::Info  => defmt::info!("{}", body),
            Level::Debug => defmt::debug!("{}", body),
            Level::Trace => defmt::trace!("{}", body),
        }
    }

The `match` is forced, not stylistic: defmt macros require literal format strings, so there is no
runtime-level defmt call. Check that `defmt` accepts a `&str` under `{}` in 1.1.x with a real
build; if it wants `{:str}`, use that.

Rejected alternatives, so nobody retries them: `log-to-defmt` 0.1.0 has not been published since
2023 and its own docs admit it hard-wires levels to the most verbose level and discards
information; converting the ~20 firmware call-sites to direct `defmt!` macros would abandon the
facade TASK-006 committed to ("when the probe arrives it should be a feature flag, not a
refactor").

Buffer arithmetic to record in a comment (acceptance criterion #7): defmt-rtt 1.3.0 compiles
`BUF_SIZE` from `DEFMT_RTT_BUFFER_SIZE`, default **1024** (build.rs), and `channel.rs:93` refuses
any single write of `BUF_SIZE` or more — such a frame is dropped even in blocking mode. Worst case
here is a 256-byte body plus defmt's per-frame overhead, comfortably inside 1023. State the
numbers; do not enlarge the buffer.

## Step 3 — decide "both features" out loud

With `log-usb` and `log-defmt` together, keep the facade pointed at the framed console — that is
its current behaviour and what TASK-031, TASK-032 and TASK-038 parse — and let RTT carry what the
facade does not own. This is not a compromise, it is what the crate graph already does:
daisy-embassy and embassy-stm32 call `defmt::info!` directly (daisy-embassy ca9bcc9
`src/audio.rs:5`, plus the codec modules), so driver chatter reaches RTT the moment defmt-rtt is
linked, whatever `Backend` says. Write that in the crate doc-comment, because otherwise "why are
my records on USB and embassy's on RTT?" is a mystery handed to the next person.

## Step 4 — the five binaries

Each carries the same no-op `#[defmt::global_logger]` block (main.rs:36-54, blinky.rs:26-44,
ledtest.rs:63-76, panictest.rs:79-91, podtest.rs:30-48) with a NOTE claiming it cannot live in a
lib crate — TASK-010 confirmed the link failure, so that note is true and stays true. Replace each
block with:

    #[cfg(feature = "log-defmt")]
    use defmt_rtt as _;      // supplies _defmt_write / _defmt_acquire / _defmt_release / _defmt_flush

    #[cfg(not(feature = "log-defmt"))]
    mod defmt_logger { /* the existing no-op stub, unchanged */ }

Keep `#[defmt::panic_handler]` in **every** configuration: `_defmt_panic` is a separate symbol
(defmt-1.1.1/src/export/mod.rs:131) that defmt-rtt does not provide, and dropping the stub breaks
linking of every `defmt::assert!` / `unreachable!` in embassy-stm32. So the duplication halves
rather than vanishes: one cfg'd pair per binary instead of an unconditional stub. Say that plainly
in the commit message — "removed the boilerplate" would be a false claim.

## Step 5 — panic over RTT (acceptance criterion #5)

In `panic_handler::handle_panic`, after the LED write and beside the USB record, emit the same
formatted message under `#[cfg(feature = "log-defmt")]`. Keep the nop halt loop and its existing
rationale. Do **not** pull in `panic-probe` yet: it earns its place only if probe-rs cannot decode
a backtrace from what we have, and that is a hardware observation (TASK-037 AC #3), not something
to guess from here. If TASK-037 reports addresses without symbols, the follow-up is `debug = 1` or
`-C force-frame-pointers` plus `panic-probe`'s `print-defmt`, filed as its own ticket.

## Step 6 — verify (no board)

    cd firmware
    make build FEATURES="seed3"                                   # unchanged console build
    make build FEATURES="seed3 log-defmt"                         # RTT only
    make build FEATURES="seed3 log-usb log-defmt"                 # both
    cargo build --release --no-default-features --features seed3   # neither
    make clippy FEATURES="seed3 log-defmt"                        # parent's criterion #4
    ls -l firmware.bin                                            # criterion #8: report both sizes

The repetition that matters: `FEATURES="seed3"` must produce a `firmware.bin` of the same size it
had before this ticket. The console build must not change.

`DEFMT_LOG`: defmt's filter is compile-time (`DEFMT_LOG=debug`, per-module directives supported),
unlike the facade's runtime `set_max_level(LevelFilter::Info)` at lib.rs:145-153. It reaches the
build through the environment — `DEFMT_LOG=debug make build FEATURES="seed3 log-defmt"` — and
Cargo tracks it, so changing it rebuilds. Confirm that once and record the result; do not add a
Makefile variable for an env var that already works.

## Hazards to state in the commit message rather than discover on hardware

* defmt-rtt boots `MODE_NON_BLOCKING_TRIM` (src/lib.rs:113); probe-rs switches the channel to
  block-if-full on attach. Unattached means silent drops. Attached-with-a-stalled-host means the
  target spins.
* Every frame is written inside `critical_section::acquire()` (src/lib.rs:166) and reentrancy is
  `panic!("defmt logger taken reentrantly")` (:173) — fatal, not quiet. The bridge must never call
  `log::` internally; as written it does not.
* Therefore nothing may log from the audio callback. True today (`main.rs:236-255` logs nothing);
  leave a comment at the callback so nobody "just adds one `info!`".
* `drop-on-contention` avoids the IRQ-off window but requires every frame to fit `BUF_SIZE - 1`
  and drops colliding frames — the opposite of why this ticket exists. Leave it off.
* TASK-040's `write_whole` stall assertion lives on the USB path only; the defmt bridge shares no
  code with it.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## RESOLVED — the TASK-036.01 blocker above was a missing linker fragment

Discovered while landing TASK-036.01 (no probe needed to see it): `probe-rs run` and `probe-rs attach` both abort BEFORE probe discovery with 'Failed to parse defmt data / defmt version found, but no `.defmt` section - check your linker configuration'. Reproduce offline against any built firmware ELF: `probe-rs attach target/thumbv7em-none-eabihf/release/main --chip STM32H750IBKx`. Evidence: firmware/Cargo.lock resolves TWO defmt majors (0.3.100 direct, 1.1.1 transitive via daisy-embassy/embassy-stm32); the ELF carries 100 `.defmt.error.{json}` item sections from 1.x plus a 1.x version marker, but no consolidated `.defmt` section, which is what probe-rs 0.32 decodes from. AC#3 (unify onto defmt 1) is probably the fix, but verify the consolidated `.defmt` section actually appears afterwards — if it does not, defmt 1.x needs its linker fragment on the link line (firmware/.cargo/config.toml currently passes only -Tlink.x). Until this clears, probe-run/probe-log cannot stream anything even with the ST-Link attached.

It was not the defmt major. defmt 1.x emits one `.defmt.<level>.{json}` section per macro call site and relies on its own linker fragment, `defmt.x`, to concatenate them into the single `.defmt` section probe-rs reads. Without `-Tdefmt.x` on the link line the items stay loose forever, whatever major is in the lock. `firmware/build.rs` now emits `cargo:rustc-link-arg=-Tdefmt.x` when `CARGO_FEATURE_LOG_DEFMT` is set, so the console build's link line is untouched.

Measured after the change, `main` under `--no-default-features --features seed3,log-defmt`:

* one `.defmt` section, 91 bytes, `INFO` and non-`ALLOC` — which is why it stays out of `firmware.bin` (`llvm-objcopy -O binary` skips non-loadable sections) and out of RAM;
* zero loose `.defmt.*` sections, down from 100;
* `_SEGGER_RTT` present with its up-channel buffer at `0x240010e4`;
* `probe-rs attach <rtt-elf> --chip STM32H750IBKx` now parses defmt cleanly and stops at "No connected probes were found" — the furthest this host can get without the ST-Link, which is TASK-037's job.

The console-only ELF still reproduces the original error verbatim, including before probe discovery. A `log-defmt` build is therefore a precondition of the three `probe-*` make targets, not an option; the Makefile says so where it would otherwise read as a broken target.

## AC#3 is unreachable as written; what was achieved instead

`firmware/Cargo.lock` still resolves two defmt majors. `stm32-metapac` 21.0.0 pins `defmt = "0.3.0"` and embassy-stm32 turns on its `defmt` feature, so no amount of pinning on our side collapses the graph — the 0.3 entry belongs to a dependency we do not control.

What matters for correctness is that one encoder owns the wire, and it does: defmt 0.3.100 is a transition shim whose only dependency is `defmt "1"` (`[dependencies.defmt10]`) and whose `src/lib.rs` is empty apart from `#![no_std]`. Every `impl defmt::Format` in `stm32-metapac` therefore resolves through the shim to the 1.1.1 types, and probe-rs decoded the resulting ELF without a version complaint — a genuine major mismatch is a hard decode error there. Unify-onto-1 was the right instinct in the 036.01 note; it just was not the blocker.

## Sizes (criterion #8)

`firmware.bin` from `make build` per configuration, all five binaries; the 128 KB internal-flash sector is the ceiling and nothing approaches it:

| config | main | blinky | ledtest | panictest | podtest |
|---|---|---|---|---|---|
| `seed3` (console, default) | 88101 | 65030 | 17774 | 65350 | 72049 |
| `seed3 log-defmt`, no default (RTT only) | 47624 | 24688 | 19208 | 24824 | 31468 |
| `seed3 log-usb log-defmt` (both) | 90160 | 67252 | 19208 | 67572 | 74268 |
| neither transport | 45009 | 21170 | 17774 | 21298 | 27997 |

Readings worth keeping: dropping USB for RTT costs `main` 40 KB of flash, and adding defmt alongside the console costs 2059 bytes. `ledtest` grows by 1434 bytes under any `log-defmt` build despite containing no `log::` call of its own — that is defmt-rtt's ring and writer, paid per binary that links the logger.

## "Console build unchanged", stated precisely

Same absolute path, `git stash` of this ticket's diff, `make build` on each side: 88101 bytes both times, 35 bytes differing. They are defmt's interned-format indices in the loose `.defmt.*` item sections (the pool reorders slightly now that the binary crates' `#[defmt::global_logger]` compiles against 1.1.1 instead of 0.3) plus one function whose prologue got rescheduled two instructions either way. No size change, no behaviour change.

Anyone tempted to repeat this comparison: do not compare across directories. `log::info!` embeds `file!()` into `.rodata`, so the build directory is inside the image — the baseline built in `/tmp` carried `/private/tmp/asperitas-base/crates/asperitas-logging/src/usb.rs` and differed from this tree by 10482 bytes for that reason alone.

## DEFMT_LOG reaches the bridge, and where its syntax bites

Confirmed on the RTT build rather than assumed. `DEFMT_LOG=off cargo build --release --no-default-features --features seed3,log-defmt --bin main` removes `emit_frame` from the symbol table entirely — all five level arms expand to nothing — and takes the linked ELF from 122 defmt symbols to 17. Flipping the variable rebuilds on its own; no `cargo clean` was needed. Section sizes do not show this (`.text` reports 38840 either way because the region end comes from the linker script), so compare symbols, not `size -A`.

Two properties of this bridge worth knowing before reaching for a filter: every facade record arrives at five macro sites inside `asperitas_logging::defmt_log`, so a module-path directive cannot separate records by their origin crate — level filtering is the knob that works (`DEFMT_LOG=warn`). And a value the parser rejects fails somewhere you cannot see: `DEFMT_LOG=off,crate=off` produced roughly 300 `proc macro panicked: `crate` is not a valid identifier` errors inside `stm32-metapac`'s generated register code, none naming the variable or this crate. Bare levels only.

## Fixed while verifying

With no transport selected at all (`--no-default-features --features seed3`), `panic_handler.rs` warned about `core::fmt::Write` and `crate::TruncWriter` being unused — those feed `format_panic_message`, which is itself cfg-gated. The imports are gated the same way now, which is what makes `clippy -D warnings` clean in all four configurations rather than three.

## Re-measured sizes (criterion #8) + why the table above was stale

Re-ran all four configurations end to end on a clean env (no `DEFMT_LOG` exported). Console and
no-transport rows reproduce exactly; both `log-defmt` rows were ~460 bytes low above — those
numbers came from an intermediate tree mid-ticket, not from a different filter setting (`off`,
`error`, `warn`, `info`, `debug`, `trace` all measure differently from 47624). Current,
reproducible numbers:

| config | main | blinky | ledtest | panictest | podtest |
|---|---|---|---|---|---|
| `seed3` (console, default) | 88101 | 65030 | 17774 | 65350 | 72049 |
| `seed3 log-defmt`, no default (RTT only) | 48084 | 25148 | 19424 | 25284 | 31932 |
| `seed3 log-usb log-defmt` (both) | 90636 | 67716 | 19424 | 68036 | 74732 |
| neither transport | 45009 | 21170 | 17774 | 21298 | 27997 |

Same readings as before, slightly restated: dropping USB for RTT saves `main` 40 KB, adding defmt
alongside the console costs 2535 bytes, and every binary that links defmt-rtt pays ~1650 bytes for
its ring and writer. Largest artifact is `main` with both transports at 90636 bytes — 71% of the
128 KB internal-flash sector, so criterion #8 holds with ~37 KB spare.

Verified again rather than trusted: `FEATURES="seed3"` still produces 88101 bytes, matching the
pre-ticket baseline byte-for-byte in size.

## First-order gotcha found while verifying: `DEFMT_LOG` unset means ERROR-only

defmt-macros uses `LEVEL_WHEN_NOTHING_IS_SPECIFIED = Some(Level::Error)` when `DEFMT_LOG` is absent
entirely (defmt-macros-1.1.1 src/function_like/log/env_filter.rs:34). Four of the bridge's five arms
expand to nothing, so a plain `make build FEATURES="seed3 log-defmt" NO_DEFAULT=1` image contains
only ERROR frames — `info!("Booting...")` was never compiled in. Evidence: unset and
`DEFMT_LOG=error` builds of `main` are the same size exactly (48084), against warn 48136, info
48304, debug 48984, trace 49712, off 44592.

This matters for TASK-037 more than any other fact here: attaching a probe to a default-built
`log-defmt` image and seeing silence reads as "RTT is broken / the linker fragment is wrong", and
nothing would be. The facade cannot compensate — its runtime `set_max_level(Info)` happily hands a
record to an arm that does not exist. Documented where both audiences will hit it: the
`defmt_log.rs` module docs and the Makefile's probe-target preamble
(`DEFMT_LOG=info make probe-log FEATURES="seed3 log-defmt" NO_DEFAULT=1`).

The earlier claim in these notes that `DEFMT_LOG=off` removes `emit_frame` (122 → 17 defmt symbols)
reproduced exactly.

Fixup applied post-review: crates/asperitas-logging/src/defmt_log.rs:37 had an odd number of backticks in the '# Filtering' section's opening sentence (a stray backtick after "An "), which breaks CommonMark code-span pairing and would garble the rendered rustdoc for the rest of that paragraph. Fixed to "**An RTT build with `DEFMT_LOG` unset...**" and added the missing blank `//!` line before the `# Filtering` heading for consistency with the rest of the module's doc comments. Also reviewed AC#3's DEVIATION note (two defmt majors remain in firmware/Cargo.lock because stm32-metapac pins 0.3 via embassy-stm32, outside this repo's control) and judged no follow-up ticket needed — the deviation is already transparently disclosed on the AC itself and the underlying goal (one encoder owning the wire) is verified. cargo test/clippy/fmt for asperitas-logging all still pass after the fixup.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Third log backend: defmt frames over RTT, selectable per build, sharing no code with the USB stack.

`asperitas-logging` gains a `log-defmt` feature, a `Backend::Defmt` arm, `set_backend_defmt()`, and
one bridge (`src/defmt_log.rs`) that renders a `log::Record` through the same `format_body` the
console uses and emits exactly one defmt frame at the record's level. Firmware features are now
`default = ["log-usb"]`, so `--features seed3` builds exactly as before and only
`--no-default-features` drops the console; `log-defmt` adds `defmt-rtt` to the five binaries, where
the existing no-op `#[defmt::global_logger]` stub became a cfg-gated pair. The
`#[defmt::panic_handler]` stub stays in every configuration — `_defmt_panic` is a separate symbol
defmt-rtt does not provide — so the boilerplate halved rather than vanished. `handle_panic` now
delivers the formatted message over RTT beside the LED state, keeping the nop halt loop.

Two things this ticket settled that were not in the plan:

1. The TASK-036.01 blocker ("no `.defmt` section") was the missing linker fragment, not the defmt
   major. `firmware/build.rs` passes `-Tdefmt.x` when `CARGO_FEATURE_LOG_DEFMT` is set; the ELF now
   carries one 91-byte `.defmt` section instead of ~100 loose ones, and `probe-rs attach` parses it
   cleanly, stopping only at "No connected probes were found". Measured, not inferred.
2. AC#3 (single defmt major) is unreachable from here: `stm32-metapac` pins `defmt = "0.3"` and
   embassy-stm32 enables it, so the 0.3 entry is not ours to remove. What matters is that one
   encoder owns the wire — 0.3.100 is an empty shim whose sole dependency is defmt 1.1.1 — and
   probe-rs decoded the image without a version complaint, which is where a real mismatch surfaces.

Gotcha worth carrying forward: `DEFMT_LOG` unset compiles every non-ERROR defmt call to nothing, so
a plain `log-defmt` build genuinely has no INFO frames. `DEFMT_LOG=info` at build time is required
before TASK-037 concludes the channel is dead. Documented in the module docs and the Makefile.

Verification (no board): all four feature configurations build for thumbv7em-none-eabihf, clippy
`-D warnings` clean for all five binaries under both `seed3` and `seed3 log-defmt`, host
`cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` green. Largest
image is `main` with both transports at 90636 bytes, inside the 128 KB sector; the console-only
build is still 88101 bytes.
<!-- SECTION:FINAL_SUMMARY:END -->
