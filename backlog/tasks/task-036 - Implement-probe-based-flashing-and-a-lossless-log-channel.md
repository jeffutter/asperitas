---
id: TASK-036
title: Implement probe-based flashing and a lossless log channel
status: Blocked
assignee:
  - '@agent'
created_date: '2026-09-09 01:28'
updated_date: '2026-09-09 22:08'
labels:
  - planned
dependencies:
  - TASK-036.01
  - TASK-036.02
  - TASK-036.03
  - TASK-036.04
references:
  - 'https://probe.rs/docs/getting-started/probe-setup/'
documentation:
  - docs/reference/daisy-seed3.md
  - docs/reference/rust-daisy-stack.md
modified_files:
  - firmware/Makefile
  - crates/asperitas-logging/src/lib.rs
  - flake.nix
priority: high
type: feature
ordinal: 47000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
An ST-Link V3 MINIE is on order, and the software side can land ahead of it. Nothing here needs the physical probe to write and compile-verify.

Two constraints disappear once a probe is in use. Every flash currently needs a hand on BOOT and RESET, which is the reason firmware changes cannot be verified in an unattended loop. And every diagnostic byte crosses the USB console link, which loses records — TASK-030 makes that loss visible, but RTT shares nothing with the USB stack at all, so the loss stops being possible rather than becoming reportable. docs/reference/rust-daisy-stack.md already lists probe-rs as the intended tool once a probe exists, and docs/reference/daisy-seed3.md records the class of fault this would have named immediately: the RAM-length mistake hard-faulted before main in every binary, and diagnosing it meant reasoning about the first four bytes of the binary because there was no way in.

Physical unknowns are deliberately not resolved here and belong to TASK-037: the Seed3's extra ST-LINK-V3MINIE-style pads are documented as present only for mechanical alignment and not wired up, so attachment uses the 10-pin Cortex Debug footprint, and whether those pads are reachable with the Seed seated in the Pod is unverified. If they are not reachable, probe work and Pod control-surface work may not be simultaneously possible, which is worth knowing before anything is soldered.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A make target flashes over the probe using attach-under-reset, so no BOOT or RESET interaction is needed, and verifies what was written.
- [ ] #2 Existing DFU targets keep working unchanged — the probe path is additive and a board with no probe attached is still flashable.
- [ ] #3 defmt over RTT is selectable behind a Cargo feature, the existing USB console facade remains selectable, and binaries selecting neither still build.
- [ ] #4 The probe configuration compiles for thumbv7em-none-eabihf and is clippy-clean, which is verifiable without a board.
- [ ] #5 Panic diagnostics reach the host over RTT when that feature is selected, at code level; hardware confirmation belongs to TASK-037.
- [ ] #6 docs/reference/daisy-seed3.md's debugging sections describe both channels and say which to reach for, and rust-daisy-stack.md's toolchain note reflects the probe as available rather than aspirational.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## What this umbrella is

Four leaves, all `@agent`, none of which need the probe in hand. The physical half was split out as
TASK-037 (`@human`) before this plan existed and remains the only place a hardware claim gets made.

**This umbrella stays `Blocked`, not `Dev Ready`.** Every one of its six acceptance criteria is
delivered by a leaf; there is no direct implementation left here. That is the loop's own convention
for a fully-delegated ticket, and it matters mechanically: Execute takes the first Dev Ready ticket
in list order without re-checking dependencies, and this parent sorts ahead of its children by ID,
so a `Dev Ready` umbrella would be selected first and could produce nothing — the TASK-004 spin that
`CLAUDE.md` records. Promote it to `Dev Ready` only when all four leaves are Done **and** the
integration pass below has run.

## The breakdown, and why it is cut here

| Leaf | Ships | Why it stands alone |
|---|---|---|
| .01 | probe make targets, `--chip`/`--verify` flags, DWARF line tables | toolchain only; touches no Rust source; useful even if the log backend never lands |
| .02 | `led` and `panic_handler` independent of `log-usb` | a refactor that must be green by itself, or the defmt backend inherits a transport-coupled panic path |
| .03 | `log-defmt` backend, defmt 1.x unification, per-binary logger selection, panic over RTT | the only leaf with real design risk |
| .04 | documentation of both channels and what each one loses | needs .01's flags and .03's real behaviour written down truthfully |

Order `.01 → .02 → .03 → .04`; declared as `.03 ← {01,02}` and `.04 ← {01,03}`.

## Facts the whole ticket rests on — verified locally, do not re-derive

* `probe-rs` and `cargo-flash` **0.32.0** are already in the dev shell (`flake.nix:44`); confirmed
  by `nix eval` against the pinned nixpkgs. Nothing to add. Darwin needs no udev rule; a Linux bench
  would need `services.udev.packages = [ pkgs.probe-rs-tools ]` for `/dev/bus/usb/*`.
* Chip string accepted by the pinned build: `STM32H750IBKx` (bare `STM32H750IB` too), reporting NVM
  `0x08000000..0x08020000` and RAM `0x24000000..0x24080000` — the RAM region the RTT control block
  lives in is described by the target, so `--scan-region` is a fallback, not a necessity.
* Flag spellings confirmed against `--help` on 0.32.0: `--connect-under-reset`, `--verify`,
  `--preverify`, `--binary-format`/`--base-address`, `--scan-region`, `--reset`, `--chip`, and
  `--rtt-channel-mode {no-block-skip,no-block-trim,block-if-full}` with `block-if-full` the default.
  `probe-rs list` with nothing attached prints exactly `No debug probes were found.` — that string is
  the expected terminal state of every agent-side check in .01.
* Current crates: defmt-rtt 1.3.0, rtt-target 0.6.2, panic-probe 1.0.0. None are in the local cargo
  cache today, so the first build naming them needs network.
* `defmt 0.3.100` in `firmware/Cargo.lock` is a shim over defmt 1.1.1; daisy-embassy (ca9bcc9) and
  embassy-stm32 already call defmt directly, which is the only reason the five no-op logger stubs
  exist.
* Neither CI nor lefthook invokes make for firmware, so new targets cannot break either gate.

## The premise in the description, corrected

The description claims loss "stops being possible rather than becoming reportable". That holds only
while a host is attached. defmt-rtt initialises its up-channel in `MODE_NON_BLOCKING_TRIM`
(src/lib.rs:113) and probe-rs flips it to block-if-full on attach; unattached, records drop
silently, and attached-with-a-stalled-host the target spins inside a critical section — probe-rs's
own flag help warns of exactly that. So RTT is lossless-while-attached and can cost audio time,
which is acceptable only because nothing logs from the audio callback (`main.rs:236-255`) and
unacceptable the moment something does. .04 puts this in the reference document so nobody meets it
on hardware for the first time.

## Integration verification, once all four leaves are Done

Run this pass while the ticket still reads `Blocked`, then flip it to `Dev Ready` so Execute closes
it out with a commit that touches only the docs or nothing at all.

1. In `firmware/`: the four-configuration matrix from .03 builds, and `FEATURES="seed3"` yields a
   `firmware.bin` of the size .01 recorded at its step 0.
2. `make -n flash probe-flash probe-log probe-run` — DFU path unchanged, probe path fully expanded.
3. Host gates: `cargo fmt --all --check`, `cargo test --workspace`, and both clippy invocations from
   `ci.yml:23-32`.
4. Grep the docs for surviving future-tense probe claims and for broken anchors left by .04's
   renames.
5. Only then check this ticket's six acceptance criteria, each by naming the leaf that delivered it.
   Criterion #2 is proven by a diff, #4 by the clippy target, #5 by code reading — with the hardware
   half belonging to TASK-037.

## Deliberately not here

Anything needing the probe present (TASK-037: whether `--connect-under-reset` works on the V3
MINIE, ST-Link firmware ≥ 3.2, whether the SWD pads are reachable with the Seed in the Pod, attach
and flash timings); `panic-probe`, unless TASK-037 shows backtraces do not decode; wiring firmware
clippy into CI; carrying the framed console over plain RTT instead of defmt — which is impossible
anyway, since defmt-rtt declares `_SEGGER_RTT` itself precisely so `rtt-target` cannot coexist with
it; and the v1 console grammar documentation owned by TASK-030.03, which .04 must extend rather
than rewrite.
<!-- SECTION:PLAN:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-09 22:08
---
Planning complete: work fully delegated to TASK-036.01-.04, all Dev Ready. Status left at Blocked deliberately — see Implementation Plan §"What this umbrella is". Do not promote to Dev Ready until all four leaves are Done and the integration pass has run.
---
<!-- COMMENTS:END -->
