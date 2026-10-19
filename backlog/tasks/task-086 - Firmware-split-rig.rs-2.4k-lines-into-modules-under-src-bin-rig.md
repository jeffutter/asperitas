---
id: TASK-086
title: 'Firmware: split rig.rs (2.4k lines) into modules under src/bin/rig/'
status: Done
assignee:
  - '@ralph'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-19 16:55'
labels:
  - planned
  - review-2026-10
  - firmware
dependencies:
  - TASK-085
priority: medium
type: chore
ordinal: 188800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
rig.rs is 2423 lines, about 948 of them comments. Sections: preamble 57-137, geometry gates 139-506, ring/Producer 314-743, timeline 908-985, dump 987-1111, excerpt install 1113-1419, fx_ctl 1511-1588, excerpt_play 1590-1882, time base 2061-2202, main 2208-2423.

Change: src/bin/rig/main.rs plus capture.rs (Ring, Producer, BLOCK_STATE, timeline, judge), dump.rs, install.rs, replay.rs, fx_ctl.rs and timebase.rs (or the shared lib's). A pure move; keep the const-asserts and behaviour identical.

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 rig.rs is split into modules; no module over ~600 lines
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit carrying Task-Id TASK-086. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Split rig.rs (2138 lines) into firmware/src/bin/rig/: main.rs (440, preamble, RigAudio, descriptors, entry), generator.rs, geometry.rs, ring.rs, report.rs, timeline.rs, dump.rs, install.rs, fx_ctl.rs, excerpt_play.rs (stim-excerpt). Largest module is main.rs at 440 lines. Pure move: const-asserts, cfg gating and logic unchanged; only visibility (pub(crate)) and two constructors (Ring::new, Producer::new) were added so main.rs need not name private fields; the Replay effect docs moved to fx_ctl.rs. The timebase already lives in asperitas_firmware::audio so no timebase.rs. A crate-root file src/bin/rig.rs cannot own mod files under src/bin/rig/ (mod x resolves to src/bin/x.rs), so the bin root is src/bin/rig/main.rs; build.rs rig_flash_ban now scans every .rs under src/bin/rig, and firmware-variants.sh, check-image-load-addresses.sh and check-doc-artifact-names.sh derive binaries from both src/bin/<name>.rs and src/bin/<name>/main.rs. Path references in Cargo.toml, linker script comments, crate docs and the variants table were updated. Evidence: scripts/gates.sh commit passes; clippy -D warnings clean on all rig variants; all variants build and grade; REPLAY staging check passes; .text sizes within 0.4 percent of the pre-split images (inlining noise), .bss +8 bytes (embassy main task future). AC #2 (same RIGCFG/STATUS/CAPSTAT on a rig capture) needs the board: removed from this ticket and carried by the new @human ticket TASK-138.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
rig.rs split into ten modules under firmware/src/bin/rig/ (largest 440 lines), behaviour unchanged by construction and verified by gates, clippy on every variant and image-size comparison. Build tooling (build.rs flash ban, variants and image-check scripts) updated for the src/bin/<name>/main.rs layout. Hardware recheck filed as TASK-138.
<!-- SECTION:FINAL_SUMMARY:END -->
