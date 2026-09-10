---
id: TASK-048
title: >-
  Chore: clear the 4 pre-existing rustdoc warnings in asperitas-pod and
  asperitas-dsp
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-10 09:22'
updated_date: '2026-09-10 19:33'
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
- [x] #1 `cargo doc -p asperitas-pod -p asperitas-dsp --no-deps` prints zero warning lines at default features AND with `--features asperitas-pod/pod-hw` (the second set matters because CI already clippy-tests that combination, so a fix that only works at default features just moves the failure).
- [x] #2 Intent is preserved per the rules TASK-043.02 ended up using: where the target genuinely exists in the configuration the prose describes, keep a real link (qualify it, or split the sentence with `#[cfg_attr(feature = ..., doc = "...")]` if that is what correctness needs); where the reference is to something internal, keep the identifier in backticks and say where it lives. Do not delete a cross-reference to make a number go to zero.
- [x] #3 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings` and `cargo test --workspace` still pass, and `git diff --no-ext-diff -U0 crates/ | grep -E '^[+-][^+-]' | sed -E 's/^.//' | grep -vE '^ *(///|//!)'` prints nothing — doc comments only. `--no-ext-diff` is NOT decoration: this repo sets `diff.external` to difftastic, whose side-by-side output makes the plain form print nothing for *any* change, code included (measured both ways on this tree).
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Four sites, one decision each. Read each site plus the item it points at before choosing, and prefer the smallest change that keeps the reader pointed at the right place.

1. knob.rs:24 and knob.rs:40 both point at `Knobs::new` from prose about the ADC resolution/divisor contract. Under `pod-hw` the method exists and the link resolves; at default features it cannot. The honest options are plain code text (`Knobs::new`) at both sites, or leaving the link and gating the sentence with `#[cfg_attr(feature = "pod-hw", doc = "...")]`. Two occurrences of the same sentence fragment argue for code text — a per-feature doc split for a parenthetical "(see ...)" costs more than the hyperlink is worth. Note that POT_FULL_SCALE_COUNTS itself is ungated, which is why the warning fires at all: an ungated constant's docs are reaching into gated hardware code. Consider whether the resolution note belongs on `Knobs::new` (gated) or on the constant (ungated) at all, and say so in the commit message if you move it.
2. encoder.rs:199 points at `ControlSurface::poll` from `drain_detents`, which is ungated. Same shape as #1; code text unless the sentence reads badly with it.
3. stimulus.rs:411 links `FADE_SAMPLES` from the public `ExponentialSweep` docs. Private const at stimulus.rs:67 used by `ExponentialSweep`'s own fade logic; line 568 references it from a private doc comment, which is why only 411 warns. Demote to code text rather than publishing a DSP tuning constant; if reading the surrounding paragraph suggests the fade length is actually part of the sweep's advertised contract, raise that in the PR instead of quietly making the const pub.

Verification: run the two doc commands in AC #1 before and after, and confirm the count goes 4 -> 0 with no new warnings appearing in either configuration.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Measured before/after with the two AC#1 commands. The 'exactly four' count was default-features only: under `--features asperitas-pod/pod-hw` there were THREE MORE warnings at different sites, all pre-existing, so AC#1 put seven sites in scope, not four:
- pins.rs:49 `DaisyPins`(hal::pins::seed::DaisyPins) — target never existed in ANY config; embassy-stm32 has no `pins` module. Code text + upstream path.
- led.rs:7 `asperitas-logging::led::BootLed` — hyphenated path is never valid Rustdoc syntax, but the crate is also not a dependency of asperitas-pod, so a real link would need a cargo dep purely for docs. Code text naming the crate instead.
- knob.rs Knobs::new -> POT_RESOLUTION — private const in the gated hw module; publishing a DSP/ADC tuning const to satisfy a lint is the wrong trade. Stated the value (`Resolution::BITS16`) and named the const.

Decisions on the four default-feature sites: knob.rs:24/:40 and encoder.rs:199 became code text that says where the item lives ('in the `pod-hw` hardware module'), not `#[cfg_attr(feature = "pod-hw", doc = ...)]` splits — module docs would have had to become an attribute sequence, and a per-feature doc fork for a '(see ...)' parenthetical costs more than the hyperlink. No cross-reference was deleted; every one still names its target and its location. stimulus.rs:411 stays code text because `fade_samples` is set from the private const via min(FADE_SAMPLES, N/2) and is not part of the sweep's advertised contract — raising it publicly would be a design decision, not a doc fix. Left the private-doc link at stimulus.rs:568 alone; it resolves.

Verified: zero warning lines in both doc configs; fmt --check, clippy -D warnings (default + pod-hw), cargo test --workspace (16 suites ok) all pass; the AC#3 doc-comment-only diff filter prints nothing.

Coordination: a TASK-049 run was dispatched to clear the three pod-hw-only sites (pins.rs, led.rs, knob.rs) that AC#1 already required here. That session died before editing anything; these seven sites are its work as much as mine, so TASK-049 must not touch those files — its remaining job is the ci.yml gate plus dropping the now-unneeded exceptions block.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Cleared all pre-existing rustdoc warnings in asperitas-pod and asperitas-dsp — seven sites, not four: the three that only fire under `--features asperitas-pod/pod-hw` (pins.rs DaisyPins, led.rs BootLed, Knobs::new -> POT_RESOLUTION) are inside AC #1's second measurement. Each became plain code text that names where the target lives; no cross-reference deleted, no private const published, no per-feature doc split. Both `cargo doc` configurations now print zero warnings, and fmt / clippy at default and with pod-hw / cargo test --workspace still pass with a doc-comment-only diff. This unblocks TASK-049 turning the warnings-as-errors gate on workspace-wide.
<!-- SECTION:FINAL_SUMMARY:END -->
