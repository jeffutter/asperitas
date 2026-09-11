---
id: TASK-038.03.02.01
title: >-
  Add rig console verbs and one whole-record emit path to asperitas-logging,
  with host tests
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-11 15:53'
updated_date: '2026-09-11 15:53'
labels:
  - task
  - planned
dependencies: []
modified_files:
  - crates/asperitas-logging/src/console.rs
  - crates/asperitas-logging/src/frame.rs
  - crates/asperitas-logging/src/lib.rs
parent_task_id: TASK-038.03.02
priority: high
type: task
ordinal: 83600
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The crate-side half of TASK-038.03.02 — and the only half that can be tested with no board attached.

`RIGCFG`, `CAPSTAT`, `CAPMAX` and `DUMPEND` are wire grammar. The convention at `console.rs:171-178` is that a verb's field set and field order are a contract pinned by a unit test **in that same file**, because the firmware package has no host-test target: bodies written ad hoc in `rig.rs` would ship with no test at all. This leaf lands those four builders, their pinning tests, and the one public entry point `rig.rs` will commit them through.

Two smaller things live here for the same reason:

**A bound the sibling leaf's rate gate needs.** Parent plan §8 gates `CAPSTAT` traffic against the dump's own byte budget, and the gate is only honest if the number it divides by belongs to the crate that renders the record — so `CAPSTAT_MAX_BODY` is defined next to its builder and checked by a saturated-render test, not typed into `rig.rs` by hand.

**An incremental CRC.** Parent plan §7 step 3 emits `AUDEND` with a CRC over the block's raw bytes in chunk order, but the dump writer holds one `dump::CHUNK_RAW` slice at a time and never materialises the block. `frame.rs:204` offers only `crc16_ccitt(data) -> u16` over a whole buffer, so chaining chunks is impossible today; this leaf adds the update form and proves it agrees with the whole-buffer form.

Out of scope, owned by TASK-038.03.02.02: `firmware/src/bin/rig.rs` and everything in it, `firmware/Cargo.toml` features, `.github/workflows/ci.yml`, the SDRAM memory-model note in `docs/reference/daisy-seed3.md`, and the stale `steal()` comment in `spin_budget.rs`. No file under `firmware/` changes here.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `crates/asperitas-logging/src/console.rs` gains four body builders beside `status_body` — `rigcfg_body`, `capstat_body`, `capmax_body`, `dumpend_body` — each following `status_body`'s shape exactly (`TruncWriter` + `core::write!`, `proto=1` second, every field in one format string, returning `w.filled()`), rendering the field sets and orders in parent plan §2 verbatim, and each pinned by a host unit test in that file naming every field in order.
- [ ] #2 Each of the four builders has a saturated-counters test asserting its worst-case render is shorter than `frame::MAX_BODY`, modelled on `status_body_renders_saturated_counters_as_u32_max` (`console.rs:327`), which exists because a 255-byte body silently truncating at 200 is the failure nobody notices until a host parser rejects it.
- [ ] #3 `console.rs` publishes `pub const CAPSTAT_MAX_BODY: usize = 200;` beside its builder, and the saturated test asserts both `len < CAPSTAT_MAX_BODY` and `CAPSTAT_MAX_BODY <= frame::MAX_BODY`, so TASK-038.03.02.02's rate gate divides by a number this crate owns and a host test checks.
- [ ] #4 One public whole-record entry point `pub fn emit_record(level: Level, now_ms: u32, body: &[u8]) -> bool` exists in `lib.rs`, gated `#[cfg(feature = "log-usb")]` like `try_emit_dump`, and commits through the existing `commit_records` (`lib.rs:368`) — `RECORD_BUFS`, `CONSOLE.take_seq()`, `frame::encode`, `frame::write_whole(framed, LOG_PIPE.free_capacity(), …)`, outcome counted. It takes `now_ms` as an argument so `Instant::now()` is read outside the lock, the way `lib.rs:421` and `lib.rs:571` do. `tests/commit_path_no_panic.rs` still passes unchanged, including its lock-site count. Nothing added calls `usb::emit_blocking`.
- [ ] #5 `frame.rs` gains `pub fn crc16_ccitt_update(crc: u16, data: &[u8]) -> u16`, and `crc16_ccitt(data)` becomes `crc16_ccitt_update(0xFFFF, data)` so there is one implementation of the polynomial. A host test chains a real 32 KiB block at `dump::CHUNK_RAW` boundaries and asserts the chained result equals the whole-buffer result, including for the short final chunk.
- [ ] #6 All gates pass and their outputs go in the finalization notes: `cargo fmt --all --check`; `cargo test --workspace`; `cargo clippy --workspace --all-targets -- -D warnings`; `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps` in default and `--all-features` forms; `cargo build -p asperitas-logging --target thumbv7em-none-eabihf`; and both firmware release builds (`--features seed3`, `--no-default-features --features "seed3 log-defmt"`) with `size -B` for `main` and `podtest` recorded before and after, since these are additive exports no binary calls yet.
- [ ] #7 `git diff --name-only HEAD` lists nothing outside `crates/asperitas-logging/`.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
### Read first

- **Parent TASK-038.03.02's plan is the authority for this ticket's grammar and rationale.** Read §2 (record verbs: why they belong in `console.rs`, the three decisions inside the grammar — no second sample-rate field, `cpu_hz`/`icache`/`dcache` are measured values passed in, `audio_exit` as its own counter), §8 (why `CAPSTAT_MAX_BODY` lives here), and §7 step 3 (why the CRC must chain). Do not restate that reasoning; cite the section.
- `crates/asperitas-logging/src/console.rs` — `status_body` and its two tests are the template, including how a saturated render is constructed.
- `crates/asperitas-logging/src/lib.rs` — `commit_records` (:368), the private `emit_status` (:503) that `emit_record` is modelled on, `try_emit_dump`'s feature-gating style, and the `Instant::now()`-outside-the-lock pattern at :421 and :571.
- `crates/asperitas-logging/tests/commit_path_no_panic.rs` — counts lock sites; read it before touching the emit path so you know what your change must not disturb.
- `crates/asperitas-logging/src/frame.rs` — `crc16_ccitt` (:204), `MAX_BODY`, and the encode path that already calls the CRC once per record.

### Salvage before you write

`git stash list` shows `stash@{0}` ("wip-038.03.02-uncommitted"): 687 insertions across `console.rs` (+564), `frame.rs`, `lib.rs`, `spin_budget.rs` and `tests/commit_path_no_panic.rs`, left behind by the two aborted attempts at the parent ticket. Inspect it with `git stash show -p stash@{0}` and reuse the parts that are sound — it is most likely this exact work. Treat it as unreviewed third-party code: read every hunk, apply selectively (`git checkout stash@{0} -- <path>` then edit, or cherry-pick hunks by hand), and delete the stash (`git stash drop`) once its content is either committed or rejected. Note it also touches `spin_budget.rs` and `commit_path_no_panic.rs`, which belong to the sibling leaf's concerns — leave those out unless AC #4's lock-site count genuinely requires it.

### Shape of the work

1. Four builders + eight tests (AC #1-#3). Field names, order and spelling come from parent plan §2's grammar block; copy it character for character into the tests.
2. `crc16_ccitt_update` + its test (AC #5), then refactor `crc16_ccitt` onto it and confirm the existing CRC tests still pass unchanged — they are the regression net for the refactor.
3. `emit_record` (AC #4). Model on `emit_status`; the only structural difference is the `now_ms` parameter. If `commit_records` cannot take a caller-supplied timestamp without restructuring, add a private helper beside it rather than changing `emit_status`'s signature, and say in the comment why.
4. Gates (AC #6), then commit.

### How to run this ticket without dying at the deadline

The parent died twice at the 40-minute execute deadline while trying to do this *and* the firmware binary in one pass. Budget accordingly:

- Ping the orchestrator over intercom **at least every 10 minutes**, even mid-edit-stream — a silent worker is killed as if hung, whatever it was actually doing.
- Commit early and often. First commit as soon as the four builders and their tests pass `cargo test -p asperitas-logging`; later commits may be checkpoints. Ralph only requires that a commit landed, and a checkpointed tree survives a cut deadline; an uncommitted one becomes the next attempt's mystery debris.
<!-- SECTION:PLAN:END -->
