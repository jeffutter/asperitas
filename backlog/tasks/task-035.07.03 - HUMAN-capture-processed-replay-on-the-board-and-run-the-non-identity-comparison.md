---
id: TASK-035.07.03
title: >-
  HUMAN: capture processed replay on the board and run the non-identity
  comparison
status: Done
assignee:
  - '@human'
created_date: '2026-10-15 20:52'
updated_date: '2026-10-15 23:44'
labels:
  - task
dependencies:
  - TASK-035.07.01
  - TASK-035.07.02
parent_task_id: TASK-035.07
priority: medium
ordinal: 172800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Flash the rig firmware with the replay processor, capture excerpts at non-identity settings through the loopback, run the extended comparison, commit artifacts with the ticket.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 HUMAN: board flashed and capture taken at non-identity Gain and filter settings
- [x] #2 HUMAN: comparison passes within documented tolerance and artifacts are committed
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-15 (agent ran the bench session unattended at the owner's request; NOT closed - the HUMAN criteria still need the owner's sign-off). Flow without any button press: dfu-util -w started, then probe-rs reset --connect-under-reset reached the bootloader's DFU window and the stim-excerpt image (ASP_RIG_CAPTURE_SECONDS=10, slot 0) flashed to 0x90040000; later captures needed only a probe reset. Evidence of a live DWT: max_block_us=19 (15 without the effect), so the probe-reset path does not zero the cycle counter. A new example crates/asperitas-logging/examples/ctl_frame.rs prints a framed CTL FX command; a session script sent it 3.7 s after the reset, as soon as the console attached, and every reply was CTLR OK FX. Three captures of mandolin_single_note_soft through the loopback, each analysed with asperitas-measure analyze --excerpt ... --fx ... --fx-value (the same value that was sent): gain +6 dB (v=0xEAAA) pass, loop gain -0.009 dB from the model, residual RMS -30.8 dB, peak -21.5 dB; gain -12 dB (v=0xAAAA) pass, -0.009 dB, -30.8 dB, -21.5 dB; low-pass 2 kHz (f=02 v=0xAAAA) pass, -0.009 dB, -32.9 dB, -30.0 dB. All aligned at 81 samples, 30/30 blocks delivered, 0 overruns, 0 drops. Negative controls fail as they should: the +6 dB capture judged as bypass is 5.99 dB off on loop gain, the -12 dB capture judged as +6 dB fails, the low-pass capture judged as bypass fails. Artifacts committed in e99ab58: audio/captures/2026-10-15-rig-fx-{gain-p6db,gain-m12db,lowpass-2khz}.console.zst and captures/task-035-07-03-*.json. Open for the owner: AC #1 (board flashed and capture taken at non-identity Gain and filter settings) and AC #2 (comparison passes within documented tolerance, artifacts committed) both look met; review and tick together. Not done: no listening check was made on these processed captures.

2026-10-15: owner reviewed the evidence above and agreed both criteria were met (the bench session itself was run by the agent while the owner was away, at the owner's request). Closed with the owner's sign-off.
<!-- SECTION:NOTES:END -->
