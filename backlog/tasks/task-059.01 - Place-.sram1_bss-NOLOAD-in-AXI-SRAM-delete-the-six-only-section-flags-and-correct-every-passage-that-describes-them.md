---
id: TASK-059.01
title: >-
  Place .sram1_bss NOLOAD in AXI SRAM, delete the six --only-section flags, and
  correct every passage that describes them
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-13 06:53'
updated_date: '2026-09-13 08:00'
labels:
  - planned
dependencies: []
references:
  - firmware/memory.x
  - firmware/Makefile
  - scripts/check-doc-artifact-names.sh
parent_task_id: TASK-059
priority: medium
type: task
ordinal: 105800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
# What actually causes the 469 MB image

This ticket's original title guessed the fix ("give `.sram1_bss` a flash load address", the
ARMmbed/NXP prior art). Planning measured the mechanism and that guess is wrong, so this
description supersedes the one filed with TASK-057.

`.sram1_bss` is an **orphan output section**: nothing in any linker script declares it, so
rust-lld's built-in orphan rules place it, right after `.data`. lld's LMA rule then decides
the load address - "if neither `AT(lma)` nor `AT>lma_region` is specified: if the previous
section is also in the default LMA region ... otherwise, the LMA is set to the VMA" (MaskRay,
*Output section LMA*). `.data` uses `AT>FLASH`, so the orphan gets LMA == VMA inside AXI SRAM,
and `llvm-objcopy -O binary` therefore spans `0x08000000..0x24000598` and pads the gap with zeros.

Why daisy-embassy's own rule never fires: `link.x.in` performs exactly one `INCLUDE memory.x`,
and three `memory.x` files sit on the `-L` path with ours first (link-line order:
`asperitas-firmware/out`, cortex-m, embassy-stm32, cortex-m-rt, defmt, daisy-embassy,
stm32-metapac). Only one wins. daisy-embassy's `.sram1_bss (NOLOAD) ... > RAM_D2` rule *and its
whole MEMORY block* are dead code in this build, which is why the section lands in AXI SRAM as
allocated DATA rather than where their script says. Confirm all of this yourself with a link map
before changing anything (AC #1); the paragraph above is a hypothesis with someone else's
measurements attached, not your evidence.

Why it is file-backed at all despite being `MaybeUninit`: rustc emits custom-named sections as
`SHT_PROGBITS` whatever their initialiser, and only a name starting `.bss.` becomes `NOBITS`.
link.x's own `.bss` and `.uninit` escape that blowup only because link.x declares those
*output* sections `(NOLOAD)`. That is why `-R .bss -R .uninit` cannot help.

# The fix: the injection hook cortex-m-rt already provides

Append to `firmware/memory.x`:

    SECTIONS {
      .sram1_bss (NOLOAD) : ALIGN(4) {
        *(.sram1_bss)
        *(.sram1_bss*)
      } > RAM
    } INSERT AFTER .bss;

`(NOLOAD)` makes lld emit `SHT_NOBITS` even though every input section is `PROGBITS`.
`INSERT AFTER .bss` is the sanctioned hook, documented at `link.x.in:169-172`: "Allow sections
from user `memory.x` injected using `INSERT AFTER .bss` to use the .bss zeroing mechanism by
pushing `__ebss`. Note: do not change output region or load region in those user sections!" -
which is precisely why there is no `AT>` here.

Rejected alternatives, both measured:

- `> RAM AT> FLASH` without `NOLOAD` (the mbed/NXP shape) collapses the image but writes 1,027
  bytes of junk zeros into flash that nothing copies - cortex-m-rt copies only
  `__sidata..__edata`, and a copied-from-flash `.sram1_bss` would not be run through the copy
  loop anyway. Strictly worse, and it violates the note quoted above.
- Placing the rule without `INSERT AFTER .bss` also gives the right size but moves `.data` too
  (`0x24000000` -> `0x24000400`) and depends on include ordering rather than on a documented hook.

Two traps while writing it: keep `> RAM`, meaning *our* AXI region at `0x24000000`. Do not copy
daisy-embassy's regions: their `memory.x:21` does `REGION_ALIAS(RAM, DTCMRAM)` and puts these
buffers in `RAM_D2`, and DMA1/DMA2 cannot reach DTCM (cf. embassy-rs/embassy#3747) - audio would
die with no compile error. `task-038.03`'s notes already tell whoever touches this next not to
"correct" the AXI placement.

# What the change does to the layout

Buffers move from `0x24000198` to `0x240021b8..0x240025b8`; every `.bss` object shifts **down**
0x400; total RAM footprint, `__ebss`, `__sheap` and `_stack_end` stay at `0x240025b8`. Because
0x400 is a multiple of 32 the buffers keep their offset-within-cache-line, and the D-cache is off
anyway repo-wide (`docs/reference/daisy-seed3.md:659-666`), so coherency is not a live risk. They
do land inside the boot zero range `__sbss..__ebss`, which is harmless: daisy-embassy self-zeroes
them at prepare time (`ca9bcc9 src/audio.rs:67-80`).

Only `main` and `rig` link the audio module, so only they carry the section. `blinky`, `ledtest`,
`podtest` and `panictest` must still link and produce identical images.

# This commit is also the last one allowed to describe the workaround

The six `--only-section` flags in `firmware/Makefile`'s `build:` rule exist only because of this
bug, and ~18 sentences across three files recite the 469 MB story as the reason those flags are
load-bearing (TASK-057 wrote them, TASK-058 gated part of them). Fixing the layout without
deleting the crutch leaves the repo documenting a hazard it no longer has, and keeps the doc
footgun alive: `--only-section=.typo` exits 0 writing a 0-byte file. So this ticket deletes the
flags and corrects the prose in the same commit. Splitting them would leave main green but lying.

Out of scope: the regression gate (TASK-059.02) and the bench session (TASK-059.03).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A link map built on the unmodified tree establishes which linker input places `.sram1_bss`, and both the answer and the exact command that produced it are recorded in this ticket's notes. Use `-C link-arg=-Map=<file>`; the `-C link-arg=-Wl,-Map=out.map` form named in the original criterion fails outright (`rust-lld: error: unknown argument '-Wl,-Map=...'`). The notes must also say which `memory.x` cortex-m-rt actually included, which settles whether daisy-embassy's `(NOLOAD)` rule is dead code rather than merely overridden. Confirm rather than assume.
- [x] #2 After the change, `rust-objdump -h` on rebuilt `main` and `rig` shows `.sram1_bss` as a `NOBITS`/`BSS` section rather than allocated `DATA`, and no ALLOC + PROGBITS section in any of the six images has an LMA outside flash. Record the addresses you measured.
- [x] #3 Plain `cargo objcopy --release --features seed3 --bin main -- -O binary /tmp/out.bin`, with no `--only-section` flags, yields an image within a few percent of what `make build BINARY=main` produces instead of 469,763,480 bytes, and the same holds for `rig`. Expected 88,581 and 106,811 bytes - the sizes `make build` produces today.
- [x] #4 All six bin targets still link under both cfg sets (`--features seed3` and `--no-default-features --features "seed3 log-defmt"`), and the notes carry a before/after byte table for all six in both configs with every non-zero difference explained. Total RAM footprint, `__ebss`, `__sheap` and `_stack_end` must be unchanged, and the first four little-endian bytes of every image must still read `0x24080000` - the initial stack pointer is the only host-visible witness that the board did not bus-fault before `main`.
- [x] #5 The six `--only-section` flags are gone from `firmware/Makefile`'s `build:` recipe, and every passage that described them or quoted their numbers is corrected in the same commit: `firmware/Makefile:89-115`, `firmware/Cargo.toml:41-55` (table re-measured, prose updated), `docs/reference/daisy-seed3.md:110-127` plus the cache-line table row at `:621-627` recomputed from fresh `nm` output. No `*.bin` size claim anywhere in the repo still describes pre-fix behavior.
- [x] #6 `scripts/check-doc-artifact-names.sh` no longer proves defmt survival by grepping for `--only-section='*.defmt*'` (that flag list no longer exists), its replacement assertion still fails when a keep-list comes back, and all twelve of its checks plus `bash scripts/gates.sh ci` pass in `nix develop .#default`.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Work top to bottom. Every number you meet below is someone else's measurement at `80c7276`;
re-measure it and write down what *your* run said, including when it agrees.

**Step 0 - Baseline, before touching anything.** Inside `nix develop .#default`, from
`firmware/`:

- Build both cfg sets: `cargo build --release --features seed3`, then
  `cargo build --release --no-default-features --features "seed3 log-defmt"`.
- For each of the six bins (`main rig blinky ledtest podtest panictest`) record: plain image size
  via `rust-objcopy -O binary target/thumbv7em-none-eabihf/release/<bin> /tmp/before-<bin>.bin`,
  `size -A` output, `rust-objdump -h ... | grep -E 'sram1_bss|\.data'`, and
  `rust-nm -n --print-size ... | grep -E '__ebss|__sheap|_stack_end|RX_BUFFER|TX_BUFFER'`.
- Save all of it to `/tmp/task059-before.txt`. Expected plain sizes (console cfg): main 469,763,480,
  rig 469,763,480, blinky 65,638, ledtest 17,774, podtest 72,689, panictest 65,958; RTT cfg main
  48,360 / rig 65,696.

Use `rust-objcopy` / `rust-objdump` with an explicit ELF path, not `cargo objcopy` /
`cargo objdump`: verified 2026-09-13 that the bare-tool form is a pure passthrough (0.16 s, no
build), whereas the `cargo` forms rebuild and would silently re-point
`target/thumbv7em-none-eabihf/release/main` at whichever cfg set ran last - the exact hazard
`scripts/gates.sh`'s ordering rule 1 exists to prevent. Finish one cfg set's measurements before
starting the other. Run `cargo objcopy` only where AC #2 names it, and run it last.

**Step 1 - Root cause (AC #1).** Produce the link map on the *unmodified* tree:

    cd firmware && cargo rustc --release --features seed3 --bin main -- -C link-arg=-Map=/tmp/before.map

`-C link-arg=-Wl,-Map=out.map`, the form TASK-059's original AC #1 named, fails outright here:
`rust-lld: error: unknown argument '-Wl,-Map=...'`. Use the lld-native form. From the map, record:
which object feeds `.sram1_bss` (expect a `daisy_embassy-*.rlib` member), which output section
precedes it, and its VMA/LMA. Also record which `memory.x` was included, and confirm by inspection
that our file has no `SECTIONS` block while daisy-embassy's does - that is the proof their rule is
dead code rather than merely overridden. Put the command and the answer in this ticket's notes.

Do not bother with `--orphan-handling=error` as a guard: measured, rust-lld accepts it but names
only `.debug_*`, `.comment` and `.ARM.attributes`, not `.sram1_bss`. Say so in the notes so nobody
proposes it again.

**Step 2 - The change.** Append the `SECTIONS { ... } INSERT AFTER .bss;` block from the
description to `firmware/memory.x`, with a comment covering what the code cannot say: that ours is
the only `memory.x` cortex-m-rt includes, so any placement these buffers need must live *here*;
that `(NOLOAD)` is what makes lld emit `NOBITS` despite rustc emitting `PROGBITS` inputs; that
`> RAM` means AXI on purpose because DMA1 cannot reach DTCM, so do not copy daisy-embassy's
regions; and that `link.x.in:169-172` forbids changing the output or load region inside injected
sections. Keep the existing MEMORY-block comments untouched - the 512K length there is
load-bearing for the initial stack pointer.

**Step 3 - Placement and type (AC #2).** Rebuild the console cfg and check:

- `rust-objdump -h` shows `.sram1_bss` as type `BSS` at `0x240021b8` (not `DATA` at `0x24000198`).
- No ALLOC+PROGBITS section has an LMA outside `0x08000000..0x08020000`. Check every row, not just
  the interesting ones - `.defmt.*` and `.gnu.sgstubs` are numerous but flash-loaded.
- `rust-nm -n` shows `__ebss == __sheap == _stack_end` still `0x240025b8`, i.e. zero RAM growth.

If `.sram1_bss` is still `DATA`, the rule did not match its input sections and lld orphan-placed it
again; the fix would be silently inert. That is why AC #2 asks for the type, not just the size.

**Step 4 - Sizes (AC #3, #4).** Repeat Step 0's whole matrix for both cfg sets into
`/tmp/task059-after.txt`, plus `make build BINARY=<bin>` for each bin so the plain image can be
compared against what the repo actually flashes. Expect: plain == keep-list output byte-for-size for
all six, main 88,581 / rig 106,811 / blinky 65,638 / ledtest 17,774 / podtest 72,689 /
panictest 65,958, and RTT main 48,360 / rig 65,696. Put the before/after table in the notes with
every non-zero delta explained. Sizes are the invariant; bytes are not - expect roughly 200 bytes of
`.text` *content* to differ in `main` because literal pools follow the moved statics, at identical
size. Record that explicitly, because it is the reason AC #2's size agreement is necessary but not
sufficient and why TASK-059.03 exists.

**Step 5 - Fault-before-main witness.** For all six images, the first four little-endian bytes of
the `.bin` are the initial SP and must still read `0x24080000`
(`head -c4 <bin>.bin | xxd -e`). A layout mistake that bus-faults before `main` is invisible to USB
and LEDs (`docs/reference/daisy-seed3.md:183-190`); this host-side reading is the only cheap signal
for it, and `task-038.03`'s Renode-backed note shows a naive NOLOAD placement causing exactly that.

**Step 6 - Delete the crutch.** In `firmware/Makefile`, drop the six `--only-section` lines from the
`build:` recipe and rewrite the comment at `:89-115`. It currently explains why the flags are
load-bearing; it must now explain why the plain image is correct (one loadable region, in flash;
`.sram1_bss` is `NOLOAD` and contributes nothing) and keep the two things still worth knowing: the
measured flash-budget table for the current profile, and why each artifact carries `$(BINARY)` in its
name. Delete the `--only-section=.typo` silent-truncation anecdote - it documents a footgun that no
longer exists.

**Step 7 - `firmware/Cargo.toml`.** Re-measure all four cells of the DWARF-cost table at `:44-55`
(both rows, both binaries, both cfg sets) and update the prose at `:41-43` that cites them. Keep
TASK-057's convention of dating any disagreement between two figures rather than deleting one.

**Step 8 - `docs/reference/daisy-seed3.md`.** Rewrite `:110-127`: the flags are gone, so the passage
must stop saying they are load-bearing, stop quoting 469,763,480 as the current behavior, and delete
the sentence pointing at TASK-059 "until it lands". State what a reader should do instead (use
`make build`, or a plain `-O binary` if they must) and why it is now safe. Then fix the cache-line
table row at `:621-627`: `RX_BUFFER`/`TX_BUFFER` move, so recompute their addresses and their
offset-within-cache-line from your own `nm` output - do not copy the numbers in this ticket. Leave
`:659-666` (nothing enables I- or D-cache) alone; this change does not touch it.

**Step 9 - `scripts/check-doc-artifact-names.sh`.** Its R5 assertion greps the Makefile for
`--only-section='*.defmt*'` to prove defmt records survive into the image. That string disappears
with the flags, so replace the assertion with one that stays true and still earns its place: the
gate should require that the `build:` recipe contains **no** `--only-section` flag at all, so a
future commit that reintroduces the keep-list is noticed, and the defmt-survival claim moves to
TASK-059.02's gate (if that has landed) or to a direct check that the produced image covers every
flash-LMA section. Do not weaken the other eleven checks.

**Step 10 - Gate everything.** `bash scripts/gates.sh ci` from the repo root inside the dev shell -
twelve doc-gate checks plus every cargo gate green. Note that `gates.sh` builds the console cfg then
the RTT-only cfg, so after it runs, `release/main` holds the RTT image: re-run your console-cfg
measurements after the gate run, not before.

**Step 11 - Land it.** One commit carrying memory.x, the Makefile, Cargo.toml, the doc, and the gate
script change, with the before/after table and the link-map finding in the commit message body.
Record both in this ticket's notes too. Then stop: TASK-059.03 owns the bench, and per CLAUDE.md no
agent may mark anything in this family Done while a `HUMAN:` criterion is open. Write the notes as if
the next person has never seen this file, because they have not.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## AC #1 - what places `.sram1_bss`, measured on the unmodified tree at `d096b28`

Command that works (`-C link-arg=-Wl,-Map=out.map` fails outright with
`rust-lld: error: unknown argument`):

    cd firmware && cargo rustc --release --features seed3 --bin main -- -C link-arg=-Map=/tmp/before.map

The map names the input and the neighbours (`/tmp/before.map`:1857):

    24000198 24000198      400     4 .sram1_bss
    24000198 24000198      400     4         .../libdaisy_embassy-6c7c5ff991887a26.rlib(...-cgu.0.rcgu.o):(.sram1_bss)
    24000198 24000198      200     1                 daisy_embassy::audio::RX_BUFFER
    24000398 24000398      200     1                 daisy_embassy::audio::TX_BUFFER

The output section immediately before it is `.data` (VMA `0x24000000`, LMA `0x08015808`, so
`AT>FLASH`). That is the whole mechanism: no linker script declares `.sram1_bss`, rust-lld's orphan
rules place it after `.data`, and lld's LMA rule ("if the previous section is also in the default
LMA region ... otherwise the LMA is set to the VMA") hands it LMA == VMA because `.data` is not in
the default load region.

Which `memory.x` cortex-m-rt included: **ours**. `cargo build -vv` shows the `-L` order as
`asperitas-firmware/out`, cortex-m, embassy-stm32, cortex-m-rt, defmt, daisy-embassy, stm32-metapac,
and exactly three of those dirs contain a `memory.x`: ours, embassy-stm32's, daisy-embassy's.
`link.x:23` performs one and only one `INCLUDE memory.x`, so the first wins. Ours has no `SECTIONS`
block; daisy-embassy's has the `(NOLOAD) ... > RAM_D2` rule plus `REGION_ALIAS(RAM, DTCMRAM)`. Their
MEMORY block never entered this link at all: `grep -cE 'RAM_D2|DTCMRAM|ITCMRAM|SDRAM|QSPIFLASH'
/tmp/before.map` returns **0**. Dead code, not an overridden rule.

Two things the map gave that the plan did not predict:

- `__edata` sat at `0x24000598`, i.e. past the orphan, while `.data` itself ends at `0x24000198`.
  cortex-m-rt copies `__sidata..__edata`, so startup was copying 0x598 bytes instead of 0x198 and
  writing flash bytes belonging to `.defmt.*` over the top of both DMA buffers. Harmless only
  because `prepare_interface` (`ca9bcc9 src/audio.rs:66-78`) zeroes them before use. After the
  change `__edata` is back to `0x24000198`.
- `--orphan-handling=error` is confirmed useless as a guard, by my own run rather than planning's:
  on the unmodified tree rust-lld emits a wall of `.debug_info` / `.comment` placements and stops at
  its error limit without ever naming `.sram1_bss`. Do not propose it.

## AC #2 - type and addresses after the change

`rust-objdump -h`, `.sram1_bss` row (VMA | LMA | type):

| image | before | after |
|---|---|---|
| `main` console | `24000198 / 24000198 / DATA` | `240021b8 / 240021b8 / BSS` |
| `rig` console | `24000198 / 24000198 / DATA` | `24001e10 / 24001e10 / BSS` |
| `main` RTT | `240001d0 / 240001d0 / DATA` | `24000ce4 / 24000ce4 / BSS` |
| `rig` RTT | `240001d0 / 240001d0 / DATA` | `24000e34 / 24000e34 / BSS` |

`nm` on the rebuilt RTT `main`: `RX_BUFFER 0x24000ce4`, `TX_BUFFER 0x24000ee4`; console `main` puts
them at `0x240021b8` / `0x240023b8`. Both stay 24 bytes into their 32-byte cache line, as they were
at `0x24000198` (the move is a multiple of 32 either way).

No ALLOC + PROGBITS section loads outside flash in any of the twelve images. Checked every row of
`rust-objdump -h` programmatically, parsing right-to-left because `.defmt.*` names contain spaces,
and excluding only rows whose type column is `BSS` (NOBITS, contributes nothing) or `DEBUG`
(non-ALLOC, LMA 0). Zero hits across six bins x two cfg sets.

## AC #3, #4 - sizes, both cfg sets, plain vs `make build`

Plain `cargo objcopy ... -O binary` (no flags) against what `make build` writes, bytes:

| bin | plain before | plain after | make before | make after |
|---|---|---|---|---|
| main (console) | 469,763,480 | 88,581 | 88,581 | 88,581 |
| rig (console) | 469,763,480 | 106,811 | 106,811 | 106,811 |
| blinky | 65,638 | 65,638 | 65,638 | 65,638 |
| ledtest | 17,774 | 17,774 | 17,774 | 17,774 |
| podtest | 72,689 | 72,689 | 72,689 | 72,689 |
| panictest | 65,958 | 65,958 | 65,958 | 65,958 |
| main (RTT-only) | 469,763,536 | 48,360 | 48,360 | 48,360 |
| rig (RTT-only) | 469,763,536 | 65,696 | 65,696 | 65,696 |
| blinky | 25,176 | 25,176 | 25,176 | 25,176 |
| ledtest | 19,448 | 19,448 | 19,448 | 19,448 |
| podtest | 31,960 | 31,960 | 31,960 | 31,960 |
| panictest | 25,312 | 25,312 | 25,312 | 25,312 |

Every non-zero delta is the four audio-bearing cells, and each is exactly the gap-closing this
ticket is about; the RTT figures are 56 bytes larger than console before the fix because `.data` is
that much bigger there, which is why the span differs too. Plain and `make build` agree byte-for-size
on all twelve measurements after the change. All six binaries still link under both cfg sets.

Invariants held: `__ebss` and `_stack_end` are identical before/after for every bin in both cfg sets
(console main `240025b8`, rig `24002210`, blinky `240016dc`, ledtest `24000354`, podtest `2400174c`,
panictest `24001704`; RTT main `240010e4`/`240014e4`, rig `24001234`/`24001634`, blinky
`24000484`/`24000884`, ledtest `24000394`/`24000794`, podtest `240004f4`/`240008f4`, panictest
`240004ac`/`240008ac`), so zero RAM growth, and the first four little-endian bytes of all twelve
images read `00 00 08 24` = initial SP `0x24080000`.

Sizes are the invariant, bytes are not. Comparing before against after over each image's own post-fix
length: blinky, ledtest, podtest and panictest are **byte-identical** in both cfg sets (they link no
audio); `main` differs in 209 bytes (console) and 67 (RTT), `rig` in 210 and 132, and every one of
those diffs falls inside `.text` - literal pools following the moved statics, at identical section
size. That is precisely why TASK-059.03 exists: no host-side check can hear it.

## AC #5 - prose and numbers corrected in this commit

- `firmware/Makefile`: six `--only-section` lines deleted from `build:`; the comment now explains why
  the plain image is correct, keeps the flash-budget table (re-measured today, same four numbers),
  and drops the `--only-section=.typo` anecdote along with the flags that made it worth knowing.
- `firmware/Cargo.toml`: all four DWARF-cost rows re-cut today, one fresh target dir per row, RTT-only
  `main` (ELF 261,484 / 3,012,380 / 4,738,792 / 9,496,376; shipped cell `main.bin` 48,360, `.text`
  39,136, flash price still 236 bytes). Old numbers kept beside the new ones per TASK-057's dating
  convention. `DEFMT_LOG=info` re-measured too: 48,504, unchanged. Clean builds 20 s at `false` against
  21 s at `2`. The reproduce line now uses `CARGO_PROFILE_RELEASE_DEBUG`, because setting `RUSTFLAGS`
  overrides `firmware/.cargo/config.toml`'s `-C link-arg=-Tlink.x` and links an unrelated 8,896-byte
  image (measured the hard way).
- `docs/reference/daisy-seed3.md`: the objcopy passage no longer calls the flags load-bearing and no
  longer points at TASK-059 "until it lands"; it says a plain `-O binary` is now correct, quotes the
  twelve post-fix sizes, and keeps the 469 MB story as history with the reason it looked intermittent.
  The cache-line table needed no change: `_SEGGER_RTT` `0x24000008` and `defmt_rtt::BUFFER`
  `0x240010e4` reproduce exactly (fresh `nm`, RTT-only `main`), because `.uninit` rose to fill the hole
  the move left; the sentence now records that re-run and warns that `.bss` objects do move (down 1 KiB).

## AC #6 - gates

`scripts/check-doc-artifact-names.sh` had no defmt/`--only-section` assertion to remove - R1/R2 only
ever checked doc tokens and hand-copied recipes. Added R3: the expanded `build:` recipe must contain
no `--only-section` flag at all, read from `make -n -C firmware build` rather than from file text.

Red run recorded: re-adding `--only-section=.text` to the recipe makes the gate exit 1 with
"the build: recipe passes --only-section... if you think one is needed again, an ALLOC section is
loading outside flash, which is the thing to fix", and the Makefile was restored from the backup
before committing. Green run exits 0.

`bash scripts/gates.sh ci` in `nix develop .#default`: **17 gates, 148.0 s, exit 0**. The AC's
"twelve checks" is stale - the tier has grown since the plan was written (`gates.sh --list` prints
counts: commit 9, push 16, ci 17).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Placed .sram1_bss as a NOLOAD output section in firmware/memory.x with INSERT AFTER .bss, so its LMA is outside flash; deleted the six --only-section flags from make build and added gate R3, which fails if any keep-list comes back. Plain -O binary now equals make build for all twelve measurements: console main 88,581 and rig 106,811 against 469,763,480 before, RTT main 48,360 and rig 65,696 against 469,763,536, four non-audio bins byte-identical in both cfg sets. The link map shows lld placed the section after .data and gave it LMA == VMA under its orphan rules; daisy-embassy's own memory.x never enters the link at all. gates.sh ci: 17 gates, 148.0 s, exit 0. HUMAN bench verification stays open as TASK-059.03.
<!-- SECTION:FINAL_SUMMARY:END -->
