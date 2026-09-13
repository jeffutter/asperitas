---
id: TASK-060.01
title: Clear the four rustfmt hunks in firmware/src/bin/rig.rs
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-13 00:08'
updated_date: '2026-09-13 00:11'
labels:
  - planned
dependencies: []
parent_task_id: TASK-060
priority: medium
type: chore
ordinal: 93800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`firmware/src/bin/rig.rs` arrived with TASK-038.03.02.03 and has never been formatted. Measured 2026-09-12 in `nix develop .#default`: `cargo fmt --all --check` inside `firmware/` is red at `rig.rs:21, :28, :445, :554` (four hunks, not the three named when TASK-060 was filed), while root `cargo fmt --all --check` exits 0 because `Cargo.toml:3` declares `exclude = ["firmware"]`.

Nothing gated it and nothing will until TASK-060.02 lands. That gate must arrive green - TASK-058's own plan records why: "a gate that arrives red gets disabled rather than fixed". So the drift is cleared first, on its own, in a commit that contains nothing else. Precedent: TASK-044 cleared the same drift in `main.rs`/`podtest.rs` on 2026-09-10 (commit `f2719d1`) and left no gate behind, which is how `rig.rs` drifted straight back in.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 cd firmware && cargo fmt --all --check exits 0, and root cargo fmt --all --check still exits 0.
- [ ] #2 The change is formatting only: git show --stat lists exactly one file (firmware/src/bin/rig.rs), and cd firmware && cargo clippy --release --features seed3 --bins -- -D warnings exits 0 both before and after (measured exit 0 at HEAD today, so any warning appearing after the edit means the edit was not formatting-only - stop rather than fix it here).
- [ ] #3 Notes record the bench side effect honestly: editing rig.rs legitimately makes firmware/target/thumbv7em-none-eabihf/release/main older than its input, so make elf-check fails closed afterwards. That is the check working, not the mtime false positive TASK-056 owns; do not delete or relink the ELF as part of this ticket.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Shape

One command, one file, one commit. No gate wiring - that is .02 and .03. Assignee stays `@agent`; nothing here needs a board, ears, or an instrument.

## Step 1 - format

    cd firmware && cargo fmt -- src/bin/rig.rs

Verified 2026-09-12: this clears all four hunks and touches nothing else (one file, 5 insertions / 4 deletions). Prefer the targeted form over `cargo fmt --all` inside `firmware/` so the commit cannot silently pick up unrelated drift.

## Step 2 - prove it is formatting only

    cargo fmt --all --check                                    # root, expect 0
    cd firmware && cargo fmt --all --check                     # expect 0 (was 1)
    cd firmware && cargo clippy --release --features seed3 --bins -- -D warnings   # expect 0, same as before the edit

Cross clippy over all six bins exits 0 at HEAD today (measured), so any warning appearing after the edit means the edit was not formatting-only. Stop in that case; do not "fix" it in this commit.

## Step 3 - commit

`style(firmware): cargo fmt rig.rs`, body naming TASK-060 as the reason the tree is being cleaned ahead of the gate, with the usual `Task-Id: TASK-060.01` trailer. Do not bundle any gate change here.

## Bench side effect to record in notes

Editing `rig.rs` legitimately makes `firmware/target/thumbv7em-none-eabihf/release/main` older than one of its inputs, so `make elf-check` (`firmware/Makefile:223-236`) fails closed afterwards. That is the check working, not the false positive TASK-056 owns. Do not delete or relink the ELF: it is the decoder for whatever image is on the board right now, and rebuilding it overwrites the only host copy of those symbols.
<!-- SECTION:PLAN:END -->
