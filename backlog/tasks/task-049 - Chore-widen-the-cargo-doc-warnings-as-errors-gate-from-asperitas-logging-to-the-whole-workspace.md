---
id: TASK-049
title: >-
  Chore: widen the cargo doc warnings-as-errors gate from asperitas-logging to
  the whole workspace
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-10 09:22'
updated_date: '2026-09-10 09:27'
labels:
  - chore
dependencies:
  - TASK-043
  - TASK-048
priority: low
ordinal: 78500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Follow-up to TASK-043, which gates `cargo doc` with warnings-as-errors for asperitas-logging only. Narrow on purpose: asperitas-pod and asperitas-dsp carry four pre-existing warnings of their own (TASK-048), and diluting AC #1 of TASK-043 with crates it never touched would have made that ticket uncloseable for reasons unrelated to its own work.

Once TASK-043 and TASK-048 are both done, widen the same two commands to the whole workspace so a new broken link anywhere fails the push rather than accumulating. Also document asperitas-cli, which has no warnings today but sits inside `cargo doc --workspace` anyway.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The lefthook pre-push doc command and the ci.yml doc step both run `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` (plus the `--features asperitas-pod/pod-hw` variant, since CI already treats that combination as first-class), replacing the `-p asperitas-logging` scoping without otherwise restructuring the files.
- [ ] #2 Each crate in the workspace carries its own `[lints.rustdoc]` deny stanza, matching what TASK-043.02 established for asperitas-logging, so `cargo doc` fails locally for anyone who forgets the flag. Prove one of them bites by introducing a throwaway bad link and confirming a non-zero exit.
- [ ] #3 Measured cost recorded in the ticket notes: wall time for the widened commands, warm and cold, from a clean target dir, so the next person can judge whether the gate is still cheap enough to keep in pre-push.
- [ ] #4 `lefthook run pre-push --command <doc-command-name>` passes locally and the CI workflow file parses (act-free check: the step is inside the existing `nix develop ... bash -c` heredoc, so verify quoting by running the exact command string through `bash -c` yourself).
<!-- AC:END -->
