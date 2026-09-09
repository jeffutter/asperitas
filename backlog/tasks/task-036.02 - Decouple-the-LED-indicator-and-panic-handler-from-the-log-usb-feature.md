---
id: TASK-036.02
title: Decouple the LED indicator and panic handler from the log-usb feature
status: Done
assignee:
  - '@agent'
created_date: '2026-09-09 21:42'
updated_date: '2026-09-09 23:12'
labels:
  - planned
dependencies: []
documentation:
  - docs/reference/daisy-pod.md
modified_files:
  - crates/asperitas-logging/Cargo.toml
  - crates/asperitas-logging/src/lib.rs
  - crates/asperitas-logging/src/panic_handler.rs
parent_task_id: TASK-036
priority: high
type: task
ordinal: 68500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`pub mod led` and `pub mod panic_handler` in asperitas-logging are gated behind `log-usb` (lib.rs:183-190) even though the LED indicator needs nothing but GPIO, embassy-time and a StaticCell. Consequence: any binary that selects the coming defmt backend, or no backend at all, silently loses the boot-stage LED *and* the shared panic handler — which is precisely the "board dies before USB enumerates" case the probe exists to serve. The gating also contradicts the crate's own doc-comment at lib.rs:9-12, which promises feature-selected backends rather than feature-selected diagnostics.

This ticket is a pure refactor: introduce a `boot-led` feature that owns embassy-stm32/embassy-time/static_cell, have `log-usb` imply it, and cfg-gate the one genuinely USB-specific call inside `handle_panic`. No new backend, no behaviour change on the current matrix, no new dependency versions. It exists so the defmt backend lands on a facade whose diagnostic surface is orthogonal to its transport.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 asperitas-logging declares a `boot-led` feature covering dep:embassy-stm32, dep:embassy-time and dep:static_cell, and `log-usb` enables it, so every configuration that builds today keeps building with the same shape.
- [x] #2 pub mod led and pub mod panic_handler are gated on `boot-led` rather than `log-usb`; handle_panic calls usb::emit_panic_record only under #[cfg(feature = "log-usb")] and keeps its existing order — LED first, then serial, then halt.
- [x] #3 cargo check -p asperitas-logging --target thumbv7em-none-eabihf --features boot-led compiles the crate with the LED and panic handler present and the USB transport absent.
- [x] #4 Host gates are unchanged and green: cargo test --workspace, cargo clippy --workspace --all-targets -- -D warnings, and the pod-hw variant of the same clippy invocation.
- [x] #5 cd firmware && cargo build --release --features seed3 still produces a working image, and firmware/Cargo.lock is byte-identical afterwards (no dependency movement in a refactor).
- [x] #6 git diff touches only crates/asperitas-logging/{Cargo.toml,src/lib.rs,src/panic_handler.rs} plus doc comments; none of the five firmware binaries change.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Approach

One feature, three files, zero behaviour change. Land it green on the current matrix before
anyone builds a second transport on top of it.

## Step 1 — features (`crates/asperitas-logging/Cargo.toml:6-8`)

Today:

    default = []
    log-usb = ["dep:embassy-usb", "dep:embassy-stm32", "dep:embassy-futures",
               "dep:embassy-time", "dep:static_cell", "dep:embassy-sync"]

Change to:

    default = []
    boot-led = ["dep:embassy-stm32", "dep:embassy-time", "dep:static_cell"]
    log-usb  = ["boot-led", "dep:embassy-usb", "dep:embassy-stm32", "dep:embassy-futures",
                "dep:embassy-time", "dep:static_cell", "dep:embassy-sync"]

Repeating the three `dep:` entries under `log-usb` alongside `boot-led` is intentional: it keeps
`--no-default-features --features log-usb` honest about what it needs even if `boot-led` changes
later. The name describes what the feature delivers — visible boot stages — not which peripheral
it pokes.

## Step 2 — module gating (`src/lib.rs:183-190`)

The block currently puts all three modules behind `log-usb`. Move `pub mod led` and
`pub mod panic_handler` behind `feature = "boot-led"`; `pub mod usb` stays where it is. Update the
crate doc-comment at lib.rs:9-12 in the same pass: it advertises backend-selected-by-feature and
now also has diagnostics-selected-by-feature, and leaving the stale text is how the next person
concludes the gating is accidental.

## Step 3 — `handle_panic` (`src/panic_handler.rs:42-68`)

Exactly one statement is USB-specific: `crate::usb::emit_panic_record(msg)` (around :57-58). Gate
it `#[cfg(feature = "log-usb")]` and keep the existing sequence — LED state (:43-44), serial
message, halt (:66-68) with its written rationale for avoiding `bkpt()`. With `log-usb` off and
`boot-led` on the function degrades to LED-plus-halt, which is precisely what a board that dies
before USB enumeration can still do. Fix the module doc-comment so it stops promising a serial
message unconditionally.

## Step 4 — one wording fix in the facade (`src/lib.rs:46-62`)

`Backend::NoOp`'s comment describes itself as "the state before `usb::init` switches the backend".
With a second transport coming, generalise to "`usb::init` is one of the things that may switch
it". Comment only. Do not design the second backend in this ticket.

## Step 5 — verify

Host gates, the ones CI already runs:

    cargo test --workspace
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings

The configuration CI never builds, and the evidence for acceptance criterion #3 — LED and panic
handler present, USB transport absent:

    cargo check -p asperitas-logging --target thumbv7em-none-eabihf --features boot-led

Run it from the repo root: the root workspace has no `.cargo/config.toml`, so no link args are
injected and `check` links nothing. embassy-stm32 gets its device feature from this crate's own
optional-dependency declaration. If it fails for a reason unrelated to the gating (linker script,
memory-x), report that in the final summary instead of papering over it — the firmware build below
exercises the same code paths for real.

Firmware, unchanged behaviour, and the lock file must not move:

    cd firmware && cargo build --release --features seed3 && git diff --exit-code Cargo.lock

Then confirm the diff touches only the three files listed in modified_files.

## Notes

* `led.rs` is named for LEDs but needs more than GPIO: embassy-time for `blink_task`, static_cell
  for the singleton. That is why `boot-led` carries three dependencies and why bundling it under
  `log-usb` was ever done.
* Touch none of the five binaries. Their `#[panic_handler]` wrappers keep calling
  `asperitas_logging::panic_handler::handle_panic`, which stays exported whenever `boot-led` is on
  — and `log-usb` implies `boot-led`, so every build today sees the same symbol.
* `ledtest.rs:45-50` deliberately keeps its own nop-loop handler and is unaffected.
* TASK-010 established that `#[defmt::global_logger]` cannot be hoisted out of a bin crate.
  Nothing here tries.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Shipped exactly the plan's shape, plus four `#[cfg(feature = "log-usb")]` gates the plan did not
list but boot-led-only demands: `use core::fmt::Write`, the `PANIC_MSG_BUF` const,
`format_panic_message`, and a `let _ = info;` under `not(log-usb)` so the unread parameter is not
a warning. Without those four the new configuration builds with dead-code warnings, which is the
one thing a refactor ticket must not leave behind.

Evidence for AC#3, beyond `cargo check -p asperitas-logging --target thumbv7em-none-eabihf
--features boot-led` finishing clean with zero warnings: `cargo doc` for that same feature set
lists `led` (init, set_global_state, blink_task) and `panic_handler` (handle_panic), and emits no
`usb/` module at all. Also checked the two neighbouring configurations so nothing regressed
sideways: `--no-default-features --features log-usb` and bare `--no-default-features` both check
clean on thumbv7em.

Rustdoc noise deliberately held at baseline. Default features gave 5 unresolved-link warnings
before the change and 5 after. Two things were done to keep it there: the new crate-doc bullet
refs `led`/`panic_handler` in backticks rather than as intra-doc links, and panic_handler's
existing `[`crate::usb::emit_blocking`]` link became plain text — it resolved before this ticket
only because the module was compiled whenever `usb` was, and boot-led-only would have made it the
ticket's one new warning. `boot-led` alone reports 13, `log-usb` 21; every extra is pre-existing
`led.rs` private-item linking, unrelated to this change. `cargo doc` is in neither CI nor
lefthook, so this is hygiene, not a gate.

Measured, not assumed, that the refactor is codegen-neutral. Built all five firmware images at
HEAD and with the change, then extracted the flashable sections the way `make build` does
(`.vector_table .text .rodata .data *.sgstubs *.defmt*`). Sizes identical across the board —
blinky 65030, ledtest 17774, main 88101, panictest 65350, podtest 72049. main, panictest and
podtest differ in exactly **2 bytes each**: embedded panic-site line numbers, 138→150 and
290→304, i.e. the doc lines added above the sites. blinky and ledtest additionally differ by one
28-byte function placed in a different order with its branch displacements re-tuned — same
instructions, linker layout shuffle. HEAD's main.bin hashes to e3b68f4ba6e403d8…, matching what
TASK-036.01 recorded, so the comparison is against the right baseline.

Gotcha worth carrying forward: **host gates never compile `led` or `panic_handler`.** This
crate's `default = []` and embassy-stm32 cannot build for the host, so `cargo clippy --workspace
--all-targets` and `cargo test --workspace` lint neither module even though they are green. Real
compile coverage for them is only the thumbv7em `cargo check` per feature set and the firmware
release build. A future ticket that breaks those modules will pass CI; run the target checks.

<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
`boot-led` now owns the LED indicator and the shared panic handler: three optional deps
(embassy-stm32, embassy-time, static_cell), `log-usb` implies it and repeats them so it stays
honest on its own. `pub mod led` and `pub mod panic_handler` moved off `log-usb`; the single
USB-specific statement in `handle_panic` — `usb::emit_panic_record` — is gated, leaving LED-then-
halt for a board that dies before enumeration. Crate docs gained a diagnostics-features section
that says which surface belongs to which feature, and NoOp's comment no longer names `usb::init`
as the only thing that can switch the backend.

All six ACs verified without hardware: host gates (fmt, test workspace, clippy x2 including the
pod-hw variant, pod-hw tests, dump_reassemble selftest) green; thumbv7em checks green for
boot-led, log-usb and no-features; firmware release build green with `firmware/Cargo.lock`
byte-identical; diff confined to the three ticketed files. Flashable images are size-identical
and differ only in relocated panic line numbers, so "no behaviour change" is a measurement here
rather than an intention. Nothing reaches silicon — the defmt backend (TASK-036.03) now lands on
a facade whose diagnostics are orthogonal to its transport.
<!-- SECTION:FINAL_SUMMARY:END -->
