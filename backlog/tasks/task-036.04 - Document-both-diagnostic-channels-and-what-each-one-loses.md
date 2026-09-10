---
id: TASK-036.04
title: Document both diagnostic channels and what each one loses
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-09 21:42'
updated_date: '2026-09-10 03:16'
labels:
  - planned
dependencies:
  - TASK-036.01
  - TASK-036.03
documentation:
  - docs/reference/daisy-seed3.md
  - docs/reference/rust-daisy-stack.md
modified_files:
  - docs/reference/daisy-seed3.md
  - docs/reference/rust-daisy-stack.md
  - README.md
parent_task_id: TASK-036
priority: high
type: task
ordinal: 70500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Three documents currently describe the probe as something that might happen someday: docs/reference/daisy-seed3.md:158 is headed "Debug probe, when one is available" and says only which pads are wired; :148 opens with "Losing probe-rs also loses defmt/RTT logging"; README.md:200 promises "When a probe arrives, probe-rs restores cargo run-style flashing and RTT logging"; rust-daisy-stack.md:101 lists probe-rs with "**requires an ST-Link probe**". After .01 and .03 those sentences are false, and a false reference document is worse than none — it is what makes the next session waste an hour rediscovering that the chip string is accepted, or that the probe target needs the ELF.

The part that must not be written as marketing: the two channels count different things. The framed USB console reports sequence gaps and `dropped_full` for records it was asked to send; RTT reports nothing about what it dropped, drops silently whenever no host is attached, and can freeze a critical-section writer when the ring fills while attached. A loss number from one does not describe the other, and the docs should say that in one sentence a tired person at 1am can act on.

Boundary with TASK-030.03, which is still To Do and edits the same region (daisy-seed3.md:146-157, README.md:163-200) with no dependency either way: that ticket owns the v1 record grammar, CRC parameters, short-packet rule and loss-counter documentation. This one owns the probe channel and the "which channel do I reach for" decision. If 030.03 has landed by the time you pick this up, extend its text rather than rewriting it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 docs/reference/daisy-seed3.md replaces "Debug probe, when one is available" with a section that states the actual workflow — make probe-flash, make probe-log, the --chip STM32H750IBKx string, why the probe path takes the ELF and DFU takes firmware.bin — and every command it prints appears verbatim in `make -n` output.
- [x] #2 The same file names the three RTT regimes explicitly: unattached (NON_BLOCKING_TRIM, records dropped or truncated), attached-and-keeping-up, and attached-but-stalled (block-if-full spinning inside a critical section, i.e. the application freezes), together with probe-rs --rtt-channel-mode no-block-skip and no-block-trim as the host-side escape hatch and the rule that nothing may log from the audio callback.
- [x] #3 The corrected sentence at daisy-seed3.md:148 no longer frames probe-rs as lost, and the RAM-length hard-fault note (:137-144) now points at the probe as the way to see a fault that fires before main instead of at reasoning about the first four bytes of firmware.bin.
- [x] #4 Any heading rename keeps cross-document anchors resolving: rust-daisy-stack.md:103 links to #flashing-without-a-debug-probe, and if that heading changes, the link changes in the same commit. Checked by grepping every relative link and anchor in docs/ and README.md.
- [x] #5 README.md replaces the future-tense promise at :200 with the real commands and adds the Linux-only permission caveat: probe-rs talks to the ST-Link over /dev/bus/usb/*, not /dev/ttyACM*, so a Linux bench needs services.udev.packages = [ pkgs.probe-rs-tools ]; macOS needs nothing.
- [x] #6 rust-daisy-stack.md:101 reflects the pinned reality — probe-rs/cargo-flash 0.32.0 supplied by the flake, chip string, ELF-not-.bin — and stops reading as aspirational.
- [x] #7 One paragraph states that the two channels count different things and that a loss figure from one does not describe the other, and every hardware claim in the new text is attributed to TASK-037 as still-unverified rather than stated as measured.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Approach

Three files, and the discipline to write what the channels actually do rather than what would be
nice. Every command you print must come from `make -n`; every hardware claim gets attributed to
TASK-037 as unverified.

## Step 0 — check who landed first

TASK-030.03 edits the same region (`docs/reference/daisy-seed3.md:146-157`, `README.md:163-200`)
and neither ticket depends on the other. Read the current text before writing: if 030.03 has
landed, extend its grammar/CRC/short-packet documentation and slot the probe material beside it. If
it has not, leave its subject matter alone entirely — the v1 record grammar is not this ticket's to
describe.

## Step 1 — `docs/reference/daisy-seed3.md`

Heading structure today: `## Flashing without a debug probe` (69) with `### Enter DFU mode` (76),
`### Build and flash blinky` (81), `### Notes` (118), `### Debugging without a probe` (146),
`### Debug probe, when one is available` (158).

Replace the future-tense probe section (158-162) with a real one: the commands from TASK-036.01,
the `--chip STM32H750IBKx` string, why the probe path takes the ELF while DFU takes
`firmware.bin`, `--verify` as read-back checking, and the `PROBE_EXTRA` fallback with the reason
(ST-Link V3 MINIE connect-under-reset reliability, probe-rs #3516). Keep the pad-wiring facts at
160-162 — they are true and TASK-037 AC #1 still has to check reachability with the Seed seated in
the Pod.

Correct line 148, which currently opens "Losing `probe-rs` also loses `defmt`/RTT logging" — that
framing is what makes the section read as pre-probe history.

Reword the RAM-length note at 137-144 so the closing sentence points at the probe as the way to see
a fault that fires before `main`, instead of at reasoning about the first four bytes of
`firmware.bin`. That sentence is the ticket's motivating story and it should say what to do now.

Add the three RTT regimes explicitly, because a reader must be able to predict behaviour from the
document:

| State | Mode | Consequence |
| no host attached | NON_BLOCKING_TRIM (defmt-rtt src/lib.rs:113) | records dropped or truncated, silently |
| attached, host keeping up | block-if-full set by probe-rs on attach | nothing lost |
| attached, host stalled | block-if-full | target spins inside a critical section; audio suffers |

Name the host-side escape hatch: `probe-rs --rtt-channel-mode no-block-skip` / `no-block-trim`
(values confirmed against `--help` on 0.32.0, default `block-if-full`), and the rule that follows
from row three — nothing logs from the audio callback.

Also worth one line each, both cheap to state and expensive to rediscover: STM32H750xB internal
flash is one 128 KB sector so any probe erase wipes the whole application; and sleep modes break
RTT discovery on several STM32 parts (probe-rs #350, `DBGMCU_CR` workaround), which is why the busy
loop in `executor-thread` is the safe state and not to be "optimised".

## Step 2 — anchor safety after any rename

`rust-daisy-stack.md:103` links to `daisy-seed3.md#flashing-without-a-debug-probe`. If that heading
(L69) is renamed — reasonable, since flashing now has two routes — the link changes in the same
commit. Check mechanically:

    grep -rn "](\./\|#flashing\|#debugging" docs README.md

## Step 3 — `README.md`

Replace the future-tense promise at :200 ("When a probe arrives, `probe-rs` restores `cargo run`-style
flashing and RTT logging.") with the actual commands, and retitle `## Debugging Without a Probe`
(163) so it covers both routes. Add the Linux-only permission caveat: probe-rs talks to the ST-Link
over `/dev/bus/usb/*`, not `/dev/ttyACM*`, so a Linux bench needs
`services.udev.packages = [ pkgs.probe-rs-tools ];` — macOS needs nothing. Note that the quick-start
line at :36 already lists `probe-rs` as provided, so the flake and the README agree once :200 is
fixed.

## Step 4 — `docs/reference/rust-daisy-stack.md`

Line 101 is the whole of acceptance criterion #6 for this file: replace
`probe-rs — flash + defmt/RTT logging; **requires an ST-Link probe**` with the pinned reality —
probe-rs/cargo-flash 0.32.0 from the flake, chip string, ELF-not-`.bin`, and the fact that the
probe is on order rather than hypothetical (hardware confirmation is TASK-037). Respect the file's
own snapshot warning at L3-5: date what you assert.

## Step 5 — the paragraph that keeps anyone honest

One short paragraph, in daisy-seed3.md where the two channels are introduced, stating that they
count different things: the framed console reports sequence gaps and `dropped_full` for records it
was asked to send, while RTT reports nothing about what it dropped and drops freely when unattached.
A loss figure from one does not describe the other, and a clean RTT stream is not evidence that a
capture is complete. Say it in flat prose, not a bulleted flourish — the reader is tired and holding
a screwdriver.

## Step 6 — verify

* Every command printed appears verbatim in `make -n` output.
* `grep -rniE "when a probe arrives|when one is available|requires an st-link|losing .probe-rs" docs README.md` returns nothing.
* Every relative link and anchor resolves.
* No new sentence asserts a measured number that TASK-037 has not measured; attach times, flash
  times and throughput stay blank until then.

## Boundary notes

* TASK-030.03 owns the v1 grammar, CRC parameters, short-packet rule and loss-counter prose. Stay
  out of it unless extending.
* Do not touch CLAUDE.md's summary of these documents in passing; if its one-line description goes
  stale, that is its own small ticket.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
From TASK-036.03: when documenting the RTT channel, lead with the filter default. `DEFMT_LOG` unset makes defmt-macros compile every non-ERROR call to nothing (defmt-macros-1.1.1 src/function_like/log/env_filter.rs:34), so a plain `make build FEATURES="seed3 log-defmt" NO_DEFAULT=1` image has no INFO frames and an attach looks like a dead channel. The facade's runtime set_max_level cannot compensate. Working form: `DEFMT_LOG=info make probe-log FEATURES="seed3 log-defmt" NO_DEFAULT=1`. Also: only a log-defmt ELF loads under probe-rs at all (build.rs adds -Tdefmt.x; console-only ELFs are rejected before probe discovery). Both facts are already in crates/asperitas-logging/src/defmt_log.rs and the firmware/Makefile preamble.

TASK-030.03 had NOT landed at pickup (status To Do), so its subject matter is untouched: no record grammar, no CRC parameters, no short-packet rule. 'What each channel loses' names the console counters (seq gaps, dropped_full, bytes_dropped, trunc, ep_err) only as far as AC#7 requires and says explicitly that TASK-030.03 owns the field set.

Heading renames: '## Flashing without a debug probe' -> '## Flashing the Seed3' (rust-daisy-stack.md link updated to #flashing-the-seed3 in the same edit) and '### Debugging without a probe' -> '### Debugging with nothing attached'. README '## Debugging Without a Probe' -> '## Debugging', since it now contains a probe subsection. Anchor check is mechanical, not eyeballed: a python pass extracts every ](...) target in README.md, CLAUDE.md, docs/**/*.md and audio/README.md, resolves relative paths and GitHub-slugged anchors -> all resolve. Only one inbound anchor existed repo-wide, which is what made the rename cheap.

Verbatim-command check: extracted every cargo/probe-rs line from the code blocks in the new probe section and matched against 'make -n probe-flash probe-log FEATURES="seed3 log-defmt" NO_DEFAULT=1' output normalised for whitespace -> 3 of 3 match exactly. The printed make invocations themselves dry-run rc=0, including PROBE_EXTRA="--speed 1000".

Host-side facts re-measured here rather than inherited from .01/.03 notes: probe-rs 0.32.0 'chip info STM32H750IBKx' -> NVM 0x08000000..0x08020000 128 KiB, AXI SRAM 0x24000000..0x24080000 512 KiB (bare STM32H750IB resolves too); 'attach --help' -> rtt-channel-mode values no-block-skip / no-block-trim / block-if-full with block-if-full default, and row three of the RTT table quotes its own warning about freezing inside a critical section; defmt-rtt 1.3.0 src/lib.rs:113 NON_BLOCKING_TRIM init and :168 critical_section::acquire(); probe-rs-tools-0.32.0 store path really does carry etc/udev/rules.d/69-probe-rs.rules, which is what makes the services.udev.packages claim safe to print. Nothing about this board is asserted anywhere in the new text: attach/flash/throughput numbers stay absent and every such claim points at TASK-037.

Also fixed a dangling reference found while grepping: firmware/Cargo.toml:29 pointed at docs/reference/diagnostics.md, a file TASK-036.03 anticipated but never created. Now names the real section. Follow-up filed as TASK-042 for CLAUDE.md's now-incomplete one-line index entry, per this ticket's instruction not to edit CLAUDE.md in passing.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Three reference documents stopped describing a probe that has since been wired up. daisy-seed3.md: heading '## Flashing without a debug probe' -> '## Flashing the Seed3' with an intro naming both routes; '### Debug probe, when one is available' replaced by '### Flashing and logging over an ST-Link probe' carrying the real workflow (make probe-flash / make probe-log, expanded commands copied verbatim out of make -n, --chip STM32H750IBKx with its chip-info numbers, ELF-not-firmware.bin and why, --verify as probe-rs read-back rather than dfu-util's grep idiom, PROBE_EXTRA and probe-rs #3516); the DEFMT_LOG-unset default and the log-defmt-only-ELF gate lead the section so a silent channel isn't misread as a dead probe; the three RTT regimes as a table with --rtt-channel-mode no-block-skip / no-block-trim as the host-side escape hatch and 'nothing logs from the audio callback' as the rule row three implies; single-128 KB-sector erase and DBGMCU_CR/sleep-vs-RTT-discovery (#350) one line each; new '### What each channel loses' stating that the console keeps a ledger and RTT keeps none. 'Losing probe-rs also loses defmt/RTT logging' became '### Debugging with nothing attached', and the memory.x RAM note now sends a pre-main fault to the probe instead of to firmware.bin's first four bytes. README.md gained '### Flashing and logging over an ST-Link probe' with the working commands and the Linux-only /dev/bus/usb udev caveat (services.udev.packages = [ pkgs.probe-rs-tools ]; macOS needs nothing), replacing the future-tense promise. rust-daisy-stack.md:101 states the pinned reality (probe-rs/cargo-flash 0.32.0 from the flake, chip string, ELF not .bin, dated 2026-09-10 per that file's snapshot warning) and its link follows the rename. Every hardware claim is attributed to TASK-037 as unmeasured; no timing or throughput number appears anywhere. Also repaired firmware/Cargo.toml's pointer at a docs/reference/diagnostics.md that was never written, and filed TASK-042 for CLAUDE.md's now-incomplete index line.
<!-- SECTION:FINAL_SUMMARY:END -->
