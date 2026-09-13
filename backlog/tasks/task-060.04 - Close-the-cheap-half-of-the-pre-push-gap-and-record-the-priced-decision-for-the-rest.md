---
id: TASK-060.04
title: >-
  Close the cheap half of the pre-push gap and record the priced decision for
  the rest
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-13 00:10'
updated_date: '2026-09-13 01:02'
labels:
  - planned
dependencies:
  - TASK-060.03
references:
  - lefthook.yml
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
modified_files:
  - lefthook.yml
parent_task_id: TASK-060
priority: medium
type: chore
ordinal: 96800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-060 AC #3 asks for a recorded decision, not necessarily a fix, on the three checks CI has that `lefthook.yml`'s pre-push does not: the `asperitas-pod/pod-hw` clippy/test pair, `dump_reassemble --selftest`, and the RTT-only cross-compile.

Priced locally 2026-09-12 in `nix develop .#default` (warm target dir): the whole verbatim ci.yml suite is 138 s, of which `cargo test --workspace` is 66 s and `cargo test --workspace --features asperitas-pod/pod-hw` is 67 s. Everything else in the suite, including both firmware cross-compiles and every clippy invocation, is under 4 s each. So the gap is one expensive item and several free ones, and pretending they are one decision is why four tickets kept deferring it.

The cheap framing, stated once so the numbers are on the record: pre-push today runs ~70 s warm, dominated entirely by its existing `cargo test --workspace`. Adding the three free items costs ~2 s. Adding the pod-hw test too would roughly double the hook to ~140 s, to protect a feature-gated host code path, on a hook that - unlike pre-commit - the autonomous loop never executes (`~/.pi/agent/extensions/ralph/index.ts` calls git only for `rev-parse`, `log` and `rebase --autosquash`; nothing passes `--no-verify`, and `backlog-execute/SKILL.md:113-119` forbids it). The precedent cuts the same way: TASK-018.01's fixup closed the identical hole for `pod-hw` by adding the pair to CI only (landed in `c44b9c1`, six lines at what is now `ci.yml:40-41,57-58`) and left the hook cheaper.

Decision this ticket implements and records: take the free coverage, keep the priced item in CI, and write the exception down twice - in the ticket notes and in a comment in `lefthook.yml` - so the next reader does not "helpfully" re-diverge the two lists or quietly delete the caveat.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 pre-push gains three commands mirroring CI verbatim: cargo run -p asperitas-logging --example dump_reassemble -- --selftest; cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings; and cd firmware && cargo build --release --no-default-features --features "seed3 log-defmt". Measured local warm cost 0 s / 1 s / 1 s. The RTT entry must not use root: "firmware/" (path filter plus lefthook's skip-on-empty-set makes a gate skippable).
- [x] #2 The one remaining divergence is recorded twice - in this ticket's notes and in a comment in lefthook.yml naming the item (cargo test --workspace --features asperitas-pod/pod-hw), its price (67 s local warm, more than every other pre-push command combined), why that is acceptable now (CI runs it; the ralph loop never pushes, so this hook executes rarely; TASK-018.01's fixup made the same CI-only split in c44b9c1), and the condition that should reopen the decision.
- [x] #3 The prose that describes the gates matches what actually runs: backlog/docs/doc-001 - Asperitas-Project-Plan.md:223-228, :303, :33 and :117 are read against the final lefthook.yml + ci.yml and corrected, including the honest residual - firmware docs are still ungated by cargo doc, pedantic lints are unadopted, and the pod-hw test stays CI-only.
- [x] #4 Notes end with a matrix: every check x {pre-commit, pre-push, CI}, cells marked present or deliberately-absent-with-reason, short enough that someone will actually keep it updated. Prior inventories (TASK-030.05's "Gates that actually exist", TASK-060's own description) went stale within two tickets because nobody wrote the whole surface down at once.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Shape

Three new `pre-push.commands` entries, one caveat comment, one prose accuracy pass, one table in the notes. Depends on .02 and .03 because they change the same two files and the same claims about them; land last so the doc rewrite sees the final gate set.

## Step 1 - add the free items to pre-push

Mirror CI exactly (same commands, same order relative to each other):

    dump-reassemble-selftest:
      run: cargo run -p asperitas-logging --example dump_reassemble -- --selftest
    clippy-pod-hw:
      run: cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings
    firmware-cross-compile-rtt:
      run: cd firmware && cargo build --release --no-default-features --features "seed3 log-defmt"

Local warm cost measured 2026-09-12: 0 s, 1 s, 1 s. Do not use `root: "firmware/"` on the RTT entry for the reason given in .03 (path filtering plus lefthook's skip-on-empty-set makes a gate skippable); do not touch the existing `firmware-cross-compile` job.

## Step 2 - record the priced exception

Add a comment block above the pre-push commands stating, with the numbers, that exactly one CI check is deliberately absent here:

- `cargo test --workspace --features asperitas-pod/pod-hw` - 67 s local warm, more than every other pre-push command combined, and it re-runs the whole host suite under one non-default feature flag. Its compile-time half *is* here (`clippy-pod-hw`), so what stays remote is runtime coverage of `pod-hw` code paths.
- Why that is acceptable rather than lazy: CI is the authority for it, TASK-018.01's fixup made the same split (CI-only, commit `c44b9c1`), and the loop that generates most commits here never pushes, so growing this hook buys latency almost nowhere it runs. If that changes - if pushes become routine or the loop starts pushing - revisit with fresh numbers, and say so in the comment so the caveat has an expiry condition instead of being permanent folklore.

Paste the same reasoning into this ticket's notes: that is what TASK-060 AC #3 actually requires.

## Step 3 - make the prose match reality

`backlog/docs/doc-001 - Asperitas-Project-Plan.md` describes gates that do not exist and denies ones that do. Read each of these against the final `lefthook.yml` + `ci.yml` and correct:

- `:223-228` - "pre-push - full `fmt` + `clippy -D warnings` across both workspaces, full test suite, and a `--target thumbv7em-none-eabihf` build". After .02/.03 the both-workspaces claim becomes true for fmt/clippy; the test-suite and build halves need the pod-hw caveat named.
- `:303` - risk row "Two-workspace friction | Low | Pre-push and CI cross-compile explicitly, so the firmware can't rot unnoticed". This has been false since before this ticket (TASK-044's drift, then `rig.rs`); rewrite it to name the actual mechanism now in place, and keep the honest residual: firmware *docs* are still ungated (`cargo doc` covers `crates/*` only, per TASK-049's note), and pedantic lints remain unadopted.
- `:33` ("CI | `lefthook` locally **and** GitHub Actions") and `:117` ("the cost is `cargo test` at the root not covering firmware; the pre-push hook and CI compensate by cross-compiling explicitly") need the same read-through.

Do not let `scripts/check-doc-artifact-names.sh` surprise you: it scans `README.md` and `docs/**/*.md` only, so `backlog/docs/` is outside it - but any `.bin` filename you type there must still be a real one (`blinky.bin ledtest.bin main.bin panictest.bin podtest.bin rig.bin`, `capture.bin` allowlisted), because the next person reads it as truth even though no gate checks it.

## Step 4 - the table

End with a matrix in the notes: rows = every check, columns = pre-commit / pre-push / CI, cells = present or deliberately absent-with-reason. Every prior inventory in this repo went stale within two tickets precisely because nobody wrote the whole surface down at once; TASK-030.05's "Gates that actually exist" section and TASK-060's own description are both now history rather than fact. Keep the table short enough to survive contact - check names and cost class, not commentary.

## Cost summary to record

pre-push warm, local, before: ~70 s (of which 66 s is `cargo test --workspace`). After .02/.03/.04: ~75 s (+1 s firmware fmt, +3 s firmware clippy both sets, +2 s the three items above). Cold, add roughly 40 s for the two cross-clippy runs on a fresh target dir. Label all of it local, per .03 Step 3.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Executed 2026-09-13 on top of TASK-060.03 (commit 413e17a), direnv-loaded nix develop .#default, rust 1.97.1, aarch64-darwin. All figures LOCAL, warm target dir.

AC #1 - pre-push gained three commands mirroring CI verbatim, visible in 'lefthook dump' and executed by a real 'lefthook run pre-push --force': dump-reassemble-selftest 0.55 s rc 0, clippy-pod-hw 0.22 s rc 0, firmware-cross-compile-rtt 0.13 s rc 0. Individually timed at 0.69 s / 0.38 s / 0.50 s when each had to relink something. The RTT entry uses 'cd firmware && ...' with no root: key, so nothing can skip it by touching files outside firmware/; the existing firmware-cross-compile job was left alone.

Hook cost, same machine, two full runs of 'lefthook run pre-push --force' either side of the change: 72.6 s before, 72.5 s after, with cargo test --workspace at 66.9 s and 66.5 s respectively. The three additions are worth ~0.9 s warm, which is inside the run-to-run noise of the suite they joined - so the honest statement is that pre-push got its missing coverage for free, not that it got slower by a measurable amount. Cold, add roughly 20 s per cross-clippy run on a fresh target dir (TASK-060.03 measured 20.4 s).

AC #2 - the one remaining divergence, recorded twice. In lefthook.yml as a comment block above pre-push.commands, and here: cargo test --workspace --features asperitas-pod/pod-hw stays CI-only. Price 67 s local warm, more than every other pre-push command combined, spent re-running the whole host suite under one non-default feature flag whose compile-time half (clippy-pod-hw) IS on the hook. Acceptable because CI is the authority for that check, TASK-018.01's fixup made the same split deliberately (CI-only, commit c44b9c1), and the autonomous loop that writes most commits here never pushes - ralph touches git via rev-parse, log and rebase --autosquash only, so pre-commit is where its work actually gets gated and this hook executes rarely. Reopen condition written into the comment: if pushes become routine or the loop starts pushing, re-measure and revisit rather than inheriting today's answer.

AC #3 - backlog/docs/doc-001 read against the final lefthook.yml + ci.yml and corrected in four places. :33's CI row now says pre-commit is the hook that runs on every commit and that CI is pre-push plus one priced check. :113-117 no longer claims the pre-push hook and CI 'compensate by cross-compiling explicitly' as the whole story: it names what the exclusion actually costs (fmt, clippy, doc and test at the root all stop at it), which second invocations now exist, and that tests remain the genuine gap because a no_std target has no harness to link. :223-230 rewritten to describe both hooks as they now are, with measured costs and the no-root: rule stated where a reader would copy from. :234's 'GitHub Actions re-runs the pre-push set. Same checks' became the accurate superset statement with the one exception named and priced. :303's risk row 'Pre-push and CI cross-compile explicitly, so the firmware can't rot unnoticed' - false since TASK-044 - now names the mechanism actually in place and keeps the residuals: firmware docs are still ungated (cargo doc covers crates/* only), pedantic clippy is unadopted, and the pod-hw test is CI-only. scripts/check-doc-artifact-names.sh exits 0 after the edit; it scans README.md and docs/**/*.md, so backlog/docs is outside it, and no image filename was introduced there anyway.

AC #4 - the gate matrix, present (Y) or deliberately absent (reason):

| check                                      | pre-commit | pre-push | CI |
|--------------------------------------------|-----------|----------|----|
| cargo fmt --all --check (host)             | Y         | Y        | Y  |
| cargo fmt --manifest-path firmware (firm)  | Y         | Y        | Y  |
| clippy --workspace --all-targets           | Y         | Y        | Y  |
| clippy asperitas-logging log-usb           | Y         | Y        | Y  |
| clippy asperitas-logging log-defmt         | Y         | Y        | Y  |
| clippy firmware --bins (console cfg)       | Y         | Y        | Y  |
| clippy firmware --bins (RTT-only cfg)      | Y         | Y        | Y  |
| clippy --features asperitas-pod/pod-hw     | absent: added to pre-push/CI by this leaf; whether the hook sets should hold the same list is TASK-061's decision, not this one's | Y | Y |
| cargo doc (workspace)                      | absent: cost, ~1.5 s x 2 and rarely the thing a commit breaks | Y | Y |
| cargo doc --all-features                   | absent: same reason | Y | Y |
| cargo test --workspace                     | absent: 67 s on the hook the loop runs every commit | Y | Y |
| cargo test --features asperitas-pod/pod-hw | absent: priced, see AC #2 | absent: PRICED, 67 s, see AC #2 | Y |
| dump_reassemble --selftest                 | absent: host runtime check, belongs with the suite | Y | Y |
| docs artifact names (make -n based)        | Y         | Y        | Y  |
| firmware build --features seed3            | absent: builds don't belong in a per-commit hook | Y | Y |
| firmware build RTT-only                    | absent: same reason | Y | Y |

Nothing in this table is silent: every absent cell carries a reason, and the only absence that is a divergence between pre-push and CI is the pod-hw test, priced in both places.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
pre-push now matches CI except for one priced check. Added dump_reassemble --selftest (0.55 s), clippy-pod-hw (0.22 s) and the RTT-only cross-compile (0.13 s, no root: key so it cannot skip itself); full-hook cost measured either side of the change at 72.6 s and 72.5 s warm, since cargo test --workspace dominates at 67 s either way. The pod-hw test stays CI-only and says so twice - in lefthook.yml with its price, precedent (TASK-018.01, c44b9c1) and reopen condition, and in these notes. doc-001's four stale gate claims rewritten against the final files, residuals named rather than glossed, and the whole gate surface written down once as a check x hook matrix with a reason in every absent cell.
<!-- SECTION:FINAL_SUMMARY:END -->
