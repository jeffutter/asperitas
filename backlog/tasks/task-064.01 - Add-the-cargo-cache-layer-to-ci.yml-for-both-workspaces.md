---
id: TASK-064.01
title: Add the cargo cache layer to ci.yml for both workspaces
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-13 05:36'
updated_date: '2026-09-13 05:38'
labels: []
dependencies:
  - TASK-061
references:
  - .github/workflows/ci.yml
  - scripts/gates.sh
parent_task_id: TASK-064
priority: medium
type: chore
ordinal: 103800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-064's implementation half. Nothing here needs a board or ears; everything here needs a runner to prove, which is TASK-064.02 and stays human-owned. Do not mark the parent done from this ticket.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 ci.yml gains one cache layer, and its keys cover BOTH target directories - the host workspace's `target` and firmware's own `firmware/target` - because root Cargo.toml excludes firmware/, so a single-workspace key silently caches half the build and reads as green while the cross-builds stay cold.
- [ ] #2 The key cannot turn a stale artifact into a false pass: it is restored into the same path cargo would use, and a run whose inputs changed must not reuse fingerprints keyed on something narrower than Cargo.lock plus the sources. State in a comment what the key is made of and why, and name the failure mode a wrong key produces - clippy or test saying "finished" having compiled nothing.
- [ ] #3 Works inside the nix shell the step already enters (`nix develop .#default --command bash scripts/gates.sh ci`), where cargo comes from the flake rather than a setup-rust action, so any toolchain-pinning assumption inherited from upstream examples is checked rather than copied. Record which of Swatinem/rust-cache (multi-workspace support, nix-shell support) or `RUSTC_WRAPPER=sccache` was chosen and why, against the alternative.
- [ ] #4 Local behaviour is unchanged: `scripts/gates.sh ci` still exits 0 with the same 17 gates, and `--list` is untouched - the cache is a runner concern and must not leak a single gate or env var into the shared definition.
<!-- AC:END -->
