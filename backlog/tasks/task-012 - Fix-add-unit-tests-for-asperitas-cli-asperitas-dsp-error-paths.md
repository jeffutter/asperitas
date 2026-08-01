---
id: TASK-012
title: 'Fix: add unit tests for asperitas-cli/asperitas-dsp error paths'
status: To Do
assignee: []
created_date: '2026-08-01 19:55'
updated_date: '2026-08-01 19:55'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): This is a Rust workspace with host crates (crates/asperitas-dsp, crates/asperitas-cli) and a separate embedded firmware/ workspace. ALL commands must run inside the Nix dev shell: either run 'direnv allow' once, or prefix every command with 'nix develop -c'. Work from the repository root unless told otherwise. Do not change pinned dependency versions.

1. crates/asperitas-cli/src/wav_io.rs: add a `#[cfg(test)] mod tests` block. Write a small helper that writes a minimal WAV file to a temp path (use `std::env::temp_dir()` + a unique filename, or the `tempfile` crate if already a dependency — check Cargo.lock first before adding a new dependency) with a given `hound::WavSpec`, so tests can construct fixtures without needing files in the repo. Add three tests: (a) an 8-bit or 24-bit PCM WAV is rejected by `read_wav` with an error mentioning "16", (b) a mono (1-channel) WAV is rejected with an error mentioning "stereo" or "2", (c) a float-format WAV (`hound::SampleFormat::Float`) is rejected with an error mentioning "Int". Assert on `Result::is_err()` and that the error string contains the expected substring — do not assert exact wording beyond that.

2. crates/asperitas-cli/src/process.rs: add a `#[cfg(test)] mod tests` block with one test that calls `parse_param_pairs` (make it `pub(crate)` if it is not already visible to the test module — it currently is a private fn in the same file, so `mod tests` can call it directly) with input `vec!["not_a_kv_pair".to_string()]` and asserts the result is `Err` containing "key=value".

3. crates/asperitas-dsp/src/gain.rs: add a `#[cfg(all(test, feature = "std"))] mod tests` block (the parse function is std-gated) with two tests: `parse_params_from_cli` rejects `[("bogus_key".to_string(), "1.0".to_string())]` with an error mentioning "unknown parameter", and rejects `[("gain_db".to_string(), "not_a_number".to_string())]` with an error mentioning "invalid value".

4. crates/asperitas-dsp/src/filter.rs: same pattern as step 3, for `cutoff_hz`/`OnePoleLowPass::parse_params_from_cli`.

5. Run: nix develop -c cargo test -p asperitas-cli -p asperitas-dsp --all-features (the dsp crate's tests need the std feature enabled to compile the cfg-gated test modules; asperitas-cli already depends on asperitas-dsp with the std feature on, but running -p asperitas-dsp directly needs --features std or --all-features).

6. Run: nix develop -c cargo clippy -p asperitas-cli -p asperitas-dsp --all-targets --all-features -- -D warnings and fix any warnings.

7. Run: nix develop -c cargo fmt --check -p asperitas-cli -p asperitas-dsp (or cargo fmt if lefthook's fmt-check hook requires it) before committing.
<!-- SECTION:PLAN:END -->
