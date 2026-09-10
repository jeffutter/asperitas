---
id: TASK-043
title: >-
  Chore: clear the 7 pre-existing rustdoc broken intra-doc link warnings in
  asperitas-logging
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-10 04:59'
labels:
  - chore
  - review-followup
dependencies: []
priority: low
ordinal: 72500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while verifying TASK-040. `nix develop -c cargo doc -p asperitas-logging --no-deps` reports 7 warnings, all pre-existing and unrelated to that ticket (TASK-040 deliberately kept its own new reference as prose rather than adding an 8th): links to private items `Backend`, `emit`, `usb::init`, `PANIC_FRAME`, and unresolved links to `Decoder`, `status_body`, `StatusGate::due`, `BlockAssembler`. Each is either a public doc string pointing at an item that is `mod`-private or feature-gated out of the default feature set, so the rendered docs lose a cross-reference the author intended. Nothing fails today because no CI job runs `cargo doc` with warnings-as-errors, so this quietly degrades further every time a doc link is added.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 cargo doc -p asperitas-logging --no-deps generates zero rustdoc::broken_intra_doc_links warnings under the default features AND under --features boot-led,log-usb,log-defmt (the combination the device build uses).
- [ ] #2 Fixes preserve intent rather than deleting links: prefer making the target public/gated consistently, or documenting the gated case, over stripping the cross-reference. Note any link genuinely meant to stay internal as plain code text instead.
- [ ] #3 nix develop -c cargo fmt --all --check, clippy -p asperitas-logging --all-targets -- -D warnings, and cargo test --workspace still pass.
- [ ] #4 If a doc job is cheap to add to the existing lefthook/CI setup, gate it on RUSTDOCFLAGS="-D warnings" for this crate; otherwise record in the notes why not.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Discovered during TASK-040 verification: adding a link to crate::panic_handler produced 'unresolved link to crate::panic_handler' under default features because that module is behind the boot-led feature. Dropped the link to prose to avoid growing the count; the rest are pre-existing.
<!-- SECTION:NOTES:END -->
