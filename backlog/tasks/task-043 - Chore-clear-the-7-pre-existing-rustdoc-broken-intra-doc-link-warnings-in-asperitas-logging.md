---
id: TASK-043
title: >-
  Chore: clear the 7 pre-existing rustdoc broken intra-doc link warnings in
  asperitas-logging
status: In Progress
assignee:
  - '@agent'
created_date: '2026-09-10 04:59'
updated_date: '2026-09-11 01:59'
labels:
  - chore
  - review-followup
  - planned
dependencies:
  - TASK-043.01
  - TASK-043.02
priority: low
ordinal: 72500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while verifying TASK-040. `nix develop -c cargo doc -p asperitas-logging --no-deps` reports 7 warnings, all pre-existing and unrelated to that ticket (TASK-040 deliberately kept its own new reference as prose rather than adding an 8th): links to private items `Backend`, `emit`, `usb::init`, `PANIC_FRAME`, and unresolved links to `Decoder`, `status_body`, `StatusGate::due`, `BlockAssembler`. Each is either a public doc string pointing at an item that is `mod`-private or feature-gated out of the default feature set, so the rendered docs lose a cross-reference the author intended. Nothing fails today because no CI job runs `cargo doc` with warnings-as-errors, so this quietly degrades further every time a doc link is added.

Corrected scope, measured during planning (rustc/cargo 1.97.1). Seven is the default-features count; the device configuration AC #1 also demands produces **24**, because `usb.rs` and `led.rs` are only documented when `boot-led,log-usb,log-defmt` are on. So AC #1 is roughly sixteen decisions, not seven, and they fall into two groups that want different treatment — see the plan. Four of the seven report no source location at all, which is not sloppiness but rust-lang/rust#134904: an outer `///` comment on a `pub mod foo;` declaration makes that module's own `//!` links resolve against the crate root and lose their spans. Fixing that structurally (TASK-043.01) drops the counts to 4 and 15 and, because correct spans let rustdoc actually see what those headers contain, *reveals* a warning nobody had ever been shown (`redundant explicit link target`, dump.rs:75).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 cargo doc -p asperitas-logging --no-deps generates zero rustdoc::broken_intra_doc_links warnings under the default features AND under --features boot-led,log-usb,log-defmt (the combination the device build uses).
- [x] #2 Fixes preserve intent rather than deleting links: prefer making the target public/gated consistently, or documenting the gated case, over stripping the cross-reference. Note any link genuinely meant to stay internal as plain code text instead.
- [x] #3 nix develop -c cargo fmt --all --check, clippy -p asperitas-logging --all-targets -- -D warnings, and cargo test --workspace still pass.
- [x] #4 If a doc job is cheap to add to the existing lefthook/CI setup, gate it on RUSTDOCFLAGS="-D warnings" for this crate; otherwise record in the notes why not.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Two planned sub-tickets do the documentation work; this ticket then owns the gate (AC #4) and final verification. Read TASK-043.01 and TASK-043.02 first — everything below assumes their findings, all of which were measured on this tree rather than reasoned about.

Why the split falls here
- TASK-043.01 is mechanical and judgement-free: move six module summaries onto their own `//!` headers, restoring correct intra-doc link scope. Reviewable on its own, and it changes nothing anyone can call from code.
- TASK-043.02 is where the taste lives: fifteen remaining sites, each with a keep-the-link or demote-to-code-text decision, plus the `[lints.rustdoc]` deny stanza. It depends on 043.01 because until spans are correct, half these warnings are misdiagnosed and fixing them by fully qualifying paths would silence real lints instead of resolving them.
- The gate stays here rather than becoming a third sub-ticket: it is ~12 lines across two config files, it cannot land before both children (it would fail CI red on arrival), and it is the only thing that makes AC #1 durable.

Integration work this ticket does itself, after both children are Done

1. lefthook.yml — add two commands under `pre-push`, alongside fmt/clippy/test/firmware-cross-compile:
   doc-links:            RUSTDOCFLAGS="-D warnings" cargo doc -p asperitas-logging --no-deps
   doc-links-device:     RUSTDOCFLAGS="-D warnings" cargo doc -p asperitas-logging --no-deps --features boot-led,log-usb,log-defmt
   Both carry `forward_stderr: true` like their neighbours. Verify without waiting through a full push: `lefthook run pre-push --command doc-links` filters to one command (lefthook 2.1.10).
2. .github/workflows/ci.yml — inside the existing `nix develop ... bash -c '...'` block, after the two clippy steps and before `cargo test`, add the same two commands with matching `echo "=== ... ==="` banners. Keep them inside the single shell step so the nix environment is still built once; watch the quoting, since the whole script is single-quoted.
3. Keep RUSTDOCFLAGS even though TASK-043.02 adds the declarative deny. One layer makes `cargo doc` fail for a developer who just types it; the other makes the CI step fail loudly for *any* rustdoc warning, including ones the Cargo.toml stanza does not name. Losing either layer is survivable; having neither is how the original 24 accumulated.
4. AC #3 re-run at the end, on the merged result of both children: fmt, `clippy -p asperitas-logging --all-targets -- -D warnings`, `cargo test --workspace`, plus `cd firmware && cargo build --release --features seed3` if either child touched anything beyond doc comments (neither should).
5. AC #1 final check must be read off all three invocations, not the two AC #1 names: default features, the device feature set, and `--all-features`. Note AC #1 as written names only `rustdoc::broken_intra_doc_links`; both children hold the stricter line of zero warnings of *any* kind, which is what actually matters — the majority of the twenty-four are `private_intra_doc_links`.

Cost, measured, for AC #4's "if cheap" clause
From a completely empty target directory, `RUSTDOCFLAGS="-D warnings" cargo doc -p asperitas-logging --no-deps --features boot-led,log-usb,log-defmt` took **13 s** on a 10-core macOS box, including compiling every dependency for the host target — embassy-stm32 included, which nothing else in CI builds for host, because the root workspace never enables `log-usb` and the firmware cross-compile uses a separate target dir and arch. Warm re-runs are under a second. GitHub's ubuntu runners have fewer cores, so treat 13 s as a lower bound and expect a couple of minutes cold at worst; the default-features command costs seconds either way. That is cheap against the firmware release cross-compile pre-push already runs, so AC #4 resolves to "add it", not "record why not".

Out of scope, filed separately so this ticket stays closable
- TASK-048: four pre-existing warnings in asperitas-pod and asperitas-dsp (three of them the same feature-gating shape as ours). Deliberately not folded into AC #1 — this ticket is about asperitas-logging, and gating on crates it never touched would make it uncloseable for unrelated reasons.
- TASK-049: widen the gate from `-p asperitas-logging` to `--workspace` once TASK-048 lands, and give every crate its own deny stanza. Unplanned on purpose: it needs fresh cost measurements.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
--------------------------------------------------
Discovered during TASK-040 verification: adding a link to crate::panic_handler produced 'unresolved link to crate::panic_handler' under default features because that module is behind the boot-led feature. Dropped the link to prose to avoid growing the count; the rest are pre-existing.

Planning pass corrections worth keeping (the description above carries the counts):

Four of the original seven warnings carry no file:line at all. That is rust-lang/rust#134904 ("Module items not in scope for module //! comment when module also has /// comment", related to #119965 and #78591), reproduced minimally outside this repo: an outer `///` on `pub mod two;` plus `//!` in two.rs makes a link from two.rs's header to a `pub` item of its own module report "no item named X in scope", with the span discarded. Removing the outer `///` fixes it with no other change. Fully qualifying the affected links also silences those four messages but leaves every other link in the same header resolving in the wrong scope — measured consequence: `dump.rs:75`'s redundant-explicit-target warning was invisible until the structural fix gave rustdoc real spans. Do not take that shortcut.

Two links cannot be fixed as links by any amount of work, because the target lives behind the opposite `#[cfg]`: `[`usb::init`]` sits in an `init()` compiled only when `log-usb` is *off*, and `[`init`]` sits in `set_backend_defmt`, whose configurations have `log-usb` on. Plain code text satisfies AC #1 and AC #2 together; per-feature `#[cfg_attr(feature = ..., doc = "...")]` fragments would too, at the cost of duplicating a sentence to preserve one hyperlink.

`--document-private-items` is not an answer to the private-target half: it is incompatible with warnings-as-errors gating (rust-lang RFC 1946 leaves that combination undefined), and it would publish internals of a no_std crate to make a number go down.

Also verified while planning, so nobody re-derives it: `[lints.rustdoc]` in a member manifest is honoured by cargo 1.97.1 and turns an unresolved link into a hard error with no RUSTDOCFLAGS, while leaving check/clippy/build/test untouched; and `cd firmware && cargo metadata` still parses the new table.

The owner-facing judgement call (`make emit pub`, or keep it private and reference it in prose five times) is recorded on TASK-043.02's notes with the reasoning and the exact reversal path. This ticket stays @agent: nothing here needs a board, ears, or an instrument, and the one API question has a conservative default that a reviewer can reverse in one commit.

Repo-wide gotcha found while writing the sub-tickets' guards: this repo sets `diff.external` to difftastic, so `git diff | grep -E '^[+-][^+-]'` prints nothing for *any* diff, including one that changes code. Any "doc comments only" acceptance criterion must use `git diff --no-ext-diff`. Verified both ways on this tree. TASK-030.05's AC #2 uses the ext-diff form, so that check was vacuous when it was signed off — worth knowing before trusting similar green guards elsewhere.

Parked Blocked by the executor: both children are still open, so AC #1 and #2 cannot be met here and AC #4's gate would land red.

Evidence from the tree at 50669dd, not from ticket statuses. Measured just now with rustc/cargo 1.97.1 via nix develop:
- cargo doc -p asperitas-logging --no-deps -> 7 warnings (Backend, emit, usb::init, Decoder, status_body, StatusGate::due, BlockAssembler)
- same with --features boot-led,log-usb,log-defmt -> 24 warnings
Both match the planning counts exactly, which means neither TASK-043.01 nor TASK-043.02 has touched crates/ yet. Dependencies were already recorded on this ticket (043.01, 043.02), so it resumes on its own once they close.

Next actionable step is TASK-043.01, which is in To Do and shows under backlog task list --ready. It is mechanical (delete six outer /// comments in lib.rs, move their rationale prose onto the matching //! headers) and deliberately leaves 4 and 15 warnings behind; TASK-043.02 finishes the count to zero, then this ticket adds the lefthook/CI RUSTDOCFLAGS gate and re-runs the AC #3 gates on the merged result.

Finalization (TASK-043 itself, tree at fec9b9e + these two config files). Both children had already
merged — 14b32cd (043.01) and 75f622d (043.02) — so `crates/` needed nothing further here.

AC #1/#2 measured on this tree with rustc/cargo 1.97.1 via nix develop, all three invocations clean
of warnings of ANY kind (not just broken_intra_doc_links): default features,
`--features boot-led,log-usb,log-defmt`, and `--all-features`. The `[lints.rustdoc]` deny stanza from
043.02 is present in crates/asperitas-logging/Cargo.toml:54 and makes an unresolved link a hard error
with no RUSTDOCFLAGS at all.

AC #4 done as "add it". lefthook.yml pre-push gains `doc-links` and `doc-links-device`; ci.yml gains
the same two inside the existing single `nix develop ... bash -c` step, after the clippy steps and
before cargo test, so the nix env is still built once. Verified per-command without a full push:
`lefthook run pre-push --command doc-links` and `... --command doc-links-device` both pass warm in
under a second. Cold cost is the dependency compile for host target (embassy-stm32 etc.), which CI's
firmware cross-compile does not share a target dir with — minutes at worst on ubuntu runners, against
a pre-push that already runs a release cross-compile. `nix flake check` passes, so the ci.yml quoting
still evaluates.

Both layers kept deliberately: the Cargo.toml stanza fails a developer who just types `cargo doc`;
RUSTDOCFLAGS fails CI on any rustdoc warning the stanza does not name. Having neither is how 24
accumulated.

AC #3 re-run on the merged result: fmt --all --check clean, `clippy -p asperitas-logging --all-targets
-- -D warnings` clean, `cargo test --workspace` all green. No firmware rebuild — this ticket touches
only lefthook.yml and .github/workflows/ci.yml (`git diff --no-ext-diff --stat`: 21 added lines, 2
config files; note the ext-diff form, plain `git diff` is swallowed by difftastic).
<!-- SECTION:NOTES:END -->
