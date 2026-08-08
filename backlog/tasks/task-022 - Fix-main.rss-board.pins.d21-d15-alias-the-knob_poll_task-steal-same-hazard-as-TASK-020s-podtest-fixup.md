---
id: TASK-022
title: >-
  Fix: main.rs's board.pins.d21/d15 alias the knob_poll_task steal(), same
  hazard as TASK-020's podtest fixup
status: To Do
assignee: []
created_date: '2026-08-08 05:25'
labels:
  - review-followup
dependencies:
  - TASK-019.01
priority: high
ordinal: 100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found incidentally while reviewing TASK-020 (firmware/src/bin/podtest.rs). main.rs's knob_poll_task (firmware/src/bin/main.rs:118-130) steals ADC1/PC4/PC0 directly via unsafe { steal() }, justified only by the comment at main.rs:120-121 ('daisy-embassy does not consume these, so steal() is safe on single-core Cortex-M'). That comment is incomplete: PC4 and PC0 are also named fields on the DaisyPins struct returned by new_daisy_board!() — board.pins.d21 == PC4 and board.pins.d15 == PC0 (confirmed against daisy-embassy's crates/.../pins/pins_seed.rs: SeedPin21 = PC4, SeedPin15 = PC0). main()'s board binding (firmware/src/bin/main.rs:139) is never destructured to discard d21/d15 — it stays alive for the whole function (board.audio_peripherals is used later at line ~163), so board.pins.d21 and board.pins.d15 remain live, unread Peri handles to the exact same physical pins that knob_poll_task's steal() also claims. Nothing reads them today, so there is no active double-driver bug, but the codebase has an explicit, established convention for exactly this hazard: board.usb_peripherals is discarded with a comment stating 'This field must never be read; using it would create a second Peri handle... defeating Peri's exclusivity guarantee' (main.rs:141-143), and TASK-017 exists specifically because an earlier version of usb::init() got this wrong. main.rs's knob pins never received the same treatment. Resilience/Clarity axis: an incomplete safety comment on an unsafe block is worse than a correct one — a future contributor reading 'daisy-embassy does not consume these' could reasonably believe board.pins.d21 is inert and touch it, creating two live Output/Adc drivers on one physical pin. TASK-020's podtest.rs had the identical issue and was fixed in commit 2e63cf7 (fixup! a1af49a) by explicitly discarding the aliased board.pins fields with a corrected comment; mirror that exact pattern here.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 firmware/src/bin/main.rs explicitly discards board.pins.d21 and board.pins.d15 (unread) before or immediately after knob_poll_task's steal() calls execute, e.g. via a let _ = (board.pins.d21, board.pins.d15); statement in main()
- [ ] #2 the comment on knob_poll_task's steal() block (main.rs:120-121) is corrected to state the real invariant: these pins are also exposed via board.pins under Seed d-numbers, and the discard in main() is what keeps the steal() sound — not that daisy-embassy 'does not consume' them
- [ ] #3 nix develop -c cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin main --release succeeds
- [ ] #4 nix develop -c cargo clippy --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin main -- -D warnings passes
- [ ] #5 nix develop -c cargo fmt --manifest-path firmware/Cargo.toml --check passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): This is a Rust embedded firmware project (firmware/, crates/) for the Daisy Seed3 in a Daisy Pod, built with embassy-stm32/embassy-executor async on no_std. ALL commands must run inside the Nix dev shell: either run 'direnv allow' once, or prefix every command with 'nix develop -c'. Work from the repository root unless told otherwise. Do not change pinned dependency versions.

1. Read firmware/src/bin/main.rs lines 133-150 (the main() function's board setup) and lines 118-130 (knob_poll_task). Confirm board.pins.d21 (PC4) and board.pins.d15 (PC0) are never read anywhere else in the file (grep -n 'd21\|d15' firmware/src/bin/main.rs should only show the definitions/mapping, nothing that reads them).
2. Look at firmware/src/bin/podtest.rs's steal block (search for 'The Pod's control-surface pins are wired' — added in commit 2e63cf7) as the reference pattern: it discards board.pins fields aliased by its own steal() calls, with a comment explaining that board.pins also names these physical pins under Seed d-numbers, and that the discard is what makes steal() sound.
3. In main.rs, immediately after board: DaisyBoard<'_> = new_daisy_board!(p) (and before or after the existing board.usb_peripherals discard at line 145), add: let _ = (board.pins.d21, board.pins.d15); with inline comments identifying them as knob1/PC4 and knob2/PC0.
4. Replace the comment at main.rs:120-121 (above knob_poll_task's steal() calls) with corrected wording mirroring podtest.rs's fix: explain that board.pins.d21/d15 are the same physical pins, and that the caller (main()) is responsible for discarding them unread — reference this file's own discard added in step 3, not a false claim that daisy-embassy never exposes them.
5. Run: nix develop -c cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin main --release
6. Run: nix develop -c cargo clippy --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin main -- -D warnings
7. Run: nix develop -c cargo fmt --manifest-path firmware/Cargo.toml --check
8. Record in the task's implementation notes that this mirrors the TASK-020 fixup (commit 2e63cf7) for the identical hazard in podtest.rs.
<!-- SECTION:PLAN:END -->
