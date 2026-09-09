---
id: TASK-029.01
title: >-
  Add transition-gated raw A/B encoder logging to podtest so a capture can
  record per-detent clusters and rest state
status: In Progress
assignee:
  - '@ralph'
created_date: '2026-09-09 01:25'
updated_date: '2026-09-09 02:23'
labels:
  - planned
dependencies: []
documentation:
  - docs/reference/daisy-pod.md
modified_files:
  - firmware/src/bin/podtest.rs
  - crates/asperitas-pod/src/encoder.rs
parent_task_id: TASK-029
priority: high
type: task
ordinal: 43000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-029 (HUMAN) must record, from a fresh podtest capture, the **per-detent transition-cluster
sizes** (its AC #3) and the encoder's **rest state** — the two facts that decide whether
TASK-024's remainder-carrying accumulator was the right call or whether the rest-state full-step
variant is needed. podtest cannot produce either today: `firmware/src/bin/podtest.rs:199` logs only
the decoded sum (`ENC {:+}`), never the 2-bit A/B state, and the state is computed inside
`ControlSurface::poll` (`crates/asperitas-pod/src/encoder.rs:286-292`) without being exposed. Until
this ticket lands, TASK-029 can verify the *net* ratio (+10/-10) but not the cluster shape, and the
rest state stays unmeasured.

WHAT TO BUILD: log one line per encoder **state change**, carrying the millisecond timestamp that
podtest already stamps on every line plus the 2-bit raw state, and keep the existing decoded-delta
line. Exposing the state needs a read-only path — either a `ControlSurface::encoder_state()`
accessor returning the last sampled 2-bit state, or an additional argument/closure value handed to
`poll()`'s callback. Decoder behaviour and `ControlEvent` must not change; this is observability
only.

WHY GATED, NOT EVERY TICK: the USB CDC path is the bottleneck, not the poll loop. An unthrottled
per-tick line costs ~1000 lines/s (~6 MB per 240 s capture), and the transport already dropped
8.8% of lines (1226 of 13968, ~58 lines/s) in the 2026-08-08 capture recorded in
`backlog/tasks/task-018.04 - Verify-every-Pod-control-on-hardware.md`. Gate on state change: during
deliberate detents that is at most ~4 lines per detent (~40 lines/s worst case at 10 detents/s) and
zero while the shaft is still, which is both analysable and under the observed loss threshold. Imitate
the existing throttle-documentation style at `firmware/src/bin/podtest.rs:77-81`
(`KNOB_LOG_THROTTLE`) and state the rate arithmetic in the new constant's doc comment.

The same gating yields the rest state for free: the state held between transition bursts is the
mechanical rest position, which is what TASK-024's deferred design question needs.

Out of scope: changing the decoder, `ControlEvent`, or any log line other than the encoder's.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 podtest emits one encoder line per raw 2-bit state CHANGE (timestamp + A/B state), and no encoder line while the shaft is still; the existing decoded ENC delta line is kept
- [x] #2 worst-case encoder log rate during deliberate detents is stated in the new constant's doc comment and is below the ~58 lines/s at which the 2026-08-08 capture lost 8.8% of lines
- [x] #3 decoder behaviour is unchanged: the raw state reaches the log through a read-only path (accessor or extra poll callback value) with no change to ControlEvent or ENCODER_LUT
- [x] #4 cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings passes and cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest --release succeeds
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Scope

Observability only. Give `podtest` the ability to show the raw 2-bit encoder state changing over
time, so one board session (shared with TASK-027) can answer TASK-029 AC #3 (per-detent transition
cluster sizes) and measure the encoder's mechanical rest state. Do not change decoder behaviour.

## Step 1 — expose the sampled state (read-only)

Two three-line accessors; do NOT add a `ControlEvent` variant and do NOT change `poll()`'s signature
(the callback contract is used by `firmware/src/bin/podtest.rs:197-220` and any future control task):

- `crates/asperitas-pod/src/encoder.rs`, `impl EncoderDecoder`: `pub fn state(&self) -> u8` returning
  `self.previous_state` — i.e. the last state the decoder latched, which is exactly what the LUT
  consumed. Doc it as "the last quadrature state accepted by `update()`, A = bit 1, B = bit 0".
- In the `pod-hw` module's `impl ControlSurface`: `pub fn encoder_state(&self) -> u8` delegating to
  `self.encoder.state()`. Doc it as diagnostic-only, valid only after a `poll()`.

Keeping this behind `pod-hw` means host `cargo test --workspace` will not compile it: the
`--features asperitas-pod/pod-hw` clippy/test run and the firmware build are the only checks. That is
why AC #4 names them.

## Step 2 — log transitions in podtest, gated on change

In `firmware/src/bin/podtest.rs`, beside the existing `tick`/`knob_log_tick` counters
(near :160), hold `let mut last_enc_state: Option<u8> = None;`. After `controls.poll(...)` (:197-220)
each iteration:

```rust
let st = controls.encoder_state();
if last_enc_state != Some(st) {
    info!("[podtest] t={} ENCRAW {}", now_ms, EncoderState bits as two chars);
    last_enc_state = Some(st);
}
```

Print the state as two ASCII characters (`AB=00`, `AB=01`, `AB=11`, `AB=10`) rather than a decimal
number: the Gray walk is legible at a glance in a text capture and `00→01→11→10` reads directly as a
clockwise cycle, which is what the analysis needs. Leave the existing decoded `ENC {:+}` line exactly
as it is — the pair of streams is what lets a reader attribute a cluster to a decoded detent.

Do not log every tick. Document the rate arithmetic in a short comment above the block, matching the
`KNOB_LOG_THROTTLE` style at :77-81: at most 4 transitions per detent, so a brisk 10 detents/s is
~40 lines/s, and zero lines while the shaft is still. Note honestly that the transport-loss figure
this is judged against is approximate — the 2026-08-08 capture delivered 13968 lines over 240 s with
about 8.8% garbled or missing (`backlog/tasks/task-018.04 - Verify-every-Pod-control-on-hardware.md`
notes), measured across all line types, so treat ~40 lines/s as "same order as what already worked",
not as a proven headroom. Fallback if TASK-029's capture shows line loss: drop the timestamp from
`ENCRAW` lines (the preceding decoded line already carries one) or coalesce a burst into one line per
cluster. Record whichever was needed in this ticket's implementation notes.

## Step 3 — gates

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings`
- `cargo test --workspace` and `cargo test --workspace --features asperitas-pod/pod-hw`
- `cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest --release`

## Step 4 — what this ticket can and cannot claim

Compiling is not evidence here: nothing in this ticket proves the log lines arrive intact or that the
clusters are legible, because that needs the board. Say so in the final summary and leave the
hardware confirmation to TASK-029, which owns the capture session. Update
`docs/reference/daisy-pod.md` only if the capture afterwards records a new fact (rest state); the
harness itself does not need its own doc section.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented (all four gates green locally):

- crates/asperitas-pod/src/encoder.rs: EncoderDecoder::state() -> u8 returns previous_state, the last quadrature state update() latched (A=bit1, B=bit0, always 0..=3 because update masks before storing). Purely additive; ENCODER_LUT, drain_detents and ControlEvent untouched. New host test state_reports_last_accepted_quadrature_state_in_range covers fresh/at-rest/masked-input cases (36 pod tests pass).
- Same file, pod-hw module: ControlSurface::encoder_state() delegates to the decoder, documented diagnostic-only and valid only after poll(). Does not change poll()'s signature or its callback contract.
- firmware/src/bin/podtest.rs: after controls.poll(), one line per raw STATE CHANGE: '[podtest] t=<ms> ENCRAW AB=<bits>', labels from ENCRAW_STATE_LABELS = ["00","01","10","11"] so a capture reads as the Gray walk (00 01 11 10 = clockwise). last_enc_state starts None, so the very first line records the state the harness boots in; between bursts the held value is the mechanical rest position. The decoded 'ENC {:+}' line is unchanged and still precedes each burst.
- Rate gate: ENCRAW_MAX_LINES_PER_DETENT(4) x ENCRAW_MAX_DETENTS_PER_SECOND(10) = ENCRAW_WORST_CASE_LINES_PER_SECOND(40), documented against the ~58 lines/s where the 2026-08-08 capture lost ~8.8% of 13968 lines, with an explicit note that the figure is approximate and shared with ~100 knob lines/s, so 40/s is 'same order as a rate that already survived', not proven headroom. A const assert keeps the comparison true if either assumption is retuned. Zero lines while the shaft is still.
- TASK-030 has NOT landed, so ENCRAW stays a plain info! line in the existing format (fixed field order, space-separated, no padding, per TASK-031's parseability need). Fallback if a capture shows loss: drop the timestamp from ENCRAW, or coalesce a burst into one line per cluster.
- Gates: cargo fmt --all --check; clippy --workspace --all-targets -D warnings (with and without asperitas-pod/pod-hw); cargo test --workspace (with and without pod-hw); cargo build --manifest-path firmware/Cargo.toml --target thumbv7em-none-eabihf --bin podtest --release. Also confirmed 'ENCRAW AB=' survives into the linked ELF (strings), so the line is really in the shipped binary.
- Evidence limit, stated plainly: compiling is not evidence. Nothing here proves the lines arrive intact over USB CDC or that clusters are legible on a real capture -- AC #1 and AC #2 are established by construction plus inspection only. TASK-029 owns the board confirmation, including whether transport loss cuts a 4-transition cluster down to 3.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-09 01:39
---
Ordering note, added after planning because other tickets appeared while this one was being written.

These new log lines ride the same USB CDC path that **TASK-030** (Make USB console log records self-verifying) exists to fix: the pipe write path discards the tail of a record when the 512-byte buffer is nearly full and the mutex is a no-op, which is how roughly 8.8% of the 2026-08-08 capture's lines were lost or cut mid-token. Raw transition lines are exactly the data where silent loss is worst — a dropped line inside a burst looks identical to a transition that never happened, so a cluster reads as 3 when it was 4.

No hard dependency is imposed on purpose: TASK-030 is a transport project, and gating this harness behind it would delay TASK-029's board session (which TASK-027 also needs) by more than the risk warrants. Instead:

- If TASK-030 has landed by the time this is executed, emit the `ENCRAW` lines through whatever self-verifying record format it introduced rather than inventing a second line format, and say so in the implementation notes.
- If it has not, keep the plain `info!` line, and make TASK-029's capture protocol tolerate loss explicitly: turn each detent direction twice and compare the two runs, so a cluster missing a line shows up as a disagreement rather than as a short cluster. Record that comparison in TASK-029's notes.

Related: **TASK-031** (host-side rig runner producing integrity-checked captures) will eventually read these files programmatically, so keep the line format trivially parseable — fixed field order, space-separated, no alignment padding.
---
<!-- COMMENTS:END -->
