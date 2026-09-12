---
id: TASK-060
title: >-
  Firmware is invisible to every fmt and lint gate; rig.rs has already drifted
  again
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-09-12 21:26'
updated_date: '2026-09-12 21:39'
labels: []
dependencies: []
references:
  - .github/workflows/ci.yml
  - lefthook.yml
  - 'firmware/Makefile:251-255'
priority: medium
type: chore
ordinal: 92800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Discovered while checking what TASK-057's host gates would actually cover. `ci.yml` runs `cargo fmt --all --check`
(:24) and four clippy invocations (:27,32,38,41) - all at the repo root, where root `Cargo.toml:5` declares
`exclude = ["firmware"]`. `firmware/` is its own workspace (own `Cargo.lock`, own `target/`, single member
`asperitas-firmware`), so no formatting or lint gate in CI or lefthook ever looks at firmware code, and the pass is
silent about it.

Reproduced read-only 2026-09-12 inside `nix develop .#default`:

    cargo fmt --all --check           -> rc 0
    cd firmware && cargo fmt --check  -> rc 1, real diffs in firmware/src/bin/rig.rs:21, :28, :445

`firmware/Makefile:251-255` already states the situation plainly - "Neither CI nor lefthook invokes make inside
firmware/, so this target [`make clippy`] is the only place firmware clippy runs" - but nothing enforces running it,
and the drift proves it: TASK-044 cleared pre-existing fmt drift in `main.rs` and `podtest.rs` on 2026-09-10 and closed
Done; `rig.rs` arrived afterwards (TASK-038.03.02.03) and drifted immediately, invisibly.

Two separable holes, both needing a decision rather than a mechanical fix, which is why this is unplanned:

1. Formatting. Either add a firmware fmt step (needs cwd handling, the pattern already used by `lefthook.yml:60-63`'s
   `firmware-cross-compile`), or fold firmware into the root workspace so `--all` means all. The second is tempting and
   probably wrong: keeping firmware out of the root workspace is what stops host tooling from trying to build `no_std`
   code for the host.
2. Lints. Cross clippy works today without any sysroot flag because `flake.nix:24-32` folds the
   `thumbv7em-none-eabihf` std into the same sysroot as `clippy-driver` (that was TASK-009's whole point), so
   `cd firmware && cargo clippy --release --features seed3 --bin main -- -D warnings` is viable in CI as-is. Unknowns
   worth measuring first: how long each of the six bin targets takes under clippy, and whether any of them is currently
   red - if `rig.rs` drifted fmt-wise it may well carry warnings too, and the ticket should clear them before turning
   the gate on.

Also worth folding in while the gates are open: lefthook's pre-push is missing three things CI has (the `pod-hw`
clippy/test pair, `dump_reassemble --selftest`, the RTT-only cross-compile), so local-green is not CI-green. Decide
whether to close that gap here or leave pre-push deliberately cheap.

Depends on nothing. Do not create a firmware clippy target - `make clippy` (`firmware/Makefile:252-255`) already exists;
this ticket is about calling it from a gate.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A gate runs `cargo fmt --check` against the firmware workspace and is red on a planted diff today - `firmware/src/bin/rig.rs:21,:28,:445` are already drifted while root `cargo fmt --all --check` exits 0 - and green once they are cleared.
- [ ] #2 A gate runs cross-target clippy over the firmware bin targets with `-D warnings`, with the measured wall time recorded, and whatever warnings it surfaces are either cleared in the same change or filed as their own ticket rather than silenced.
- [ ] #3 The decision on lefthook's pre-push gap (no pod-hw clippy/test, no `dump_reassemble --selftest`, no RTT-only cross-compile) is recorded one way or the other in the ticket notes, not left implicit.
- [ ] #4 Host gates green in `nix develop .#default`, verbatim from ci.yml.
<!-- AC:END -->
