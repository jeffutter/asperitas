---
id: TASK-036.05
title: Compile the log-defmt backend in an unattended gate
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-10 21:05'
updated_date: '2026-09-10 21:32'
labels:
  - planned
dependencies: []
modified_files:
  - .github/workflows/ci.yml
  - lefthook.yml
parent_task_id: TASK-036
priority: high
type: task
ordinal: 71500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-036.03 put the whole RTT backend behind cfg gates that **no unattended gate ever compiles**. CI (.github/workflows/ci.yml) runs clippy on asperitas-logging with `--features log-usb` only, and cross-compiles the firmware with `--features seed3`; lefthook.yml's pre-commit/pre-push mirror the same two. So `crates/asperitas-logging/src/defmt_log.rs`, the five `#[cfg(not(feature = "log-defmt"))]` no-op logger stubs in firmware/src/bin/*.rs, and `firmware/build.rs:27-34`'s `-Tdefmt.x` link line are all outside every check the autonomous loop runs — the next agent to touch a shared path (format_body, Backend, panic_handler) can break defmt decoding and see eight green gates. Verified by reading both gate definitions against the feature table; nothing here needs the probe.

Fix is additive and cheap: one host-side lib clippy invocation with `--features log-defmt` and one firmware build with `--no-default-features --features "seed3 log-defmt"`. Both forms were run on this machine before the ticket was written and pass today (rc=0), so the change is adding lines, not making them pass.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 #1 .github/workflows/ci.yml gains `cargo clippy -p asperitas-logging --features log-defmt --lib -- -D warnings` alongside the existing log-usb variant, and it passes locally inside nix develop .#default.
- [x] #2 #2 .github/workflows/ci.yml gains a firmware cross-compile of the RTT-only image — `cd firmware && cargo build --release --no-default-features --features "seed3 log-defmt"` — passing alongside the existing `--features seed3` build, so build.rs's -Tdefmt.x branch is compiled by CI.
- [x] #3 #3 lefthook.yml's pre-commit and pre-push gain the log-defmt lib clippy (cheap, host target). The second firmware cross-compile stays out of the hooks; hook runtime is the reason, and CI still covers it.
- [x] #4 #4 No existing gate command changes: fmt, the workspace clippy pair, the log-usb clippy, cargo test --workspace, dump_reassemble --selftest and the console-form firmware build all appear verbatim as before.
- [x] #5 #5 Every new command is run locally first and its exit code recorded in this ticket's notes — a gate that is added red is worse than no gate.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Scope

Two gate lines each, additive. Nothing here changes what any build produces — it makes an
already-working configuration fail loudly when it stops working.

## Step 1 — host-side lib clippy (cheap, goes in all three gates)

`crates/asperitas-logging` compiles for the host target even with `log-defmt` on, exactly as it
already does with `log-usb`. Measured on this machine before the ticket was filed:

    cargo clippy -p asperitas-logging --features log-defmt --lib -- -D warnings   # rc=0

Add it directly beside the existing `clippy-log-usb` command in `.github/workflows/ci.yml` (the
block at ci.yml:23-32 that already carries the log-usb variant with the same rationale comment) and
in **both** `lefthook.yml` hook blocks (`pre-commit` and `pre-push`), each named
`clippy-log-defmt` with `forward_stderr: true` like its sibling. Mirror the existing comment style:
say *why* (default features leave the defmt bridge unlinted) rather than restating the command.

## Step 2 — firmware cross-compile of the RTT-only image (CI only)

This is the only thing that compiles `firmware/build.rs`'s `-Tdefmt.x` branch and the five
`#[cfg(not(feature = "log-defmt"))]` logger stubs together. Measured before filing, via the Makefile
form of the same command:

    cd firmware && cargo build --release --no-default-features --features "seed3 log-defmt"   # rc=0

Put it immediately after the existing `cd firmware && cargo build --release --features seed3` line
at ci.yml:49. Keep `set -euo pipefail` semantics — do not add `|| true`, and do not wrap it in the
Makefile, because CI deliberately invokes cargo directly there.

`DEFMT_LOG` stays unset in CI and that is correct: the env filter affects which frames are compiled
*in*, not whether the crate compiles, so a plain build still exercises `defmt_log.rs` and the link
fragment. Do not add `DEFMT_LOG=info` just to make the log line look nicer — it changes nothing
about coverage and adds a knob.

Leave the hooks alone for this step. A second thumbv7em cross-compile on every `git push` is the
cost that gets the hook disabled later; CI covers it.

## Step 3 — prove the gates are honest, in this order

1. `cargo fmt --all --check` (nothing to format, but lefthook runs it first).
2. Each new command individually, inside `nix develop .#default`, recording exit codes in Notes.
3. The full pre-push hook once: `lefthook run pre-push`. That is the real proof the added lines
   don't slow or break a commit.
4. Deliberately break one covered path in a scratch copy — e.g. introduce a warning in
   `defmt_log.rs` — and confirm the new clippy line is the one that catches it, then revert. A gate
   nobody has seen go red is unverified.

## Guard: no existing command may change

AC #4 exists because ci.yml's console-form firmware build is also what TASK-038's rig builds lean
on. Prove it with a diff of the workflow file: every pre-existing `run:` line must be untouched, and
`git diff` should show additions plus comments only.

## Not in scope

Wiring firmware *clippy* into CI generally (the umbrella explicitly excluded that, and it is a
separate cost decision about CI minutes); caching the thumbv7em target dir in Actions; DEFMT_LOG
policy; anything needing a probe (TASK-037).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Gate commands run locally inside `nix develop .#default`, exit codes as measured:

- `cargo fmt --all --check` — rc=0
- `cargo clippy -p asperitas-logging --features log-defmt --lib -- -D warnings` — rc=0
- `cd firmware && cargo build --release --no-default-features --features "seed3 log-defmt"` — rc=0
- `lefthook run pre-push` (with the new line in place) — rc=0, 68.88s wall; `clippy-log-defmt` itself took 0.24s of it, so the hook budget barely notices.
- `lefthook run pre-commit` — rc=0 (and exercised for real by this commit, which had staged files).

The RTT-only build really does exercise the link branch: `rust-readobj --section-headers` on
`firmware/target/thumbv7em-none-eabihf/release/main` shows one consolidated `.defmt`
(SHT_PROGBITS), i.e. build.rs emitted `-Tdefmt.x` and the fragment merged the loose
`.defmt.<level>.*` inputs. Before this gate that property was invisible to CI.

Negative control, per plan step 3.4: added a temporary `#[cfg(feature = "log-defmt")]`
`clippy::len_zero` defect to `defmt_log.rs` (`#[allow(dead_code)]`, so nothing rustc-visible) and
ran the three clippy gates — workspace rc=0, `--features log-usb` rc=0, **`--features log-defmt`
rc=101** naming `defmt_log.rs:149`. Only the new gate sees it, which is the whole point of the
ticket. Reverted (`git checkout`) and re-ran: rc=0.

AC #4 guard: `git diff --numstat` gives 13 added / 0 deleted for ci.yml and 9 / 0 for
lefthook.yml — additions and comments only, every pre-existing `run:` line byte-identical. Both
files still parse (`yq`), and the workflow's inline bash script passes `bash -n` after the edit.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Two additive gate lines make the defmt-over-RTT backend fail loudly instead of silently.

`ci.yml` gains `cargo clippy -p asperitas-logging --features log-defmt --lib -- -D warnings` beside
the existing log-usb variant, plus a firmware cross-compile of the RTT-only image
(`--no-default-features --features "seed3 log-defmt"`) after the console-form build — the only
thing that compiles `defmt_log.rs`, the five `#[cfg(not(feature = "log-defmt"))]` logger stubs and
build.rs's `-Tdefmt.x` link line together. `lefthook.yml` gains the lib clippy in both
`pre-commit` and `pre-push`; the second thumbv7em build stays out of the hooks on runtime grounds
(0.24s of a 68.88s push hook for the one that did go in).

Verified by measurement, not by reading: every new command rc=0 inside `nix develop .#default`, the
whole pre-push hook green with the new line in place, and a temporary `clippy::len_zero` defect in
`defmt_log.rs` caught by the new gate alone (workspace and log-usb clippy both rc=0) then reverted.
The RTT ELF carries one consolidated `.defmt` section, confirming the link fragment is really
exercised. Diff is additions only — 13/0 and 9/0 numstat — so no pre-existing gate command moved.
<!-- SECTION:FINAL_SUMMARY:END -->
