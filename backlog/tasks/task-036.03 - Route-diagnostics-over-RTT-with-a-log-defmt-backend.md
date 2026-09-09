---
id: TASK-036.03
title: Route diagnostics over RTT with a log-defmt backend
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-09 21:42'
updated_date: '2026-09-09 22:40'
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
  - crates/asperitas-logging/Cargo.toml
  - firmware/Cargo.toml
  - firmware/src/bin/main.rs
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
- [ ] #1 asperitas-logging exposes a `log-defmt` feature; Backend gains a Defmt arm and set_backend_defmt(), and a log record reaches defmt through one bridge function that renders the body into a fixed window with the existing TruncWriter and emits exactly one defmt frame per record.
- [ ] #2 All four firmware configurations compile for thumbv7em-none-eabihf: FEATURES="seed3" (USB console, unchanged), FEATURES="seed3 log-defmt" (RTT only), FEATURES="seed3 log-usb log-defmt" (both), and --no-default-features --features seed3 (neither).
- [ ] #3 firmware depends on defmt = "1" rather than the 0.3 compatibility shim, and firmware/Cargo.lock resolves a single defmt major (1.1.x) shared with daisy-embassy and embassy-stm32; the lock file is expected to change for this ticket.
- [ ] #4 Each of the five binaries carries `use defmt_rtt as _` under #[cfg(feature = "log-defmt")] and the existing no-op logger under #[cfg(not(...))], so the link symbols _defmt_write/_defmt_acquire/_defmt_release/_defmt_flush resolve in every configuration; the separate #[defmt::panic_handler] stub stays in all configurations because defmt-rtt does not provide _defmt_panic.
- [ ] #5 With log-defmt selected, handle_panic delivers the formatted panic message over RTT in addition to the LED state, at code level; the nop halt loop is unchanged and no second panic handler is introduced.
- [ ] #6 cargo clippy --release --features "seed3 log-defmt" -- -D warnings is clean, and the host gates (cargo test --workspace, cargo clippy --workspace --all-targets -- -D warnings) stay green.
- [ ] #7 The RTT buffer arithmetic is recorded rather than assumed: defmt-rtt 1.3.0 defaults BUF_SIZE to 1024 bytes via DEFMT_RTT_BUFFER_SIZE, a single frame larger than BUF_SIZE - 1 is dropped even in blocking mode, and the maximum rendered frame is shown to fit with a comment stating the numbers.
- [ ] #8 make build output size is reported before and after for both feature sets, and firmware.bin stays under the 128 KB DFU ceiling.
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
Discovered while landing TASK-036.01 (no probe needed to see it): `probe-rs run` and `probe-rs attach` both abort BEFORE probe discovery with 'Failed to parse defmt data / defmt version found, but no `.defmt` section - check your linker configuration'. Reproduce offline against any built firmware ELF: `probe-rs attach target/thumbv7em-none-eabihf/release/main --chip STM32H750IBKx`. Evidence: firmware/Cargo.lock resolves TWO defmt majors (0.3.100 direct, 1.1.1 transitive via daisy-embassy/embassy-stm32); the ELF carries 100 `.defmt.error.{json}` item sections from 1.x plus a 1.x version marker, but no consolidated `.defmt` section, which is what probe-rs 0.32 decodes from. AC#3 (unify onto defmt 1) is probably the fix, but verify the consolidated `.defmt` section actually appears afterwards — if it does not, defmt 1.x needs its linker fragment on the link line (firmware/.cargo/config.toml currently passes only -Tlink.x). Until this clears, probe-run/probe-log cannot stream anything even with the ST-Link attached.
<!-- SECTION:NOTES:END -->
