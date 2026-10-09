---
id: TASK-064.02
title: 'Measure the cache on a runner, cold run against warm run'
status: Done
assignee:
  - '@agent'
created_date: '2026-09-13 05:37'
updated_date: '2026-10-09 14:13'
labels:
  - planned
dependencies:
  - TASK-063
  - TASK-064.03
references:
  - .github/workflows/ci.yml
  - scripts/gates.sh
parent_task_id: TASK-064
priority: medium
type: chore
ordinal: 104800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-064's human half. Depends on TASK-063 only because that ticket owns the pending push of a main that sits ~90 commits ahead of origin; if the push happens for its own sake, measure here. Same reason it cannot be an agent's: observing GitHub execute anything is outside this machine.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Push twice, or once with the cache empty and once warm (a cold key does the first by itself), and record both run URLs. Owner confirmed 2026-10-09 that agents may push main; the pushes carry only the ticket-note commits, nothing that changes the build, and run 37925833149 (the first run after TASK-064.01) may serve as the cold run if its cache was empty
- [x] #2 Record the total job wall time for each run, and the per-gate '--- N.NNs' lines scripts/gates.sh ci prints (gh run view --log), so the question 'did the cache work' is answered by gate rather than one aggregate
- [x] #3 Say plainly whether the second run actually reused artifacts - a warm run that recompiles everything looks exactly like success in a green check mark. Evidence: the rust-cache step's restore log and the per-gate times
- [x] #4 Post the two numbers where the local figures live, doc-001 section 5's CI discussion, labelled as runner-side
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 5e57f80

Approach: single ticket, no sub-tasks; all steps are gh CLI observation plus one doc edit.
1. Cold run: gh run view 37925833149 (17m13s, first run after TASK-064.01). Confirm via 'gh run view 37925833149 --log' that the rust-cache step printed no 'Restored from cache' full match (empty cache). If it did restore, trigger a cold run instead (bump cache key / gh cache delete) .
2. Warm run: push a ticket-note-only commit to main (owner approved agents pushing main; nothing build-changing), wait with 'gh run watch', get its id via 'gh run list'.
3. For both runs capture: total job wall time (gh run view --json jobs for startedAt/completedAt), per-gate '--- N.NNs' lines from scripts/gates.sh ci in the log, and per comment #1: Cache Key line, Environment considered Rust Versions lines, 'Restored from cache key ... full match: true', plus the 'Toolchain identity' step's 3 lines (must show /nix/store paths, not /home/runner/.cargo/bin; if not, reopen TASK-064.01 rather than just recording).
4. Judge plainly whether run 2 reused artifacts: compare per-gate times (compile-heavy gates should drop sharply) and rust-cache restore/save logs.
5. Add a runner-side cold vs warm table to doc-001 section 5 '### CI' (around line 403), labelled runner-side, with both run URLs.
6. Tick ACs 1-4 with evidence, final summary, mark Done.
Verification: ACs 1-4 each backed by pasted log lines/URLs. Risk: pushing main sits ~90 commits ahead may trigger other work; if the cold run's cache was not empty, a cold measurement needs a cache purge.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-09: Reassigned @human -> @agent. gh can run, list and read Actions logs, and the owner allows pushing main. TASK-063 is Done.

2026-10-09 (agent): Blocked by TASK-064.03. Cold run 37925833149 (job wall 17m10s: 11:46:48-12:03:58; gates.sh ci step 6m13s; 'Toolchain identity' step 10m41s building the nix toolchain) shows the cache never engaged: rust-cache logged '##[error]Command failed: nix develop .#default --command rustup run sync hooks: (pre-commit, pre-push) rustc -vV' / 'error: toolchain sync is not installed' because flake.nix shellHook's 'lefthook install' prints to stdout and corrupts cmd-format output. No Cache Key line, no restore, no save. Toolchain identity step: rustc is /nix/store/...-rust-default-1.97.1, but rustup is reachable at /home/runner/.cargo/bin/rustup. Per-gate heavy lines of that run: 36.51s, 123.90s, 131.10s. A warm run now would measure nothing, so do not push for measurement until TASK-064.03 lands; then the first post-fix run is the cold run and a second push the warm run. Next step: implement TASK-064.03, then resume this ticket with the plan steps.

2026-10-09: Unblocked. TASK-064.03 is Done; cold run with a working cache key is 37938450034 (14m54s, 'No cache found'). The warm run is the next push.

2026-10-09: Cold run 37938450034 (https://github.com/jeffutter/asperitas/actions/runs/37938450034): 'No cache found', job 13:39:52-13:54:46 = 14m54s, gates.sh ci sum 298.6 s. Warm run 37940574487 (https://github.com/jeffutter/asperitas/actions/runs/37940574487, a ticket-note/CLAUDE.md push only): rust-cache logged 'Cache restored successfully', ~419 MB, key v0-rust-nix-...-check-Linux-x64, job 13:57:16-14:13:09 = 15m53s, gates.sh ci sum 239.9 s. Per gate cold -> warm: cargo clippy 8.9->1.9 s, clippy log-usb 15.5->1.5, clippy log-defmt 6.2->0.6, clippy pod-hw 4.0->1.3, dump_reassemble selftest 5.7->0.7, firmware rig build stim-ess 30.4->2.2, firmware clippy all bins 13.5->0.8, cargo test 96.8->109.2, cargo test pod-hw 102.9->107.2. Reuse verdict: yes for compile-bound gates (artifacts reused), no for the two cargo test runs (run time, and workspace test binaries are not cached), and total job wall time did not improve because the nix dev-shell step (558 s cold, 683 s warm) dominates and rust-cache does not cover it. Recorded in doc-001 section 5. A cache for the nix store (e.g. cachix or nix-community/cache-nix-action) is the next lever and is not filed.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-14 10:33
---
Planning TASK-064.01 added one thing worth pasting alongside the timings this ticket already asks for, because wall time alone cannot tell 'the cache missed' from 'the cache was never wired to the toolchain'. From the rust-cache step of BOTH runs, quote: (1) the `Cache Configuration` group's `Cache Key:` line, (2) the `.. Environment considered:` -> `Rust Versions:` lines -- more than one version listed means the runner image's own rustup answered and the key contains a toolchain nobody installed here, and (3) whether the second run printed `Restored from cache key "..." full match: true`. The workflow also gains a step named 'Toolchain identity the cache key is built from'; its three lines record which rustc/cargo the nix shell resolves and whether a stray rustup is reachable inside it. If those say /home/runner/.cargo/bin rather than a /nix/store path, cmd-format is not doing what TASK-064.01's plan claims and this ticket should reopen it rather than just recording a slow run.
---
<!-- COMMENTS:END -->
