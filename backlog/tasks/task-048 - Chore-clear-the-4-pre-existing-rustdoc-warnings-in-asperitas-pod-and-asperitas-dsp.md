---
id: TASK-048
title: >-
  Chore: clear the 4 pre-existing rustdoc warnings in asperitas-pod and
  asperitas-dsp
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-10 09:22'
updated_date: '2026-09-10 09:26'
labels:
  - chore
  - planned
dependencies: []
priority: low
ordinal: 77500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while scoping TASK-043 and deliberately left OUT of that ticket's acceptance criteria, so its gate would not be diluted by crates it never touched. `cargo doc --workspace --no-deps` on today's tree reports exactly four warnings outside asperitas-logging:

- crates/asperitas-pod/src/knob.rs:24 and :40 — unresolved link to `Knobs::new`. `Knobs` lives in the `hw` module behind the `pod-hw` feature, so at default features the target does not exist at all.
- crates/asperitas-pod/src/encoder.rs:199 — unresolved link to `ControlSurface::poll`, same cause (`pub use hw::ControlSurface` only exists with `pod-hw`).
- crates/asperitas-dsp/src/stimulus.rs:411 — public docs for `ExponentialSweep` link to `FADE_SAMPLES`, which is a private const (stimulus.rs:67).

Three of the four are the same shape as the two links TASK-043 could not fix at all: the referring documentation compiles in a feature configuration where the target does not. None of these bite today because no CI job runs cargo doc; they are worth clearing only so the workspace-wide gate in the follow-up chore can be turned on.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `cargo doc -p asperitas-pod -p asperitas-dsp --no-deps` prints zero warning lines at default features AND with `--features asperitas-pod/pod-hw` (the second set matters because CI already clippy-tests that combination, so a fix that only works at default features just moves the failure).
- [ ] #2 Intent is preserved per the rules TASK-043.02 ended up using: where the target genuinely exists in the configuration the prose describes, keep a real link (qualify it, or split the sentence with `#[cfg_attr(feature = ..., doc = "...")]` if that is what correctness needs); where the reference is to something internal, keep the identifier in backticks and say where it lives. Do not delete a cross-reference to make a number go to zero.
- [ ] #3 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings` and `cargo test --workspace` still pass, and `git diff --no-ext-diff -U0 crates/ | grep -E '^[+-][^+-]' | sed -E 's/^.//' | grep -vE '^ *(///|//!)'` prints nothing — doc comments only. `--no-ext-diff` is NOT decoration: this repo sets `diff.external` to difftastic, whose side-by-side output makes the plain form print nothing for *any* change, code included (measured both ways on this tree).
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Four sites, one decision each. Read each site plus the item it points at before choosing, and prefer the smallest change that keeps the reader pointed at the right place.

1. knob.rs:24 and knob.rs:40 both point at `Knobs::new` from prose about the ADC resolution/divisor contract. Under `pod-hw` the method exists and the link resolves; at default features it cannot. The honest options are plain code text (`Knobs::new`) at both sites, or leaving the link and gating the sentence with `#[cfg_attr(feature = "pod-hw", doc = "...")]`. Two occurrences of the same sentence fragment argue for code text — a per-feature doc split for a parenthetical "(see ...)" costs more than the hyperlink is worth. Note that POT_FULL_SCALE_COUNTS itself is ungated, which is why the warning fires at all: an ungated constant's docs are reaching into gated hardware code. Consider whether the resolution note belongs on `Knobs::new` (gated) or on the constant (ungated) at all, and say so in the commit message if you move it.
2. encoder.rs:199 points at `ControlSurface::poll` from `drain_detents`, which is ungated. Same shape as #1; code text unless the sentence reads badly with it.
3. stimulus.rs:411 links `FADE_SAMPLES` from the public `ExponentialSweep` docs. Private const at stimulus.rs:67 used by `ExponentialSweep`'s own fade logic; line 568 references it from a private doc comment, which is why only 411 warns. Demote to code text rather than publishing a DSP tuning constant; if reading the surrounding paragraph suggests the fade length is actually part of the sweep's advertised contract, raise that in the PR instead of quietly making the const pub.

Verification: run the two doc commands in AC #1 before and after, and confirm the count goes 4 -> 0 with no new warnings appearing in either configuration.
<!-- SECTION:PLAN:END -->
