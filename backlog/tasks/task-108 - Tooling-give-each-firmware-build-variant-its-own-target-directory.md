---
id: TASK-108
title: 'Tooling: give each firmware build variant its own target directory'
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-16 18:13'
labels:
  - review-2026-10
  - tooling
  - firmware
dependencies: []
priority: medium
type: chore
ordinal: 210800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The console, RTT-only and bootloaded builds all write to target/thumbv7em-none-eabihf/release/<bin>. Machinery exists only to cope: gates.sh ordering rule 1 (gates.sh:27-36, 445-486), the firmware-elf-provenance gate, the build-then-check loop in check-bootloaded-builds.sh:5-7,22-23, and the cfg-closure check in make elf-check (Makefile:91, 388-392) that derives expected features via cargo metadata. The incident behind it (decoding bench output with the wrong ELF) was real, but this repo created the hazard.

Change: build each layout and log mode into its own --target-dir (Makefile and gates.sh), then delete ordering rule 1, the firmware-elf-provenance gate and the cfg-closure logic; keep the embedded source digest for probe-log. Cost is disk space and colder first builds.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each variant builds into its own target dir; ordering rule 1 and the provenance gate are deleted
- [ ] #2 probe-log still refuses a mismatched ELF
- [ ] #3 gates.sh push tier is green and CI time is recorded
<!-- AC:END -->
