---
id: TASK-050
title: >-
  Read the Seed3 schematic for the nRESET path: supervisor, buffer, or RC plus
  button?
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-10 22:19'
labels:
  - planned
dependencies: []
references:
  - 'https://github.com/electro-smith/DaisyWiki'
  - docs/reference/daisy-seed3.md
priority: high
type: task
ordinal: 79500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
docs/reference/daisy-seed3.md's probe section now says what --connect-under-reset actually does on an ST-Link (probe-rs 0.32 skips its custom reset sequence for every native ST-Link and just drives the pin) and that what remains is electrical: whether the probe pulls this board's nRESET net down far enough, long enough. The one fact that decides whether that failure mode applies here at all is unanswered: does the Seed drive nRESET through a reset supervisor (e.g. MIC6315) or a logic buffer, or just RC plus the front-panel button? In probe-rs #3516 the root cause was precisely a MIC6315 loading the ST-Link's nRESET output, plus a 74-series buffer the STM32 cannot tolerate as a push-pull input; revising that circuit is what made under-reset work there, while CubeProgrammer managed throughout. A second 2026-04 report in the same thread puts the same probe class against unmodified STM32U5 boards with intermittent failures, so nobody knows which case the Seed is. This is a look at a public schematic, not a bench session — no board, no ears.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The Seed/Seed3 schematic is read and the nRESET net traced from the SWD header pin to whatever hangs off it (supervisor part number, buffer part number, RC values, button), with the source of the schematic recorded (revision/date).
- [ ] #2 docs/reference/daisy-seed3.md's open question about the nRESET path is replaced by the answer, stated with the same confidence limits as the rest of the probe section, and says explicitly whether the #3516 failure mode is plausible on this board or ruled out by the circuit.
- [ ] #3 If the answer changes what TASK-037 should try first, its notes say so; if it rules the failure mode out, the try-with-and-without advice stays anyway because probe-rs's FAQ recommends both regardless.
<!-- AC:END -->
