---
id: TASK-072
title: >-
  Cache the nix dev-shell build in CI, which is now 9 to 11 minutes of a 15
  minute job
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-09 14:19'
labels: []
dependencies: []
priority: medium
type: chore
ordinal: 149800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-064.02 measured the cache TASK-064 added. Swatinem/rust-cache works for the compile-bound gates (clippy 4-15 s down to 0.6-1.9 s, the stim-ess rig build 30 s down to 2 s) but total job wall time did not improve: 14m54s cold, 15m53s warm (runs 37938450034 and 37940574487). The step that dominates is entering the nix dev shell, 558 s cold and 683 s warm, which rust-cache does not cover. That is the next lever, and nothing in the repo prices or attempts it.

Candidates, to be chosen on measurement and not by taste: a nix store cache action (nix-community/cache-nix-action), a binary cache such as cachix or an attic instance, or trimming what the default dev shell has to build or fetch (the patched probe-rs from nix/probe-rs-cortex-m-reset-catch.patch is a suspect, since CI never uses a probe and a runner has to build it from source). Check first what the 9-11 minutes actually is: fetch from cache.nixos.org versus building derivations locally; the step log with nix develop -L will say. If the time is the patched probe-rs build, the cheapest fix may be a CI-only dev shell without it, but the shell CI uses must still be the one whose toolchain the cache key and the elf-check provenance describe, so state how that stays true.

Pushing main and reading CI with gh are agent work (owner confirmed 2026-10-09); each measurement needs a push.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The 9-11 minute dev-shell step is broken down from a CI log into what it spends time on (substituting from a cache versus building derivations locally), with the derivations that were built named
- [ ] #2 One change is chosen from the candidates with the measurement that justified it and the rejected ones recorded, and implemented in the workflow or flake
- [ ] #3 A cold run and a warm run on the changed workflow are compared against 14m54s and 15m53s from TASK-064.02, with run URLs and the dev-shell step time for each; the warm job wall time is reported as a number whether or not it improved
- [ ] #4 The toolchain CI gates with still matches what the rust-cache key and elf-check provenance describe, stated in the notes, with the guard from TASK-064.03 (dev shell entry prints nothing on stdout) still passing
- [ ] #5 The runner-side paragraph in doc-001 section 5 is updated with the new figures, each marked gate-costs:exempt where the cost-figure gate requires it
<!-- AC:END -->
