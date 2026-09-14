---
id: TASK-059.02
title: >-
  Gate every firmware image's load addresses, so a RAM-loaded section cannot
  come back unnoticed
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-13 06:56'
updated_date: '2026-09-14 05:12'
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
- [x] #1 `scripts/check-image-load-addresses.sh` exists and follows `scripts/check-doc-artifact-names.sh`'s conventions verbatim: `#!/usr/bin/env bash` with no `set -u` (gates.sh runs under `set -euo pipefail`), derive rules from the build rather than a hand-kept list, count failures then exit 1, one prefixed line per failure, a `usage()` function, and a header that explains what went wrong badly enough to justify the check.
- [x] #2 It reads the six release ELFs with the bare binutils shims (`rust-objdump -h <path>`, `rust-objcopy -O binary <elf> <out>`), never `cargo objdump` or `cargo objcopy`. Verified 2026-09-13 that the bare form is a pure passthrough (0.16 s, no build); the cargo forms rebuild, and a rebuild after `gates.sh`'s cross-build pair silently re-points `release/main` at a different cfg set, which is the hazard gates.sh's ordering rule 1 exists to prevent. A missing ELF is a hard failure with a message naming the build that produces it - not an mtime-based skip, and not the `test -f ... || true` else-branch that makes elf-check's own hint useless (TASK-062).
- [x] #3 All three assertions are implemented and pass on the post-TASK-059.01 tree for both cfg sets, printing the per-binary image length and the highest flash LMA end it observed, so a future reader can tell a pass from a vacuous pass.
- [x] #4 The check is demonstrated failing, not merely written. Temporarily comment out the new `SECTIONS` rule in `firmware/memory.x`, rebuild `main`, run the script against that ELF, and record in the notes the exit code and the exact failure lines; restore, rebuild, confirm green. A gate nobody watched go red is a guess. Do not propose `--orphan-handling=error` as a substitute: measured, rust-lld accepts it but names only `.debug_*`, `.comment` and `.ARM.attributes`, never `.sram1_bss`.
- [x] #5 Registered as `gate push` in `scripts/gates.sh`, positioned after the doc-artifact gate and after both cross-build gates so it sees their artifacts and builds nothing itself. The header's local-warm cost table and the tier-population comment are updated in the same commit, `scripts/gates.sh --list` shows the new counts, and the measured local warm cost of this gate is recorded in the notes. It stays out of the `commit` tier because pre-commit builds no firmware, so the ELFs it needs may not exist there.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Overview

One new host-side script, one new gate line. The invariant is *no file-backed section may load
outside flash*, expressed as three assertions over the release ELFs, checked with the bare
binutils shims so the gate reads artifacts and builds nothing.

Everything below was verified on commit `0f672ba` on this tree, including a full red/green run of
the defect itself (step 7). Numbers quoted here are measurements, not estimates.

## Where the ticket's wording does not match the tree

Read this before step 1. Each item is verified, and each one is a place where following the words
literally costs an hour or produces a vacuous pass.

1. **There is no readelf anywhere.** On PATH: `rust-objdump`, `rust-objcopy`, `rust-readobj`,
   `rust-size` (cargo-binutils 0.4.0) and clang's `objdump`/`objcopy`. No `readelf`, no
   `llvm-readelf`, no `rust-readelf`. Step 3's "`rust-readelf -W -S` alternative" is dead on arrival -
   drop it. It was also the wrong tool: `readelf -W -S` has no LMA column at all (its `Address` is
   the VMA), and ELF section headers carry no load address - LMA lives in `PT_LOAD.p_paddr`, which is
   why objdump reconstructs the column per section from each segment's `p_paddr - p_vaddr` delta.
2. **`scripts/elf-provenance.sh` contains no ELF reader to reuse.** Comment #1 (2026-09-13) told this
   ticket to "call `elf-provenance.sh show` instead of duplicating objcopy plumbing"; that is wrong.
   `show` prints exactly one line, `default=N features=a,b defmt_log=`, read out of the `.asp.prov`
   note via `rust-objcopy --dump-section`; it never opens a section header and never calls objdump
   (zero occurrences in 771 lines). Nothing it prints describes a load address. What does transfer is
   its *shape*: bare shims behind a `command -v` guard (`:88-89`), `$PROG:`-prefixed `die()`
   (`:56-59`), hard failure on a missing artifact naming the command that produces it (`:87`),
   exit codes 0/1/2 with 2 reserved for "the check could not run", and `mktemp -d` plus
   `trap ... EXIT` (`:446-447`).
3. **`check-doc-artifact-names.sh` has no `usage()` function** - it takes no arguments. Copy its
   actual conventions (`set -uo pipefail` at `:47`, ROOT anchor `:49-50`, `die()` `:62-65`,
   `VIOLATIONS+=()` accumulation with `report()` returning 1 at `:195-210`, failure lines shaped
   `path:line: thing - reason`, a 45-line header that ends with an explicit exit-code contract) and
   take `usage()` from `elf-provenance.sh:61-67`. Note AC #1 says "no `set -u`", which contradicts
   both siblings; the substantive half of that clause is *no `-e`*, so findings accumulate instead of
   aborting at the first one. Use `set -uo pipefail`.
4. **`gates.sh` has no local-warm cost table.** Costs are prose above their own gate line (`:202`
   "Measured 0.89 s warm", `:282` "Measured warm: 0.08 s inside a `ci` run", `:292`, `:307`), the
   header says those figures are LOCAL warm numbers (`:40-42`), and tier counts are printed by the
   runtime at `:318`, not written in a comment. AC #5's obligation therefore resolves to three real
   edits: a measured figure in the comment above your `gate` line, the regenerated matrix in doc-001
   (step 6), and the prose figures doc-001 restates. Do not invent a table to satisfy the wording.
5. **Coordinates have drifted.** Insertion point is after `gates.sh:289` (the provenance gate spans
   `:287-289`) and before the clippy comment at `:291`; the doc-artifact gate is at `:188`, both
   cross-builds at `:262` and `:268-269`. Current counts are commit 10 / push 18 / ci 19.
6. **Do not reach for an `ASSERT()` in `memory.x` as a substitute or a complement.** rust-lld
   supports it and `cortex-m-rt`'s `link.x.in` uses one, but it cannot name the offending input
   section and only sees symbols `link.x` already defined. The original defect overflowed no region,
   which is exactly why no link diagnostic existed - lld only refuses when a region is full
   (`error: section '.data' will not fit in region 'ram'`).

## Step 1 - Write the header first, in the sibling's voice

`scripts/check-image-load-addresses.sh`, ~45 lines of header, structured like
`check-doc-artifact-names.sh:1-45`: one-line purpose, then the rules, then why they are load-bearing,
then what is out of scope, then the exit-code contract. The war story is real and specific: without
the placement rule in `firmware/memory.x`, daisy-embassy's `.sram1_bss` statics become an orphan
output section that rust-lld places with LMA == VMA in AXI SRAM, and `objcopy -O binary` writes a
memory dump spanning lowest to highest *load* address - 469,763,536 bytes for `main`, measured again
today (step 7). Arm documents this class as a scatter-file error (KA002145); Zephyr gates it with a
hand-curated section allowlist (`extra_sections` in `scripts/twister`) and we deliberately do the
opposite, deriving the rule from an address range plus `firmware/memory.x`, because a hand-kept list
is precisely the thing that killed the `--only-section` keep-list.

Say plainly in the header what this does not cover, in one sentence each: it validates one cfg set per
run, whichever cross-build ran last, which the provenance gate above it names (the RTT-only image in
every tier that runs the pair) - so no apology about provenance blindness is needed any more, just the
fact; ELF staleness belongs to `make -C firmware elf-check`; cfg identity belongs to the provenance
gate; section placement belongs here; and the ban on re-introducing a keep-list is enforced by
`check-doc-artifact-names.sh`'s pass 2, which greps the expansion of `make -n -C firmware build` for
`--only-section`.

## Step 2 - Derive every input from the build

- **Flash range**: parse `FLASH : ORIGIN = 0x08000000, LENGTH = 128K` out of `firmware/memory.x`,
  accept a `K`/`M` suffix, and `die` if either field fails to parse. A move to the 8 MB QSPI region is
  a live plan in this project; a hardcoded `0x08020000` would then be a gate that passes by lying.
- **Which binaries**: enumerate `firmware/src/bin/*.rs` and map each to
  `firmware/target/thumbv7em-none-eabihf/release/<stem>` - the same derivation
  `check-doc-artifact-names.sh:91-131` uses, so a seventh bin joins the gate without anyone editing a
  list, and "six" never appears as a magic number. Print how many ELFs were checked.
- **Arguments**: default to the derived set; accept explicit ELF paths as arguments so the script is
  usable ad hoc against one mutated copy. Unknown flags -> `usage >&2; exit 2`.
- **Missing ELF is a hard failure**, exit 2, naming the thing that produces it, e.g. `no ELF at
  firmware/target/thumbv7em-none-eabihf/release/rig - run 'bash scripts/gates.sh push', or
  'make -C firmware build-elf BINARY=rig FEATURES="seed3 log-defmt" NO_DEFAULT=1'`. Never
  `test -f ... || true`: a skip path is how elf-check's hint became useless (TASK-062).
- Guard both shims with `command -v` and exit 2 naming `nix develop .#default`, per
  `elf-provenance.sh:88-89`. Call the bare shims, never `cargo objdump`/`cargo objcopy`: the cargo
  forms rebuild, and a rebuild after the cross-build pair silently re-points `release/main` at a
  different cfg set (ordering rule 1, `gates.sh:26-38`).

## Step 3 - Parse `rust-objdump -h --show-lma` correctly

Pass `--show-lma` explicitly. llvm-objdump turns the LMA column on by default only "unless any section
has different VMA and LMAs", so a positional parser otherwise depends on a column that can vanish.

Measurements from all six current ELFs, which are the reason for each rule below:

- **Right-anchored regex, then trim.** Row fields are `Idx Name Size VMA LMA Type` with Size/VMA/LMA
  always eight hex digits in an elf32 image. Anchor on the three hex triples and let the name be
  whatever precedes them:
  `^[[:space:]]*[0-9]+[[:space:]]+(.*)[[:space:]]+([0-9a-fA-F]{8})[[:space:]]+([0-9a-fA-F]{8})[[:space:]]+([0-9a-fA-F]{8})[[:space:]]*(.*)$`
  Names legitimately contain spaces - a defmt interned-string section is named
  `.defmt.error.{"package":"...","tag":"a b",...}` - so left-to-right splitting shreds them. Verified
  identical behaviour on bash 5.3.15 and 3.2.57 (both must keep working, as in elf-provenance).
- **The name capture keeps the column padding.** Bash matches leftmost-longest, so `(.*)` swallows the
  padding: on `  7 .sram1_bss      00000400 ...` it captures `.sram1_bss     `, five trailing spaces.
  Trim it (`name=${name%"${name##*[![:space:]]}"}`, verified on both bashes) before comparing. This is
  not cosmetic: untrimmed, `[ "$name" = ".sram1_bss" ]` never fires, assertion #2 reports `absent` for
  every binary, and the gate passes vacuously - the exact failure AC #3 exists to prevent. A prototype
  of this script hit precisely that bug during planning.
- **Guard the format.** Require `file format elf32-littlearm` from the same output and exit 2 otherwise:
  the eight-hex-digit assumption comes from it, and thumbv7em is the only thing this repo links.
- **Classify by a closed set, and treat empty as normal.** The Type column is composed flags, not one
  token: llvm-objdump concatenates `TEXT`/`DATA`/`BSS` (+ `DEBUG`), so `DATA BSS` is legal, and
  non-allocated rows print *nothing*. Present today: `TEXT`, `DATA`, `BSS`, `DEBUG`, and empty on
  `.comment`, `.ARM.attributes`, `.asp.prov`, `.symtab`, `.strtab`, `.shstrtab`, `.defmt` and index 0.
  So: `TEXT`/`DATA` -> loaded and file-backed; `BSS` -> loaded, no file content; empty or `DEBUG` ->
  not loaded; anything else, including a multi-word combination this script has not been told about ->
  hard error naming the section and the raw row. Empty must NOT be an error, or every ELF fails.
- **Exclude zero-size sections from the high-water mark.** `.gnu.sgstubs` is `ALLOC`+`PROGBITS` with
  size 0 at `0x0800b9c0`, *above* the real flash high-water `0x0800b9b8`, and belongs to no `PT_LOAD`.
  Counting it breaks rule 3's equality by 8 bytes on a correct build. Report it rather than hiding it
  (see the summary line below), so the exclusion is visible.

## Step 4 - The three assertions, with numbers on success

Per binary, print one summary line even when everything passes - a silent pass cannot be told apart
from a pass that checked nothing:

`main         image=47544  low=0x08000000 high=0x0800b9b8 expected=47544  loaded=4 skipped=21  excluded_zero_size=.gnu.sgstubs  .sram1_bss=BSS@0x24000ce0(1024)`

Those are today's real values. Specifically:

1. Every `TEXT`/`DATA` section's LMA end lies inside the parsed flash range. Today only `.bss`,
   `.sram1_bss` and `.uninit` sit outside it, and all three are `BSS`, so the rule is green now and
   stays green for the right reason.
2. If `.sram1_bss` is present it is `NOBITS` (objdump prints `BSS`), reported as present-or-absent
   rather than implied. Only `main` and `rig` have it today - `blinky`, `ledtest`, `podtest` and
   `panictest` carry no SAI buffers at all - so a green run proves this assertion on two binaries out
   of six, and printing it is what stops a reader assuming six.
3. Build the plain image with `rust-objcopy -O binary <elf> "$TMP/x.bin"` and require its length to
   equal `highest flash LMA end - FLASH origin`. Measured today, exactly equal for all six: `main
   47544`, `rig 65696`, `blinky 25176`, `ledtest 19448`, `podtest 31960`, `panictest 25312`. Plain
   `-O binary` is deliberate - it mirrors `firmware/Makefile:119`, so the gate measures the same image
   `dfu-util -D` writes to the board. Two details: also require the lowest file-backed LMA to equal
   `FLASH origin` (it does today, at `.vector_table`; if it ever stopped, the length formula would be
   silently wrong *and* the image would no longer start where DFU loads it), and skip this objcopy for
   any binary whose rule 1 already failed, printing `image length not measured: sections already
   outside flash`. Without that short-circuit a broken tree writes six ~469 MB temp images (peak RSS
   measured 473 MB for one). Clean the temp dir on `trap ... EXIT`.

Exit 1 with one prefixed line per finding, counting them all in one run, per the sibling's contract.

AC #3 asks for a pass "for both cfg sets", and a single run only ever sees the residue of whichever
cross-build finished last. Satisfy it as a demonstration, not a claim, and do it without leaving the
tree in a state that breaks ordering rule 1:

```
bash scripts/check-image-load-addresses.sh                      # the RTT-only residue, green
(cd firmware && cargo build --release --features seed3)         # all six console images
bash scripts/check-image-load-addresses.sh                      # console set, green
touch firmware/src/bin/main.rs
make -C firmware build-elf BINARY=main FEATURES='seed3 log-defmt' NO_DEFAULT=1   # put the pair's residue back
bash scripts/elf-provenance.sh check firmware/target/thumbv7em-none-eabihf/release/main "seed3 log-defmt" 1
```

Record both sets' numbers in the notes. The last two lines are not optional: without them `release/main`
holds a console image, which is precisely the condition the provenance gate above refuses, and the next
`gates.sh push` would fail for a reason nobody caused.

## Step 5 - Register it in `scripts/gates.sh`

Insert after `:289` (end of the provenance gate) and before the clippy comment at `:291`:

`gate push "=== image load addresses ===" bash scripts/check-image-load-addresses.sh`

That single coordinate satisfies both halves of AC #5: it is after the doc-artifact gate (`:188`) and
after both cross-builds (`:262`, `:268-269`), it sees their artifacts, and it builds nothing. Above
the line, write the usual comment: what it asserts, why it sits here rather than in `commit`
(pre-commit builds no firmware, so the ELFs may not exist, and inventing a skip path for that would
recreate elf-check's blind spot), and the measured warm figure. Follow the precedent at `:284-286`
for arguing placement off the artifacts rather than off a claim.

## Step 6 - Regenerate doc-001 and fix the prose figures

AC #5 does not mention this and it is the third file every previous gate commit touched. doc-001
(`backlog/docs/doc-001 - Asperitas-Project-Plan.md`) embeds the matrix at `:250-276` under "This block
is generated - regenerate it with...", plus counts at `:276` and cost prose at `:283` and nearby. TASK-067
(`0f672ba`) is the model: paste fresh `--list` output, move the counts, and correct every restated
tier cost in the same commit. A push-tier gate here moves the counts from commit 10 / push 18 / ci 19 to
commit 10 / push 19 / ci 20; if `--list` says otherwise, the gate line is in the wrong tier.

## Step 7 - Prove it red, on the real defect

Not a hand-edited ELF, and not `--orphan-handling=error` (measured: rust-lld accepts it and names only
`.debug_*`, `.comment`, `.ARM.attributes` - never `.sram1_bss`). Planning-time run of this exact
recipe, warm, 2.85 s to build the bad image and 1.33 s to come back:

```
cp firmware/memory.x /tmp/memx.bak
perl -0pi -e 's{^SECTIONS \{}{/* RED RUN - rule disabled */\n/*\nSECTIONS \{}m; s{^\} INSERT AFTER \.bss;$}{*/}m' firmware/memory.x
make -C firmware build-elf BINARY=main FEATURES='seed3 log-defmt' NO_DEFAULT=1
bash scripts/check-image-load-addresses.sh main; echo "exit=$?"
git checkout -- firmware/memory.x
touch firmware/src/bin/main.rs
make -C firmware build-elf BINARY=main FEATURES='seed3 log-defmt' NO_DEFAULT=1
```

`build.rs:33` emits `rerun-if-changed=memory.x`, so editing the script relinks without the `touch`;
the `touch` afterwards is `firmware/Makefile:236`'s documented remedy, needed because restoring the
bytes leaves them older than the ELF you just built. Expect these observations, which the executor
must reproduce with the finished script and paste into the ticket notes and the commit body:

- the ELF row becomes `  5 .sram1_bss  00000400 240001d0 240001d0 DATA` - `DATA`, i.e. ALLOC +
  PROGBITS, placed by lld with LMA == VMA right after `.data`, and `.bss` shifts up to `0x240005d0`;
- rule 1 fails naming `.sram1_bss` at LMA `0x240001d0`, exit 1;
- plain `-O binary` yields 469,763,536 bytes against an expected 47,544, so rule 3 catches the same
  defect independently;
- after restore and rebuild: `47544`, `BSS@0x24000ce0`, exit 0, and
  `bash scripts/elf-provenance.sh check firmware/target/thumbv7em-none-eabihf/release/main "seed3 log-defmt" 1`
  still agrees, which confirms the experiment left the residue in the state rule 1 promises.

## Step 8 - Price it

Time the gate alone and all three tiers warm inside `nix develop .#default` on a clean tree. The
whole-script floor measured with a prototype over all six binaries is 0.678 s wall (6 x objdump at
~0.058 s, 6 x objcopy at ~0.03 s). Put the banner figure in the gate comment, the tier deltas in
doc-001, and both in the notes. If it lands far above ~1 s, find out why before landing it.

## Step 9 - Land it

`bash scripts/gates.sh ci` green from the repo root, then one commit adding
`scripts/check-image-load-addresses.sh` and touching only `scripts/gates.sh`,
doc-001 and the ticket file, with the red-run transcript in the body. Confirm
`bash scripts/gates.sh --list` and `--dry-run` both look right before committing.

## Non-goals

- No `--selftest` here. It is queued as its own ticket (TASK-068) rather than folded into this commit,
  following how `elf-provenance.sh` got its 19-case suite after landing (TASK-067). The parser in step
  3 is the fragile part and that ticket names the four traps to lock down.
- No change to `firmware/memory.x`, the Makefile, or any Rust. This ticket gates the fix; it does not
  move it.
- No hardware. Nothing here needs a board or ears; TASK-059.03 already owns hearing `main` and `rig`
  on the device.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Landed today, with the coordinates this time (2026-09-13, TASK-062.03).

`=== firmware ELF cfg provenance ===` is now registered at scripts/gates.sh:268 (`gate push`, calling
`bash scripts/elf-provenance.sh check firmware/target/thumbv7em-none-eabihf/release/main
"$RTT_ONLY_FEATURES" 1`), between the RTT-only cross-build at :249 and the two cross-clippy gates at
:279/:282. Put your line after :268 and before those clippy gates and both halves of your AC #5 hold
with one coordinate. Counts after this change: commit 9, push 17, ci 18.

Point 1 stands as written: call `scripts/elf-provenance.sh show <elf>` rather than writing objcopy
plumbing again. Point 3 also stands - the apology about provenance blindness can come out, because the
gate above you now names the cfg set of the ELF you are about to read, and it is always the RTT-only
one in every tier that runs the pair.
---

Executed 2026-09-14 against `0f672ba`. Landed as described below; every number here is a measurement
from this tree, and the three places where the plan and the tree disagreed are called out at the end.

## What landed

`scripts/check-image-load-addresses.sh` (~420 lines, most of it header and comment), registered at
`scripts/gates.sh` as `gate push "=== image load addresses ==="`, positioned after the provenance gate
and before both cross-clippy gates, so it reads the artifacts the cross-build pair leaves and builds
nothing itself. Inputs all derived: FLASH extent grepped from `firmware/memory.x` (K/M suffix handled),
target triple and release directory read from `firmware/.cargo/config.toml`'s `[build] target`, ELF list
enumerated from `firmware/src/bin/*.rs`. No hardcoded `0x08020000`, no hardcoded `thumbv7em`, no
literal six. Exit codes 0 / 1 / 2 with 2 reserved for "could not run", findings accumulated and printed
at the end, one prefixed line each, `usage()`, explicit `--show-lma`, `file format elf32-littlearm`
guard, closed-set type classifier that refuses an unrecognized Type rather than guessing, zero-size
file-backed sections excluded from the water marks and named on the summary line.

## Green on both cfg sets

RTT-only residue (`bash scripts/gates.sh push` then bare run), and the console set built with
`cargo build --release --features seed3`. Image lengths equal `highest flash LMA end - FLASH origin` on
every binary in both sets.

| binary    | RTT-only image | console image | high-water (RTT) | `.sram1_bss` |
| --------- | -------------- | ------------- | ---------------- | ------------ |
| main      | 47544          | 88805         | `0x0800b9b8`     | `BSS@0x24000ce0(1024)` |
| rig       | 65696          | 115065        | `0x080100a0`     | `BSS@0x24000e34(1024)` |
| blinky    | 25176          | 58064         | `0x08006258`     | absent |
| ledtest   | 19448          | 41336         | `0x08004bf8`     | absent |
| podtest   | 31960          | 67192         | `0x08007cd8`     | absent |
| panictest | 25312          | 58200         | `0x080062e0`     | absent |

The console row for `main` (88,805 = `0x08015ae5`) independently confirms `firmware/Makefile:149`'s
"about 106,811 bytes" refers to the *rig* console image, which measured 106,811 exactly when built with
that recipe. Residue was put back afterwards and
`scripts/elf-provenance.sh check .../release/main "seed3 log-defmt" 1` still agrees, so ordering rule 1
holds.

## Red runs, four of them

AC #4 asked for one. The first is the real defect, done as the plan prescribed: SECTIONS rule commented
out in `memory.x`, `make -C firmware build-elf BINARY=main FEATURES="seed3 log-defmt" NO_DEFAULT=1`, then
`bash scripts/check-image-load-addresses.sh main`. Exit **1**, three findings:

```
main: .sram1_bss: LMA 0x240001d0..0x240005d0 is outside FLASH 0x08000000..0x08020000 - a file-backed
section loading outside flash makes 'objcopy -O binary' span the gap, which is the 469 MB image of
TASK-059; give the section '(NOLOAD)' placement in firmware/memory.x, or a load address with AT>
main: plain '-O binary' would write 469763536 bytes (0x08000000..0x240005d0) where the parts inside
flash hold 47544 - ...
The rule is that a section with file content loads inside flash. ...
```

Its summary line read `image=not-written(would-be-469763536)`. A hand-run plain `-O binary` on that same
ELF wrote exactly 469,763,536 bytes, so the computed length is not a guess. Restore, relink, green at
47544 with `.sram1_bss=BSS@0x24000ce0(1024)`.

The other three were synthetic, on copies in `/tmp`, because two of the rules cannot be reached through
`memory.x` at all:

- **R2 fires on its own axis.** Flipping `.sram1_bss`'s `sh_type` from `SHT_NOBITS` to `SHT_PROGBITS`
  makes objdump print `DATA` for it, and the gate adds: `.sram1_bss: typed 'DATA' rather than BSS -
  firmware/memory.x places it '(NOLOAD)', so a file-backed section by that name means the SECTIONS rule
  stopped matching daisy-embassy's input sections`. Exit 1.
- **The general rule is general.** Retargeting the `PT_LOAD` that carries `.data` to `p_paddr
  0x24010000` puts a differently-named section in SRAM while `.sram1_bss` stays healthy, and R1 names
  `.data` at `0x24010000..0x240101d0`. That is the case a future dependency would produce, and it is
  caught without any name appearing in the rule.
- **Unreadable input refuses rather than skipping.** Truncating an ELF by 64 bytes makes objdump say
  `section table goes past the end of file`; the script exits **2** quoting that. Deleting
  `firmware/.cargo/config.toml` likewise exits 2 naming the file.

## Cost

Gate alone, warm, over all six ELFs: **0.70 s**. Tiers warm on aarch64-darwin inside
`nix develop .#default`: commit **3.0 s**, push **76.0 s**, ci **141.0 s** (two ci runs: 141.0 and
142.0, the delta being cargo, not this gate). doc-001's generated matrix now shows commit 10 / push 19 /
ci 20; its prose said ci 141 s before and says it after, which is honest because 0.7 s rounds away.

## Where the plan and the tree disagreed

1. `.sram1_bss`'s LMA is **not** `0x24000000` on a healthy build. It is `0x24000ce0` on `main`, equal to
   its VMA, i.e. rust-lld gives the `(NOLOAD)` output section LMA == VMA regardless of the rule. What
   actually keeps the image honest is that the section is `SHT_NOBITS`, so objcopy has no file content
   for it and never writes toward that address. TASK-059.03 should not write "LMA 0x24000000" into the
   reference doc as a fact about healthy builds; the ticket text and planning notes both say it.
2. Rule 3's mismatch branch cannot be provoked by editing a linked ELF, which is worth knowing before
   TASK-068 writes fixtures. objdump reconstructs the per-section LMA column from each `PT_LOAD`'s
   `p_paddr - p_vaddr` delta, so any hand-edited `sh_size`, `sh_offset` or `p_filesz` moves the
   reconstructed LMA too and R1 refuses first; widening a segment's `p_filesz` instead leaves objcopy's
   output unchanged, because it clamps to real section content. Verified instead by agreement: the
   formula equals the real image size on all twelve measurements above, and on the mutated ELF the
   predicted 469,763,536 matched what objcopy really wrote.
3. The plan hardcoded `firmware/target/thumbv7em-none-eabihf/release`. The script reads the triple out
   of `firmware/.cargo/config.toml` instead, so the directory it walks is the one cargo writes. Same
   reasoning as parsing FLASH out of `memory.x`.

Left for TASK-068, unchanged from the plan: the parser's fixture suite, including the defmt-name trap,
the padding trap, `.gnu.sgstubs`, and the four exit codes graded through real child processes.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-13 13:05
---
Coordination note from planning TASK-062 (2026-09-13), answering the note left here earlier.

TASK-062 landed its plan as three leaves. .01 stamps a non-allocated .asp.prov note section into all six firmware ELFs from build.rs (measured: VMA/LMA 0x0 beside .defmt, no KEEP fragment needed, boardless probe-rs still reaches "No connected probes were found", main.bin grows 156 B). .02 adds scripts/elf-provenance.sh, the one host-side reader, plus elf-check enforcement and a remedy that forces a real relink. .03 registers a push-tier gate asserting what the two cross-builds leave in target/.

Three things this ticket should then do differently from its current plan.

1. Do not write your own ELF reader. Call scripts/elf-provenance.sh show <elf> instead of duplicating objcopy plumbing, and keep rust-objdump -h for the load-address rules themselves.
2. Gate position. Your AC #5 wants "after the doc-artifact gate AND after both cross-build gates", and your step 6 points at gates.sh:264 for the doc-artifact gate - that coordinate is stale, the doc-artifact gate is at :186, well before both cross-builds, and :264 is inside the ci-only pod-hw block. Put your line after TASK-062.03's provenance gate, which sits directly after the RTT-only cross-build at :241-242, and both halves of AC #5 become satisfiable with one coordinate.
3. Your header no longer has to confess provenance blindness as accepted. Once .03 lands, the gate above you names the cfg set of the ELF you are about to read, so say which set that is and drop the apology. What stays true is that you validate one cfg set per run, not both at once.

Your ban on the test -f ... || true else-branch is the same rule .02's AC #5 implements for the provenance path, so the two checks will fail alike on a missing artifact. Good riddance to the skip path either way.
---

created: 2026-09-14 03:38
---
Planning round 2026-09-14 (commit 0f672ba). Plan rewritten against the tree as it stands; three things earlier notes here assert turned out to be false when measured, and the plan's "Where the ticket's wording does not match the tree" section is the authoritative correction. Summary for whoever reads this before the plan:

1. Comment #1's advice to call `scripts/elf-provenance.sh show` instead of writing ELF plumbing does not apply. `show` prints one line from the `.asp.prov` note (`default=... features=... defmt_log=`); the script never opens a section header and never calls objdump. Only its conventions transfer, and the plan lists which.
2. The `rust-readelf -W -S` alternative in the old step 3 is unreachable: there is no readelf, llvm-readelf or rust-readelf on PATH here (cargo-binutils ships rust-objdump/rust-objcopy/rust-readobj), and `readelf -W -S` has no LMA column anyway. `rust-objdump -h --show-lma` is the tool, and `--show-lma` must be explicit because llvm-objdump only turns the column on by default when some section already differs.
3. There is no local-warm cost table in gates.sh, and doc-001 embeds the gate matrix that AC #5 leaves unmentioned. Both have real substitutes, named in the plan.

Everything load-bearing in the new plan was measured rather than reasoned about: rules 1-3 hold exactly on all six current ELFs (image lengths 47544/65696/25176/19448/31960/25312, each equal to highest flash LMA end minus 0x08000000), and the defect itself was reproduced end to end - disable the SECTIONS rule, relink main (2.85 s warm), and `.sram1_bss` comes back as DATA at LMA 0x240001d0 with a 469,763,536-byte image, tripping rules 1 and 3 independently; restore and relink (1.33 s) returns it to BSS at 0x24000ce0 and 47544 bytes. Two parser traps found that way are called out explicitly, because both produce a green run that checked nothing: bash keeps objdump's column padding in the name capture, so an untrimmed comparison reports .sram1_bss absent from every binary; and `.gnu.sgstubs`, zero-size but ALLOC+PROGBITS, sits above the true high-water mark and breaks rule 3 by 8 bytes on a correct build.

No sub-tickets: the script, its registration, the red run and the pricing ship as one commit or not at all. The parser's own fixture suite is deliberately split out to TASK-068 rather than widened into this ticket, following how elf-provenance.sh got its suite after landing. Nothing here needs hands - no board, no ears, no instrument - so nothing is @human; TASK-059.03 already owns hearing main and rig on hardware.
---
<!-- COMMENTS:END -->
