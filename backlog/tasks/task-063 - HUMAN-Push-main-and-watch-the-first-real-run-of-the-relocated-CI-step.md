---
id: TASK-063
title: 'HUMAN: Push main and watch the first real run of the relocated CI step'
status: Done
assignee:
  - '@agent'
created_date: '2026-09-13 01:30'
updated_date: '2026-10-09 13:12'
labels: []
dependencies:
  - TASK-060
  - TASK-061
references:
  - .github/workflows/ci.yml
  - scripts/gates.sh
type: chore
ordinal: 99800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-060's integration pass found ci.yml could not parse: its whole check list sat inside an inline `bash -c '...'` string, so an apostrophe in a comment closes the quote and re-parses the rest of the file. Broken from 6d7d38a (2026-09-10) onward, seven comments deep by HEAD, invisible because `gh run list` puts the last CI execution at 2026-09-09 (success) and nothing has been pushed since.

Fixed by moving the list to .github/ci-steps.sh and reducing the workflow to one command line, with `bash -n .github/ci-steps.sh` now gating both hooks. Locally the verbatim run exits 0 in 2m22s (cargo test twice = 133 s of it). What has NOT happened is the thing only a person can do: push, and watch GitHub execute it. Until then 'CI is the authority' is again an unverified claim rather than an observed fact -- which is precisely the failure mode TASK-060 exists to kill.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 git push origin main succeeds (owner confirmed 2026-10-09 that agents may push main)
- [x] #2 the Actions run for that push goes green, in particular the single ci-steps.sh step, which had only run locally on aarch64-darwin before
- [x] #3 the observed CI wall time is recorded against the job in the notes
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Cheap to check whether it worked: `gh run list --limit 3`. If the step dies, the likely culprits are runner-side nix behaviour (install-nix-action v31 + flake .#default) or a path assumption in ci-steps.sh, which assumes cwd is the repo root and reaches firmware/ via a subshell cd.

2026-10-09: Closed by an agent after the owner reassigned this to @agent. main was pushed by the ralph loop's TASK-071.03 executor. CI run 37925833149 (push of 04c324d, 2026-10-09, https://github.com/jeffutter/asperitas/actions/runs/37925833149), conclusion success. Job 'check' wall time 17m10s (11:46:48 to 12:03:58). Biggest steps: 'Toolchain identity the cache key is built from' (nix develop) 10m41s; the single ci-steps.sh step 'fmt + clippy + doc + test + firmware cross-build' 6m13s. Per-gate from gates.sh ci: cargo doc (workspace) 0.51s, cargo doc (workspace, all features) 2.21s, cargo clippy 10.81s, firmware rig build (stim-ess) 36.51s, cargo test 123.90s, cargo test (asperitas-pod pod-hw) 131.10s. Caveat: Swatinem/rust-cache was present on this run; whether it was a cold or warm cache is TASK-064.02's question, so the doc figures may be cache-assisted.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-13 03:12
---
TASK-061 is now planned and changes what this ticket watches. Its AC #2 names "bash .github/ci-steps.sh", but that file is being replaced by scripts/gates.sh (tier argument ci), which prints a label and a wall time for every gate instead of one anonymous block. So: watch the single step run "nix develop .#default --command bash scripts/gates.sh ci" if .02 has landed when you push, and read the per-gate timings it prints as the record AC #3 asks for - those numbers also settle most of TASK-052, which has been owed runner-side figures since TASK-049. If TASK-061 has not landed, the old path still works and this ticket stands as written.
---

created: 2026-09-13 04:16
---
Repoint from TASK-061.02, which landed between this ticket being filed and anyone pushing. The step to watch is no longer `nix develop .#default --command bash .github/ci-steps.sh`: that file is deleted, and the step is now `nix develop .#default --command bash scripts/gates.sh ci`. This ticket's `references` entry was repointed from the deleted file to `scripts/gates.sh` for the same reason - flagging it out loud because it is your ticket, not mine, and the alternative was leaving a dead path in it.

Substance unchanged and still entirely human: push main, watch the one step go green, read the times. It is easier than when this ticket was filed - the script prints a per-gate `--- N.NNs` line under each banner, so one observed run discharges most of TASK-052's debt as well as this step's total. Local warm expectation for the whole tier: 139 s, of which the two `cargo test` invocations are 134 s.
---

created: 2026-09-13 05:38
---
TASK-061's integration pass re-measured what you should expect locally before you read a runner number: the new `ci` tier is **140 s warm** on aarch64-darwin, against **141 s** for the old list run in the same session ten minutes earlier - so the shape did not get slower, and the 138 s figure in older prose does not reproduce even for the list it was measured from. Expect the per-gate `--- N.NNs` lines to look like this at the top: gate definition parses 0.01, docs artifact names 0.16, cargo fmt 0.25, firmware fmt 0.41, clippy 0.35, then five gates under 0.7 s, two `cargo doc` runs at 1.33 and 1.58, both cross-builds at 0.34 and 0.13, both cross-clippies at 0.43 and 0.22 - and then cargo test at 66.30 and cargo test (pod-hw) at 66.52, which are 132.8 of the 140 s. If your runner spends far more than that, the extra is almost certainly cold compilation rather than anything the relocation broke: ci.yml has no cache at all today, and that gap now has its own ticket, TASK-064.
---
<!-- COMMENTS:END -->
