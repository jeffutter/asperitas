---
id: TASK-064.02
title: 'HUMAN: Measure the cache on a runner, cold run against warm run'
status: To Do
assignee:
  - '@human'
created_date: '2026-09-13 05:37'
updated_date: '2026-09-14 10:33'
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
- [ ] #1 HUMAN: After TASK-064.01 lands, push twice - or once with the cache empty and once warm, which is what a cold key does by itself - and attach both run URLs.
- [ ] #2 HUMAN: Record the total job wall time for each run, and the per-gate `--- N.NNs` lines `scripts/gates.sh ci` prints, so the question "did the cache work" is answered by gate rather than by one aggregate that could hide a workspace left uncached.
- [ ] #3 HUMAN: Say plainly whether the second run actually reused artifacts - a warm run that recompiles everything is the failure mode this ticket exists to catch, and it looks exactly like success in a green check mark.
- [ ] #4 HUMAN: Post the two numbers where the local figures live, doc-001 section 5's CI discussion, labelled as runner-side. Every cost figure in this repo is currently labelled local warm, and TASK-052 has owed these since TASK-049.
<!-- AC:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-14 10:33
---
Planning TASK-064.01 added one thing worth pasting alongside the timings this ticket already asks for, because wall time alone cannot tell 'the cache missed' from 'the cache was never wired to the toolchain'. From the rust-cache step of BOTH runs, quote: (1) the `Cache Configuration` group's `Cache Key:` line, (2) the `.. Environment considered:` -> `Rust Versions:` lines -- more than one version listed means the runner image's own rustup answered and the key contains a toolchain nobody installed here, and (3) whether the second run printed `Restored from cache key "..." full match: true`. The workflow also gains a step named 'Toolchain identity the cache key is built from'; its three lines record which rustc/cargo the nix shell resolves and whether a stray rustup is reachable inside it. If those say /home/runner/.cargo/bin rather than a /nix/store path, cmd-format is not doing what TASK-064.01's plan claims and this ticket should reopen it rather than just recording a slow run.
---
<!-- COMMENTS:END -->
