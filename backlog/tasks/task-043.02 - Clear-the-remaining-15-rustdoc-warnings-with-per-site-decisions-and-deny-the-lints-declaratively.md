---
id: TASK-043.02
title: >-
  Clear the remaining 15 rustdoc warnings with per-site decisions and deny the
  lints declaratively
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-10 09:17'
updated_date: '2026-09-10 18:58'
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
After TASK-043.01 restored correct link scope, the measured residue under `cargo doc -p asperitas-logging --no-deps` is **4** warnings at default features, **14** at `boot-led,log-usb,log-defmt`, and **14** at `--all-features` — 15 distinct documentation sites, listed one-by-one in the plan with exact replacement strings. The whole set was applied together on HEAD 42aa50e and measured: zero warnings at all three invocations, with `cargo fmt --all --check`, workspace clippy at `-D warnings`, workspace tests, and the firmware workspace's `cargo metadata` all unaffected. Applied verbatim, the ticket is 15 one-line doc edits plus an 8-line Cargo.toml stanza.

Two of the fifteen cannot be links in any configuration, because their target lives behind the *opposite* `#[cfg]`: `[`usb::init`]` sits in an `init()` that only compiles when `log-usb` is off, and the crate-level `init` is named by a `set_backend_defmt` whose callers have `log-usb` on. Those become code text — splitting each sentence into per-feature `#[cfg_attr(feature = ..., doc = "...")]` fragments would double the prose to keep one hyperlink.

The one real judgement call is `emit`, named by public documentation at five sites (crate root, `usb`'s header, `LOG_PIPE`, and twice in `try_emit_dump`). Making it genuinely `pub` would keep all five links live, but publishing the record-commit path invites a binary to frame a record around the `log` facade on a device whose correctness argument is that every commit happens inside one critical section. That alternative and the reason it was not taken go into the notes for the owner to reverse (AC #6); nothing here decides it. Everything else demoted is a private static, a private const, or a private helper — what AC #2 of TASK-043 calls "genuinely meant to stay internal". Links whose targets really are public (`[`frame::write_whole`]`, `[`usb`]`, `[`crate::frame::MAX_BODY`]`, `[`AtomicU32`]`, `[`BootLed`]`) stay links.

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
Every replacement below was applied as a set on HEAD 42aa50e (rustc/cargo 1.97.1) and measured, then reverted. Result: **zero** rustdoc warnings at default features, at `boot-led,log-usb,log-defmt`, and at `--all-features`; and unchanged results for `cargo fmt --all --check`, `cargo clippy -p asperitas-logging --all-targets -- -D warnings`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cd firmware && cargo metadata --format-version 1 --no-deps`. The `[lints.rustdoc]` stanza was proven to bite (part C). Apply the strings verbatim — they are the ones that measured green.

Re-measured starting point on this tree: **4** warnings at default features, **14** at the device feature set, **14** at `--all-features` — 15 distinct documentation sites. Comment #1 is correct that the old `EMIT_TIMEOUT` site is gone after b830069. The 15 sites that do remain account for every warning line at both feature sets: `lib.rs:630` warns only at default features, the 11 sites in part B only when the device features are on, and `lib.rs:20`, `lib.rs:47` and `dump.rs:81` in both. Each site below carries the `file:line` of the warning it clears. Locate sites by the quoted current text, never by line number — numbers shift as edits land.

Rule for the whole ticket: demote a link to code text, never delete the reference, and do not introduce any *new* intra-doc link (an ungated doc pointing into a `log-usb`-only item just adds a warning — the rule recorded in TASK-046/TASK-047). Never put an outer `///` on a `pub mod` declaration line (rust-lang/rust#134904, recorded in TASK-043.01).

A. Sites that warn at default features (4)

A1. `src/lib.rs:20` (crate-root header). Current: "whatever [`Backend`] says." -> New: "whatever the selected `Backend` says." Target is `pub(crate) enum Backend`.

A2. `src/lib.rs:47` (crate-root header). Current: "See [`emit`] and [`frame::write_whole`]." -> New: "See the crate-private `emit()` and [`frame::write_whole`]." At default features `emit` is cfg'd out (unresolved link); at the device set it warns as a private item. `write_whole` is `pub` and stays a real link.

A3. `src/lib.rs:630`, inside the `#[cfg(not(feature = "log-usb"))] pub fn init()` doc. Current: "([`usb::init`] installs the logger as part of bringing up the device)" -> New: "(`usb::init` installs the logger as part of bringing up the device)". Cannot be a link in any configuration: the `usb` module exists only when `log-usb` is on, i.e. exactly when this function is compiled out.

A4. `src/dump.rs:81`. Current: "[`MAX_BODY`](crate::frame::MAX_BODY)" -> New: "[`crate::frame::MAX_BODY`]". Fixes `redundant explicit link target`, which TASK-043.01's span fix exposed; dump.rs glob-imports frame, so the shorthand resolves.

B. Sites that warn only with the device features on (11)

B1. `src/usb.rs:4` (module header). Current: "committed to a framed ring by [`crate::emit`]" -> New: "committed to a framed ring by the crate-private `emit()`".

B2. `src/usb.rs:338` (`emit_panic_record`). Current: "Built in [`PANIC_FRAME`] **without taking the record lock**" -> New: "Built in the static `PANIC_FRAME` **without taking the record lock**" (private `static mut`). The separate `// Safety: see [`PANIC_FRAME`]` note lower in that function is an ordinary comment, not a doc comment — leave it alone.

B3. `src/led.rs:5` (module header). Current: "single constant, [`LED_ACTIVE_LOW`]." -> New: "single constant, `LED_ACTIVE_LOW`." (`pub(crate)` const).

B4. `src/led.rs:18` (module header). Current: "//! [`get_mut`]. An [`AtomicU32`] coordinates" -> New: "//! `with_led`. An [`AtomicU32`] coordinates". `get_mut` has never existed as an item — the accessor is `fn with_led<R>(f: impl FnOnce(&mut BootLed) -> R) -> Option<R>`, and its own doc explains that a callback was chosen precisely so no `&'static mut BootLed` is ever held across an `.await`. Do not word the header so it implies a `get_mut`-style borrow escapes; `with_led` is also honest for the panic path, because `set_global_state` reaches the pins through it. Keep the `AtomicU32` link — it resolves through the module's own `use`.

B5. `src/led.rs:251` (`blink_task`). Current: "Drives the singleton [`BootLed`] through [`with_led`]" -> New: "Drives the singleton [`BootLed`] through `with_led`". `BootLed` stays a link.

B6. `src/lib.rs:121` (`set_backend_defmt`). Current: "/// Unlike [`set_backend_usb`] there is no device to wait for" -> New: "/// Unlike the `log-usb` backend switch `set_backend_usb()` there is no device to wait for". `set_backend_usb` is `pub(crate)` and itself `log-usb`-gated, so the extra wording carries the meaning the dead link used to.

B7. `src/lib.rs:123` (`set_backend_defmt`). Current: "Callers reach it through [`init`];" -> New: "Callers reach it through the crate-level `init()`, which exists only when `log-usb` is off;". Cannot be a link: crate-level `init` is `#[cfg(not(feature = "log-usb"))]` and this feature set turns `log-usb` on.

B8. `src/lib.rs:246` (`LOG_PIPE`). Current: "/// The log pipe: [`emit`] commits framed records, [`usb`]'s drain task empties it." -> New: "/// The log pipe: the crate-private `emit()` commits framed records, [`usb`]'s drain task empties it." KEEP the `usb` link: `LOG_PIPE` is itself `log-usb`-gated, so `usb` is in scope wherever this renders.

B9. `src/lib.rs:517` (`try_emit_dump`). Current: "as [`emit`] does it," -> New: "as the crate-private `emit()` does it,".

B10. `src/lib.rs:522` (`try_emit_dump`). Current: "///   [`RECORD_BUFS`], and two interleaved" -> New: "///   `RECORD_BUFS`, and two interleaved". Keep the two leading spaces after `///` exactly: the line continues a bulleted list, and the following line already names `RECORD_BUFS` in backticks.

B11. `src/lib.rs:555` (`try_emit_dump`). Current: "shape this function mirrors from [`emit`]." -> New: "shape this function mirrors from `emit()`.".

C. Make the failure mode declarative (`crates/asperitas-logging/Cargo.toml`)

Append:

    # Read by rustdoc only: `cargo build/check/clippy/test` never see this table, so a plain
    # `cargo doc` fails on a broken cross-reference without anyone setting RUSTDOCFLAGS.
    [lints.rustdoc]
    broken_intra_doc_links = "deny"
    private_intra_doc_links = "deny"

Measured proof of both halves of AC #4, on this tree with `RUSTDOCFLAGS` unset: a throwaway crate-header line linking `[`NoSuchThing`]` made `cargo doc -p asperitas-logging --no-deps` exit **101** with `error: unresolved link to \`NoSuchThing\``; the same position with `[`Backend`]` exited **101** with `error: public documentation for \`asperitas_logging\` links to private item \`Backend\``; removing the line returned it to exit 0 with no warning lines. Inertness also measured: `cargo verify-project` reports success, and crate-level clippy with `-D warnings` passes with the stanza present. Put the probe line among the crate-root `//!` lines, *above* `#![no_std]` — an outer `///` immediately before an inner attribute is itself a compile error ("an inner attribute is not permitted following an outer doc comment"), which is how the first probe attempt failed here. Delete the probe line before committing.

Verification order

1. Apply A and B, then run all three of AC #1's doc commands and grep for `^warning` (a clean run prints no summary line to count). Expect no output at all from each.
2. Add C, prove it bites per AC #4, remove the probe, confirm clean again.
3. AC #5's gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cd firmware && cargo metadata --format-version 1 --no-deps` (firmware is an excluded workspace consuming this crate by path, so the new `[lints]` table must parse there too — it does).
4. AC #6's two notes paragraphs are already drafted in the ticket. Re-read them against the final diff and correct anything that drifted — a `file:line`, the count of `emit` sites — rather than rewriting them; they record the two things a reviewer cannot recover from the diff.
5. Commit. Check AC #1..#6 explicitly in the final summary.

Out of scope here: the lefthook/CI gate stays with the parent TASK-043, which owns it and needs this ticket's zero-warning state landed first. TASK-049 widens the same deny stanza to the rest of the workspace afterwards.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Judgement call left open deliberately, for the owner: `emit` is named by public documentation at five sites — the crate root (`lib.rs:47`), `usb`'s module header, `LOG_PIPE`, and twice in `try_emit_dump`. Making it `pub` would keep all five as live hyperlinks and clear five of the fourteen device-feature warnings on its own. This plan does not do that. `emit(level, fill: impl FnOnce(&mut [u8; console::BODY_WINDOW]) -> usize)` is the record-commit path; publishing it lets a binary commit a framed record without going through the `log` facade, on a device whose whole correctness argument is that every commit happens in one critical section. If the owner decides the crate should expose it after all, the change is: make `emit` `pub`, then restore those five links exactly as they read today. Same reasoning, smaller stakes, applies to `with_led` (a callback that exists precisely so no `&'static mut BootLed` escapes — see its own doc) and to `LED_ACTIVE_LOW` (publishing a polarity boolean invites a second, disagreeable copy). None is worth widening the API surface of a no_std crate for a hyperlink.

Two references cannot be links in any single build, and no amount of work turns them into links, because each sits on the far side of a `#[cfg]` from its target. `[`usb::init`]` is cited by the crate-level `init()`, which compiles only when `log-usb` is off, while the `usb` module exists only when it is on. The crate-level `init` is cited by `set_backend_defmt`, whose callers run with `log-usb` on, so `init` is compiled out exactly where that sentence renders. The per-feature alternative — `#[cfg_attr(feature = "...", doc = "...")]` fragments, one carrying the link — doubles the prose and creates a feature-pair combination nothing else builds, to save two pairs of backticks. Both stay code text, worded so a reader still learns where the thing lives.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-10 14:36
---
Recounted while re-planning TASK-043.01 (HEAD b830069, rustc/cargo 1.97.1). After 043.01 lands, the device feature set leaves **14** warnings, not 15: site B.7 (usb.rs emit_blocking -> [`EMIT_TIMEOUT`]) no longer exists — commit b830069 moved the panic spin budget into spin_budget.rs and reworded that doc so EMIT_TIMEOUT_CYCLES / EMIT_TIMEOUT_MAX_POLLS are code text, not links. Site A.4's dump.rs:75 is now :81 once 043.01 inserts its new section; your plan already says to locate by quoted text, so that one self-heals. Measured baselines for the record: default 7 -> 4, boot-led,log-usb,log-defmt 23 -> 14, --all-features 23 -> 14. Two of 043.01's restored-span warnings survive into your list with changed shape: led.rs:5 becomes "links to private item `LED_ACTIVE_LOW`" and led.rs:18 stays an unresolved `get_mut` (your B.8/B.9 cover both).
---

created: 2026-09-10 18:58
---
Re-planned against HEAD 42aa50e (rustc/cargo 1.97.1) after TASK-043.01 landed. Re-measured starting counts: 4 / 14 / 14 warnings (default / boot-led,log-usb,log-defmt / --all-features); the site list is otherwise unchanged, with old B.7 (`EMIT_TIMEOUT`) gone exactly as comment #1 predicted. The full fix set was applied here and measured, then reverted: zero warnings at all three invocations, and `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cd firmware && cargo metadata --format-version 1 --no-deps` all stayed green. `[lints.rustdoc]` proven to bite with RUSTDOCFLAGS unset — exit 101 on both an unresolved link and a private-item link, clean again once the probe line goes; `cargo verify-project` succeeds and crate-level clippy is untouched by the stanza. No sub-tickets: one atomic doc-only change, nothing in it needs the board, ears, or an owner decision, so it stays @agent end to end.
---
<!-- COMMENTS:END -->
