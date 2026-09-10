---
id: TASK-043.02
title: >-
  Clear the remaining 15 rustdoc warnings with per-site decisions and deny the
  lints declaratively
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-10 09:17'
updated_date: '2026-09-10 09:20'
labels:
  - planned
dependencies:
  - TASK-043.01
parent_task_id: TASK-043
priority: medium
type: task
ordinal: 76500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After TASK-043.01 restores correct link scope, 15 rustdoc warnings remain under `cargo doc -p asperitas-logging --no-deps --features boot-led,log-usb,log-defmt` (4 of them also appear at default features). Every one is listed in the plan below with a specific fix, and the whole set was prototyped end-to-end on this tree: applying it reaches **zero** warnings at default features, at the device feature set, and at `--all-features`.

Two of the fifteen cannot be links at all, because their target lives behind the *opposite* `#[cfg]`: `[`usb::init`]` sits in an `init()` that only compiles when `log-usb` is off, and `[`init`]` sits in a `set_backend_defmt` whose callers have `log-usb` on. Those become code text — splitting each sentence into per-feature `#[cfg_attr(feature = ..., doc = "...")]` fragments would double the prose to keep one hyperlink.

The one real judgement call is `emit`, referenced by public docs five times. The alternative — make it genuinely `pub` and keep every link live — is recorded in this ticket's notes for the owner to reverse; it is not decided here. `emit(level, fill)` is the record-commit path, and making it callable from a binary invites bypassing the `log` facade on a device where the entire point of the function is that commits happen inside one critical section. Everything else demoted here is a private static, a private const, or a private helper, which is exactly what AC #2 of TASK-043 calls "genuinely meant to stay internal".

Part 2 of 2 under TASK-043. When this lands, AC #1 of the parent is satisfied.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Zero rustdoc warnings, verified by running all three: `cargo doc -p asperitas-logging --no-deps`, `cargo doc -p asperitas-logging --no-deps --features boot-led,log-usb,log-defmt`, and `cargo doc -p asperitas-logging --no-deps --all-features`. Each must print no warning lines at all — grep the output for `^warning`, since a clean run prints no summary line to count.
- [ ] #2 Intent is preserved rather than deleted: every reference demoted from a link keeps its identifier in backticks with enough wording to say where the thing lives (for instance "the crate-private `emit()`", "the static `PANIC_FRAME`"), so a reader can still find it in the source. No cross-reference is dropped silently. Links whose target really is public — `[`frame::write_whole`]`, `[`usb`]`, `[`crate::frame::MAX_BODY`]`, `[`AtomicU32`]` — stay links.
- [ ] #3 led.rs's header stops pointing at `get_mut`, which has never existed as an item, and names `with_led`, the accessor the design actually uses. Read the comment above `with_led` (led.rs ~156) first: it explains why the design rejected handing out `&'static mut BootLed` in favour of a callback, and the header sentence must not imply the rejected shape.
- [ ] #4 crates/asperitas-logging/Cargo.toml gains a `[lints.rustdoc]` section with `broken_intra_doc_links = "deny"` and `private_intra_doc_links = "deny"`, and the deny is proven to bite rather than assumed: add a throwaway inner-doc line linking to `NoSuchThing`, confirm `cargo doc -p asperitas-logging --no-deps` exits non-zero with NO RUSTDOCFLAGS set, then remove the line and confirm clean again. Also confirm the stanza is inert for the rest of the toolchain — `cargo clippy -p asperitas-logging --all-targets -- -D warnings` still passes (verified on 1.97.1: rustdoc lints are never handed to rustc).
- [ ] #5 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass, and `cd firmware && cargo metadata --format-version 1 --no-deps` still succeeds — firmware is an excluded workspace that consumes this crate by path, so the new `[lints]` table has to parse there too.
- [ ] #6 The notes record, in plain sentences, the two things a reviewer cannot recover from the diff: the `emit`-visibility alternative with the reason it was not taken, and the two cfg-impossible links with why no amount of work turns them into links.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Locate every site by the quoted text, not by line number — numbers shift as edits land. All fixes below were applied and measured together on this tree during planning; the result was zero warnings under all three invocations in AC #1, with fmt/clippy/test unaffected.

A. Default-feature sites (4)

1. lib.rs crate-root header, ~line 20. Current: "whatever [`Backend`] says". New: "whatever the selected `Backend` says". Target is `pub(crate) enum Backend` (lib.rs ~63).
2. lib.rs crate-root header, ~line 47. Current: "See [`emit`] and [`frame::write_whole`]." New: "See the crate-private `emit()` and [`frame::write_whole`]." `emit` is a private fn (lib.rs ~345); `write_whole` is pub and stays a real link.
3. lib.rs `init()` doc, ~line 534 — itself `#[cfg(not(feature = "log-usb"))]`. Current: "([`usb::init`] installs the logger ...". New: "(`usb::init` installs the logger ...". Unfixable as a link: the `usb` module exists only when `log-usb` is on, i.e. exactly when this function is compiled out.
4. dump.rs ~line 75. Current: "`MAX_BODY`](crate::frame::MAX_BODY)". New: "[`crate::frame::MAX_BODY`]". Fixes the redundant-explicit-target lint that TASK-043.01 uncovers; dump.rs glob-imports frame, so the shorthand already resolves.

B. Device-feature-only sites (11)

5. usb.rs header ~4: "committed to a framed ring by [`crate::emit`]" -> "committed to a framed ring by the crate-private `emit()`".
6. usb.rs `emit_panic_record` ~334: "Built in [`PANIC_FRAME`]" -> "Built in the static `PANIC_FRAME`" (private `static mut`, usb.rs ~87).
7. usb.rs `emit_blocking` ~375: "or after [`EMIT_TIMEOUT`]" -> "or after `EMIT_TIMEOUT`" (private const, usb.rs ~51).
8. led.rs header ~5: "single constant, [`LED_ACTIVE_LOW`]." -> "single constant, `LED_ACTIVE_LOW`." (`pub(crate)` const, led.rs ~38).
9. led.rs header ~18: "//! [`get_mut`]. An [`AtomicU32`] coordinates" -> "//! `with_led`. An [`AtomicU32`] coordinates". Keep the `AtomicU32` link — it resolves through the module's own `use`.
10. led.rs `blink_task` ~251: "through [`with_led`]" -> "through `with_led`".
11. lib.rs `set_backend_defmt` ~121: "Unlike [`set_backend_usb`]" -> "Unlike `set_backend_usb`" (`pub(crate)`, log-usb-gated).
12. lib.rs `set_backend_defmt` ~123: "Callers reach it through [`init`]" -> "Callers reach it through `init`". Unfixable as a link: crate-level `init` is `#[cfg(not(feature = "log-usb"))]`, and this feature set turns `log-usb` on.
13. lib.rs `LOG_PIPE` doc ~240: name the crate-private `emit()` in prose, and KEEP the [`usb`] link — LOG_PIPE is itself log-usb-gated, so `usb` is in scope wherever this renders.
14. lib.rs `try_emit_dump`, three separate spots (~427, ~432, ~462): "as [`emit`] does it" -> "as the crate-private `emit()` does it"; the continuation line "  [`RECORD_BUFS`], and two interleaved" -> two leading spaces then `RECORD_BUFS` in backticks (private Mutex static, lib.rs ~291) — keep the indentation exact, that line continues a bulleted list; "mirrors from [`emit`]." -> "mirrors from `emit()`.".

C. Make the failure mode declarative

15. Append to crates/asperitas-logging/Cargo.toml a `[lints.rustdoc]` section setting `broken_intra_doc_links = "deny"` and `private_intra_doc_links = "deny"`, with a short comment saying only rustdoc reads it — so check, clippy, build and test are unaffected and nobody has to remember RUSTDOCFLAGS. Prove it per AC #4 rather than trusting it.

Verification order: apply A and B, run the three doc commands (AC #1), then C with its proof, then AC #5's gates, then write the notes (AC #6) and commit. Do not fold the CI/lefthook gate into this ticket — the parent owns it, and it needs this ticket's zero-warning state landed first.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Judgement call left open deliberately, for the owner: `emit` is named by public documentation five times (crate root twice, LOG_PIPE, try_emit_dump x3) plus once as `crate::emit` in usb.rs. Making it `pub` would keep all six as live hyperlinks and clear six of the fifteen warnings on its own. This plan does not do that. `emit(level, fill: impl FnOnce(&mut [u8; console::BODY_WINDOW]) -> usize)` is the record-commit path; publishing it lets a binary commit a framed record without going through the `log` facade, on a device whose whole correctness argument is that every commit happens in one critical section. If the owner decides the crate should expose it after all, the change is: make `emit` `pub`, then restore the six links exactly as they read today. Same reasoning, smaller stakes, applies to `with_led` (a callback that exists precisely so no `&'static mut BootLed` escapes) and to `LED_ACTIVE_LOW` (publishing a polarity boolean invites a second, disagreeable copy). Neither is worth widening the API surface of a no_std crate for a hyperlink.
<!-- SECTION:NOTES:END -->
