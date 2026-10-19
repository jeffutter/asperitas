---
id: TASK-139
title: 'Fix: asperitas-stimulus unit tests do not compile standalone (Vec in no_std)'
status: Done
assignee:
  - '@agent'
created_date: '2026-10-19 17:22'
updated_date: '2026-10-19 17:35'
labels:
  - planned
dependencies: []
ordinal: 265800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
cargo test -p asperitas-stimulus fails with E0433/E0425: fx_chain.rs:178 and replay_fx.rs:144 use Vec with no extern crate std/alloc. Tests only build when another workspace member unifies the std feature. Fix by making the tests import std so the crate tests alone.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 cargo test -p asperitas-stimulus passes standalone with default features
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-139. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added extern crate std and use std::vec::Vec to the cfg(test) modules in fx_chain.rs and replay_fx.rs. Verified: cargo test -p asperitas-stimulus (23 pass, default and --features std), clippy --all-targets clean, fmt clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Test modules in asperitas-stimulus now import std::vec::Vec so the crate tests standalone under no_std defaults.
<!-- SECTION:FINAL_SUMMARY:END -->
