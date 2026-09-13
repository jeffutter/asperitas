---
id: TASK-066.01.01
title: Run the nix rebuild that deploys unblocked-todo.sh --explain
status: To Do
assignee:
  - '@human'
created_date: '2026-09-13 10:59'
labels: []
dependencies: []
parent_task_id: TASK-066.01
priority: high
type: chore
ordinal: 113800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The last step of TASK-066.01. The code is committed to home-manager as dc4398780c9ed71a842cbc35b13e27dce69497a5 and already built by `nix build .#darwinConfigurations.mbp16.system`; only the activation is missing, so the deployed ralph extension is still the pre-change script and TASK-066.02 cannot call --explain yet.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: Run `~/bin/rebuild` in a terminal (it is a `sudo darwin-rebuild switch` and cannot take a password from an agent session), then confirm `shasum -a 256 ~/.pi/agent/extensions/ralph/unblocked-todo.sh` after resolving the symlink equals bea239d84ac544dcdfeed0abfcdf32b4d39ee001e37585c90dbebe82781f12f7.
- [ ] #2 HUMAN: `~/.config/home-manager` working tree is clean afterwards and no other home-file changed (verified before this ticket was filed: diff -rq over the deployed vs newly built home-files trees names exactly one file).
<!-- AC:END -->
