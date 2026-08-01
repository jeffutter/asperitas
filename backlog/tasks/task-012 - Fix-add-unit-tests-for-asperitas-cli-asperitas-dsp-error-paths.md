---
id: TASK-012
title: 'Fix: add unit tests for asperitas-cli/asperitas-dsp error paths'
status: Dev Ready
assignee: []
created_date: '2026-08-01 19:55'
updated_date: '2026-08-01 21:32'
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
## Implementation Plan for TASK-012

### Overview

Add unit tests covering error paths in four files: `wav_io.rs`, `process.rs`, `gain.rs`, and `filter.rs`. All tests are inline `#[cfg(test)] mod tests` blocks — no new test files. Adds `tempfile` as a dev-dependency for `asperitas-cli` to create temporary WAV fixtures.

### Step 1: Add `tempfile` dev-dependency to `asperitas-cli`

**File:** `crates/asperitas-cli/Cargo.toml`

`tempfile` 3.27.0 is already in `Cargo.lock` (transitive via clap). Adding it explicitly avoids manual temp-dir path management and gives auto-cleanup on `Drop`.

```toml
[dev-dependencies]
tempfile = "3"
```

### Step 2: Add error-path tests to `wav_io.rs`

**File:** `crates/asperitas-cli/src/wav_io.rs` — append a `#[cfg(test)] mod tests` block at end of file.

Helper function `write_test_wav(spec)` creates a minimal 2-sample stereo WAV via `hound::WavWriter` into a `NamedTempFile`, returns the path. Three tests exercise each branch of `validate_spec`:

| Test | WavSpec variation | Expected error substring |
|---|---|---|
| `read_wav_rejects_wrong_bit_depth` | `bits_per_sample: 8` | `"16"` |
| `read_wav_rejects_mono` | `channels: 1` | `"stereo"` |
| `read_wav_rejects_float_format` | `sample_format: Float` | `"Int"` |

Each test calls `read_wav(&path)`, asserts `is_err()`, then checks `err.contains(...)`.

### Step 3: Add error-path test to `process.rs`

**File:** `crates/asperitas-cli/src/process.rs` — append a `#[cfg(test)] mod tests` block.

One test: `parse_param_pairs_rejects_malformed_arg` calls `parse_param_pairs(&["not_a_kv_pair".to_string()])` and asserts the error contains `"key=value"`. No visibility change needed — `fn` is private but same-file `mod tests` can call it.

### Step 4: Add error-path tests to `gain.rs`

**File:** `crates/asperitas-dsp/src/gain.rs` — append a `#[cfg(all(test, feature = "std"))] mod tests` block.

Two tests:

| Test | Input | Expected error substring |
|---|---|---|
| `gain_parse_rejects_unknown_key` | `[("bogus_key", "1.0")]` | `"unknown parameter"` |
| `gain_parse_rejects_non_numeric_value` | `[("gain_db", "not_a_number")]` | `"invalid value"` |

Feature-gated because `parse_params_from_cli` is behind `#[cfg(feature = "std")]`.

### Step 5: Add error-path tests to `filter.rs`

**File:** `crates/asperitas-dsp/src/filter.rs` — same pattern as step 4.

Two tests:

| Test | Input | Expected error substring |
|---|---|---|
| `filter_parse_rejects_unknown_key` | `[("bogus_key", "1000.0")]` | `"unknown parameter"` |
| `filter_parse_rejects_non_numeric_value` | `[("cutoff_hz", "not_a_number")]` | `"invalid value"` |

### Step 6: Verify tests pass

```bash
nix develop -c cargo test -p asperitas-cli -p asperitas-dsp --all-features
```

The `--all-features` flag ensures `asperitas-dsp` builds with `std` enabled so its cfg-gated test modules compile.

### Step 7: Run clippy and fmt

```bash
nix develop -c cargo clippy -p asperitas-cli -p asperitas-dsp --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check -p asperitas-cli -p asperitas-dsp
```

Fix any warnings before committing.

### Risk assessment

**Low risk.** Purely additive — only adds test code and one dev-dependency. No production code changes. Each test exercises exactly one error branch, keeping tests focused and maintainable.
<!-- SECTION:PLAN:END -->
