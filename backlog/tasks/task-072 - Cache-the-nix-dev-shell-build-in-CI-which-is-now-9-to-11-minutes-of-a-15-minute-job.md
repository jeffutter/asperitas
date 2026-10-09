---
id: TASK-072
title: >-
  Cache the nix dev-shell build in CI, which is now 9 to 11 minutes of a 15
  minute job
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-09 14:19'
updated_date: '2026-10-09 15:18'
labels:
  - planned
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
- [x] #1 The 9-11 minute dev-shell step is broken down from a CI log into what it spends time on (substituting from a cache versus building derivations locally), with the derivations that were built named
- [x] #2 One change is chosen from the candidates with the measurement that justified it and the rejected ones recorded, and implemented in the workflow or flake
- [x] #3 A cold run and a warm run on the changed workflow are compared against 14m54s and 15m53s from TASK-064.02, with run URLs and the dev-shell step time for each; the warm job wall time is reported as a number whether or not it improved
- [x] #4 The toolchain CI gates with still matches what the rust-cache key and elf-check provenance describe, stated in the notes, with the guard from TASK-064.03 (dev shell entry prints nothing on stdout) still passing
- [x] #5 The runner-side paragraph in doc-001 section 5 is updated with the new figures, each marked gate-costs:exempt where the cost-figure gate requires it
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by 990241b. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
AC1 (run 37945671555, nix develop .#default -L): 155 paths substituted from cache.nixos.org in about 40 s; one meaningful local build, probe-rs-tools-0.32.0 patched (14:39:15 to 14:49:05, 9m50s, of a 10m34s step), plus trivial rust-overlay wrapper drvs (rust-default, rustc, cargo etc, seconds). AC2: chosen = CI-only devShell 'ci' (flake.nix mkDevShell {withProbe=false}); no gate uses probe-rs (grep of scripts/gates.sh). Rejected: cache-nix-action / cachix / attic - they would cache a build no CI job needs, add a 10 GB-budget competitor to rust-cache or a secret/owner decision, and still pay restore time; with probe-rs gone the step is 50-57 s so there is little left to cache. AC3: cold (rust-cache key busted) run 37948852414: job 6m18s, dev shell step 57 s, gates 5m02s vs 14m54s. Warm run 37949893837: job 5m22s, dev shell step 50 s, gates 4m06s vs 15m53s - warm wall time 5m22s, improved by 10m31s. AC4: both shells share one rustToolchain and shellHook; locally rustc -vV sha1 identical for default and ci; on the runner the rust-default-1.97.1 store path (bvlih9g3019sjdhsr4mif06fi8qqm1pj) is the same before (run 37940574487) and after, and the rust-cache key (v0-rust-nix-2832c96d...-7d9baf81-123a858d) is unchanged, so it hit on the first run. scripts/check-devshell-stdout.sh now takes a shell name; CI passes ci and it passes; default also passes locally. elf-check provenance is produced by the toolchain, which is identical. Caveat: gate-costs.json environment still says .#default; the only difference is probe-rs, which no gate exercises. AC5: doc-001 section 5 runner paragraph updated, gate-costs check clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
CI now enters nix develop .#ci (dev shell without the from-source patched probe-rs, which was 9m50s of the 10m34s dev shell step). Dev shell step fell to about 50-57 s; job wall time fell from 14m54s cold / 15m53s warm to 6m18s cold / 5m22s warm. Toolchain and rust-cache key unchanged.
<!-- SECTION:FINAL_SUMMARY:END -->
