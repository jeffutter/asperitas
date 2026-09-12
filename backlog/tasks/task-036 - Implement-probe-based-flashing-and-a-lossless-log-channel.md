---
id: TASK-036
title: Implement probe-based flashing and a lossless log channel
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-09 01:28'
updated_date: '2026-09-12 08:14'
labels:
  - planned
dependencies:
  - TASK-036.01
  - TASK-036.02
  - TASK-036.03
  - TASK-036.04
  - TASK-036.05
  - TASK-036.06
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
- [x] #1 A make target flashes over the probe using attach-under-reset, so no BOOT or RESET interaction is needed, and verifies what was written.
- [x] #2 Existing DFU targets keep working unchanged — the probe path is additive and a board with no probe attached is still flashable.
- [x] #3 defmt over RTT is selectable behind a Cargo feature, the existing USB console facade remains selectable, and binaries selecting neither still build.
- [x] #4 The probe configuration compiles for thumbv7em-none-eabihf and is clippy-clean, which is verifiable without a board.
- [x] #5 Panic diagnostics reach the host over RTT when that feature is selected, at code level; hardware confirmation belongs to TASK-037.
- [x] #6 docs/reference/daisy-seed3.md's debugging sections describe both channels and say which to reach for, and rust-daisy-stack.md's toolchain note reflects the probe as available rather than aspirational.
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

## Integration pass RAN 2026-09-10 — outcome, and what now gates promotion

The five steps above were executed; measurements and the criterion→leaf attribution are in
Implementation Notes. All six acceptance criteria are checked. Summary of the verdict: **the four
original leaves delivered what they claimed**, verified against source rather than their own notes —
`make -n` expansions, a 0-line diff on the DFU path, the four-configuration matrix, every host gate,
and an audit finding no false probe claim or broken anchor left by `.04`.

Promotion to `Dev Ready` is **still withheld**, for one reason that did not exist when this plan was
written: the pass surfaced two gaps, filed as `TASK-036.05` and `TASK-036.06`, both `@agent`, both
planned and Dev Ready.

* `.05` matters because the RTT backend's cfg pairs are compiled by *no unattended gate* — CI and
  lefthook only ever build `log-usb` and `--features seed3`. Criterion #4 is true today and nothing
  keeps it true, which is how a later agent breaks defmt with eight green gates.
* `.06` matters because TASK-037 spends the scarcest resource here (a person at the bench) and would
  walk in without three things it will need: the fact that V3-class probes skip custom reset
  sequences so under-reset failing is expected, a Makefile variable to drop that flag, and the
  D-cache↔RTT trap that any future caching change springs silently.

So this umbrella stays where its own convention put it — `Blocked`, `planned`, fully delegated. When
`.05` and `.06` are Done: re-run steps 2 and 3 of the integration list (the DFU-expansion diff and
the host gates, since `.05` edits both gate definitions), confirm nothing else moved, then promote.
Do not promote while either leaf is open — by ordinal this parent sorts ahead of every child, and
Execute takes the first Dev Ready ticket without re-checking dependencies, which is the TASK-004
spin recorded in CLAUDE.md.

## Integration re-run at planning time (2026-09-12), all six leaves Done

Every gate in `.github/workflows/ci.yml` was run inside `nix develop .#default` against the tree as it
stands with all six leaves complete: `cargo fmt --all --check`, the four clippy invocations (`--workspace
--all-targets --lib --bins`, `--workspace --lib --examples --features host_target`, `-p asperitas-core
--no-default-features --lib`, `-p asperitas-logging --features log-defmt --lib`), both
`RUSTDOCFLAGS="-D warnings"` doc runs, `cargo test --workspace`, `cargo run --bin dump -- --selftest`,
and both firmware cross-compiles plus their two clippy invocations. All returned 0.

The DFU path is still provably unchanged by TASK-036.01's Makefile work: diffing the pre-change recipe
expansions against the current ones for `build flash flash-all check` shows no semantic delta, only an
empty-variable double space in the rustc line from `$(CARGO_DEFAULTS)`.

## Follow-up tickets raised from this planning pass (siblings, not children)

Re-reading the shipped probe surface and the loss model against upstream sources turned up work that does
not belong under this umbrella, whose own acceptance criteria are all met. They are siblings so closing
them cannot reopen TASK-036, and so TASK-036 can reach Dev Ready now.

* **TASK-053** - make the probe path safe to drive unattended. Our three probe recipes omit
  `--non-interactive`, which hangs forever on stdin; `probe-log` decodes a running board with whatever ELF
  happens to be on disk; nothing records the probe path's exit-code contract or the recovery levers
  (`--cycle-power`, `--read-flasher-rtt`, `--dry-run`, `--disable-double-buffering`); and there is no
  non-destructive smoke target. Host-side only, no board.
* **TASK-054** - choose the release DWARF level deliberately. Measured here: `probe-rs attach --list-rtt`
  prints "Insufficient DWARF info; compile your program with `debug = 2` to enable location info" on every
  build except `debug = 2`, so today's `line-tables-only` silently costs defmt log locations over the
  channel this ticket built. Moving to `2` costs 236 bytes of flash and turns a 3 MB host ELF into 9.5 MB.
* **TASK-055** - correct six wrong or unenforced statements in the RTT record (a mis-cited defmt-rtt code
  path behind the frame-size margin, a manifest claim Cargo.lock contradicts, stale binary sizes, the
  missing host-detaches-mid-run loss regime, an understated cache-alignment requirement with both block
  addresses measured unaligned, and foreign defmt frames present in the linked image), and pin the margin
  with a compile-time assert the CI `log-defmt` gate will actually check. Depends on 053 and 054 because
  all three touch the same comment blocks and documentation section.

TASK-037 now depends on TASK-053 and TASK-054 so its measurements describe the configuration we intend to
keep, and its notes carry the bench-session facts gathered here (ST-Link firmware floors, the CubeProgrammer
udev trap, the `--list-rtt` smoke test, partial-backtrace expectations, the mid-run detach hazard).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Integration verification pass — measured 2026-09-10, `nix develop .#default`, all inside one detached run

Every number below is from this machine, not from a leaf's notes.

| Check | Result |
|---|---|
| `make -n probe-flash probe-log probe-run` | rc=0 each; fully expanded, only `$(PROBE_EXTRA)` empty by design |
| DFU path unchanged | `make -n build flash flash-all check` vs the same expansion against HEAD's Makefile in a scratch dir: **diff = 0 lines** |
| Four-config matrix (`seed3` / `+log-defmt` NO_DEFAULT / `log-usb log-defmt` / neither) | all rc=0; firmware.bin 88613 / 48084 / 91148 / 45009 bytes |
| `make clippy FEATURES="seed3"` and `FEATURES="seed3 log-defmt" NO_DEFAULT=1` | rc=0 both (warnings fatal, thumbv7em) |
| Root `cargo fmt --all --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo clippy -p asperitas-logging --features log-usb --lib -- -D warnings` | rc=0 |
| `cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings` | rc=0 |
| `cargo test --workspace` | rc=0, every suite ok (largest 41 passed) |
| `cargo run -p asperitas-logging --example dump_reassemble -- --selftest` | rc=0, 9 cases |
| New, for TASK-036.05: `cargo clippy -p asperitas-logging --features log-defmt --lib -- -D warnings` | rc=0 today — nothing runs it unattended |

## Acceptance criteria → delivering leaf

* #1 `.01` — `probe-rs download $(ELF) --chip STM32H750IBKx --connect-under-reset --verify --reset` (Makefile:153). Literal AC miss, accepted: `--verify` does not exist on `probe-rs attach`, so `probe-log` (Makefile:169) carries neither flag — attaching must not reset a running board. Recorded in `.01`'s notes and daisy-seed3.md:379.
* #2 `.01` — proven by the 0-line expansion diff above, plus `build`/`flash` bodies byte-identical since `3b18693`.
* #3 `.03` — `firmware/Cargo.toml` `default = ["log-usb"]`, `log-defmt = [... , "dep:defmt-rtt"]`; the fourth matrix row (neither backend) built at 45009 bytes.
* #4 `.03` — both clippy invocations rc=0. Durability is now TASK-036.05, because no CI or hook command compiles these cfg pairs.
* #5 `.02`+`.03` — `panic_handler.rs:68-86`: USB emit under `cfg(log-defmt)`-independent gate, defmt emit under `cfg(feature = "log-defmt")`, then halt. Exactly one Rust `#[panic_handler]` per binary and defmt 1.1.1 ships none, so no double-emitted panic text. Hardware proof stays TASK-037.
* #6 `.04` — audited: no false future-tense probe claim survives in docs/ or README (the old "when one is available" / "restores cargo run-style" / "requires an ST-Link probe" wording is gone), zero broken anchors across rust-daisy-stack.md:109,111 and daisy-seed3.md's internal links, printed commands match `make -n` apart from make's trailing spaces.

## Deviations worth naming

* `firmware.bin` for `FEATURES="seed3"` is **88613 bytes**, not the 88101 `.01` recorded. Not a regression: firmware gained code in `e60de88`, `fec97a5`, `d7918df` etc. after `.01`. The invariant `.01` actually established — DWARF costs no flash — still holds; re-measure per change, don't chase a frozen byte count.
* Two defmt majors remain in `firmware/Cargo.lock` (0.3.100 shim + 1.1.1). Unavoidable today: `stm32-metapac 21.0.0` and `embassy-net-driver 0.2.0` still ask for 0.3. The shim's own lock entry depends only on defmt 1.1.1, so exactly one encoder owns the wire format — which is what `.03` needed.
* Release ELFs carry more than line tables: `.debug_info` ~997 KB, `.debug_aranges`, `.debug_pubnames`, even though `-C debuginfo=line-tables-only` is provably the flag passed and no env override exists in flake.nix or either cargo config. Harmless — all of it is non-ALLOC, and `firmware.bin` is the same size either way — so no ticket: the Cargo.toml comment's claim ("costs no flash") is true and was re-confirmed here. If someone later wants a smaller ELF, that's a new question.
* Bare `make probe-flash` (default FEATURES) fails at *image load*, not probe discovery: a console-only ELF has no consolidated `.defmt` section, so probe-rs declines before looking for a probe. Already documented in the Makefile preamble and daisy-seed3.md; the working form is `DEFMT_LOG=info make probe-flash FEATURES="seed3 log-defmt" NO_DEFAULT=1`.

## Integration re-run closing the umbrella - measured 2026-09-12, inside the nix dev shell

Ran because the plan gates promotion on steps 2 and 3 being re-run once `.05` and `.06` landed
(`.05` edits both gate definitions). Every number here is from this machine, not from a leaf's notes.

### Step 2 - DFU path still provably unchanged

`make -n build flash flash-all check` against the pre-probe Makefile (`d40d72c`) expanded in a scratch
dir: **2 lines differ, both whitespace only** - `--release  --features` from the empty
`$(CARGO_DEFAULTS)`. Same verdict as the 2026-09-10 pass, re-measured rather than trusted.

Probe side expands fully and exits where it must with no board: `make -n probe-flash probe-log
probe-run` rc=0 each; `make probe-flash FEATURES="seed3 log-defmt" NO_DEFAULT=1` builds, then stops at
`Error: No connected probes were found.` (rc=2). `probe-rs list` prints `No debug probes were found.`

### Step 3 - host gates, all rc=0

fmt; clippy `--workspace --all-targets`; clippy `-p asperitas-logging --features log-usb --lib`; same
with `log-defmt`; clippy `--features asperitas-pod/pod-hw`; both `RUSTDOCFLAGS=-D warnings cargo doc`
runs; `cargo test --workspace`; `cargo test --workspace --features asperitas-pod/pod-hw`;
`dump_reassemble -- --selftest`. Firmware: both ci.yml cross-compiles, plus `make clippy` under
`FEATURES="seed3"` and `FEATURES="seed3 log-defmt" NO_DEFAULT=1`.

### Four-config matrix, re-measured today (firmware.bin, `main`)

| config | bytes | recorded by |
|---|---|---|
| `seed3` (console default) | 88677 | .03 said 88101, the 2026-09-10 pass said 88613 |
| `--no-default-features --features "seed3 log-defmt"` | 48084 | identical to the 2026-09-10 pass (.03 said 47624) |
| `seed3 log-usb log-defmt` | 91012 | pass said 91148, .03 said 90160 |
| `--no-default-features --features seed3` (neither transport) | 45009 | identical to both .03 and the pass |

Two of the four moved since 2026-09-10 through unrelated commits (TASK-038's logging verbs added code);
the RTT-only and no-backend rows came out byte-identical to the 2026-09-10 pass. Nothing regressed here,
and a frozen byte count is not the invariant - stale numbers baked into prose are TASK-055 AC #3's job,
not this ticket's.

**Config trap, recorded so nobody rediscovers it:** the fourth matrix row is *neither transport*, which
still needs `seed3` - that feature carries `boot-led`, and every firmware binary references
`asperitas_logging::led`. Bare `--no-default-features` with no features at all has never built and is
not a supported configuration: there is no non-Seed3 hardware path in this workspace, since
`daisy-embassy` and `embassy-stm32` are pulled in unconditionally. It fails with five "could not find
`led` in `asperitas_logging`" errors. Not a defect; running it is how you waste twenty minutes.

### Step 4 - claim and anchor audit

Zero broken links: every relative `.md#anchor` across `docs/`, `README.md` and `CLAUDE.md` resolves
against a real heading (checked programmatically, code fences excluded). No surviving aspirational
probe claim in any document.

### One stale claim this ticket itself created, fixed here

`crates/asperitas-logging/src/panic_handler.rs` carried three sentences written before the probe
existed, and its own `.03` work invalidated them:

- the module doc claimed the panic text goes only via the `log-usb` module, and that "`boot-led` on,
  `log-usb` off" is the LED-only degradation - false since `.03` added the RTT emit beside it;
- the numbered behaviour list omitted the `log-defmt` step entirely;
- the halt rationale asserted **"This project has no debug probe (see the README)"**. The README now
  documents `make probe-*`, so the pointer had nowhere to land and the claim was no longer the reason.
  Rewritten to the reason that does hold: both emits are synchronous and have already returned, so a
  halted core protects nothing either way.

Comment/doc only - no executable line changed. Re-greened after: fmt, both logging-feature clippy
invocations, `cargo doc -p asperitas-logging --all-features` under `-D warnings`, `cargo test
--workspace`, and the RTT-only cross-compile. The five `firmware/src/bin/*.rs` comments saying "with no
debug probe attached `bkpt()` escalates to HardFault" are conditional statements about runtime and stay
correct; left alone.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-09 22:08
---
Planning complete: work fully delegated to TASK-036.01-.04, all Dev Ready. Status left at Blocked deliberately — see Implementation Plan §"What this umbrella is". Do not promote to Dev Ready until all four leaves are Done and the integration pass has run.
---

created: 2026-09-10 21:15
---
Planning round 2026-09-10: found all four leaves already Done, so this run performed the gated integration verification instead of fresh planning — results in Implementation Notes, all six ACs checked with their delivering leaf named. Two gaps surfaced and filed as TASK-036.05 (no unattended gate compiles the log-defmt cfg pairs) and TASK-036.06 (bench-facing facts unwritten + no way to drop --connect-under-reset from probe-flash). Status deliberately NOT promoted to Dev Ready: the umbrella still has two open children and sorts ahead of them by ordinal. Promote only after both land and the DFU-expansion diff plus host gates are re-run.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Umbrella closed by verification, not by new implementation: all six leaves (`.01`-`.06`) were already
Done, so this run re-ran the integration pass the plan gates promotion on and fixed one stale claim the
ticket itself left behind.

Delivered, leaf by leaf: `#1`/`#2` `.01` (probe make targets, `--chip`/`--verify`, release line tables;
DFU path unchanged), `#3` `.03` (`log-defmt` behind a feature, console still selectable, no-backend image
still builds), `#4` `.03`+`.05` (thumbv7em clippy-clean *and* now compiled by CI and lefthook), `#5`
`.02`+`.03` (panic text emitted over RTT at code level; hardware proof is TASK-037's), `#6` `.04` (both
channels documented with what each loses, toolchain note reflects the probe as present).

Evidence, measured today rather than quoted from the leaves: DFU expansion diff against the pre-probe
Makefile is 2 whitespace-only lines; every ci.yml host gate returned 0; the four-config firmware matrix
built at 88677 / 48084 / 91012 / 45009 bytes; `make probe-flash` with no board reaches probe discovery
and stops there; anchor audit found zero broken links across docs/, README and CLAUDE.md.

Code change made here: `crates/asperitas-logging/src/panic_handler.rs` still said "This project has no
debug probe (see the README)" and described the panic path as USB-only in both its module doc and its
numbered behaviour list - three statements this ticket's own work invalidated. Comment/doc only, no
executable line touched, re-greened through fmt, both logging-feature clippy gates, rustdoc under
`-D warnings`, the workspace tests and the RTT-only cross-compile.

Nothing is left open under this umbrella. The three follow-ups raised during planning are siblings on
purpose - TASK-053 (probe path safe to drive unattended), TASK-054 (release DWARF level), TASK-055 (six
corrections to the RTT record) - and TASK-037 remains the only place a hardware claim gets made.
<!-- SECTION:FINAL_SUMMARY:END -->
