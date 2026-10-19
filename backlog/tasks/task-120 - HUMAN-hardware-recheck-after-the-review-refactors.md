---
id: TASK-120
title: 'HUMAN: hardware recheck after the review refactors'
status: To Do
assignee:
  - '@human'
created_date: '2026-10-16 18:13'
updated_date: '2026-10-19 16:25'
labels:
  - review-2026-10
  - hardware
dependencies:
  - TASK-082
  - TASK-085
  - TASK-087
  - TASK-088
  - TASK-093
  - TASK-094
  - TASK-101
  - TASK-106
priority: medium
type: chore
ordinal: 222800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Several review tickets touch bench-verified firmware paths. After they land, walk this checklist at the bench with the agent: flash main and listen (both knobs, no zipper noise, survives a power cycle); flash rig and run a capture (RIGCFG/STATUS/CAPSTAT as in TASK-059.03: sent advancing, dropped_full=0, overrun=0); asperitas-rig query and restart against both main and rig; panictest (LED solid red, panic reaches USB); podtest encoder and buttons (10 detents each way, 10 presses each); LED 1 boot, running and panic states.

Applies to: effect, fw_lib, fw_rig_split, fw_knobstate, fw_ctl, log_commit, log_init, pod_decoder, rgb_led, rig_session (as they land).

Found in the 2026-10-16 whole-codebase review (six read-only Opus reviewers).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 HUMAN: main is heard with both knobs working and survives a power cycle
- [x] #2 HUMAN: a rig capture shows STATUS sent advancing, dropped_full=0, overrun=0, and query/restart answer on main
- [x] #3 HUMAN: panictest reaches the panic LED and USB; podtest encoder and buttons count correctly
- [ ] #4 HUMAN: any regression is filed as a bug ticket with the capture
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LED 1 boot/running/panic recheck is tracked as TASK-106.01; walk it in the same bench session.

AC1 checked in a live bench session with the owner (2026-10-19): main built at 86fe0ae-dirty, flashed over DFU, fed the mandolin chord corpus clip from the MacBook line out into the Pod input. Owner heard the effect from boot, gain and low-pass sweep both working, no zipper or crackle, and it came back right away after a power cycle. Owner signed off on this criterion.

Environment finding: `make build` fails in this nix shell (cargo-objcopy 0.4.0 rejects --target-dir, passed via CARGO_DIR). Worked around with CARGO_TARGET_DIR=firmware/target/internal-default-seed3 and `make build CARGO_DIR=`. Needs a fix ticket. Seed enumerates as /dev/cu.usbmodem101 here; a second port, usbmodem1102, does not speak the rig protocol.

AC2 partial (2026-10-19, live bench with the owner). Board moved to the bootloaded layout: Daisy bootloader v6.4 (intdfu, 2000 ms) installed at 0x08000000, apps in QSPI at 0x90040000 (rig does not fit the 128 KB internal sector with the USB console). Probe-reset capture of rig-bootloaded (fw 86fe0ae-dirty), 20 s: RIGCFG present, STATUS sent 11 -> 51, dropped_full=0, bytes_dropped=0, overrun=0 in all 21 CAPSTAT, max_block_us 39-44, bad_frames=0, seq_gaps=0. Verdict is fail on discarded_bytes=34 (bytes before the first valid frame), the same verdict TASK-059.03 rig-run1..3 recorded (discarded_bytes 36/14/16). A capture that attaches ~27 s after boot instead shows dropped_full=34 (records emitted with no reader); not a regression. main-bootloaded: `query` and `restart` both answer (restart re-enumerated, reset_reason=software).

AC2 NOT met for rig: `asperitas-rig query` against rig answers `device rejected the command: UnknownVerb`. handle_control in firmware/src/bin/rig.rs serves only FX/SET/REL and answers every other verb unknown-verb, so VER/RSN/RST cannot work on rig until TASK-088 (one control-channel dispatcher for every binary) lands. AC2 should either wait on TASK-088 or be reworded to main only; left unchecked for the owner to decide.

AC2 reworded with the owner (2026-10-19): query/restart on rig was never implemented (rig's handle_control serves FX/SET/REL only; VER/RSN/RST were wired into main and podtest), so the criterion now asks for query/restart on main only. The rig capture counters and main query/restart were verified at the bench as noted above; owner agreed and signed off. query/restart on rig remains with TASK-088.

AC3 checked in the same live bench session with the owner (2026-10-19), bootloaded layout, fw 86fe0ae-dirty. panictest: countdown 8..1 and `E ... PANIC: panictest: deliberate panic, exercising the LED + serial panic path at src/bin/panictest.rs:123:9` reached the host intact over USB (capture attached late: BOOT and countdown 10/9 missed, discarded_bytes=34). Owner saw LED 1 steady green for the 10 s countdown then steady red after the panic; the brief boot red was not clearly seen (before USB, behind the bootloader's 2 s grace), so TASK-106.01 boot state stays open. podtest 75 s capture: ENC +1 x10, ENC -1 x10, BTN1 press/release 10/10, BTN2 10/10, encoder CLICK 10/10, each detent with the full AB=00,01,10,11 walk. device_dropped=4489 is podtest's ~100 Hz knob stream (TASK-033.01 recorded 4041 in 240 s), unrelated to the input counts. Owner signed off on this criterion.
<!-- SECTION:NOTES:END -->
