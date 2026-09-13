---
id: TASK-064
title: 'Give CI a cargo cache, so a push does not pay for seventeen cold builds'
status: To Do
assignee:
  - '@human'
created_date: '2026-09-13 05:36'
labels:
  - planned
dependencies:
  - TASK-061
references:
  - .github/workflows/ci.yml
  - scripts/gates.sh
priority: medium
type: chore
ordinal: 102800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-061 leaves CI running one step, `nix develop .#default --command bash scripts/gates.sh ci`, with no build cache anywhere in the workflow: no actions/cache, no Swatinem/rust-cache, no sccache. Every push therefore compiles the host workspace, the firmware workspace, both firmware cfg sets, the pod-hw feature variants and the test suite twice, from cold, having first installed a nix flake.

The local figure does not describe that. The same tier costs 140 s warm here, of which 132.8 s is the two `cargo test` invocations and every other gate rounds to 0-2 s - because everything is already fingerprinted. A runner starts with nothing, so none of those numbers transfer, and nobody has ever observed one.

Named out of scope by TASK-061's plan as worth its own ticket, with prior art surveyed there: Swatinem/rust-cache handles multiple workspaces (`workspaces: ".\nfirmware -> target"`) and nix shells, or `RUSTC_WRAPPER=sccache`. This is also the prerequisite for any thought of fanning the gate list out into one job per gate - seventeen jobs against no cache means seventeen cold builds, so that option cannot even be priced until this exists.

Split because the win cannot be measured locally: writing the keys is agent work, watching two runs and recording the delta is not. The parent inherits the strictest assignee and so is @human.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Parent umbrella: both subtasks are done.
<!-- AC:END -->
