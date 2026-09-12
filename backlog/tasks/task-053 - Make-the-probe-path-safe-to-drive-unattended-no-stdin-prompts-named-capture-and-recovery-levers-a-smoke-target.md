---
id: TASK-053
title: >-
  Make the probe path safe to drive unattended: no stdin prompts, named capture
  and recovery levers, a smoke target
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-12 07:05'
updated_date: '2026-09-12 09:31'
labels:
  - planned
dependencies: []
references:
  - 'https://github.com/pyocd/pyOCD/issues/1700'
  - 'https://probe.rs/docs/tools/probe-rs/'
documentation:
  - docs/reference/daisy-seed3.md
  - docs/reference/rust-daisy-stack.md
priority: high
type: task
ordinal: 84700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-036 shipped a probe path that works when a person types the command and watches the terminal. It does not hold up when an agent drives it, which is the reason the probe path exists at all. Four gaps, all fixable without a board, all confirmed against probe-rs 0.32.0 on this machine (the pinned build, `/nix/store/...-probe-rs-tools-0.32.0`).

1. **A probe command can hang forever on stdin.** Without `--non-interactive`, probe-rs prompts on stdin to choose a probe as soon as more than one is present. Today exactly one probe exists so nobody has seen it; the day a second ST-Link or an FT2232 shows up, every unattended `probe-*` invocation blocks. Verified present on `download`, `run` and `attach` (`--non-interactive`, env `PROBE_RS_NON_INTERACTIVE`); `probe-rs list` rejects the flag, so it must not go there.
2. **`probe-log` attaches to whatever ELF is on disk.** `probe-log` (Makefile:180) has no `build-elf` prerequisite while `probe-flash`/`probe-run` do, so it can decode a running board with a stale binary's symbols and `.defmt` section - the exact failure its own comment (Makefile:173-174) warns about. Either make the ELF match the source or make the mismatch loud.
3. **Nothing describes capturing output without a TTY.** A rig runner needs `--target-output-file defmt=out.txt`, `--no-timestamps`, `--log-format oneline`, `--disable-progressbars` (all confirmed on `attach`/`run`), and it needs to know what success looks like: measured here, `probe-rs download ... --non-interactive` with no board exits 1 printing `Error: No connected probes were found.` The DFU path documents dfu-util's exit-74 lie thoroughly (daisy-seed3.md:128-134); the probe path documents no exit code at all.
4. **The recovery levers have no names in print.** `--cycle-power`, `--read-flasher-rtt`, `--dry-run` and `--disable-double-buffering` are reachable through `PROBE_EXTRA` today but appear nowhere in the repo. Double buffering deserves a sentence of its own: pyOCD #1700 reports silent STM32H750 flash corruption with double buffering on roughly 1 in 5 attempts of a 30 kB image, which is also the strongest independent argument that `probe-flash`'s `--verify` is load-bearing rather than decorative. Probe pinning for a multi-probe bench (`--probe VID:PID:serial`, `PROBE_RS_PROBE`, `[presets]` with `PROBE_RS_CONFIG_PRESET`, precedence CLI > preset > env) belongs in the same place.

Also worth shipping here: a `--list-rtt` smoke target. `probe-rs attach <ELF> --chip STM32H750IBKx --list-rtt` attaches, prints the RTT channel table and exits - it neither reflashes nor depends on the host keeping up, which makes it the cheapest possible "is RTT discoverable at all" question, from the bench or from a script.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 All three probe recipes pass --non-interactive and `probe-rs list` does not gain it. Prove it two ways: the `make -n probe-flash probe-run probe-log` expansions contain the flag, and `make -n build flash flash-all check` is byte-identical to the pre-change expansion apart from empty-variable spacing.
- [x] #2 `probe-log` can no longer silently decode a running board with a stale ELF: it either builds the ELF first or fails loudly when a source is newer than the ELF. The chosen mechanism is stated in one Makefile comment line, and the prose-only warning it replaces is gone.
- [x] #3 A scripted (non-TTY) capture invocation is documented and proven to expand via `make -n`, and the probe path records its exit-code contract as measured on this machine: the command, the numeric rc, and the verbatim stderr string for the no-probe case, kept distinct from `probe-rs list` wording and placed beside the existing dfu-util exit-74 note.
- [x] #4 docs/reference/daisy-seed3.md gains an `#### Running the probe path unattended` subsection that names --cycle-power, --read-flasher-rtt, --dry-run and --disable-double-buffering with what each is for (pyOCD #1700 cited for the last, with what it implies for --verify), plus multi-probe pinning via --probe / PROBE_RS_* / [presets] and the measured CLI > preset > env precedence. No existing heading is renamed, so the anchors from rust-daisy-stack.md:108-109 and README.md:247 still resolve.
- [x] #5 A make target runs `probe-rs attach ... --list-rtt` and exits, reachable with no board attached and expanding correctly under `make -n`.
- [x] #6 Every probe command printed in docs/ and README.md matches `make -n` output modulo whitespace, reported as a count as TASK-036.04 did.
- [x] #7 Host gates green in nix develop: fmt, the four clippy invocations, both RUSTDOCFLAGS=-D warnings doc runs, cargo test --workspace, and both firmware cross-compiles from ci.yml.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Scope discipline

Five edits, all host-side, none needing a board. Do not add probe-rs features, do not touch
defmt-rtt's transport, do not re-open the loss model (that is TASK-055) and do not change the DWARF
level (TASK-054 owns it). This ticket is about *driving* the tool, not about what the tool records.

## Facts already measured on this machine, do not re-derive

Against the pinned `/nix/store/abbyrjisii3wmz511gylfn2gq0rzsq5z-probe-rs-tools-0.32.0`:

* `--non-interactive` ("Disable interactive probe selection", env `PROBE_RS_NON_INTERACTIVE`) exists on
  `download`, `run` and `attach`. `probe-rs list --non-interactive` fails outright:
  `error: unexpected argument '--non-interactive' found`, usage rc=2. So the flag goes in the three
  probe recipes and nowhere else.
* Env equivalents confirmed for the flags we already use: `PROBE_RS_CHIP`, `PROBE_RS_PROBE`,
  `PROBE_RS_SPEED`, `PROBE_RS_CONNECT_UNDER_RESET`, `PROBE_RS_CYCLE_POWER`, plus `-C/--config` presets
  with `PROBE_RS_CONFIG_PRESET`. `--help` states precedence plainly: "Manually specified command line
  arguments take overwrite presets, but presets take precedence over environment variables."
* Capture flags on `attach`/`run`: `--target-output-file <channel>=<path>` (its own example is
  `--target-output-file defmt=out/defmt.txt`), `--no-timestamps`, `--log-format`, and
  `--disable-progressbars` on the download side. probe-rs's *own* debug log is separate:
  `--log-file` / `--log-to-folder`.
* Recovery flags: `--cycle-power`, `--dry-run`, `--read-flasher-rtt` (download/run),
  `--disable-double-buffering` whose help says "if download fails during programming with timeout
  errors, try this option".
* Exit codes with nothing attached, measured: `probe-rs download <ELF> --chip STM32H750IBKx
  --connect-under-reset --verify --reset --non-interactive` -> rc=1, stderr
  `Error: No connected probes were found.`; `probe-rs attach <ELF> --chip STM32H750IBKx
  --non-interactive --list-rtt` -> rc=1, same string. Note this differs from `probe-rs list`, which
  prints `No debug probes were found.` - two different strings, do not conflate them in the docs.

Current inventory to edit (verified line numbers): `firmware/Makefile:164-165` (`probe-flash`),
`:169-170` (`probe-run`), `:180-181` (`probe-log`, no prerequisite), knobs at `:62` (`PROBE_EXTRA`,
add-only) and `:69-73` (`UNDER_RESET`/`UNDER_RESET_FLAG`). Docs: `docs/reference/daisy-seed3.md`
section "Flashing and logging over an ST-Link probe" spans :341-583 with **no `####` children**, and
`README.md:228-260` mirrors it. Inbound anchors exist at `rust-daisy-stack.md:108-109` and
`README.md:247`, so the `###` heading text must not change; adding `####` children is safe.

## Steps

0. Record the baseline you will diff against: save `make -n build flash flash-all check
   build-elf probe-flash probe-run probe-log` output and copy `git show HEAD:firmware/Makefile` to
   /tmp. Every later claim is a diff of these.

1. Add `--non-interactive` to all three probe recipes unconditionally, next to `--chip $(CHIP)`.
   Deliberately not a knob: prompting on stdin is never useful here, and a person who really wants to
   pick interactively passes `PROBE_EXTRA="--probe VID:PID:serial"` far more often than the reverse.
   One comment line above the first occurrence saying why (unattended loop, hang otherwise).

2. Fix the stale-ELF hole in `probe-log`. Prefer the cheap honest option: keep it non-building (a bench
   session wants to attach without reflashing) but make the mismatch loud - print the ELF's mtime and
   the newest source mtime, or fail if any source is newer than the ELF, in the style of the existing
   `flash` recipe's success-marker check (Makefile:111-122). Whichever you choose, delete the sentence
   at Makefile:173-174 that only warns in prose if the recipe now enforces it.

3. Add `probe-rtt-list` to `.PHONY` and the targets: `probe-rs attach $(ELF) --chip $(CHIP)
   --non-interactive --list-rtt $(PROBE_EXTRA)`, no `build-elf` prerequisite for the same reason as
   step 2. Document in one comment that this is the smoke test which neither erases the single 128 KB
   sector nor needs the host to drain RTT.

4. Scripted capture: do not invent a new target. Document the exact invocation under the new `####`
   subsection (step 5) as a `PROBE_EXTRA` composition, e.g. `make probe-log PROBE_EXTRA="--no-timestamps
   --log-format oneline --target-output-file defmt=out.txt" FEATURES="seed3 log-defmt" NO_DEFAULT=1`,
   and verify it expands cleanly with `make -n`. If the flag set turns out not to compose through
   `PROBE_EXTRA` (quoting, ordering), fix the recipe rather than documenting around it.

5. Write `#### Running the probe path unattended` inside the probe section of
   `docs/reference/daisy-seed3.md`, containing: the two measured exit-code facts; the capture
   invocation; the recovery levers each with what it is for, including `--disable-double-buffering`
   with pyOCD #1700 (silent STM32H750 corruption ~1 in 5 attempts of a 30 kB image) and the inference
   that `--verify` is therefore load-bearing on a chip whose internal flash is one 128 KB sector;
   multi-probe pinning via `--probe VID:PID:serial` / `PROBE_RS_PROBE` / `[presets]` with the measured
   precedence sentence; and the `probe-rtt-list` smoke test as the first thing to run at a bench.
   Mirror the user-visible commands in `README.md:228-260`.

6. Sync audit: extract every command line printed in `docs/` and `README.md` for the probe path and
   diff each against `make -n` output, whitespace-normalised. That is the procedure TASK-036.04 left
   in its notes ("3 of 3 match exactly"); re-run it and report the count.

7. Gates: `cargo fmt --all --check`, the four clippy invocations, both `RUSTDOCFLAGS=-D warnings` doc
   runs, `cargo test --workspace`, and both firmware cross-compiles from ci.yml, all inside
   `nix develop .#default`. Firmware is unaffected by Makefile edits but say so with the numbers, not
   by assumption.

## Guard: the DFU path may not move

Report the before/after diff of `make -n build flash flash-all check` in the notes. Empty-variable
expansion producing a double space is acceptable (it is what TASK-036's integration pass recorded);
any other delta is a defect.

## Not in scope

`--scan-region` tuning (probe-rs's help warns that giving no region means it "will not scan and will
not poll RTT" - relevant only if `_SEGGER_RTT` ever moves, which TASK-055 documents and defers);
wiring firmware clippy into CI; anything requiring a probe attached, which stays TASK-037.

## Citation hygiene

Every upstream claim above was read out of probe-rs 0.32.0's own `--help` or exit codes on this machine and
can be re-checked offline. pyOCD #1700 could NOT be re-read here (fetches returned nothing), so re-open it
before quoting it into docs/reference/daisy-seed3.md: if it does not describe silent STM32H750 corruption
under double buffering, drop the citation and keep only probe-rs's own help text for
`--disable-double-buffering`. Same rule for any issue number inherited from a ticket description - cite what
the page actually says, not what the ticket asserts.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## What shipped

firmware/Makefile: `--non-interactive` on all four probe recipes (`probe-flash`, `probe-run`,
`probe-log`, new `probe-rtt-list`); new `elf-check` target that fails rather than warns when an input
to the ELF is newer than it, wired as a prerequisite of `probe-log` and `probe-rtt-list`; the old
prose-only "build first if the firmware changed" sentence deleted. docs/reference/daisy-seed3.md: new
`#### Running the probe path unattended` under the existing `###` probe heading (no heading renamed, so
rust-daisy-stack.md's two anchors still resolve). README.md: mirrored commands plus the stale-ELF
refusal.

## Measured, not asserted (probe-rs 0.32.0 pinned build, nothing attached)

- AC1: `make -n probe-flash probe-run probe-log probe-rtt-list FEATURES="seed3 log-defmt"
  NO_DEFAULT=1` shows the flag on all four. `probe-rs list --non-interactive` exits 2 with
  "error: unexpected argument '--non-interactive' found". DFU guard: `make -n build flash flash-all
  check` against the same command run on `git show HEAD:firmware/Makefile` is byte-identical, not
  merely whitespace-normalised.
- AC3: the capture composition expands to exactly the line printed in the doc, character for
  character: `probe-rs attach .../main --chip STM32H750IBKx --non-interactive --no-timestamps
  --log-format oneline --target-output-file defmt=out.txt`. Exit codes re-measured this run:
  `probe-rs download` rc=1 with "Error: No connected probes were found."; `probe-rs attach --list-rtt`
  rc=1, same string one line after the DWARF WARN; `probe-rs list` rc=0 with "No debug probes were
  found."
- AC5: `make probe-rtt-list FEATURES="seed3 log-defmt" NO_DEFAULT=1` reaches probe-rs with no board and
  dies there (probe-rs rc=1, make reports 2). `make elf-check` was separately seen to fail closed
  before probe-rs runs at all.
- AC6: every `probe-rs` line inside fenced blocks in docs/ and README.md diffed against live `make -n`
  expansions, whitespace-normalised: 4 of 4 match. The two non-matching lines in those blocks are not
  probe-path recipes (a blinky `cargo objcopy` "or manually" example at daisy-seed3.md:106, and
  `cargo build -p asperitas-cli -p asperitas-dsp` in README).
- AC7: the full ci.yml gate script inside `nix develop .#default`: fmt, the four clippy invocations,
  both RUSTDOCFLAGS=-D warnings doc runs, `cargo test --workspace`, the pod-hw test pass,
  dump_reassemble selftest, and both firmware cross-compiles -> ALL GATES GREEN, outer rc=0. Makefile
  edits cannot reach the compiler; said with the numbers anyway: 20 test binaries, 0 failures.

## Two corrections to the plan, both from measuring

1. **The exit-code table had the wrong layer.** It listed `make probe-flash` with rc 1, but make turns
   any nonzero recipe status into its own 2: measured `make probe-flash` -> 2, `make probe-log` -> 2,
   `make probe-rtt-list` -> 2, each with probe-rs itself exiting 1. A stale-ELF refusal is also rc=2,
   with probe-rs never running. The table now carries separate `probe-rs rc` and `make rc` columns and
   says plainly that rc alone cannot distinguish "no probe" from "stale ELF" when driven through make.
2. **pyOCD #1700 was re-read**, which the plan asked for. Fetched through the GitHub API: opened
   2024-06-11 by nattgris, closed as completed 2025-08-13 by TeoMahnic, who disabled double buffering
   for flash algorithms by default (v0.38.0) and blamed stalls from bus-related effects on devices with
   strict timing constraints such as STM32H7xx. Every claim in the paragraph holds, with three
   tightenings made while citing: the second reporter (newbrain, programming an STM32H750 on a Daisy
   Seed) reported a success rate of "about one out of three tries", not "failing twice in three";
   probe-rs's `--disable-double-buffering` help text is now quoted in full instead of from mid-sentence;
   and the reporter's own datapoint of 100 consecutive successes with a different flash algorithm
   versus a loader that "failed after on average 2 programmings" is included, because it is the
   strongest evidence that the failure lives in the flash algorithm rather than the tool.

## Follow-up filed

TASK-056: `elf-check` uses `find -newer`, so a bulk mtime refresh (git checkout, stash pop, worktree
operations) makes it fail on a tree that is byte-identical to what built the ELF. Hit for real during
this ticket: ELF mtime 1789201869892003000 versus sources at 1789201885874687000 with a clean
`git status`. Recoverable through the printed one-liner, so it blocked nothing here; the fix is a
content digest stamped at build time, which cannot see mtimes at all.

## Working tree note

Three backlog files unrelated to this ticket (task-038.03, task-038.03.02, task-038.03.02.02) carry
uncommitted edits that predate this run by hours and were left by earlier sessions. Left untouched and
out of this commit, matching the convention that a commit carries only its own task file.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Probe path is now safe to drive with nobody watching. All four probe recipes pass `--non-interactive`
unconditionally (`probe-rs list` rejects that flag, so it stays out of every other recipe), `probe-log`
and the new `probe-rtt-list` smoke target depend on a new `elf-check` that fails closed rather than
warning when the ELF on disk is behind the sources, and docs/reference/daisy-seed3.md gained
`#### Running the probe path unattended`: the measured exit-code contract for both flashing routes kept
distinct from `probe-rs list`'s wording, a proven non-TTY capture invocation, the four recovery levers
with what each is for, multi-probe pinning with probe-rs's own precedence sentence, and pyOCD #1700
re-read at the source before quoting. README mirrors the user-visible commands.

Verification was measurement, not prose: DFU expansion byte-identical to HEAD, 4 of 4 documented
probe command lines matching live `make -n` output, the capture line matching its expansion character
for character, and every ci.yml host gate green (outer rc=0). Measuring turned up two defects in the
plan as written: the exit-code table conflated probe-rs's rc with make's (make reports 2 for both "no
probe" and "stale ELF", only stderr separates them), and three of the pyOCD citations needed tightening
against the actual issue thread. Both fixed here. One follow-up filed, TASK-056, because `elf-check`
keys on mtimes and therefore fails on a clean tree after a bulk metadata refresh; observed while
closing this ticket.
<!-- SECTION:FINAL_SUMMARY:END -->
