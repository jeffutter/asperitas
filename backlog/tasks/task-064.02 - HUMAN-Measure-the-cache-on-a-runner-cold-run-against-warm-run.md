---
id: TASK-064.02
title: 'Measure the cache on a runner, cold run against warm run'
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-13 05:37'
updated_date: '2026-10-09 13:13'
labels: []
dependencies:
  - TASK-063
references:
  - .github/workflows/ci.yml
  - scripts/gates.sh
parent_task_id: TASK-064
priority: medium
type: chore
ordinal: 104800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-064's human half. Depends on TASK-063 only because that ticket owns the pending push of a main that sits ~90 commits ahead of origin; if the push happens for its own sake, measure here. Same reason it cannot be an agent's: observing GitHub execute anything is outside this machine.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Push twice, or once with the cache empty and once warm (a cold key does the first by itself), and record both run URLs. Owner confirmed 2026-10-09 that agents may push main; the pushes carry only the ticket-note commits, nothing that changes the build, and run 37925833149 (the first run after TASK-064.01) may serve as the cold run if its cache was empty
- [ ] #2 Record the total job wall time for each run, and the per-gate '--- N.NNs' lines scripts/gates.sh ci prints (gh run view --log), so the question 'did the cache work' is answered by gate rather than one aggregate
- [ ] #3 Say plainly whether the second run actually reused artifacts - a warm run that recompiles everything looks exactly like success in a green check mark. Evidence: the rust-cache step's restore log and the per-gate times
- [ ] #4 Post the two numbers where the local figures live, doc-001 section 5's CI discussion, labelled as runner-side
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-09: Reassigned @human -> @agent. gh can run, list and read Actions logs, and the owner allows pushing main. TASK-063 is Done.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-14 10:33
---
Planning TASK-064.01 added one thing worth pasting alongside the timings this ticket already asks for, because wall time alone cannot tell 'the cache missed' from 'the cache was never wired to the toolchain'. From the rust-cache step of BOTH runs, quote: (1) the `Cache Configuration` group's `Cache Key:` line, (2) the `.. Environment considered:` -> `Rust Versions:` lines -- more than one version listed means the runner image's own rustup answered and the key contains a toolchain nobody installed here, and (3) whether the second run printed `Restored from cache key "..." full match: true`. The workflow also gains a step named 'Toolchain identity the cache key is built from'; its three lines record which rustc/cargo the nix shell resolves and whether a stray rustup is reachable inside it. If those say /home/runner/.cargo/bin rather than a /nix/store path, cmd-format is not doing what TASK-064.01's plan claims and this ticket should reopen it rather than just recording a slow run.
---
<!-- COMMENTS:END -->
