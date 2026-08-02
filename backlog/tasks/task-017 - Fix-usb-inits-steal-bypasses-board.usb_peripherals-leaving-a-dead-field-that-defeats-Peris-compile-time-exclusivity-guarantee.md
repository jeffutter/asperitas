---
id: TASK-017
title: >-
  Fix: usb::init()'s steal() bypasses board.usb_peripherals, leaving a dead
  field that defeats Peri's compile-time exclusivity guarantee
status: Done
assignee:
  - '@ralph'
created_date: '2026-08-01 23:25'
updated_date: '2026-08-02 00:34'
labels:
  - review-followup
dependencies:
  - TASK-016
priority: high
type: bug
ordinal: 100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while reviewing TASK-016 (crates/asperitas-logging/src/usb.rs:97-103, firmware/src/bin/main.rs, firmware/src/bin/blinky.rs). TASK-016 replaced the doc(hidden) Peri::new_unchecked-based extend_peri_to_static helper with direct T::steal() calls inside asperitas_logging::usb::init(). That fix is correct on its own terms (all 4 of TASK-016's ACs verified: no new_unchecked, single safety comment, release build and clippy -D warnings both clean). But it changed the call sites from passing board.usb_peripherals.usb_otg_fs / .pins.DP / .pins.DN into init() (which moved those fields out of board, so the type system prevented anyone from touching them again) to calling init(UsbIrqs) with no peripheral arguments at all. board.usb_peripherals (a DaisyBoard<'_> field of type daisy_embassy::usb::UsbPeripherals<'a>, holding live Peri<'a, USB_OTG_FS>/PA12/PA11 handles carved out of the raw Peripherals struct by new_daisy_board!) is now never consumed in either firmware/src/bin/main.rs or firmware/src/bin/blinky.rs. It sits alive and untouched for the rest of each main() function while usb::init() independently steal()s fresh 'static Peri handles to the exact same three peripherals. Two live Peri instances for the same physical peripheral now coexist for the program's whole lifetime. Nothing is broken today because nothing else reads board.usb_peripherals, but the safety comment in usb.rs:97-100 ('init() holds the only Peri constructed for each of these peripherals') is no longer accurate — a second one is constructed and merely left unused — and the compile-time exclusivity Peri exists to provide (you cannot construct a second driver for a peripheral someone already owns) is silently defeated: nothing stops a future edit from reading board.usb_peripherals.usb_otg_fs and building a second, conflicting USB driver, because the compiler no longer sees any conflict. This is a Resilient-axis regression relative to TASK-014's original (unsound but at least ownership-consuming) approach.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 board.usb_peripherals is explicitly consumed/discarded at both call sites in firmware/src/bin/main.rs and firmware/src/bin/blinky.rs (e.g. via an explicit let _ = board.usb_peripherals; or destructuring) immediately after new_daisy_board!, with a comment explaining that asperitas_logging::usb::init() steals these same peripherals directly and board.usb_peripherals must not be used
- [ ] #2 The safety comment in crates/asperitas-logging/src/usb.rs above the three steal() calls is corrected so it no longer claims init() holds 'the only Peri constructed' for these peripherals; it should instead explain that board.usb_peripherals is deliberately left unconsumed by the board API and must never be read by caller code, which is why steal() is safe here
- [ ] #3 grep -rn 'usb_peripherals' firmware/src shows the field is explicitly discarded, not silently ignored, at both call sites
- [ ] #4 cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo build --release --features seed3 --bin main --bin blinky succeeds
- [ ] #5 cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo clippy --release --features seed3 --bin main --bin blinky -- -D warnings passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): This is a Rust embedded firmware workspace (crates/ + firmware/) targeting the Electro-Smith Daisy Seed3 via embassy-stm32. ALL commands must run inside the Nix dev shell: either run 'direnv allow' once, or prefix every command with 'nix develop /home/jeffutter/src/asperitas -c'. Work from the repository root unless told otherwise. Do not change pinned dependency versions (daisy-embassy is pinned to a git branch/commit in firmware/Cargo.toml — do not bump it).

1. Read crates/asperitas-logging/src/usb.rs:86-104 (the init() function) and its safety comment above the three steal() calls (currently around lines 97-100).

2. In firmware/src/bin/main.rs, immediately after 'let board: DaisyBoard<'_'> = new_daisy_board!(p);' (around line 61) and before the call to asperitas_logging::usb::init(UsbIrqs) (around line 71), add a line that explicitly discards board.usb_peripherals, e.g.:
    let _ = board.usb_peripherals; // usb::init() steals these peripherals directly via T::steal(); this field must never be read.
   Do the same in firmware/src/bin/blinky.rs at the equivalent spot (around line 61, before the init(UsbIrqs) call around line 71).

3. Update the safety comment in crates/asperitas-logging/src/usb.rs above the three steal() calls (around lines 97-100). Replace the claim that init() 'holds the only Peri constructed for each of these peripherals' — that is no longer true by construction, since board.usb_peripherals also holds one. Instead state plainly that daisy_embassy::DaisyBoard hands out board.usb_peripherals for these same three peripherals but callers of this module MUST discard that field unread (as done at both call sites in step 2), which is the actual invariant steal()'s safety here depends on.

4. Verify no other code (including any future asperitas-logging call sites) reads board.usb_peripherals: grep -rn 'usb_peripherals' firmware/src — should show only the explicit discard added in step 2 at each of the two call sites.

5. Run:
   cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo build --release --features seed3 --bin main --bin blinky
   cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo clippy --release --features seed3 --bin main --bin blinky -- -D warnings
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation: Added explicit discard of board.usb_peripherals at both call sites (main.rs line 66, blinky.rs line 66) with explanatory comments. Updated safety comment in usb.rs to accurately describe the invariant that callers must discard the board's copy. Verified via grep that no other code reads usb_peripherals. Release build and clippy -D warnings both clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Discarded board.usb_peripherals at both binary call sites and corrected the safety comment in usb::init() so Peri exclusivity is maintained by documented caller discipline rather than an inaccurate claim of sole ownership.
<!-- SECTION:FINAL_SUMMARY:END -->
