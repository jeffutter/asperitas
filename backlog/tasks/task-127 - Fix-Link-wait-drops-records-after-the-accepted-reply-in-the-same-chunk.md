---
id: TASK-127
title: 'Fix: Link::wait drops records after the accepted reply in the same chunk'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-17 03:09'
updated_date: '2026-10-19 03:39'
labels:
  - review-fix
  - planned
dependencies: []
ordinal: 250800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
crates/asperitas-rig/src/control.rs Link::wait uses Decoder::feed, which drains the whole chunk. Records after the first accepted reply (e.g. SET/OK then REL/OK in one chunk) are never offered to pick, so the next wait() times out although the device answered. The old drain() left later validated records queued. Retain unconsumed records across wait() calls.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-127. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Link now keeps record bodies that follow the accepted reply in a read chunk (held queue) and offers them to the next wait(). Test added: records_after_the_accepted_reply_serve_the_next_request. cargo test/clippy/fmt pass for asperitas-rig.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Link::wait retains later records from the same chunk for the next wait, so back-to-back SET/OK and REL/OK in one read no longer time out.
<!-- SECTION:FINAL_SUMMARY:END -->
