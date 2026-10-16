---
id: TASK-100
title: 'Tooling: speed up the host test run and narrow the pod-hw gate'
status: Done
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-16 22:13'
labels:
  - review-2026-10
  - tooling
  - planned
dependencies: []
priority: high
type: chore
ordinal: 202800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
cargo test -p asperitas-measure --lib took 101.6 s in debug and 2.1 s in release. Experiment: profile.dev.package.{rustfft,ruzstd,asperitas-measure,asperitas-dsp,asperitas-logging}.opt-level=3 brought it to 4.4 s with 51 tests passing (the cost is fixture zstd decode plus FFT in debug). Overflow checks and debug-assertions stay on; firmware/ is a separate workspace. Separately the cargo-test-pod-hw gate (gates.sh:536-545) re-runs the whole host suite (~179 s) to cover one crate, but asperitas-pod/pod-hw adds no tests (compile coverage is clippy-pod-hw, gates.sh:372); no other crate depends on asperitas-pod. Its reopen condition in the comment ('if pushes become routine') is met, and 'main sits ~91 commits ahead' (gates.sh:48; doc-001:333,427) is false today.

Change: add the [profile.dev.package.*] opt-level=3 entries to the root Cargo.toml; delete cargo-test-pod-hw or make it cargo test -p asperitas-pod --features pod-hw; correct the stale comments.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 cargo test --workspace wall time is measured before and after and recorded in the ticket
- [x] #2 The pod-hw gate no longer re-runs the whole workspace
- [x] #3 gates.sh push tier is green
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-100. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Measured cargo test --workspace (nix develop .#default, warm): 188 s before, 137 s after the dev-profile opt-level=3 entries (rustfft, ruzstd, asperitas-measure, asperitas-dsp, asperitas-logging). cargo-test-pod-hw now runs cargo test -p asperitas-pod --features pod-hw (stays in ci tier); ledger refreshed. Push tier green: 34 gates, 117.2 s. Removed false '~91 commits ahead' claims from gates.sh, doc-001, ci.yml and rewrote the pod-hw gate comment.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added dev-profile opt-level=3 for the FFT/zstd-heavy crates (workspace test 188 s -> 137 s), narrowed the pod-hw gate to its own crate, corrected stale comments and refreshed the gate-cost ledger; push tier green.
<!-- SECTION:FINAL_SUMMARY:END -->
