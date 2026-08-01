---
id: TASK-011
title: 'Fix: document daisy-embassy PR #80''s hardcoded 32-sample block length'
status: In Progress
assignee:
  - '@ralph'
created_date: '2026-08-01 17:41'
updated_date: '2026-08-01 18:08'
labels:
  - review-followup
dependencies:
  - TASK-005.01
documentation:
  - docs/reference/rust-daisy-stack.md
priority: high
type: docs
ordinal: 100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while reviewing TASK-005.01 (backlog/tasks/task-005.01 - Implement-audio-passthrough-against-the-seed3-feature.md, Description and AC #1). TASK-005 and TASK-005.01 both originally targeted "48 kHz with a 48-sample block, matching libDaisy's Pod default" — but the pinned daisy-embassy fork (PR #80, commit 477083b0227d) hardcodes `pub const BLOCK_LENGTH: usize = 32;` in `src/audio.rs`. It is a `const`, not a field on `AudioConfig` (which only exposes `fs: Fs`), so 32-sample blocks are not a choice we can override from this project without patching the vendored fork. TASK-005.01's own record has already been corrected post-review to state the actual delivered behavior (32-sample blocks), but this fact is not yet recorded in docs/reference/rust-daisy-stack.md, which is this project's living reference for daisy-embassy/PR #80 status (Correct axis: a load-bearing ecosystem fact is undocumented, so the next person touching audio timing will have to rediscover it by reading vendored source).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 done:1;done:2;done:3
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): This is a Rust firmware project (firmware/, Embassy on Daisy Seed3) plus host-side Rust crates (crates/asperitas-dsp, crates/asperitas-cli). ALL commands must run inside the Nix dev shell: either run 'direnv allow' once, or prefix every command with 'nix develop -c'. Work from the repository root unless told otherwise. Do not change pinned dependency versions.

1. Confirm the fact before writing anything: open the vendored daisy-embassy checkout for the pinned commit (find it via `find ~/.cargo/git/checkouts -maxdepth 2 -iname '*daisy-embassy*'`, then the subdirectory matching the pinned rev prefix from firmware/Cargo.toml's `daisy-embassy = { git = ..., rev = "477083b0227d" }`) and read src/audio.rs. Confirm `pub const BLOCK_LENGTH: usize = 32;` and that `AudioConfig` (same file) only has an `fs: Fs` field, no block-length field.

2. Edit docs/reference/rust-daisy-stack.md. In the "## Seed3 support status — PR #80" section (after the existing "Consequence for us" paragraph, before "## Alternative / superseded BSPs"), add a new paragraph, e.g.:

   "**Block size is fixed at 32 samples, not 48.** `src/audio.rs` hardcodes `pub const BLOCK_LENGTH: usize = 32;`; it is not a field on `AudioConfig` (which only exposes `fs: Fs`), so this project cannot request a different block size without patching the vendored fork. This diverges from libDaisy's 48-sample default for the Pod — worth mentioning in the TASK-005.03 upstream report if block size ever becomes musically significant (e.g. for tight feedback/delay paths)."

3. Run: `nix develop -c bash -c "grep -n 'BLOCK_LENGTH' docs/reference/rust-daisy-stack.md"` — must return the new line.

4. No code changes, no cargo build/test needed — this is a docs-only fix. Do not touch firmware/ or crates/.
<!-- SECTION:PLAN:END -->
