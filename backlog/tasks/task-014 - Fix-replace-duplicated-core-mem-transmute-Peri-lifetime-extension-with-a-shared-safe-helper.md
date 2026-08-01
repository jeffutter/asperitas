---
id: TASK-014
title: >-
  Fix: replace duplicated core::mem::transmute Peri-lifetime-extension with a
  shared safe helper
status: In Progress
assignee:
  - '@ralph'
created_date: '2026-08-01 21:56'
updated_date: '2026-08-01 22:38'
labels:
  - review-followup
dependencies:
  - TASK-006.01
priority: high
ordinal: 110
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while reviewing TASK-006.01.05 (crates/asperitas-logging/src/usb.rs:106-109). Originally this also duplicated the same pattern in panic_handler.rs::set_panic_led, but TASK-013's refactor (led.rs singleton BootLed + shared panic path) deleted set_panic_led entirely, so usb.rs::init is now the only remaining call site — the cross-module duplication this ticket was filed against is gone, but the underlying Concise/Resilient-axis problem in usb.rs itself still stands: init() reaches for unsafe core::mem::transmute to coerce an owned Peri<'_, T> (T: PeripheralType, PeripheralType: Copy) into Peri<'static, T>, with a comment claiming 'safe because these peripherals live for the lifetime of the device'. This reaches for the riskiest tool available (transmute reinterprets raw bits and merely happens to work here because PhantomData is zero-sized) when embassy-hal-internal's own Peri type already exposes exactly the sanctioned unsafe primitive for this: Peri::new_unchecked(inner: T) -> Peri<'a, T> (embassy-hal-internal-0.5.0/src/peripheral.rs), reachable safely via Peri's Deref<Target = T> plus T: Copy as unsafe { Peri::new_unchecked(*pin) }. That expresses the same 'I am duplicating this singleton and promising not to use both copies' invariant Peri's own docs already describe (see clone_unchecked's doc comment), without relying on transmute's undocumented layout-compatibility assumption. usb.rs currently builds and passes clippy -D warnings, so this is not a build-breaking bug — it is a Resilient axis risk: unsafe code that doesn't need to be as unsafe as it is, used three times in the same function.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 grep -rn transmute crates/asperitas-logging/src returns no results
- [ ] #2 the shared helper's doc comment states the safety invariant once (peripherals are never dropped for the life of the device, so duplicating the singleton behind a promise not to construct two live drivers from it is sound) instead of repeating it at each call site
- [ ] #3 cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo build --release --features seed3 --bin main --bin blinky succeeds
- [ ] #4 cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo clippy --release --features seed3 --bin main --bin blinky -- -D warnings passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): This is a Rust firmware project (firmware/, Embassy on Daisy Seed3) plus a host-side Rust workspace (crates/asperitas-dsp, crates/asperitas-cli, crates/asperitas-logging). ALL commands must run inside the Nix dev shell: either run 'direnv allow' once, or prefix every command with 'nix develop -c'. Work from the repository root unless told otherwise. Do not change pinned dependency versions.

1. Read crates/asperitas-logging/src/usb.rs lines 87-117 (init(), the three transmute calls on usb_otg_fs/dp/dn). Note: panic_handler.rs no longer has any transmute calls — TASK-013 deleted set_panic_led entirely, so this is the only remaining call site (grep -rn transmute crates/asperitas-logging/src confirms only usb.rs).
2. Confirm the sanctioned primitive: read the vendored embassy-hal-internal source (find it via find ~/.cargo/registry/src -iname 'peripheral.rs' -path '*embassy-hal-internal*') and note Peri::new_unchecked(inner: T) -> Self and that Peri<'a,T>: Deref<Target=T> plus T: PeripheralType: Copy, so unsafe { Peri::new_unchecked(*p) } produces an owned Peri<'static, T> (or any lifetime) from a borrowed/owned Peri<'_, T> without transmute.
3. Add a small internal helper in crates/asperitas-logging (e.g. a private fn near the top of usb.rs, or a small internal module if it reads more clearly, gated #[cfg(feature = "log-usb")]): unsafe fn extend_static<T: embassy_stm32::PeripheralType>(p: embassy_stm32::Peri<'_, T>) -> embassy_stm32::Peri<'static, T> { unsafe { embassy_stm32::Peri::new_unchecked(*p) } } with a doc comment stating the safety invariant once (peripherals are never dropped for the life of the device, so duplicating the singleton behind a promise not to construct two live drivers from it is sound).
4. Replace the three transmute calls in usb.rs::init with calls to this helper.
5. Remove the now-redundant 'Coerce Peri lifetimes to 'static' comment at the call site, keeping only a one-line note that references the helper's doc comment.
6. Run: nix develop -c cargo build -p asperitas-logging --features log-usb (host-buildable check first, if it builds standalone; otherwise skip to the firmware build)
7. Run: cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo build --release --features seed3 --bin main --bin blinky
8. Run: cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo clippy --release --features seed3 --bin main --bin blinky -- -D warnings
9. Run: grep -rn transmute crates/asperitas-logging/src — should return no results.
<!-- SECTION:PLAN:END -->
