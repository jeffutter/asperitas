---
id: TASK-043.01
title: >-
  Move the six module summaries onto their own //! headers so intra-doc links
  resolve again
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-10 09:16'
updated_date: '2026-09-10 09:26'
labels:
  - planned
dependencies: []
parent_task_id: TASK-043
priority: medium
type: task
ordinal: 75500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`rust-lang/rust#134904`: when a `pub mod foo;` declaration carries a `///` doc comment AND `crates/asperitas-logging/src/foo.rs` carries `//!` docs, rustdoc merges the two fragments *before* resolving intra-doc links. The module's own inner-doc links then resolve against the **crate root**, and their spans are thrown away.

Measured on this tree (rustc/cargo 1.97.1): exactly four warnings report no source location at all — `Decoder`, `status_body`, `StatusGate::due`, `BlockAssembler` — and under the device feature set the led.rs header links fail the same way (`LED_ACTIVE_LOW`, `blink_task` x2, `BootLed`, `init`, `StaticCell`, `get_mut`, `AtomicU32`). Reproduced minimally outside the repo: `///` on `pub mod two;` plus `//!` in two.rs makes a link from two.rs's header to a `pub` item in that same module report "no item named X in scope"; deleting the `///` fixes it with no other change.

This ticket removes those outer comments and moves their unique prose onto the headers where it already belongs, so nothing is lost from the documentation and correct resolution comes back for free. Do NOT fix these four by fully qualifying the paths instead: qualification clears the same messages while leaving every *other* link in those headers resolving in the wrong scope, which was measured to hide a real lint — `dump.rs:75`'s redundant-explicit-target warning only became visible once the spans were correct.

Part 1 of 2 under TASK-043. Lands a deliberately non-zero warning count; TASK-043.02 finishes the job.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 lib.rs's declarations of `frame`, `console` and `dump` carry no `///` comment, and the unique prose each carried now appears in that module's own `//!` header: frame -> a new "# Why this module is not behind `log-usb`" section above "# Why framing exists" (the codec is pure byte arithmetic CI must exercise on the host with default features); console -> folded into the existing "# Why this module is not behind `log-usb`" section (ungated for the same reason as frame, because the field set is a contract the host parses); dump -> a new "# Why this module is not behind `log-usb`" section above "# Why base64 and not something denser". Each module's first `//!` line already restates the outer summary line, so verify by reading both sides that only the rationale moves.
- [ ] #2 The three stub comments — `mod defmt_log;`, `pub mod led;`, `pub mod panic_handler;` ("... See the module docs.") — are deleted with nothing moved, because each module's own first `//!` line says the same thing more fully. These three matter even though none of them warns today: they keep the #134904 hazard armed for the next link anyone adds, and defmt_log is private, so its broken scope stays invisible until someone flips it public.
- [ ] #3 Any link inside moved prose is written fully qualified — `[`crate::frame`]`, not `[`frame`]` — because it now resolves in the module's own scope rather than the crate root's. In console.rs and dump.rs `frame` is not imported, so the short form would be a brand-new unresolved link.
- [ ] #4 Verified intermediate state, by running both commands: `cargo doc -p asperitas-logging --no-deps` goes 7 -> 4 warnings and `cargo doc -p asperitas-logging --no-deps --features boot-led,log-usb,log-defmt` goes 24 -> 15. Every warning that disappears is one of the no-location ones. One NEW warning appears, `redundant explicit link target` at crates/asperitas-logging/src/dump.rs:75, because correct spans let rustdoc see that `[`MAX_BODY`](crate::frame::MAX_BODY)` spells out a target that already resolves. Leave it — TASK-043.02 owns it. AC #1 of TASK-043 is NOT met by this ticket.
- [ ] #5 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass, and `git diff --no-ext-diff -U0 crates/ | grep -E '^[+-][^+-]' | sed -E 's/^.//' | grep -vE '^ *(///|//!)'` prints nothing — doc comments only. `--no-ext-diff` is NOT decoration: this repo sets `diff.external` to difftastic, whose side-by-side output makes the plain form print nothing for *any* change, code included (measured both ways on this tree).
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Mechanical; no design choices left open.

Order of work
1. Read each of the six declaration sites in crates/asperitas-logging/src/lib.rs together with the first ~25 lines of the matching module file, so the merge point is chosen from what is actually there rather than from this plan's wording.
2. frame.rs / console.rs / dump.rs: insert the moved rationale as a short `//!` section. console.rs already has the exact section (# Why this module is not behind `log-usb`) — extend it rather than adding a second one. frame.rs and dump.rs do not; add one immediately before their first existing `//! #` heading.
3. lib.rs: delete the six outer `///` comments (frame, console, dump, defmt_log, led, panic_handler). Leave the `#[cfg(feature = ...)]` attributes and the `pub mod` / `mod` lines themselves untouched, and leave the two banner comments around them.
4. Run the two cargo doc commands in AC #4 and check the counts and the identity of the surviving warnings, not just that the number dropped. If any warning survives that is NOT on the known list (lib.rs Backend / emit / usb::init / init / set_backend_usb / LOG_PIPE-emit / try_emit_dump x3, usb.rs crate::emit / PANIC_FRAME / EMIT_TIMEOUT, led.rs LED_ACTIVE_LOW / get_mut / with_led, dump.rs:75), stop and report — it means the move changed scope somewhere unexpected.
5. AC #5 gates, then commit.

Reference: the whole change is doc comments. A prototype of exactly this step was run on this tree on the planning pass and produced 7 -> 4 and 24 -> 15 with no other effect on fmt/clippy/test.
<!-- SECTION:PLAN:END -->
