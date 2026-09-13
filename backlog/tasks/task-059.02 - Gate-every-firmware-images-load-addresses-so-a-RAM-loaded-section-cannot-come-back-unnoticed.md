---
id: TASK-059.02
title: >-
  Gate every firmware image's load addresses, so a RAM-loaded section cannot
  come back unnoticed
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-13 06:56'
updated_date: '2026-09-13 06:56'
labels:
  - planned
dependencies:
  - TASK-059.01
references:
  - scripts/gates.sh
  - scripts/check-doc-artifact-names.sh
  - firmware/memory.x
parent_task_id: TASK-059
priority: medium
type: chore
ordinal: 106800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-059.01 fixes one instance of a class. Nothing in the repo would notice the next one.

Today every firmware gate is a `cargo` invocation; none inspects an ELF. The only thing standing
between this project and a 469 MB image was a hand-written comment block plus six
`--only-section` flags - and `--only-section=.typo` exits 0 writing a 0-byte file, so the crutch
itself failed silently. Once TASK-059.01 removes those flags, a future dependency that tags a
static into a RAM-loaded section, or a daisy-embassy bump that renames `.sram1_bss` so the new
`SECTIONS` rule stops matching it, reproduces the bug with no red anywhere: `make build` still
succeeds, DFU still reports a clean transfer, and the board boots whatever prefix of itself fit in
the flash budget.

So this ticket adds the invariant as a gate, in the shape the fix leaves behind rather than the
shape of the bug: *no file-backed section may load outside flash.* That single rule would have
caught the original defect, catches any future one regardless of section name, and does not care
which linker input placed it.

Three assertions, all host-side, no board:

1. For each of the six release ELFs, every `ALLOC` + `PROGBITS` section's LMA lies in
   `0x08000000..0x08020000`. This is the general rule.
2. When `.sram1_bss` is present it must be `NOBITS`, so a rule that quietly stops matching its
   input sections fails by name instead of by coincidence.
3. Each plain `-O binary` image is exactly as long as the highest flash LMA end minus
   `0x08000000`, which proves nothing between the extremes got dropped. This is the assertion that
   lets the `--only-section` keep-list stay dead - including the `.defmt*` records, whose survival
   `scripts/check-doc-artifact-names.sh` used to "prove" by grepping for a flag.

One script, called from `scripts/gates.sh`, not a Makefile target: after TASK-061 the gate set is
named exactly once, and a check reachable only by typing `make something` is a check the loop never
runs. Follow `check-doc-artifact-names.sh`'s conventions exactly - it is the second host-side
non-cargo gate in the repo, and the two should look like siblings.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `scripts/check-image-load-addresses.sh` exists and follows `scripts/check-doc-artifact-names.sh`'s conventions verbatim: `#!/usr/bin/env bash` with no `set -u` (gates.sh runs under `set -euo pipefail`), derive rules from the build rather than a hand-kept list, count failures then exit 1, one prefixed line per failure, a `usage()` function, and a header that explains what went wrong badly enough to justify the check.
- [ ] #2 It reads the six release ELFs with the bare binutils shims (`rust-objdump -h <path>`, `rust-objcopy -O binary <elf> <out>`), never `cargo objdump` or `cargo objcopy`. Verified 2026-09-13 that the bare form is a pure passthrough (0.16 s, no build); the cargo forms rebuild, and a rebuild after `gates.sh`'s cross-build pair silently re-points `release/main` at a different cfg set, which is the hazard gates.sh's ordering rule 1 exists to prevent. A missing ELF is a hard failure with a message naming the build that produces it - not an mtime-based skip, and not the `test -f ... || true` else-branch that makes elf-check's own hint useless (TASK-062).
- [ ] #3 All three assertions are implemented and pass on the post-TASK-059.01 tree for both cfg sets, printing the per-binary image length and the highest flash LMA end it observed, so a future reader can tell a pass from a vacuous pass.
- [ ] #4 The check is demonstrated failing, not merely written. Temporarily comment out the new `SECTIONS` rule in `firmware/memory.x`, rebuild `main`, run the script against that ELF, and record in the notes the exit code and the exact failure lines; restore, rebuild, confirm green. A gate nobody watched go red is a guess. Do not propose `--orphan-handling=error` as a substitute: measured, rust-lld accepts it but names only `.debug_*`, `.comment` and `.ARM.attributes`, never `.sram1_bss`.
- [ ] #5 Registered as `gate push` in `scripts/gates.sh`, positioned after the doc-artifact gate and after both cross-build gates so it sees their artifacts and builds nothing itself. The header's local-warm cost table and the tier-population comment are updated in the same commit, `scripts/gates.sh --list` shows the new counts, and the measured local warm cost of this gate is recorded in the notes. It stays out of the `commit` tier because pre-commit builds no firmware, so the ELFs it needs may not exist there.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
**Step 1 - Read the two files you are imitating and obeying.** `scripts/check-doc-artifact-names.sh`
for shape (usage function, failure counting, header prose), and `scripts/gates.sh:1-45` plus the
`gate()` helper around `:116-131` for the ordering rules this gate must not violate. Rule 1 says no
gate may build firmware after the cross-build pair; that constraint is why this script calls the bare
binutils shims on ELF paths instead of `cargo objdump`.

**Step 2 - Derive the flash region, do not hardcode it.** Parse `FLASH : ORIGIN = ..., LENGTH = ...`
out of `firmware/memory.x` and fail loudly if that parse fails. This is the same choice
check-doc-artifact-names.sh makes when it derives legal image names from the Makefile: a rule kept by
hand is the thing that goes stale, and a move to the 8 MB QSPI region is a live plan in this project.

**Step 3 - Get section headers without touching cargo.** `rust-objdump -h <path>` per bin, from
`target/thumbv7em-none-eabihf/release/{main,rig,blinky,ledtest,podtest,panictest}`. Two traps:

- Section names contain spaces. Real example from this tree today:
  `.defmt.error.{"package":"embassy-stm32","tag":"defmt_error","data":"panicked at 'AHB frequency is too low'",...}`.
  Field-splitting left to right will shred that. Extract right to left instead: last field is the
  type word, the two before it are VMA and LMA, everything between the index and the VMA is the name.
  If you prefer `rust-readelf -W -S` for its explicit `[A]` alloc flag and `PROGBITS`/`NOBITS` types,
  check whether it survives the same names before committing to it.
- Decide deliberately what counts as file-backed-and-loaded and refuse to guess about anything else.
  `llvm-objdump -h` reports a coarse type word (`TEXT`, `DATA`, `BSS`, nothing for non-allocated
  sections). Any type word the script does not recognise must be a hard error naming the section, not
  a silent skip - a silent skip is how a future section walks through the gate unnoticed.

**Step 4 - The three assertions.** (1) every loaded file-backed section's LMA lies inside the parsed
flash range; (2) if `.sram1_bss` exists it is `NOBITS`; (3) build the plain image with
`rust-objcopy -O binary <elf> "$(mktemp -d)/x.bin"` and require its length to equal
highest-flash-LMA-end minus flash origin, which is what proves nothing between the extremes was
dropped and therefore what keeps the keep-list dead. Clean the temp dir on exit. Print the per-binary
numbers even on success: a pass that prints nothing cannot be distinguished from a pass that checked
nothing, and this repo's convention is numbers over verdicts.

**Step 5 - Say what it does not cover.** In the script header, state that the two cfg sets write the
same ELF paths, so the gate validates whichever cross-build ran last (the RTT-only image in the push
and ci tiers), not both at once, and that TASK-062 owns that provenance blindness. An honest
limitation here beats a false claim of coverage.

**Step 6 - Register it.** Add `gate push "=== image load addresses ===" bash scripts/check-image-load-addresses.sh`
immediately after the doc-artifact gate (`gates.sh:264`), which puts it after both cross-builds.
Update the header's local-warm cost table and the tier-population comment in the same commit, and
confirm `bash scripts/gates.sh --list` and `--dry-run` both look right. Keep it out of the `commit`
tier: pre-commit builds no firmware, so the ELFs may not exist there, and inventing a skip path for
that would recreate elf-check's blind spot.

**Step 7 - Prove it red (AC #4).** Comment out the `SECTIONS` block TASK-059.01 added to
`firmware/memory.x`, rebuild `main`, run the script, and capture the exit code plus the exact failure
lines into the ticket notes. Restore and prove green. Do not simulate the failure with a hand-edited
ELF: the point is that the real defect trips the real check.

**Step 8 - Price it.** Time the gate alone and the whole push tier before and after, inside
`nix develop .#default` on a warm tree, and put both figures in the notes and the gates.sh comment.
It should land near zero because it builds nothing; if it does not, find out why before landing it.

**Step 9 - Land it.** `bash scripts/gates.sh ci` green from the repo root, one commit adding the
script and touching only `scripts/gates.sh` besides, with the red-run transcript in the commit body.
<!-- SECTION:PLAN:END -->
