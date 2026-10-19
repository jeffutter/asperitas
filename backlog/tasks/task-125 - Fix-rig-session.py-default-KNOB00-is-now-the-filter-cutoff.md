---
id: TASK-125
title: 'Fix: rig-session.py default KNOB=00 is now the filter cutoff'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-17 01:16'
updated_date: '2026-10-19 03:31'
labels:
  - planned
dependencies: []
ordinal: 243800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
scripts/rig-session.py defaults KNOB to '00', which is now the filter cutoff, but sweep/soak schedules use gain-style levels. A default gain sweep drives the wrong knob and is judged against the wrong curve. Make the knob explicit or derive it from the sweep kind.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by this commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Made --knob required on the rig Session subcommand (no default); added clap parse test; updated docs/reference/asperitas-rig.md.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
asperitas-rig session now requires --knob, so a gain sweep can no longer silently drive the filter cutoff. Parse test added.
<!-- SECTION:FINAL_SUMMARY:END -->
