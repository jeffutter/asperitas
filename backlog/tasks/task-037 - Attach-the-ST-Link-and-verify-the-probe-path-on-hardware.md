---
id: TASK-037
title: Attach the ST-Link and verify the probe path on hardware
status: To Do
assignee:
  - '@human'
created_date: '2026-09-09 01:28'
updated_date: '2026-09-11 00:35'
labels: []
dependencies:
  - TASK-036
documentation:
  - docs/reference/daisy-seed3.md
priority: high
type: task
ordinal: 47500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-036 can be written and compile-verified without hardware; none of it is true until a real probe talks to a real board. This ticket also settles the one physical question that could change how the bench is wired: if the SWD pads are unreachable with the Seed in the Pod, then probe-based work and Pod control-surface work may not be simultaneously possible, and that is much cheaper to discover before committing solder.

This is also the only mechanism that recovers a board whose firmware hangs before USB enumerates. The software restart command from TASK-032 needs a live console to receive an instruction, so it cannot help a board that never got that far — the probe can, because it asserts reset from outside.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: whether the SWD pads are reachable with the Seed3 seated in the Pod is determined and recorded, together with the attachment method actually used — soldered hairlines, pogo pins, or running the Seed outside the Pod. Attachment uses the 10-pin Cortex Debug footprint, since the Seed3's extra V3MINIE-style pads are documented as unwired.
- [ ] #2 HUMAN: probe-rs attaches under reset and flashes the application with no interaction with BOOT or RESET.
- [ ] #3 HUMAN: the defmt/RTT log stream is observed live while the application runs, and a forced panic arrives with a decoded backtrace.
- [ ] #4 HUMAN: a deliberately faulted or hung binary is recovered by re-attaching under reset, demonstrating recovery when USB is dead — the case the software restart command cannot serve.
- [ ] #5 HUMAN: probe firmware version is recorded, since probe-rs requires ST-Link V3 firmware 3.2 or newer, along with measured attach time, flash time, log throughput, and anything flaky observed.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
From TASK-036.01: `make probe-run` and `make probe-log` currently abort before they ever look for a probe — 'defmt version found, but no `.defmt` section'. So an RTT logging failure at the bench is not evidence about the ST-Link; check that error string first and see TASK-036.03's notes. `make probe-flash` is unaffected: it builds, then fails only at probe discovery ('Error: No connected probes were found.' — probe-rs 0.32 download wording; the 'No debug probes were found.' string belongs to `probe-rs list`).

**The five-minute schematic check in comment #1 is done (TASK-050), and it comes back clean.** On every
public Seed-family drawing that shows the net at all - Seed Rev4 full (2020-06-08), Seed Rev7 reduced
(2024-02-01), Seed2 DFM Rev5-reduced (2025-02-17), Patch SM (2024-02-08) - the net named `RESET` carries a
10 K pull-up to `+3V3_D`, one tactile switch to GND, the MCU's `NRST` pin and the 10-pin mini-JTAG
header's **pin 10**, and Rev7 additionally a 100 nF capacitor to GND. No reset supervisor, no 74-series
buffer, no diode or transistor on the net in any of them. So the probe sinks about 0.33 mA plus one RC
time constant (~1 us through 10 K into 100 nF) instead of fighting a MIC6315 driving the other way, which
was #3516's entire root cause. Caveat, because it bounds how much to trust this: Electrosmith publishes no
Seed3 schematic, and three of those four sheets are explicitly reduced, so this is inference from
predecessors, not a reading of your board.

What changes at the bench, and what does not:

- **Nothing changes about running both ways.** `UNDER_RESET=0` alongside the default stays on the agenda -
  probe-rs's FAQ says try with and without regardless of circuit, and some parts won't attach under reset
  no matter how clean their reset net is.
- **What changes is the order of suspicion when it fails.** If `--connect-under-reset` hangs or times out,
  check ST-Link firmware >= 3.2 and whether the V3MINIE is still enumerating in MassStorage mode before
  believing anything about this board's reset circuit, and do not go hunting for a trace to cut: there is
  no second driver on this net to disconnect unless Seed3 silently added one.
- **Header pin 10 is nRESET**, which is where a scope goes if you want the direct measurement. Take it only
  if under-reset actually misbehaves - a first-try pass is itself evidence, and a scope adds nothing to a
  pass.
- **Two things that will look like findings and aren't.** The tokens printed on the reset wire next to the
  BGA symbol (`J1`, with `C6`/`D6` on `PDR_ON`/`BOOT0`) are UFBGA-169 ball coordinates, not jumpers. And
  the Daisy Pod adds nothing to this net - its schematic (2022-10-27) has no reset net at all - so working
  with the module seated changes none of the above.

Full trace with sources: docs/reference/daisy-seed3.md, "Flashing and logging over an ST-Link probe".
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-10 21:16
---
Bench notes from TASK-036's integration pass (2026-09-10) — measured or quoted upstream, so you can
trust them at the bench. Two of these change what a failure means.

**Expect `--connect-under-reset` to be the thing that fails.** On V3-class probes probe-rs 0.32 logs
"Custom reset sequences are not supported on ST-Link V3. Falling back to standard probe reset.", so
the STM32H7 attach sequence in `probe-rs/src/vendor/st/sequences/stm32cm7.rs` never runs and the flag
reduces to asserting nRESET. Upstream #3516 (V3 MINIE, open) has a 2026-04 report of exactly this on
STM32U5 where CubeProgrammer works on the same board. So "flashes fine without the flag, hangs or
times out with it" is a probe-class outcome, not a broken bench — record which worked and move on.
`TASK-036.06` is adding a Makefile variable to drop the flag without editing the file; if it hasn't
landed by your session, run the expanded command directly (`probe-rs download
target/thumbv7em-none-eabihf/release/main --chip STM32H750IBKx --verify --reset`, built with
`FEATURES="seed3 log-defmt" NO_DEFAULT=1`). Five-minute schematic check worth doing first: does the
Seed drive nRESET through a reset supervisor/buffer rather than RC + button? In #3516 that loading
(a MIC6315) was the whole root cause, and nobody here has checked our board.

**Flags that changed since the docs were written:** `--catch-hardfault` and `--catch-reset` are ON by
default in 0.32 (the plain flags are deprecated), so a caught reset during attach may be probe-rs,
not the firmware. Scriptable RTT liveness probe: `--list-rtt`. Capture to a file for a loss ledger
comparable against the USB console's counters: `--target-output-file defmt=out.txt`; `--no-location`
and the `oneline|twoline|full` presets make that file greppable.

**If `probe-rs list` shows nothing but the probe is plugged in:** the V3MINIE enumerates in
MassStorage mode until switched, and ST-Link V3 needs firmware >= 3.2, upgraded with ST's own tools.
That is why every agent-side check to date ends at "No debug probes were found."

**While attached, watch audio, not just logs.** probe-rs sets RTT to block-if-full and defmt-rtt
writes inside a critical section, so a stalled host freezes the target rather than dropping frames.
If the ~667 us block deadline starts missing only while `probe-log` is running, that is the mechanism
— `--rtt-channel-mode no-block-skip` on the host, or defmt-rtt's `disable-blocking-mode` feature on
the target (costing losslessness) is the lever.
---
<!-- COMMENTS:END -->
