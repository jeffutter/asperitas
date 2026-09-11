---
id: TASK-050
title: >-
  Read the Seed3 schematic for the nRESET path: supervisor, buffer, or RC plus
  button?
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-10 22:19'
updated_date: '2026-09-11 01:53'
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
- [x] #1 The Seed/Seed3 schematic is read and the nRESET net traced from the SWD header pin to whatever hangs off it (supervisor part number, buffer part number, RC values, button), with the source of the schematic recorded (revision/date).
- [x] #2 docs/reference/daisy-seed3.md's open question about the nRESET path is replaced by the answer, stated with the same confidence limits as the rest of the probe section, and says explicitly whether the #3516 failure mode is plausible on this board or ruled out by the circuit.
- [x] #3 If the answer changes what TASK-037 should try first, its notes say so; if it rules the failure mode out, the try-with-and-without advice stays anyway because probe-rs's FAQ recommends both regardless.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
**Answer: no supervisor and no buffer - a 10 K pull-up, one tactile button to GND, and the debug header's
pin 10, straight onto the MCU's NRST pin.** With the limit stated up front: Electrosmith publishes no
Seed3 schematic, so this is traced on the four public Seed-family drawings that do show the net, not on
the Seed3 itself.

Per-sheet contents of the net named RESET (each dated from its own title block):

| Drawing | Date | On the net |
|---|---|---|
| ES_Daisy_Seed_Rev4.pdf (full, 4 sheets) | 2020-06-08 | R19 10K -> +3V3_D; S3 PTS815 -> GND; MCU NRST; P6 mini-JTAG pin 10. No cap. |
| ES_Daisy_Seed_Rev7.pdf (reduced) | 2024-02-01 | Same three, plus C32 100NF -> GND. |
| ES_Daisy_Seed2_DFM_Rev5-REDUCED.pdf | 2025-02-17 | R1 10K -> +3V3_D; S1 -> GND; P8 M05X2MINIJTAG pin 10; MCU NRST. No cap. |
| ES_Daisy_Patch_SM_Schematic.pdf | 2024-02-08 | R1 10K -> +3V3_D; S1 -> GND; P8 pin 10. |

Method, since text extraction alone cannot answer connectivity in an EAGLE export: PyMuPDF vector reads
(stroke colour = wire, junction dots, label anchors) gave net membership and label positions; vision
subagents then read high-zoom crops, capped at four images each, and reported designators, values and what
each element's far terminal joins. The two methods agree on every element above. Rev4 came from
https://raw.githubusercontent.com/jull-taragan/Hardware/master/reference/daisy_seed/ES_Daisy_Seed_Rev4.pdf
(the fork that survived upstream's deletion); the rest from daisy.nyc3.cdn.digitaloceanspaces.com.

Negatives worth keeping, all verified rather than assumed:
- No Seed3 schematic exists publicly. The Seed3 docs page lists databrief, pinout PDF/CSV, 3D models and
  compliance zip only; plausible schematic keys return the bucket's key-absent response; archived copies of
  the page list the same five assets, so nothing was published and withdrawn.
- The Seed3 databrief has no reset content: extraction finds TAC5242 once and STM32H750, zero hits for
  RESET / NRST / supervisor across 38 pages.
- Product photography cannot close the gap: at published resolution a SOT23-class supervisor near the button
  would be a package outline at best, and Compliance.zip holds FCC photographs, not design data.
- Daisy Pod Rev5 schematic (2022-10-27) contains no reset net at all - the carrier adds no load, so in-Pod
  bench work doesn't change the analysis.
- The J1 printed on the NRST wire, and C6/D6 on PDR_ON/BOOT0, are UFBGA-169 ball coordinates, not jumpers or
  components. Reading them as a jumper would invent exactly the circuit this ticket set out to rule out.

Confidence limits carried into the doc: three of the four sheets are explicitly reduced, so absence of a
supervisor there is weaker evidence than presence of the 10K/button/cap. Against that, Seed3 is documented
as the same MCU in the same footprint with only codec and USB connector changed. Verdict written to
docs/reference/daisy-seed3.md: the #3516 failure mode is ruled out on the circuit we can see, demoted to
unlikely-but-not-excluded on the Seed3, so under-reset is "likely to work", not "known to work". TASK-037's
notes carry the bench consequence; TASK-051 (@human) asks Electrosmith the one question documents cannot
answer.

Fixup applied post-review: docs/reference/daisy-seed3.md:344-345 linked to `#flashing-and-logging-over-an-st-link-probe`, the anchor for the section this sentence itself lives in, so the link pointed at its own enclosing heading instead of the nRESET trace lower in the same section. Replaced the self-link with plain text ("whose net is traced later in this section").
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
No Seed3 schematic is public - established, not assumed - so nRESET was traced on the four public
Seed-family drawings that do show it: Rev4 full (2020-06-08), Rev7 reduced (2024-02-01), Seed2 DFM
Rev5-reduced (2025-02-17) and Patch SM (2024-02-08). Every one draws the same circuit: a 10 K pull-up to
+3V3_D, one tactile button to GND, the MCU's NRST pin, and the 10-pin mini-JTAG header's pin 10; Rev7 alone
adds a 100 nF capacitor. No reset supervisor and no logic buffer on the net in any of them - which is the
exact thing that made probe-rs #3516 fail elsewhere. The probe therefore sinks about 0.33 mA plus a ~1 us RC
instead of fighting a second driver. docs/reference/daisy-seed3.md now states that the #3516 mode is ruled
out on the drawn circuit and merely unlikely on the Seed3 - reduced sheets omit parts by design, so absence
of a supervisor is weak evidence - and keeps the try-with-and-without advice because probe-rs recommends it
regardless. TASK-037's notes reorder what to suspect if under-reset fails, TASK-051 (@human) asks
Electrosmith the question documents cannot answer, and the negative-evidence trail is kept in
docs/reference/seed3-schematic-search-log.md.
<!-- SECTION:FINAL_SUMMARY:END -->
