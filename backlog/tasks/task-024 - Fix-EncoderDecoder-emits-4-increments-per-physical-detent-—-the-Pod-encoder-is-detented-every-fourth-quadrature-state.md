---
id: TASK-024
title: >-
  Fix: EncoderDecoder emits 4 increments per physical detent — the Pod encoder
  is detented every fourth quadrature state
status: In Progress
assignee:
  - '@ralph'
created_date: '2026-08-09 04:32'
updated_date: '2026-09-09 02:02'
labels:
  - review-followup
  - planned
dependencies:
  - TASK-018.03
documentation:
  - docs/reference/daisy-pod.md
modified_files:
  - crates/asperitas-pod/src/encoder.rs
  - docs/reference/daisy-pod.md
priority: high
type: bug
ordinal: 36000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Measured on hardware 2026-08-08 from a podtest capture (~/podtest.log, 240 s). Ten deliberate clockwise detents produced a net +40; ten counter-clockwise detents a net -38. Per-detent transition clusters were [4,4,3,4,4,3,4,4,4,4,4] and [-4,-4,-3,-4,-4,-4,-3,-4,-3,-4]. The Pod's encoder rests at every FOURTH quadrature state, so one physical detent walks the full 00->01->11->10 Gray cycle. ENCODER_LUT in crates/asperitas-pod/src/encoder.rs emits +/-1 on every valid transition, so ControlSurface reports four increments per click.

Direction is CORRECT: clockwise is positive. Only the ratio is wrong. This is the residual failure of TASK-018.04 AC #3 ('the encoder produces one increment per physical detent in both directions, clockwise positive').

ControlEvent::EncoderDelta's own doc comment says 'signed detent increment', so the contract is detents and the implementation delivers quarter-detents. Every downstream consumer (TASK-019 parameter mapping) would be off by 4x.

DESIGN CONSTRAINT — the divide-by-four must CARRY THE REMAINDER, not truncate per poll. Three of the ten counter-clockwise detents registered 3 transitions rather than 4 (contact bounce the LUT correctly filters, plus transitions arriving closer together than the poll period). A per-poll 'delta / 4' discards those clusters as zero, turning a cosmetic ratio bug into dropped detents. An accumulator that emits one detent per 4 accumulated quarter-steps and retains the remainder does not.

Consider also the more robust 'full-step' variant used for detented encoders: emit a detent only on arrival at the rest state, using accumulated direction. That tolerates a missing intermediate transition outright rather than letting the accumulator phase-slip. Confirm the actual rest state from hardware before choosing it.

Note this is separable from TASK-025 (poll rate): the 4:1 ratio is present at any poll rate. The brisk-spin segments of the same capture produced +86 and -90 against ~21-22 actual detents, i.e. counts came out HIGH not low, so there is no evidence of mass detent-dropping at 625 Hz.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 one physical detent — meaning four valid quadrature transitions reaching the decoder — produces exactly one ControlEvent::EncoderDelta(±1), clockwise positive, asserted by host unit tests
- [x] #2 the quarter-step-to-detent conversion carries its remainder across drains rather than truncating per poll, so a detent whose cluster registered only 3 counted transitions defers emission instead of being silently discarded; truncation is toward zero so no detent is ever fabricated
- [x] #3 host-side unit tests in crates/asperitas-pod/src/encoder.rs drive the decoder with Gray-code walks reconstructed from the per-detent transition clusters recorded in this ticket's description — including the 3-transition clusters, each carrying one both-bits-change observation standing for the aliased transition — and assert emitted detent counts plus the invariant detents*4 + residue == counted quarter-steps; no test may re-implement the divide. Raw per-transition captures do not exist (~/podtest.log is gone and podtest never logged the 2-bit state), so these are reconstructions from the recorded cluster sizes and must be labelled as such in the tests
- [x] #4 docs/reference/daisy-pod.md's encoder detent-ratio section records what shipped — the QUARTER_STEPS_PER_DETENT constant, carry-across-drain semantics, truncation toward zero and why, and the caveat that a detent whose transitions straddle one poll window contributes fewer than four counts and therefore defers rather than loses a step
- [x] #5 cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings (with and without --features asperitas-pod/pod-hw), cargo test --workspace (both feature configurations), and the thumbv7em-none-eabihf release build of firmware/bin/podtest stay green — the feature-gated runs are what catch a missed drain_detents rename at the ControlSurface call site
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# Plan — TASK-024: convert quarter-steps to detents in `EncoderDecoder`

Everything here is agent work. Hardware confirmation stays where it already lives: **TASK-029**
(`@human`), whose own prerequisite **TASK-029.01** adds the raw-transition logging to `podtest`. Do
not re-add `HUMAN:` criteria to this ticket, and do not claim hardware verification in its final
summary.

## Decision: remainder-carrying quarter-step accumulator over the existing LUT

Keep `ENCODER_LUT` emitting ±1 per valid quadrature transition (`encoder.rs:108`) and convert the
accumulated quarter-steps to whole detents at drain time. Two alternatives were considered and
rejected:

- **Rest-state full-step state machine** (emit only on arrival at the encoder's mechanical rest
  state, QDEC-style). Rejected *for now*, not on merit: it requires knowing which of the four states
  the Pod's encoder rests in, and that fact is recorded nowhere in the repo — `previous_state: 0`
  (`encoder.rs:118`, inside `new()`) is an assumption, not a measurement. This ticket's own description says to
  confirm the rest state before choosing it. Measuring it is now TASK-029.01's job; if the capture
  shows the accumulator under-reporting, that measurement is the entry fee for this design.
- **Direction-guarded diagonal recovery** (treat a both-bits change as ±2 when it agrees with recent
  direction). Rejected: genuine contact bounce also presents as a both-bits change, so a bounce
  excursion `01→10→01` during a clockwise turn would count +4 — a whole phantom detent — regressing
  TASK-018.03 AC #3 ("contact bounce does not double-count"). It also cannot live in the current
  table (`all_lut_entries_valid` and `lut_symmetry_clockwise_vs_counter` at `encoder.rs:410-443`
  both assume entries in {−1,0,+1} and antisymmetry, which a direction-dependent value breaks). Its
  correctness needs raw per-transition data we do not have. Same follow-up path as above.

## What this fixes, and what it deliberately does not

Fixed: the 4× unit error. One physical detent walks the full `00→01→11→10→00` Gray cycle, and the
contract on `ControlEvent::EncoderDelta` (`encoder.rs:52`, "signed detent increment") becomes true.

Not fixed, and not to be papered over: the missing quarter-counts behind the −38 net. When two
transitions land inside one ~1 ms poll window the decoder sees a both-bits change, the LUT yields 0,
and the detent contributes fewer than four counts. Carrying the remainder **redistributes** that
deficit rather than erasing it: over the recorded counter-clockwise run it reports nine detents for
ten clicks and retains the leftover quarter-steps, whereas per-poll `delta / 4` would have thrown
them away. State exactly this in the docs and the final summary. Whether the deficit survives at the
corrected ~1 kHz poll rate (TASK-025 is Done) is TASK-029's question; its AC #4 files the residual as
a new bug with timestamps if it reproduces.

## Step 1 — the conversion (`crates/asperitas-pod/src/encoder.rs`, host-testable section)

Add a named constant in the style of `knob.rs:38-42` (`POT_FULL_SCALE_COUNTS`: bold measured fact +
date + pointer to the reference doc):

```rust
/// Quadrature transitions per physical detent on the Pod's encoder.
///
/// The Pod's encoder is detented at every **fourth** quadrature state: measured 2026-08-08, ten
/// deliberate detents produced a net ±40 LUT counts. See
/// `docs/reference/daisy-pod.md` § "Encoder detent ratio".
const QUARTER_STEPS_PER_DETENT: i16 = 4;
```

Rename the field `accumulated_delta: i8` → `quarter_steps: i16` (`encoder.rs:88`), use
`saturating_add` in `update()` (`encoder.rs:134`; matching the `DebouncedSwitch::consecutive`
precedent at `encoder.rs:182` plus its regression test — host `cargo test` runs with
`overflow-checks`, so an unsaturated accumulator panics in debug and silently wraps in release), and
replace `drain_delta()` (:141) with:

```rust
/// Drain whole detents accumulated since the last call, retaining the leftover quarter-steps.
pub fn drain_detents(&mut self) -> i8 {
    let detents = (self.quarter_steps / QUARTER_STEPS_PER_DETENT).clamp(-127, 127);
    self.quarter_steps -= detents * QUARTER_STEPS_PER_DETENT;
    detents as i8
}
```

Two subtleties to keep in the doc comment, because they are the parts a future reader will get wrong:

- **Truncation must be toward zero**, which is plain Rust `/`. Do not "tidy" this into
  `div_euclid`/`rem_euclid`: `div_euclid(-3, 4)` is `-1`, which would report a detent whose four
  quarter-steps never arrived. Truncating toward zero means a detent is never fabricated and the
  remainder keeps the sign of the rotation.
- **The remainder outlives the drain.** That is the whole point (AC #2), and it changes the meaning
  of the method relative to today's `drain_delta()`. Say so on the method, not only at the call
  site — TASK-025's post-review fixup exists precisely because a scheduling constraint had been left
  at the call sites instead of on the interface.

Leave `update()`'s signature and its `& 0b11` masking untouched (TASK-018.03 fixup, tested by
`out_of_range_state_is_masked_not_indexed_out_of_bounds`).

## Step 2 — the hardware call site (`#[cfg(feature = "pod-hw")] mod hw`)

`encoder.rs:290-293`: call `drain_detents()` instead of `drain_delta()`. Emission shape is unchanged
— still one aggregated `EncoderDelta(n)` per poll when non-zero. Nothing downstream cares about
aggregated versus per-step events today: `poll()` already aggregates and the only consumer in the
repo is the log line at `firmware/src/bin/podtest.rs:199`. `main.rs` has no control-surface task at
all yet, and neither TASK-019 nor TASK-019.01 specifies delta units, acceleration, or step sizes.

Because `hw` is behind a non-default feature, a plain `cargo test --workspace` will not notice a
missed rename. The feature-gated clippy/test runs in AC #5 are what catch it.

## Step 3 — tests (`mod tests`, same file)

Existing tests encode the buggy quarter-count semantics and must be re-expressed, not deleted:

| Test (`encoder.rs`) | Becomes |
|---|---|
| `clockwise_one_detent_from_zero` :338 | rename to `one_transition_is_a_quarter_step_and_does_not_emit`; assert `drain_detents() == 0` and `quarter_steps == 1` |
| `counter_clockwise_one_detent_from_zero` :346 | mirror, `quarter_steps == -1` |
| `full_clockwise_cycle` :354 | `four_transitions_clockwise_emit_one_detent`: `drain_detents() == 1`, `quarter_steps == 0` |
| `full_counter_clockwise_cycle` :365 | mirror, `== -1` |
| `bounce_both_bits_00_to_11_yields_zero` :376, `..._01_to_10_...` :384 | keep; assert no *quarter-step* is added (`quarter_steps == 0`) — the LUT is unchanged, so these stay meaningful and guard bounce filtering against this change |
| `mixed_rotation_and_bounce_filters_bounce` :399 | net is one quarter-step, so assert `drain_detents() == 0` with `quarter_steps == -1`, and rename to say what it now proves (bounce is filtered *and* the sub-detent residue is retained) |
| `out_of_range_state_is_masked_...` :422 | assert masking via `quarter_steps == 1` rather than a detent of 1 |
| `all_lut_entries_valid` :410, `lut_symmetry_...` :431, `no_movement_yields_zero` :392 | unchanged (LUT untouched) |

New tests, in this order of importance:

1. `remainder_carries_across_drains` — three clockwise transitions, `drain_detents() == 0`; then the
   fourth, `== 1`. This is AC #2 verbatim.
2. `truncation_is_toward_zero_not_toward_negative_infinity` — accumulate −1..−3 quarter-steps and
   assert `drain_detents() == 0`, with a comment naming `div_euclid(-3, 4) == -1` as the trap.
3. `reversal_cancels_retained_remainder` — +3 quarter-steps, then −4: assert no detent is emitted in
   either direction, because the shaft never travelled a full detent net. This is the behaviour a
   naive per-poll divide gets backwards in both directions.
4. **Reconstructed captured runs (AC #3).** Read the cluster arrays in this ticket's description
   (`[4,4,3,4,4,3,4,4,4,4,4]` and `[-4,-4,-3,-4,-4,-4,-3,-4,-3,-4]`) and drive the decoder with Gray
   walks built from them: a cluster of n is n LUT-valid transitions in rotation order, and any
   cluster shorter than 4 additionally contains a both-bits-change observation standing for the
   transition that got aliased away. A test-local helper may *build walks* (that is data, not the
   formula) but must never compute an expected value.
   Be explicit in a comment about what is and is not captured data: **no raw per-transition capture
   exists anywhere.** `~/podtest.log` is gone from disk and `podtest.rs:199` never logged the 2-bit
   state, so these are reconstructions from the recorded cluster sizes. Raw captures arrive with
   TASK-029.01 + TASK-029.
   Assert the invariants, not a pinned total, and sweep the diagonal's position inside each short
   cluster (first / middle / last) since the aggregate record cannot say where it fell:
   - `detents_emitted * 4 + quarter_steps_left == total_quarter_steps_counted` (nothing is silently
     discarded — the exact wording of AC #2),
   - emitted detents never exceed the number of physical detents turned (no fabrication),
   - emitted sign matches rotation direction.
   For the counter-clockwise run, expect **nine** emitted detents against ten physical clicks with
   the residue retained, and name the test accordingly
   (`ccw_captured_run_reports_nine_of_ten_with_residue_retained`) with a comment pointing at TASK-029
   AC #4 for filing the residual. Pinning 9 is deliberate: it documents the limit of this fix, and a
   future change that recovers the aliased counts must update it on purpose.
5. `drain_clamps_rather_than_wrapping_when_undrained` — accumulate 4×200 quarter-steps without
   draining and assert the first drain returns `127` and the second the remainder. Defensive only:
   `poll()` drains every call, so this path is unreachable today; it exists so the `as i8` narrowing
   in `drain_detents()` is covered by a decision rather than by luck.
6. `held_still_emits_nothing_on_repeated_drains` — 1000 identical readings, every drain 0. Guards
   the accumulator against drift on a static input.

## Step 4 — documentation

- `docs/reference/daisy-pod.md` § "Encoder detent ratio" (:88-102), AC #4. Keep the measured facts
  and the "+40 / −38" numbers; change the prescription from "divide-by-four is required" to what
  shipped: the `QUARTER_STEPS_PER_DETENT` constant, carry-across-drain semantics, truncation toward
  zero and why, and the explicit caveat that a detent whose transitions straddle one poll window
  contributes fewer than four counts and therefore defers a step rather than losing one. Match the
  house style of the neighbouring "(verified)" sections: dated, names the binary/protocol, states the
  discriminating numbers, and says what the evidence rules out. Record here too that the two recorded
  cluster arrays do not sum to their stated nets (42 vs +40 clockwise, 37 vs −38 counter-clockwise),
  which is why AC #3 asserts invariants rather than those totals.
- `encoder.rs` module doc (:14-17): the sentence "Bounce states … map to delta = 0 in the transition
  table, so contact bounce produces spurious detents automatically" is missing its negation — it
  currently asserts the opposite of the truth. Fix it while here, and extend the same block to state
  the transition-versus-detent distinction and the ~1 kHz precondition it inherits from TASK-025/026.
- `ControlEvent::EncoderDelta`'s doc (:52) needs no change — after this ticket it finally means what
  it says. Worth a note in the final summary rather than an edit.

## Step 5 — gates

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings`
- `cargo test --workspace` and `cargo test --workspace --features asperitas-pod/pod-hw`
- `cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest --release`

## Sizing and blast radius

`EncoderDecoder` has exactly one production caller (`ControlSurface::poll`) and one consumer
(podtest's log line); no DSP, CLI, or audio code references it. Expect roughly 40 lines changed in
the decoder, ~150 in tests, plus the doc section. No public API outside `encoder.rs` changes.

## Follow-up trigger (do not act on it now)

If TASK-029's capture — taken at the corrected ~1 kHz and with TASK-029.01's raw logging — still shows
clusters below four transitions, the deficit is snap-through aliasing and needs either direction-
guarded diagonal recovery or the rest-state machine, whichever the measured rest state supports. File
it per TASK-029 AC #4 with timestamps; do not fold it into this ticket.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Shipped as planned, no deviation from the plan's chosen design.

**Decoder (`crates/asperitas-pod/src/encoder.rs`)**
- `QUARTER_STEPS_PER_DETENT: i16 = 4` added, documented in `knob::POT_FULL_SCALE_COUNTS` house style (measured date + pointer to the reference doc).
- Field `accumulated_delta: i8` -> `quarter_steps: i16`; `update()` uses `saturating_add`. Widening to i16 is what makes an undrained accumulator saturate far away rather than wrap the sign of a rotation within a few hundred polls.
- `drain_delta()` replaced by `drain_detents()`: truncating `/`, clamped to the i8 range, remainder retained by subtracting `detents * 4` rather than zeroing. Both load-bearing properties (the remainder outlives the drain; truncation toward zero, NOT `div_euclid`) are documented on the method, not at the call site.
- `ControlSurface::poll` (pod-hw) calls `drain_detents()`; event shape unchanged. No public API outside encoder.rs changed apart from that rename. `firmware/src/bin/podtest.rs`: one comment recording that its ENC log line is now in detents, which is what TASK-029 reads off the board.
- Module doc: fixed the sentence asserting bounce states "produce spurious detents" (missing negation) and added the transition-versus-detent distinction, including that an aliased transition cannot be recovered by arithmetic.

**Tests** (`encoder.rs`: 19 test fns before, 27 after; the `asperitas-pod` host suite goes 27 -> 35). Existing LUT tests re-expressed for quarter-step semantics rather than deleted; bounce/masking tests now assert `quarter_steps`. New: `remainder_carries_across_drains`, `truncation_is_toward_zero_not_toward_negative_infinity`, `reversal_cancels_retained_remainder`, `drain_clamps_rather_than_wrapping_when_undrained`, `held_still_emits_nothing_on_repeated_drains`, plus the reconstructed runs.

**Reconstructed runs (AC #3).** The cluster arrays from the description drive Gray-code walks; a short cluster carries one both-bits-change observation standing for the aliased transition, so it registers exactly its recorded count. Walk builders construct data only — expected values come from the decoder. Each test sweeps all four positions the aliased transition could have fallen in, since the aggregate record cannot say. Assertions: `detents*4 + residue == counted`, emitted detents never exceed clicks turned, sign follows direction. Clockwise run -> +10 detents with +2 retained; counter-clockwise -> -9 detents against ten clicks with -1 retained (pinned deliberately: it documents the limit of this fix, and any future recovery of the aliased counts must update it on purpose). A cadence test adds coverage the AC did not ask for — draining every observation, every 2/4/7, and once at the end all agree, which is the direct check that the remainder lives in the decoder rather than at the call site.

One fixture check does read the LUT directly: it confirms each reconstructed walk registers the number of transitions the capture recorded, i.e. it validates the fixture, not the decoder. Labelled as such in the code.

**Deliberately not fixed.** Snap-through aliasing: a click whose two transitions land inside one poll window presents as a both-bits change, so it contributes fewer than four counts and its detent defers. Carrying the remainder redistributes that deficit (nine reported of ten CCW clicks); it does not erase it. The two candidate fixes — rest-state full-step machine, direction-guarded diagonal recovery — need facts nobody has: the encoder's measured rest state, and raw per-transition bounce data (TASK-029.01). Landing either here would regress TASK-018.03 AC #3 (bounce must not double-count) without corroboration logic.

**Gate note.** All AC #5 commands green. The thumbv7em podtest release build emits a pre-existing `cannot find entry symbol _start` linker warning — podtest has no `#[entry]` yet; unrelated to this change and present before it.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: pi
created: 2026-09-09 00:18
---
Dependency on TASK-018.04 removed 2026-09-09. It deadlocked: 018.04 AC #3 ('the encoder produces one increment per physical detent in both directions') is the very behaviour this ticket exists to fix, so 018.04 could never be verified while the bug stood, and this ticket could never unblock until 018.04 was Done. Neither could ever close. TASK-018.03 (Done) is the real prerequisite — it shipped the decoder being fixed here.

Remaining hazard, unresolved deliberately: AC #4 is prefixed HUMAN and cannot be satisfied by agent work, while this ticket is assigned @agent. Per this repo's CLAUDE.md the mixed case should be split rather than left in one ticket; left as it stands, the autonomous loop will pick this up, land the code, be unable to check AC #4, revert status, get re-picked, and halt on the repeated-choice guard. See TASK-025/TASK-027 for how the same shape was handled after the fact.
---

author: pi
created: 2026-09-09 00:27
---
AC #4 (HUMAN: fresh podtest capture showing net +10/-10) moved out to TASK-029 on 2026-09-09, which depends on this ticket. What remains here — the decoder fix, the remainder-carrying conversion, host unit tests replaying the captured transition sequences, and the daisy-pod.md detent-ratio section — is all satisfiable by agent work, so this ticket can now reach Done honestly and the loop will not stall on it at finalization. TASK-029 is the record that the ratio is code-verified but not yet hardware-verified; it shares a board session with TASK-027.
---

created: 2026-09-09 01:36
---
Planned 2026-09-09. Two changes worth their own record.

**AC #1 and #3 were reworded, not weakened.** AC #1 as written ("one physical detent produces exactly one increment") cannot be satisfied by any unit-conversion fix, because part of the recorded deficit is *aliasing*: when two transitions land inside one ~1 ms poll window the decoder sees a both-bits change, the LUT correctly returns 0, and that detent never contributes four counts in the first place. A remainder-carrying divide therefore reports nine detents for ten counter-clockwise clicks and keeps the residue — redistributing the deficit, not erasing it. AC #1 now says what is checkable (four counted transitions ⇒ one increment) and the residual is named in AC #4's doc update plus this ticket's plan, with TASK-029 AC #4 filing it as a bug if it reproduces at the corrected ~1 kHz. AC #3 asked for "ACTUAL transition sequences captured from hardware", which does not exist: `~/podtest.log` is no longer on disk, `firmware/src/bin/podtest.rs:199` logs only the decoded sum and never the 2-bit state, and no `.log` capture is committed anywhere in the repo. The surviving evidence is aggregate — the cluster arrays in the description. AC #3 now says so explicitly and requires the tests to be labelled as reconstructions, because an executor left with the old wording would either fail honestly or claim captured data it does not have. Note also that neither recorded array sums to its stated net (+42 vs +40 clockwise, −37 vs −38 counter-clockwise), so the tests assert invariants rather than those totals.

**New prerequisite elsewhere, not here:** TASK-029.01 adds transition-gated raw A/B logging to podtest and blocks TASK-029. It is deliberately *not* a child of this ticket — making it one would put a human board session back on the critical path of the code fix, which is the stall that comments #1 and #2 on this ticket exist to prevent. This ticket's plan defers two designs that need facts nobody has measured yet (the encoder's rest state) or data nobody has captured (raw bounce patterns): the rest-state full-step machine, and direction-guarded diagonal recovery. The latter would convert bounce into phantom detents without corroboration logic, regressing TASK-018.03 AC #3.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed the 4x encoder unit error. `EncoderDecoder` now counts raw quadrature transitions in a `i16` accumulator and converts them to whole detents at drain time via `drain_detents()` (`QUARTER_STEPS_PER_DETENT = 4`, remainder carried across drains, truncation toward zero so a detent is never fabricated). `ControlSurface::poll` calls it, so `ControlEvent::EncoderDelta` finally means "detent" — clockwise positive, one per physical click. The `asperitas-pod` host suite goes from 27 tests to 35 (`encoder.rs`: 19 to 27), including runs replaying the captured per-detent cluster arrays (labelled as reconstructions: no raw per-transition capture exists) with the invariant `detents*4 + residue == counted quarter-steps`. `docs/reference/daisy-pod.md` records what shipped and its limit.

Verification is **code-level only**. Hardware confirmation remains TASK-029 (`@human`), which also owns the residual this fix does not remove: three of ten recorded counter-clockwise clicks registered only three counted transitions, and the decoder cannot count a transition it never sampled — that run replays as nine reported detents for ten clicks, with the leftover quarter-steps retained rather than discarded. The stronger designs (rest-state full-step machine, direction-guarded diagonal recovery) are deferred behind TASK-029.01's raw A/B logging and a measured rest state, because guessing either would trade a ratio bug for phantom detents on bounce.

Gates green: `cargo fmt --all --check`; clippy `-D warnings` with and without `asperitas-pod/pod-hw`; `cargo test --workspace` in both feature configurations; thumbv7em-none-eabihf release build of `podtest` (one pre-existing `_start` linker warning, unchanged by this work).
<!-- SECTION:FINAL_SUMMARY:END -->
