---
id: TASK-003.01
title: Write lefthook config and GitHub Actions workflow
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-08-01 05:56'
updated_date: '2026-08-01 16:40'
labels: []
dependencies: []
parent_task_id: TASK-003
type: chore
ordinal: 9000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Author the hook and CI configuration. No remote exists yet, so the Actions workflow cannot be exercised here — that is TASK-003.02.

pre-commit (fast): `cargo fmt --check`, clippy on changed crates.
pre-push (full, BOTH workspaces): fmt check, `clippy --all-targets -- -D warnings`, `cargo test`, and `cargo build --release --features seed3` in firmware/. The cross-compile check is the important one — without it the firmware silently rots while day-to-day work happens on host crates.

Install hooks from the flake shellHook so a fresh clone gets them with no manual step. Use the nix flake in CI so local and CI toolchains are identical.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `lefthook install` runs automatically on entering the nix shell
- [ ] #2 pre-commit rejects a commit containing misformatted Rust
- [ ] #3 pre-push rejects a push whose firmware workspace does not cross-compile
- [ ] #4 A GitHub Actions workflow exists running fmt, clippy, test, and the thumbv7em cross-compile, via the same nix flake
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation Plan

### Overview
Create three configuration artifacts that wire up local git hooks and CI to run the same consistency checks across both workspaces. The hooks prevent bad commits/pushes locally; CI catches what slips through (e.g., `--no-verify`).

### Files to create/modify (3 files, ~120 lines total)

#### 1. `lefthook.yml` (new file at repo root)

**pre-commit** (fast, host workspace only):
- `cargo fmt --all --check` — rejects misformatted Rust in the root workspace (firmware is excluded from it)
- `cargo clippy --workspace --all-targets -- -D warnings` — lints host crates; runs on all targets including tests so test-only code is checked too

**pre-push** (full, both workspaces):
- `cargo fmt --all --check` — root workspace
- `cargo clippy --workspace --all-targets -- -D warnings` — root workspace
- `cargo test --workspace` — root workspace (12 property tests in asperitas-dsp)
- `cargo build --release --features seed3` with `root: 'firmware/'` — **the critical check**; ensures firmware cross-compiles to thumbv7em-none-eabihf before any push lands

**No clippy on firmware**: The firmware workspace defaults to thumbv7em-none-eabihf via its `.cargo/config.toml`. Running `cargo clippy` there requires the embedded sysroot to be available to clippy-driver. The release build already validates compilation; adding clippy on firmware can be done later if the combined fenix toolchain fully supports it.

#### 2. `flake.nix` (modify existing — add shellHook)

Append `lefthook install` to a `shellHook` string on the `mkShell` call. This ensures a fresh clone gets hooks installed automatically when entering the nix shell. Current flake has no shellHook, so this is a simple addition:

```nix
shellHook = ''
  lefthook install
'';
```

#### 3. `.github/workflows/ci.yml` (new file)

**Trigger**: push and pull_request to any branch.

**Steps** (all inside `nix develop .#default --command ...`):
1. actions/checkout@v5 → cachix/install-nix-action@v31
2. `nix develop .#default --command "cargo fmt --all --check"`
3. `nix develop .#default --command "cargo clippy --workspace --all-targets -- -D warnings"`
4. `nix develop .#default --command "cargo test --workspace"`
5. `nix develop .#default --command "cd firmware && cargo build --release --features seed3"`

Uses ubuntu-latest runner. No caching needed initially (can be added later as optimization). Cannot be tested until TASK-003.02 creates the GitHub remote.

### Key decisions

- **Two-workspace awareness**: Root `Cargo.toml` has `exclude = ["firmware"]`, so `cargo <cmd> --workspace` at repo root never touches firmware. Firmware commands must explicitly `cd firmware` or use lefthook's `root` property.
- **CI mirrors pre-push exactly**: Same four checks, same order. Local hooks are faster because they use the cached nix shell; CI rebuilds the environment each run.
- **No parallelization in pre-push**: Sequential execution gives clear error ordering. If fmt fails, we don't waste time running clippy.
- **No sub-tickets**: All three files are configuration that must coexist. Partial states (e.g., lefthook.yml without shellHook) are incomplete and not independently verifiable.

### Verification commands

```bash
# Test lefthook install works
nix develop .#default --command "lefthook install"

# Verify pre-commit catches formatting issues
git commit --allow-empty -m "test"  # should pass if code is formatted
# Introduce a formatting violation and retry — should fail

# Verify pre-push firmware check (requires nix shell for embedded target)
nix develop .#default --command "cd firmware && cargo build --release --features seed3"

# Confirm CI workflow YAML is valid
nix develop .#default --command "yq '.' .github/workflows/ci.yml"
```
<!-- SECTION:PLAN:END -->
