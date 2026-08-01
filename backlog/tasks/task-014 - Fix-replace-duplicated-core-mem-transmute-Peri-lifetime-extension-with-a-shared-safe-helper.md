---
id: TASK-014
title: >-
  Fix: replace duplicated core::mem::transmute Peri-lifetime-extension with a
  shared safe helper
status: Needs Plan
assignee: []
created_date: '2026-08-01 21:56'
updated_date: '2026-08-01 22:24'
labels:
  - review-followup
dependencies:
  - TASK-006.01
priority: high
ordinal: 110
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while reviewing TASK-006.01.05 (crates/asperitas-logging/src/usb.rs:106-109, crates/asperitas-logging/src/panic_handler.rs:52-55). Both files independently reach for unsafe core::mem::transmute to coerce an owned Peri<'_, T> (T: PeripheralType, PeripheralType: Copy) into Peri<'static, T>, each with a near-identical safety comment ('safe because these peripherals live for the lifetime of the device'). This duplicates the same unsafe knowledge in two modules (CLAUDE.md information-hiding: 'the same knowledge in multiple places... will cause pain during changes') and it reaches for the riskiest tool available (transmute reinterprets raw bits and merely happens to work here because PhantomData is zero-sized) when embassy-hal-internal's own Peri type already exposes exactly the sanctioned unsafe primitive for this: Peri::new_unchecked(inner: T) -> Peri<'a, T> (embassy-hal-internal-0.5.0/src/peripheral.rs), reachable safely via Peri's Deref<Target = T> plus T: Copy as unsafe { Peri::new_unchecked(*pin) }. That expresses the same 'I am duplicating this singleton and promising not to use both copies' invariant Peri's own docs already describe (see clone_unchecked's doc comment), without relying on transmute's undocumented layout-compatibility assumption. Both binaries currently build and pass clippy -D warnings, so this is not a build-breaking bug — it is a Resilient/Concise-axis risk: unsafe code that doesn't need to be as unsafe as it is, copy-pasted.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 crates/asperitas-logging has a single shared function (e.g. a small unsafe fn extend_peri_static<T: embassy_stm32::PeripheralType>(p: Peri<'_, T>) -> Peri<'static, T> in a new or existing internal module) that both usb.rs::init and panic_handler.rs::set_panic_led call, implemented via Peri::new_unchecked(*p) — not core::mem::transmute
- [ ] #2 grep -rn transmute crates/asperitas-logging/src returns no results
- [ ] #3 the shared helper's doc comment states the safety invariant once (peripherals are never dropped for the life of the device, so duplicating the singleton behind a promise not to construct two live drivers from it is sound) instead of repeating it at each call site
- [ ] #4 cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo build --release --features seed3 --bin main --bin blinky succeeds
- [ ] #5 cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo clippy --release --features seed3 --bin main --bin blinky -- -D warnings passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): This is a Rust firmware project (firmware/, Embassy on Daisy Seed3) plus a host-side Rust workspace (crates/asperitas-dsp, crates/asperitas-cli, crates/asperitas-logging). ALL commands must run inside the Nix dev shell: either run 'direnv allow' once, or prefix every command with 'nix develop -c'. Work from the repository root unless told otherwise. Do not change pinned dependency versions.

1. Read crates/asperitas-logging/src/usb.rs lines 87-117 (init(), the three transmute calls on usb_otg_fs/dp/dn) and crates/asperitas-logging/src/panic_handler.rs lines 44-55 (set_panic_led, the three transmute calls on red_pin/green_pin/blue_pin).
2. Confirm the sanctioned primitive: read the vendored embassy-hal-internal source (find it via find ~/.cargo/registry/src -iname 'peripheral.rs' -path '*embassy-hal-internal*') and note Peri::new_unchecked(inner: T) -> Self and that Peri<'a,T>: Deref<Target=T> plus T: PeripheralType: Copy, so unsafe { Peri::new_unchecked(*p) } produces an owned Peri<'static, T> (or any lifetime) from a borrowed/owned Peri<'_, T> without transmute.
3. Add a small internal helper in crates/asperitas-logging (e.g. a private module or a few lines near the top of lib.rs, gated #[cfg(feature = "log-usb")] since both call sites are feature-gated): unsafe fn extend_static<T: embassy_stm32::PeripheralType>(p: embassy_stm32::Peri<'_, T>) -> embassy_stm32::Peri<'static, T> { unsafe { embassy_stm32::Peri::new_unchecked(*p) } } with a doc comment stating the safety invariant once.
4. Replace the three transmute calls in usb.rs::init with calls to this helper.
5. Replace the three transmute calls in panic_handler.rs::set_panic_led with calls to this helper.
6. Remove the now-duplicate 'Coerce Peri lifetimes to 'static' comments at each call site, keeping only a one-line note that references the helper's doc comment.
7. Run: nix develop -c cargo build -p asperitas-logging --features log-usb (host-buildable check first, if it builds standalone; otherwise skip to the firmware build)
8. Run: cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo build --release --features seed3 --bin main --bin blinky
9. Run: cd firmware && nix develop /home/jeffutter/src/asperitas -c cargo clippy --release --features seed3 --bin main --bin blinky -- -D warnings
10. Run: grep -rn transmute crates/asperitas-logging/src and confirm it returns nothing.
11. Confirm binary sizes are unchanged (this is a pure refactor, no behavior change) — compare against the ~65.5 KB / ~49.8 KB baseline recorded in TASK-006.01.05.
<!-- SECTION:PLAN:END -->
