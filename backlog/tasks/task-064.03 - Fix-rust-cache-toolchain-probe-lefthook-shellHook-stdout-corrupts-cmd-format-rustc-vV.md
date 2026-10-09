---
id: TASK-064.03
title: >-
  Fix rust-cache toolchain probe: lefthook shellHook stdout corrupts cmd-format
  rustc -vV
status: Done
assignee:
  - '@agent'
created_date: '2026-10-09 13:27'
updated_date: '2026-10-09 13:55'
labels:
  - planned
dependencies: []
references:
  - .github/workflows/ci.yml
  - flake.nix
parent_task_id: TASK-064
ordinal: 148800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Run 37925833149 (first run after TASK-064.01) shows the cache never engaged. The rust-cache step logged '##[error]Command failed: nix develop .#default --command rustup run sync hooks: (pre-commit, pre-push) rustc -vV' then 'error: toolchain sync is not installed'. Cause: flake.nix shellHook runs 'lefthook install', which prints 'sync hooks: ...' on stdout; rust-cache's cmd-format wraps 'rustc -vV' in 'nix develop --command', so that line is prepended to the output and the action falls through to 'rustup run <first token>'. No Cache Key, no 'Restored from cache', and the Post step produced no save. Fix: make the shell entry silent on stdout (e.g. 'lefthook install >&2' or quiet flag) so cmd-format output is clean, then confirm on a runner that the rust-cache log prints Cache Key and Rust Versions with the nix rustc. Also note the 'Toolchain identity' step shows rustup at /home/runner/.cargo/bin/rustup reachable inside the nix shell (rustc itself resolves to /nix/store), and that step took 10m41s building the toolchain with no nix store cache - worth a decision whether to cache the nix store too.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 flake.nix shellHook emits nothing on stdout, so 'nix develop .#default --command rustc -vV' prints only rustc's output (check locally)
- [x] #2 scripts or a selftest guards that 'nix develop .#default --command true' stdout is empty
- [x] #3 HUMAN-free: a pushed CI run's rust-cache step log shows a Cache Key line and Rust Versions listing the nix-provided rustc, with no ##[error] annotation
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 96f1a41
Approach: (1) flake.nix shellHook line 72: change 'lefthook install' to 'lefthook install >&2' so stdout stays empty (stderr is not captured by rust-cache's cmd-format probe). Also consider '|| true' is NOT added; keep failures visible.
(2) AC#2 guard: add a small script (e.g. scripts/check-devshell-stdout.sh) running 'nix develop .#default --command true 2>/dev/null' and failing if stdout is non-empty; wire it into the existing gate mechanism in scripts/gates.sh / lefthook.yml following how check-doc-artifact-names.sh is registered (look at gates.sh for the gate definition format and keep it out of the fast tier if nix develop is slow).
(3) Verify locally: 'nix develop .#default --command rustc -vV' prints only rustc output.
(4) AC#3 needs a pushed CI run; an agent cannot push unattended, so leave AC#3 for the follow-up measurement ticket TASK-064.02 and note this in the final summary. Out of scope: nix store caching decision - mention in final notes only.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
flake.nix shellHook now 'lefthook install >&2'. Verified locally: 'nix develop .#default --command rustc -vV' stdout is only rustc output. Added scripts/check-devshell-stdout.sh and a CI step ahead of rust-cache. Deliberately NOT in scripts/gates.sh: a new gate line forces a gate-costs ledger measurement and regenerated docs. AC#3 needs a pushed CI run; left unchecked, to be confirmed by TASK-064.02's measurement. Nix store caching decision still open.

2026-10-09: AC #3 verified by the owner-authorised push of 8ecbb48. CI run 37938450034 (https://github.com/jeffutter/asperitas/actions/runs/37938450034) succeeded. The Swatinem/rust-cache step now logs 'Cache Key:' and 'Rust Versions:' with no ##[error] annotation, then 'No cache found.' (cold, as expected for the first keyed run); the post step saved the cache in 20 s. The nix rustc is rust-default-1.97.1. Job wall time 14m54s (13:39:52 to 13:54:46), against 17m10s for run 37925833149 where the cache never engaged; the toolchain-identity step took 2 s there is no 10 min nix wait in the cache-key path.
<!-- SECTION:NOTES:END -->
