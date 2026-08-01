---
id: TASK-012
title: 'Fix: add unit tests for asperitas-cli/asperitas-dsp error paths'
status: To Do
assignee: []
created_date: '2026-08-01 19:55'
labels:
  - review-followup
dependencies:
  - TASK-008.01
priority: high
type: task
ordinal: 100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while reviewing TASK-008.01 (crates/asperitas-cli/src/wav_io.rs, crates/asperitas-cli/src/process.rs, crates/asperitas-dsp/src/gain.rs, crates/asperitas-dsp/src/filter.rs). TASK-008.01 shipped six golden-file regression tests, all happy-path only (crates/asperitas-cli/tests/golden_tests.rs) — there is not a single test anywhere in the touched crates for the error branches the commit itself added: `wav_io::validate_spec` (wrong bit depth, wrong channel count, wrong sample format all return `Err`, none exercised), `process::parse_param_pairs` (malformed `key=value` string), and `Gain`/`OnePoleLowPass::parse_params_from_cli` (unknown key, unparseable numeric value). Correct axis: error-handling code with zero test coverage means a future refactor can silently change or break these error messages/behavior and nothing will catch it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 crates/asperitas-cli/src/wav_io.rs has unit tests covering validate_spec rejecting wrong bit depth, wrong channel count, and non-Int sample format
- [ ] #2 crates/asperitas-cli/src/process.rs has a unit test covering parse_param_pairs rejecting a malformed (non key=value) argument
- [ ] #3 crates/asperitas-dsp/src/gain.rs and crates/asperitas-dsp/src/filter.rs each have a unit test covering parse_params_from_cli rejecting an unknown key and a non-numeric value
- [ ] #4 nix develop -c cargo test -p asperitas-cli -p asperitas-dsp passes
- [ ] #5 nix develop -c cargo clippy -p asperitas-cli -p asperitas-dsp --all-targets -- -D warnings passes
<!-- AC:END -->
