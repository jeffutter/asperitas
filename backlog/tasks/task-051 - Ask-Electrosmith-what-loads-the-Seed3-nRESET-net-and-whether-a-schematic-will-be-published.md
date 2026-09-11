---
id: TASK-051
title: >-
  Ask Electrosmith what loads the Seed3 nRESET net, and whether a schematic will
  be published
status: To Do
assignee:
  - '@human'
created_date: '2026-09-11 00:34'
labels: []
dependencies: []
references:
  - 'https://community.daisy.audio/t/seed3-is-here/9440'
  - docs/reference/seed3-schematic-search-log.md
  - docs/reference/daisy-seed3.md
priority: high
type: task
ordinal: 80500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-050 traced nRESET on every public Seed-family schematic and got a benign answer: a 10 K pull-up to
+3V3_D, one tactile button to GND, the MCU NRST pin, and the 10-pin mini-JTAG header's pin 10 - and no
reset supervisor or logic buffer anywhere on the net, in four drawings from 2020 to 2025. That is an
inference about the Seed3, not a reading of it, because Electrosmith publishes no Seed3 schematic at all
(see docs/reference/seed3-schematic-search-log.md for the search that established that, including the
archived-page check and the CDN key probes).

Two limits keep the question open. The Seed3 databrief has zero reset content, so nothing official speaks
to the net. And three of the four sheets are explicitly reduced drawings - Electrosmith omits parts from
those on purpose, so "no supervisor drawn" is weaker evidence than "10K, button and 100nF drawn".

Why it matters: probe-rs #3516's whole root cause was a MIC6315 supervisor loading an ST-Link V3 MINIE's
nRESET output. If the Seed3 added one, TASK-037's --connect-under-reset failures would be a board problem
rather than a probe-class one, and the fix would be cutting a trace rather than updating probe firmware.
A one-line staff answer retires that branch of the search.

Posting is outward-facing, so this is @human by project convention. Suggested venue:
https://community.daisy.audio, either replying in the Seed3 announcement (topic 9440) or as a new
Hardware topic. Precedent for a yes: "Seed Rev7 Schematic Is Now Live!", topic 4846.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: An Electrosmith reply states what hangs off the Seed3 nRESET net - supervisor or buffer part number, or pull-up/button/capacitor values - or states plainly that no schematic will be published; the reply is quoted verbatim in this ticket's notes with its URL.
- [ ] #2 HUMAN: docs/reference/daisy-seed3.md's nRESET section says whether the TASK-050 inference held or broke, with the same confidence limits as the rest of the probe section, and the search log's gap list is updated to match.
<!-- AC:END -->
