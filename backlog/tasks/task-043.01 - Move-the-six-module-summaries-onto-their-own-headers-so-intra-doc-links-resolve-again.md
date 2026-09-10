---
id: TASK-043.01
title: >-
  Move the six module summaries onto their own //! headers so intra-doc links
  resolve again
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-10 09:16'
updated_date: '2026-09-10 14:47'
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
- [x] #1 lib.rs's declarations of `frame`, `console` and `dump` carry no `///` comment, and the unique prose each carried now appears in that module's own `//!` header: frame -> a new "# Why this module is not behind `log-usb`" section above "# Why framing exists" (the codec is pure byte arithmetic CI must exercise on the host with default features); console -> folded into the existing "# Why this module is not behind `log-usb`" section (ungated for the same reason as frame, because the field set is a contract the host parses); dump -> a new "# Why this module is not behind `log-usb`" section above "# Why base64 and not something denser". Each module's first `//!` line already restates the outer summary line, so verify by reading both sides that only the rationale moves.
- [x] #2 The three stub comments — `mod defmt_log;`, `pub mod led;`, `pub mod panic_handler;` ("... See the module docs.") — are deleted with nothing moved, because each module's own first `//!` line says the same thing more fully. These three matter even though none of them warns today: they keep the #134904 hazard armed for the next link anyone adds, and defmt_log is private, so its broken scope stays invisible until someone flips it public.
- [x] #3 Any link inside moved prose is written fully qualified — `[`crate::frame`]`, not `[`frame`]` — because it now resolves in the module's own scope rather than the crate root's. console.rs never imports `frame` at all, so the short form there would be a brand-new unresolved link; dump.rs does import it, but use the qualified form there too to match its existing `[`crate::frame`]` links at lines 3 and 58.
- [x] #4 Verified intermediate state, by running both commands: `cargo doc -p asperitas-logging --no-deps` goes 7 -> 4 warnings and `cargo doc -p asperitas-logging --no-deps --features boot-led,log-usb,log-defmt` goes 23 -> 14. (Re-measured on HEAD b830069: the earlier "24 -> 15" was stale — commit b830069 turned usb.rs's `EMIT_TIMEOUT` doc link into code text, removing one device-only warning.) Every warning that disappears is one of the no-location ones, except two that survive with correct spans instead of vanishing: `LED_ACTIVE_LOW` becomes "links to private item" at led.rs:5, and `get_mut` stays unresolved at led.rs:18. One NEW warning appears, `redundant explicit link target` at crates/asperitas-logging/src/dump.rs:81 — it was :75 before this ticket's own insertion shifts it, so match survivors by message text, not line number — because correct spans let rustdoc see that `[`MAX_BODY`](crate::frame::MAX_BODY)` spells out a target that already resolves. Leave it: TASK-043.02 owns it. AC #1 of TASK-043 is NOT met by this ticket.
- [x] #5 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass, and `git diff --no-ext-diff -U0 crates/ | grep -E '^[+-][^+-]' | sed -E 's/^.//' | grep -vE '^ *(///|//!)'` prints nothing — doc comments only. `--no-ext-diff` is NOT decoration: this repo sets `diff.external` to difftastic, whose side-by-side output makes the plain form print nothing for *any* change, code included (measured both ways on this tree).
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Mechanical doc-comment move: delete six outer `///` blocks in `crates/asperitas-logging/src/lib.rs`, relocate the three that carry unique rationale onto their module's own `//!` header. No design choices left open. Every number below was re-measured on this tree by applying exactly these edits, running both doc builds plus fmt/clippy/test, and reverting — reproduce them verbatim rather than re-deriving.

## Why the numbers changed since the first planning pass (7/24 -> 4/15 is stale)

Commit `b830069` ("Fix: bound panic-path emit spin by CPU cycles, not time-driver ticks") moved the panic spin budget into `spin_budget.rs` and reworded `usb.rs`'s `emit_blocking` doc so `EMIT_TIMEOUT_CYCLES` / `EMIT_TIMEOUT_MAX_POLLS` are code text now, not links. That removed one device-only warning. Re-measured on HEAD `b830069`, rustc/cargo 1.97.1, with `touch crates/asperitas-logging/src/*.rs` before each run to defeat cargo's freshness cache:

| `cargo doc -p asperitas-logging --no-deps` | before | after |
|---|---|---|
| default features | 7 | **4** |
| `--features boot-led,log-usb,log-defmt` | 23 | **14** |
| `--all-features` | 23 | **14** |

A clean run prints no summary line at all, so count `grep '^warning: ' | grep -v generated`, or look for the `generated N warnings` summary.

## Scope confirmed exhaustively

The #134904 hazard (`///` on a `mod x;` declaration + `//!` in `x.rs`) exists at exactly six declarations in the whole repo, all in this one file: lib.rs 205 `frame`, 211 `console`, 218 `dump`, 235 `defmt_log`, 239 `led`, 243 `panic_handler`. Scanned every tracked `.rs` in both workspaces including `firmware/src/bin/` (which declares no modules): `asperitas-cli`, `asperitas-dsp`, `asperitas-pod` have no `///` on any `mod` declaration, and `spin_budget` (lib.rs 224-228) is preceded by a plain `//` comment, which does not attach — leave it alone. Nothing outside this ticket's six needs touching, so TASK-048/TASK-049 need no hazard work.

Two facts worth knowing before editing: `defmt_log` and `spin_budget` are private `mod`s, so their headers never appear in `cargo doc` output (already true today, unchanged by this move); and no code fence moves, so no doctest is created or destroyed — the crate's only compiled doctests are untagged fences inside item-level `///` (e.g. `frame.rs:480`), untouched here.

## Order of work

### 1. frame.rs — insert a new section above the first heading

Anchor: lines 1-3 are `//! Console protocol v1 — every log record carries its own framing and checksum.` / `//!` / `//! # Why framing exists`. Insert between the blank `//!` and that heading:

```rust
//! # Why this module is not behind `log-usb`
//!
//! The codec is pure byte arithmetic, so CI must be able to exercise it on the host:
//! `cargo test --workspace` builds this crate with default features, i.e. without any
//! backend at all.
```

The wording drops the original's "Deliberately **not** behind `log-usb`:" preamble because the heading now says it; nothing else from lib.rs:200-204 is lost.

### 2. console.rs — fold into the section that already exists

`# Why this module is not behind \`log-usb\`` is already console.rs lines 4-14. Do NOT add a second one. The only thing the outer comment adds over what is already there is the cross-reference to `frame`; extend the existing sentence rather than restating the argument:

Current: `` //! host even though the root workspace never enables `log-usb`. That matters because ``
New:
```rust
//! host even though the root workspace never enables `log-usb`, for the same reason
//! [`crate::frame`] is ungated. That matters because
```

### 3. dump.rs — insert a new section above the first heading

Anchor: line 17 `//! something acts on that proof — see *Host-side block assembly* near the end of this module.` / line 18 `//!` / line 19 `//! # Why base64 and not something denser`. Insert between 18 and 19:

```rust
//! # Why this module is not behind `log-usb`
//!
//! Ungated for the same reason as [`crate::frame`]: it is pure byte arithmetic whose
//! equivalence with a reference implementation is proven on the host, where the oracle can
//! be a dev-dependency.
```

Qualification matters (AC #3): use `[`crate::frame`]`, never `[`frame`]`. console.rs never imports `frame` at all, so the short form would be a brand-new unresolved link; dump.rs *does* `use crate::frame::{self, ...}`, so the short form happens to resolve there, but `[`crate::frame`]` is already this file's convention (lines 3, 58) — keep it.

### 4. lib.rs — delete the six outer comments

Remove lib.rs 200-204, 207-210, 213-217 (frame/console/dump) and the single-line stubs at 233, 237, 241 (defmt_log/led/panic_handler). Leave everything else byte-identical: both banner comment rules, the `#[cfg(...)]` attributes, the `pub mod`/`mod` lines, the plain `//` comment above `spin_budget`, and the undocumented `pub mod usb;`. The region ends up as:

```rust
// ---------------------------------------------------------------------------
// Ungated modules — pure logic, no hardware types, host-testable by default
// ---------------------------------------------------------------------------

pub mod frame;

pub mod console;

pub mod dump;

// ---------------------------------------------------------------------------
// Feature-gated modules
// ---------------------------------------------------------------------------

// Gated on `any(feature = "log-usb", test)` rather than `log-usb` alone: its arithmetic
// must be testable under default features, which is the only configuration CI's
// `cargo test --workspace` builds. See the module docs.
#[cfg(any(feature = "log-usb", test))]
mod spin_budget;

#[cfg(feature = "log-usb")]
pub mod usb;

#[cfg(feature = "log-defmt")]
mod defmt_log;

#[cfg(feature = "boot-led")]
pub mod led;

#[cfg(feature = "boot-led")]
pub mod panic_handler;
```

Nothing is moved for the three stubs. Their content is restated more fully by each target's own first lines: defmt_log.rs:1 ("one `log::Record` in, exactly one defmt frame out"), led.rs:1-11 (indicator + `blink_task`'s role), panic_handler.rs:1-10 (what the handler does and how it degrades). Only the words "See the module docs." disappear, which is the point.

### 5. Verify counts AND identities (AC #4)

Run both commands (plus `--all-features` if cheap) and check the surviving list, not just the total. Line numbers shift — lib.rs warnings move UP by 17 lines (deletions), the dump.rs one moves DOWN by 6 (insertion) — so match on message text, never on line numbers.

Expected default-features survivors, 4 total:
1. `links to private item \`Backend\`` — lib.rs:20
2. `unresolved link to \`emit\`` — lib.rs:47
3. `unresolved link to \`usb::init\`` — lib.rs:540 (was 557)
4. NEW: `redundant explicit link target` — dump.rs:**81** (was 75 pre-change; correct spans let rustdoc finally see that `[`MAX_BODY`](crate::frame::MAX_BODY)` spells out a target that already resolves). Leave it — TASK-043.02 owns it.

Expected device-feature-set survivors, 14 total: those four (with `usb::init` gone, since `usb` exists in this configuration) plus usb.rs:4 `crate::emit`, usb.rs:338 `PANIC_FRAME`, led.rs:251 `with_led`, lib.rs:121 `set_backend_usb`, lib.rs:123 `init`, lib.rs:246 `LOG_PIPE`→`emit`, lib.rs:433 / :438 / :468 (`try_emit_dump` → `emit`, `RECORD_BUFS`, `emit`).

Two of the twelve no-location warnings do NOT vanish — they gain a location and change shape, which a naive text diff would flag as "new". Expect exactly these two transformations:
- `unresolved link to \`LED_ACTIVE_LOW\`` (no location) → ``public documentation for `led` links to private item `LED_ACTIVE_LOW` `` at led.rs:5:24. It now resolves; the constant is `pub(crate)`.
- `unresolved link to \`get_mut\`` (no location) → still unresolved but now located at led.rs:18:7. `get_mut` has never existed as an item; TASK-043.02 site B.9 fixes the wording.

The other ten no-location warnings must disappear outright: `Decoder`, `status_body`, `StatusGate::due`, `BlockAssembler`, `blink_task` ×2, `BootLed`, `init` (led's own), `StaticCell`, `AtomicU32`. If anything else survives, or if any of those ten survives, stop and report — it means the scope moved somewhere unexpected.

### 6. Gates, then commit

`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` — all three passed on the prototype, and rustfmt does not reflow doc comments (no rustfmt.toml anywhere, default config). Then the doc-only gate:

```
git diff --no-ext-diff -U0 crates/ | grep -E '^[+-][^+-]' | sed -E 's/^.//' | grep -vE '^ *(///|//!)'
```

It must print nothing. `--no-ext-diff` is load-bearing: `git config diff.external` is difftastic on this machine, and without the flag the command prints nothing for *any* diff, code included.

Suggested commit subject: `Docs: move module summaries onto their own //! headers so intra-doc links resolve (TASK-043.01)`.

## Downstream correction (do not fix it here, just know it)

TASK-043.02 says "15 remaining" and lists 15 sites. The true post-move count is 14: its site B.7 (`usb.rs` `emit_blocking` → `[`EMIT_TIMEOUT`]`) no longer exists, killed by `b830069`. Its A.4 also cites dump.rs:75, which becomes :81 once this ticket inserts its section — its own plan already says to locate sites by quoted text, so that one is self-healing. Recorded as a comment on TASK-043.02 during this planning pass so its executor doesn't chase a phantom site.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Executed on HEAD bd5288b (rustc/cargo 1.97.1) exactly per plan. Edits: frame.rs +6 lines (new '# Why this module is not behind `log-usb`' above '# Why framing exists'); console.rs +2/-1 (folded the `crate::frame` cross-reference into the section that already existed); dump.rs +6 lines (new section above '# Why base64 and not something denser', link written as [`crate::frame`]); lib.rs -17 (six outer /// blocks deleted: frame/console/dump rationale moved, defmt_log/led/panic_handler stubs dropped with nothing moved). grep for '///' preceding any 'mod x;' across crates/ and firmware/ now returns nothing.

Measured, with 'touch crates/asperitas-logging/src/*.rs' before each doc build: default features 7 -> 4 warnings; --features boot-led,log-usb,log-defmt 23 -> 14; --all-features 23 -> 14. Survivor identities matched the plan's list exactly (matched by message text, not line number). Default survivors: links-to-private 'Backend' lib.rs:20, unresolved 'emit' lib.rs:47, unresolved 'usb::init' lib.rs:540 (was 557), NEW 'redundant explicit link target' dump.rs:81 (was :75 pre-insertion) — left for TASK-043.02. The two predicted transformations happened as stated: 'LED_ACTIVE_LOW' became 'public documentation for `led` links to private item' at led.rs:5:24, 'get_mut' stayed unresolved but gained a location at led.rs:18:7. All ten other no-location warnings vanished (Decoder, status_body, StatusGate::due, BlockAssembler, blink_task x2, BootLed, init(led), StaticCell, AtomicU32). No warning outside the predicted set appeared.

Gates: cargo fmt --all --check clean; cargo clippy --workspace --all-targets -- -D warnings exit 0; cargo test --workspace 244 passed / 0 failed incl. 1 asperitas-logging doctest. Doc-only gate 'git diff --no-ext-diff -U0 crates/ | grep -E "^[+-][^+-]" | sed -E "s/^.//" | grep -vE "^ *(///|//!)"' printed nothing.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Deleted all six outer /// doc comments on module declarations in asperitas-logging/src/lib.rs, disarming the rust-lang/rust#134904 hazard (outer /// on 'mod x;' plus //! in x.rs makes the module's own header links resolve against the crate root and discards their spans). The three rationales that were unique moved onto their module's own //! header: frame.rs and dump.rs each gained a '# Why this module is not behind `log-usb`' section above their first heading, console.rs's existing section gained the cross-reference to [`crate::frame`]. Moved links are fully qualified ([`crate::frame`]) because they now resolve in module scope. The defmt_log/led/panic_handler stubs were dropped outright - each target's first //! line already says more.

Rustdoc warnings 7 -> 4 with default features and 23 -> 14 under boot-led,log-usb,log-defmt (and --all-features); every surviving warning was predicted by the plan and matched by message text, including the two that gained locations instead of vanishing (led.rs LED_ACTIVE_LOW now 'links to private item', led.rs get_mut still unresolved) and one new dump.rs:81 'redundant explicit link target' that correct spans finally made visible. Deliberately non-zero: TASK-043.02 owns the remaining 14, so TASK-043 AC #1 is still not met. fmt/clippy/test all clean; diff is doc comments only.
<!-- SECTION:FINAL_SUMMARY:END -->
