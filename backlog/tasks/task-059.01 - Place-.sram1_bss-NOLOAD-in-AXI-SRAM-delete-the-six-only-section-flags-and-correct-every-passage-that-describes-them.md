---
id: TASK-059.01
title: >-
  Place .sram1_bss NOLOAD in AXI SRAM, delete the six --only-section flags, and
  correct every passage that describes them
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-13 06:53'
updated_date: '2026-09-13 06:53'
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
- [ ] #1 A link map built on the unmodified tree establishes which linker input places `.sram1_bss`, and both the answer and the exact command that produced it are recorded in this ticket's notes. Use `-C link-arg=-Map=<file>`; the `-C link-arg=-Wl,-Map=out.map` form named in the original criterion fails outright (`rust-lld: error: unknown argument '-Wl,-Map=...'`). The notes must also say which `memory.x` cortex-m-rt actually included, which settles whether daisy-embassy's `(NOLOAD)` rule is dead code rather than merely overridden. Confirm rather than assume.
- [ ] #2 After the change, `rust-objdump -h` on rebuilt `main` and `rig` shows `.sram1_bss` as a `NOBITS`/`BSS` section rather than allocated `DATA`, and no ALLOC + PROGBITS section in any of the six images has an LMA outside flash. Record the addresses you measured.
- [ ] #3 Plain `cargo objcopy --release --features seed3 --bin main -- -O binary /tmp/out.bin`, with no `--only-section` flags, yields an image within a few percent of what `make build BINARY=main` produces instead of 469,763,480 bytes, and the same holds for `rig`. Expected 88,581 and 106,811 bytes - the sizes `make build` produces today.
- [ ] #4 All six bin targets still link under both cfg sets (`--features seed3` and `--no-default-features --features "seed3 log-defmt"`), and the notes carry a before/after byte table for all six in both configs with every non-zero difference explained. Total RAM footprint, `__ebss`, `__sheap` and `_stack_end` must be unchanged, and the first four little-endian bytes of every image must still read `0x24080000` - the initial stack pointer is the only host-visible witness that the board did not bus-fault before `main`.
- [ ] #5 The six `--only-section` flags are gone from `firmware/Makefile`'s `build:` recipe, and every passage that described them or quoted their numbers is corrected in the same commit: `firmware/Makefile:89-115`, `firmware/Cargo.toml:41-55` (table re-measured, prose updated), `docs/reference/daisy-seed3.md:110-127` plus the cache-line table row at `:621-627` recomputed from fresh `nm` output. No `*.bin` size claim anywhere in the repo still describes pre-fix behavior.
- [ ] #6 `scripts/check-doc-artifact-names.sh` no longer proves defmt survival by grepping for `--only-section='*.defmt*'` (that flag list no longer exists), its replacement assertion still fails when a keep-list comes back, and all twelve of its checks plus `bash scripts/gates.sh ci` pass in `nix develop .#default`.
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
