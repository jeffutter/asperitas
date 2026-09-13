---
id: TASK-063
title: 'HUMAN: Push main and watch the first real run of the relocated CI step'
status: To Do
assignee:
  - '@human'
created_date: '2026-09-13 01:30'
labels: []
dependencies:
  - TASK-060
references:
  - .github/ci-steps.sh
  - .github/workflows/ci.yml
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
- [ ] #1 HUMAN: git push origin main succeeds (main is ~90 commits ahead of origin/main as of 2026-09-13; pushing is an outward action, so no agent does it)
- [ ] #2 HUMAN: the Actions run for that push goes green, in particular the single 'nix develop .#default --command bash .github/ci-steps.sh' step -- that file has never executed on a runner, only locally on aarch64-darwin
- [ ] #3 HUMAN: record the observed CI wall time against the job, since every cost figure in lefthook.yml, ci-steps.sh and doc-001 is labelled as a local warm measurement and TASK-052 still owes the runner-side numbers
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Cheap to check whether it worked: `gh run list --limit 3`. If the step dies, the likely culprits are runner-side nix behaviour (install-nix-action v31 + flake .#default) or a path assumption in ci-steps.sh, which assumes cwd is the repo root and reaches firmware/ via a subshell cd.
<!-- SECTION:NOTES:END -->
