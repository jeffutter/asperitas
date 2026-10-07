---
id: TASK-037
title: Attach the ST-Link and verify the probe path on hardware
status: To Do
assignee:
  - '@human'
created_date: '2026-09-09 01:28'
updated_date: '2026-10-07 23:38'
labels: []
dependencies:
  - TASK-053
  - TASK-054
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

## Pre-flight facts for the bench session (gathered while planning TASK-036, 2026-09-12)

**Order of work at the bench.** First thing, before anything erases flash:

    probe-rs attach firmware/target/main.elf --chip STM32H750IBKx --non-interactive --list-rtt

It reads memory through the debug port, neither needs the host to drain RTT nor erases the single 128 KB
sector, and it proves the probe, the wiring, the target description and the ELF's RTT symbols together.
TASK-053 is expected to wrap it as `make probe-rtt-list`. Then DFU (`make flash`) for the image under
test, then the probe only for observation.

**Probe-side gotchas read out of probe-rs 0.32.0's own source or measured here:**

* An ST-Link/V3 must be at least V3MIN `0x0359`, or V3SET `0x0410` if it is a V3SET; older firmware makes
  probe-rs refuse the probe. Check with `probe-rs st-link doctor`, fix with `probe-rs st-link upgrade`.
  probe-rs cannot detect an ST-Link/V2 at all without libusb at build time and reports that in a way that
  reads like a permissions problem, so know which probe you are holding before chasing udev rules.
* A machine that has had STM32CubeProgrammer installed may need its `stlink-v2.rules.py` restored, and
  CubeProgrammer's systemd services can hold the probe.
* `--connect-under-reset` works here because PA13/PA14 stay powered in the VBAT domain. **Do not also pass
  `--reset`**: daisy-rs records that combination faulting SWD on the same part.
* Consider `--disable-double-buffering` for repeated flashes: pyOCD #1700 reports silent STM32H750 flash
  corruption roughly 1 attempt in 5 on a ~30 kB image. Whatever you conclude about it, note that `--verify`
  is load-bearing on a chip whose internal flash is one 128 KB sector - every flash erases the whole app.
* Exit codes as measured with nothing attached: probe path rc=1 with stderr
  `Error: No connected probes were found.`; `probe-rs list` prints a different string
  (`No debug probes were found.`); dfu-util `-a 0 -s 0x08000000:leave` exits 74 when DFU-mode match fails.
* Until TASK-054 lands, expect `WARN ... Insufficient DWARF info; compile your program with debug = 2 to
  enable location info` on every probe-rs invocation against our builds, and expect defmt log lines to
  lack source locations. Partial backtraces are normal either way: probe-rs stops unwinding at the first PC
  lacking debug info (#896), reports truncated frames with custom panic handlers (#2274), and has shipped
  wrong stack traces before (#3309). Record what you actually get, not what you hoped for.

**Clock chain, if USB never enumerates but the board boots:** the 48 MHz domain comes from PLLP, not
PLLSAI, and `RCC.d2.cdcfg.d2ppre` set to the wrong value hangs the kernel outright (daisy-rs). HSE runs in
BYPASS mode driven by the Pod jumper `OSC_BY` (PB15 high = oscillator enabled); a missing oscillator looks
identical to a dead codec.

**Reading the two channels together:** RTT keeps no ledger, so a drop count taken from the framed console
does not describe an RTT capture, and defmt frame loss is never evidence of a SAI overrun. A `log-defmt`
only build (`NO_DEFAULT=1 FEATURES="seed3 log-defmt"`) has no console at all, so a board that never
enumerates USB gives you RTT and nothing else - plan the first session around that. If an RTT capture
stops and the target appears wedged after closing probe-rs, that is the documented mid-run detach hazard
in defmt-rtt, not a firmware bug; power-cycle before concluding anything.

**Backtrace quality is measured at `debug = 2`, and TASK-054 just changed that level.** `[profile.release] debug` was `"line-tables-only"`; it is now `2`, because probe-rs refuses to put file:line on a decoded `defmt` record at anything lower (it prints "Insufficient DWARF info; compile your program with `debug = 2` to enable location info." and decodes without locations anyway). Cost was 236 bytes of flash on the RTT image and a 3 MB host ELF becoming 9.5 MB; both measured, both in `firmware/Cargo.toml`. So AC #3's "arrives with a decoded backtrace" is now judged at full debug info, which is the best case this profile offers.

**Judge a partial trace against probe-rs, not against the build.** Three upstream issues describe failure modes that survive complete DWARF: #896 - unwinding stops at the first PC that has no debug info, so a frame inside a crate built without it truncates the trace there and says nothing about why; #2274 - reported frames come back truncated when a custom panic handler is in play, which is exactly what this firmware has (`#[panic_handler]` in `asperitas-logging`); #3309 - stack traces shipped wrong outright. Expect a trace that ends early or names a frame you can't reconcile, and record what you got rather than concluding the image is misbuilt. The one thing worth checking before believing any of it: the ELF on the host is the one that produced the image on the board (`make elf-check`), since decoding with a stale ELF produces output that looks like data.

Bench progress recorded 2026-10-06 from commit bbd2091 and docs/reference/daisy-seed3.md (written by the owner at the bench; an agent did not observe any of this). No acceptance criterion is ticked: each is HUMAN and only the owner can close it.

Evidence toward AC #1 (pad reachability and attachment): the SWD pad layout and cable orientation are now documented - 14 pads, straight-through 1:1 STDC14 ribbon, pin 10 nRESET, orientation found by continuity (ground pattern measured on this board). STILL TO RECORD: whether the pads are reachable with the Seed seated in the Pod, and the attachment method actually used (soldered hairlines, pogo pins, or Seed outside the Pod). The STDC14 signal assignment is from memory of UM2910, not re-checked.

Evidence toward AC #5: probe is an ST-Link V3, firmware string V3J15M7, probe-rs 0.32.0, OpenOCD 0.12.0. STILL TO RECORD: attach time, flash time, log throughput, and confirming that firmware meets probe-rs's minimum.

Finding that changes the plan for AC #2-#4: setting DBGMCU_CR.TRACECLKEN (bit 20) kills the debug port on this board until the Seed is power-cycled. probe-rs's STM32H7 sequence sets it on every connect, so any --chip STM32H750IBKx attach wedged the port; OpenOCD's stm32h7x.cfg hook fails identically. RESET, --connect-under-reset and replugging the probe do not recover it. Workaround: firmware/asperitas-h750.yaml, the stock STM32H750IB entry renamed ASPERITAS_H750IB, passed by the probe-* Makefile targets via --chip-description-path. Cause of the trace-clock fault is unknown. Because that sequence never sets DBGMCU_CR, firmware that sleeps in WFI must set the debug-in-sleep bits itself.

Measured with the workaround: a flash read (probe-rs read b32 0x08000000 4) returns the vector table on a freshly power-cycled board without wedging the port.
NOT YET MEASURED (per the commit): flashing with the renamed chip, --connect-under-reset, RTT. So AC #2, #3 and #4 are all still open, and note AC #4 (recovery by re-attaching under reset) now needs re-checking in light of the finding that under-reset did not recover a wedged port.

Unblocked: the probe is attached and both dependencies (TASK-053, TASK-054) are completed, so status moved from Blocked to To Do. Remains @human.

AC #1 attachment method, per the owner 2026-10-07: a pin header soldered onto the Seed3's 14-pad debug footprint (the centre 10 pins are the active ones). Not hairlines or pogo pins. STILL TO RECORD for AC #1: whether the Seed can be seated in the Pod with that header fitted (clearance), or whether it has to run outside the Pod.

AC #1 seated-in-Pod answer, per the owner 2026-10-07: the Seed3 seats in the Pod with the debug header fitted. The header sits on the top face and the Pod connects to the Seed's main pins from underneath, so there is no clearance conflict and probe work and Pod control-surface work can happen together. Together with the soldered-header note above, AC #1's facts are all recorded; the criterion is left unticked for the owner to close.

CORRECTION 2026-10-07: the pre-flight note above that says to check ST-Link firmware with `probe-rs st-link doctor` and fix with `probe-rs st-link upgrade` is wrong for probe-rs 0.32.0 - `probe-rs st-link` is not a subcommand (error: unrecognized subcommand 'st-link'), and no 0.32.0 subcommand prints probe firmware. `probe-rs list` shows only 'STLink V3 -- 0483:3754:... (ST-LINK)'. The firmware string V3J15M7 recorded earlier therefore came from another tool (not identified in the notes). AC #5's firmware-version check needs ST's own tooling (e.g. STM32CubeProgrammer or STLinkUpgrade). Observed same day: generic `probe-rs info --protocol swd` failed on the first attempt ('The connected chip could not automatically be determined') and succeeded immediately on a second run with --verbose, showing DPv2 / STMicroelectronics / part 0x4500 and the ROM tables; one transient attach failure to count under AC #5's 'anything flaky'.

Provenance of the V3J15M7 string (2026-10-07): the owner does not remember where it came from. It first appears in docs/reference/daisy-seed3.md next to the OpenOCD 0.12.0 trace, and that is the format OpenOCD prints for an ST-Link at startup, so it most likely came from OpenOCD's output. Inference, not confirmed. Whether V3J15M7 meets probe-rs's minimum (V3MIN 0x0359 or V3SET 0x0410, per the pre-flight facts above) is NOT established: AC #5 still needs the version confirmed and compared against that floor. Since probe-rs 0.32.0 attached and read memory with this probe, it did not refuse it, which is weak evidence it is acceptable.

CONFIRMED 2026-10-07 (agent-run, OpenOCD 0.12.0 from /nix/store, interface/stlink.cfg + target/stm32h7x.cfg with stm32h7x.cpu0 examine-end cleared, adapter speed 1000, hla_swd): 'Info : STLINK V3J15M7 (API v3) VID:PID 0483:3754', 'Target voltage: 3.249600', '[stm32h7x.cpu0] Cortex-M7 r1p1 processor detected', 8 breakpoints, 4 watchpoints, gdb server started; init completed and shut down cleanly with no SwdDpError. This replaces the inference above: V3J15M7 is what OpenOCD reports for this probe. Still open for AC #5: comparing V3J15M7 against probe-rs's minimum (not done; the V3MIN 0x0359 / V3SET 0x0410 figures are internal probe-rs version codes and I have not mapped them to the J15 string), plus attach time, flash time, log throughput.

Agent-run bench session 2026-10-07 (probe-rs 0.32.0, ST-Link V3J15M7, chip ASPERITAS_H750IB via asperitas-h750.yaml, binary main built with FEATURES='seed3 log-defmt' NO_DEFAULT=1). The owner was not asked to press BOOT or RESET; board state before the run was not checked.

AC #2 evidence: `make probe-flash` with the default --connect-under-reset: 'Finished in 3.70s', --verify and --reset passed, exit 0; whole make 5.14 s wall (build was already cached, 0.43 s). This is the first measured --connect-under-reset on this board; commit bbd2091 listed it as unmeasured. Afterwards `probe-rs read b32 0x08000000 4` returned 24080000 08000299 08006ca5 08009891, so the debug port was not wedged by flashing. One run only; repeat before calling it reliable.

AC #3 partial: `probe-rs attach ... --non-interactive` (no reset) attaches, reports 'Target voltage (VAPP): 3.25 V', uses DefaultArmSequence, scans RTT at exact address 0x24000008 and attaches to RTT channel 0. NO log lines were received in two 12-20 s captures, so I cannot say whether the firmware logs anything periodically or whether decoding works; not tested: live defmt output and a forced panic with a decoded backtrace. Note: `attach --list-rtt` does not exit on its own in 0.32.0 - it stays attached and streams, so `make probe-rtt-list` blocks forever and needs a timeout around it (checked at -20 s with no printed listing).

AC #4 not tested (no hung binary built; recovery by re-attach under reset still unverified, and the earlier finding that under-reset did not recover a TRACECLKEN-wedged port still stands for the stock chip name).

AC #5 timings so far: flash 3.70 s (download+verify+reset). Attach time and log throughput not measured. No ticks made; every criterion is HUMAN.

FIX 2026-10-07: `make probe-rtt-list` no longer runs `probe-rs attach --list-rtt` (on 0.32.0 against a live target it attaches, finds the control block, then streams forever and prints no table - not a terminal issue, checked under a pty; not fixed by resetting first). It now runs scripts/probe-rtt-list.sh, two `probe-rs read b8` calls that decode the SEGGER control block and exit: ~0.24 s, rc 0. The boardless `attach --list-rtt` DWARF check in README/docs is unchanged and still valid (it fails at probe discovery before the hang). Failure paths checked: unreadable chip description -> rc 1 with message; no args -> rc 2. NOT checked: the script with no probe attached, and an ID mismatch. Commit tier green afterwards (13 gates).

Control block as read from the live board after the flash: id 'SEGGER RTT' at 0x24000008, 1 up channel 'defmt', 1024-byte ring, 0 down, write offset 0 and read offset 0, flags 1. An empty ring after flash+reset means main has not logged a single byte over RTT, although main.rs has info!('Booting...'); that matches the empty 12-20 s attach captures. Cause not established (main stuck before its first log, or those macros routing elsewhere). Next: flash panictest, which exists in firmware/src/bin, to see defmt output and a decoded panic.

Agent-run bench session, second part, 2026-10-07 (same probe, chip description and tooling as above). No ticks; all criteria HUMAN.

AC #3 (live defmt, panic): `make probe-run BINARY=panictest FEATURES='seed3 log-defmt' NO_DEFAULT=1 UNDER_RESET=0`, streamed for 60 s under a timeout: flash 'Finished in 2.67s', then exactly one line: '10.000030 [ERROR] PANIC: panictest: deliberate panic, exercising the LED + serial panic path at src/bin/panictest.rs:183:9 (asperitas_logging asperitas-logging/src/defmt_log.rs:201)'. So live defmt over RTT works, with a device timestamp and decoded file:line locations (the debug = 2 setting is doing its job). Two gaps against the criterion as written: (1) no countdown lines were captured before the panic, only the panic; why is unknown (the bin's header says countdown stages arrive over RTT when attached before run; this run attached before run, so the log macros may not be on the RTT path for those stages). (2) NO decoded stack backtrace appeared: the panic handler loops by design with no bkpt (panictest.rs:97, asperitas-logging panic_handler.rs:94), so the core never halts and probe-rs prints no backtrace. Getting a backtrace needs a halted core (e.g. attach with probe-rs debug / a gdb session after the panic) or a probe-only build with bkpt; that is a design question for the owner, not something to change unasked.

AC #2 / #4 flakiness: `--connect-under-reset` worked once (the probe-flash earlier today, 3.70 s) and then failed on every later try: probe-run under reset ended 'Timeout while attaching to target under reset' and three direct `probe-rs read ... --connect-under-reset` runs each failed in ~0.3 s (rc 1), while a plain attach read the vector table every time. Same board, same cable, no power cycle in between, so under-reset reliability is NOT established; 1 pass, 4 fails. Without --connect-under-reset: probe-run flashed in 2.67 s and `make probe-flash UNDER_RESET=0` in 3.60 s (3.93 s wall); both reset and ran the new image without touching BOOT or RESET. AC #4's re-attach-under-reset recovery of a hung binary can therefore not be assumed to work.

AC #5: flash times 3.70 s (under reset), 3.60 s and 2.67 s (no under-reset, 3.60 s was main, 2.67 s was panictest). Log throughput and attach time not measured. Board left running main, flashed with log-defmt, vector table 24080000 08000299 08006ca5 08009891.

AC #3 backtrace VERIFIED (agent-run, 2026-10-07), criterion wording left as written per the owner. `probe-rs run target/thumbv7em-none-eabihf/release/panictest --chip ASPERITAS_H750IB --chip-description-path asperitas-h750.yaml --non-interactive --always-print-stacktrace`, left running past the 10 s countdown, then SIGINT (Ctrl+C). Output: the same '10.000030 [ERROR] PANIC: panictest: deliberate panic ... at src/bin/panictest.rs:183:9' line, then 'Received Ctrl+C, exiting' and a 17-frame backtrace for Core 0 with a source file:line on every frame: Frame 0 nop (cortex-m call_asm.rs:19), Frame 1 handle_panic (crates/asperitas-logging/src/panic_handler.rs:103), Frame 2 panic_handler (panictest.rs:93), Frame 3 panic_fmt (core panicking.rs:80), Frame 4 the async block at panictest.rs:183:9 (the deliberate panic), Frames 5-15 embassy executor poll/run and __cortex_m_rt_main (panictest.rs:145), Frame 16 Reset @ 0x80002d2. So the way to get the backtrace is to let the panic happen and Ctrl+C a probe-rs session that has --always-print-stacktrace (make probe-run PROBE_EXTRA='--always-print-stacktrace'); the panic handler does not halt the core on its own (no bkpt, by design), but probe-rs halts it on Ctrl+C and unwinds. Caveat: SIGTERM does not do this, only SIGINT; and the countdown lines before the panic still did not appear (open question, not part of the criterion's text). Board restored to main afterwards (flash 3.66 s, vector table unchanged).

Under-reset count update: since the one pass, `--connect-under-reset` has now failed 10 of 10 on `probe-rs read` (1 s apart) and 4 of 4 on `make probe-flash` (default under-reset), all with 'Timeout while attaching to target under reset' (flash) or rc 1; plain attach succeeded every time in between. Cumulative: 1 pass, 14+ fails. Recovery is not blocked: the no-reset path always attaches and UNDER_RESET=0 flashes. Untested: whether a USB-C power cycle makes under-reset work again (the one pass may have followed a fresh power-up), and whether it can recover a hung binary.

Under-reset investigation 2026-10-07 (agent-run, owner power-cycled the Seed's USB-C first):
- After the power cycle `--connect-under-reset` still failed: 5/5 on `probe-rs read`, 1/1 on `make probe-flash`. So not a stale target state.
- After erasing the flash (`probe-rs erase`, no under-reset; vector table read back ffffffff x4) it still failed 5/5. So not caused by the firmware image on the chip.
- After a bare OpenOCD init/shutdown (OpenOCD 0.12.0, stm32h7x.cfg with examine-end cleared, hla_swd, 1000 kHz) the very next `make probe-flash` under reset SUCCEEDED ('Finished in 3.61s', main restored, vector table 24080000 08000299 08006ca5 08009891). The earlier single pass (3.70 s) also came directly after an OpenOCD run.
- Immediately afterwards, with no OpenOCD in between, `probe-rs read --connect-under-reset` failed 3/3 and `make probe-flash` failed again with 'Timeout while attaching to target under reset'.
So the working hypothesis: probe-rs leaves the ST-Link in a state where its next under-reset attach times out, and an OpenOCD session (which re-initialises the probe) clears it for one operation. Evidence is two passes, both right after OpenOCD, and ~20 fails otherwise; the mechanism (probe mode/reset line state) is NOT identified, and a probe USB replug as an alternative reset was not tried. Recovery is not blocked: a plain attach always works and UNDER_RESET=0 flashes; OpenOCD also works as the reset. Whether under-reset recovers a hung binary (AC #4) remains untested.

Under-reset root-cause attempt 2026-10-07 (agent-run). Refuted: the idea that skipping probe-rs's STM32H7 DBGMCU sequence leaves the debug clocks off. DBGMCU_CR (0x5C001004) already read 0x00600007 (D1/D3 debug clocks on); writing 0x0060003F (the safe bits only, no TRACECLKEN) read back 0x0060003f, and --connect-under-reset still failed 3/3.
Where it stalls, from RUST_LOG=probe_rs=debug on a failing `probe-rs read ... --connect-under-reset`: 'Using sequence Arm(DefaultArmSequence)'; 'Custom reset sequences are not supported on ST-Link V3. Falling back to standard probe reset.'; 'Asserting target reset'; 'Target voltage (VAPP): 3.25 V'; 'Successfully initialized SWD.'; then 'ERROR probe_rs::session: Unable to wait for 0 halted: An ARM specific error occurred.' and 'Timeout while attaching to target under reset'. So SWD comes up with nRESET held and the failure is halting core 0 afterwards, not connecting. The probe also reports 'Current device mode: MassStorage' at the start of every session and 'Jtag' at the end. Not established why only the first probe-rs operation after an OpenOCD session gets past the halt.
asperitas-h750.yaml is a chip description (the stock STM32H750IB entry renamed), not a patched probe-rs; reset sequences are compiled Rust in probe-rs, so no YAML change can alter this. Options if it matters: keep UNDER_RESET=0 as the working default (verified: always attaches), or investigate/patch the Rust ARM sequence for the halt-under-reset step upstream. Not done.

Under-reset ROOT CAUSE FOUND and FIXED 2026-10-07 (agent-run; commit 'flake: patch probe-rs so --connect-under-reset halts the core'). This supersedes the 'probe-rs leaves the ST-Link in a bad state' hypothesis above, which was wrong. Since probe-rs #3485, the under-reset path arms DEMCR.VC_CORERESET, releases nRESET, and only then sets DHCSR.C_DEBUGEN; vector catch needs C_DEBUGEN already set, so the core ran past it. C_DEBUGEN survives nRESET (only power-on clears it): OpenOCD leaves it set (hence the one-shot passes), and DefaultArmSequence's session teardown writes DHCSR=0 (hence every later failure with our renamed chip). Controlled test: OpenOCD leaving DHCSR C_DEBUGEN=0 vs 1 before a stock probe-rs under-reset read gave 0/3 vs 3/3. Fix: nix/probe-rs-cortex-m-reset-catch.patch, applied to probe-rs-tools in flake.nix. Write-up: docs/upstream/probe-rs-connect-under-reset.md.

AC re-check after the fix (2026-10-07, agent-run, no ticks; every AC is HUMAN):
- AC #1: facts already recorded above (soldered header on the 14-pad footprint, Seed seats in the Pod with it fitted). Nothing new; ready for the owner to close.
- AC #2: with the patched probe-rs from `nix develop`, `probe-rs read --connect-under-reset` passed 5/5, and `make probe-flash FEATURES='seed3 log-defmt' NO_DEFAULT=1` (default --connect-under-reset --verify --reset) passed 3/3, the last 'Finished in 3.68s' with make rc 0. Before that, the scratch-built patched binary passed 8/8 reads and 2/2 downloads (3.55 s each), against 0/4 stock runs interleaved. No BOOT or RESET press at any point. Each run started with C_DEBUGEN cleared by the previous session's teardown, so this is not OpenOCD priming. Not tested: the first attempt straight after a USB-C power cycle (power-on clears C_DEBUGEN; the patch sets it regardless, so this is expected to pass, but it is unmeasured).
- AC #3: unchanged, see the backtrace note above.
- AC #4: NOT TESTED. Prepared, not flashed: a throwaway image built in the agent scratchpad (not in the repo) whose reset handler turns PA13/PA14 (SWDIO/SWCLK) into GPIO outputs and spins, so USB never comes up and a plain attach should fail. The plan was: flash it under reset, show that a plain attach fails, show that an under-reset attach succeeds, then reflash main under reset. The agent's permission policy blocked flashing it; it is the owner's call. Fallback if recovery failed: BOOT+RESET DFU.
- AC #5: unchanged except flash times under reset are now 3.55-3.68 s. Attach time and log throughput still unmeasured (an attach-timing run was also blocked by the permission policy). Firmware V3J15M7 vs probe-rs's minimum is still not compared.
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

created: 2026-09-12 21:34
---
Bench ask from TASK-057's planning pass (2026-09-12): when you have the probe on and the board in reach, also run `make flash-all BINARY=blinky` exactly as the corrected quickstart in docs/reference/daisy-seed3.md now spells it, and confirm steady green LED 1. That recipe was rewritten because the old one flashed main.bin while claiming to flash blinky; nothing in TASK-057 could verify the new wording beyond its `make -n` expansion, so this is the human half of closing it. Cheap to fold into the same session - it costs one BOOT+RESET tap.
---
<!-- COMMENTS:END -->
