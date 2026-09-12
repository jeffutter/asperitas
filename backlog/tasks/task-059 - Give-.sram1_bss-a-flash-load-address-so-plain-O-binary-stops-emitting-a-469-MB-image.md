---
id: TASK-059
title: >-
  Give .sram1_bss a flash load address so plain -O binary stops emitting a 469
  MB image
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-09-12 21:26'
updated_date: '2026-09-12 21:39'
labels: []
dependencies: []
references:
  - 'firmware/Makefile:101-109'
  - firmware/memory.x
  - 'https://github.com/ARMmbed/mbed-os/pull/14572'
priority: low
type: task
ordinal: 91800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Filed while planning TASK-057, which found that the doc's manual `cargo objcopy` line was not merely misnamed but
missing the six `--only-section` flags that `firmware/Makefile:104-109` passes. Those flags are a workaround, not a
fix: they exist because one loadable section in our images has its load address in AXI SRAM, so llvm-objcopy's raw
memory image spans from flash to RAM and pads the gap with zeros. Fix the layout and the whole crutch - and the doc
footgun it spawned - disappears.

Measured 2026-09-12 at `a1376f3` (llvm 21.1.8 via cargo-binutils 0.4.0), against already-built release ELFs, writing
output outside `target/`:

| binary | `cargo objcopy --release --features seed3 --bin X -- -O binary` | with the six keep-list flags |
|---|---|---|
| `main`   | 469,763,480 B | 88,581 B |
| `rig`    | 469,763,480 B | - |
| `blinky` | 65,638 B (**byte-identical to `make build`'s output**) | 65,638 B |

469,763,480 = `0x24000598 - 0x08000000`, i.e. lowest to highest **load** address. The section headers say exactly who
is responsible:

    Idx Name        Size     VMA       LMA       Type
      4 .data       00000198 24000000  08015808  DATA    <- harmless: LMA is in flash
      5 .sram1_bss  00000400 24000198  24000198  DATA    <- the offender: LMA == VMA, in AXI SRAM, file-backed
    107 .bss        00002020 24000598  24000598  BSS     <- NOBITS, contributes nothing

`.sram1_bss` comes from `daisy-embassy@ca9bcc9 src/audio.rs:20-22`, two `GroundedArrayCell::uninit()` statics tagged
`#[unsafe(link_section = ".sram1_bss")]` - nominally uninitialised, yet linked as an allocated, file-backed `DATA`
section, which is why `-R .bss -R .uninit` does not help (measured: still 469 MB). It exists only in binaries that
link the audio module, which is why `blinky` is unaffected and why the failure looks intermittent: a recipe verified
against blinky breaks the moment someone points it at `main`.

First task is to establish which linker input actually places the section, because the obvious candidate is
contradicted by the measurement above. `daisy-embassy` ships `memory.x` with `.sram1_bss (NOLOAD) : { *(.sram1_bss*) }
> RAM_D2` and copies it to its own OUT_DIR (`build.rs:17-22`); our `firmware/memory.x` defines only `FLASH` and `RAM`
and no SECTIONS block. The linked image puts the section in the AXI region as allocated `DATA`, so daisy-embassy's
NOLOAD rule is evidently not what placed it. Confirm with a link map (`-C link-arg=-Wl,-Map=out.map`) before designing
anything, and expect the answer to decide whether the fix is local or upstream: the `link_section` attribute is theirs,
the memory regions are ours.

Fix shape, from prior art where every case has the same root cause (a loadable section whose LMA sits in RAM): give
the section a flash load address, `> RAM AT> FLASH`, so the image stops spanning the gap. ARMmbed/mbed-os #14572
(LPC1549) resolves it exactly that way; see also StackOverflow 34666848 (22 KB -> 384 MB), 22812507 (114 KB -> 259 MB),
74074913 (STM32, ~400 MB of 0xFF), ST community F765 (15 KB hex -> 393 MB bin) and H750 custom bootloader (1.7 GB bin).
rust-lang/rust #73201 ("160 MB instead of 74 KB") is the cautionary tale for what a silent link-layout change does when
it lands behind an objcopy step. `cortex-m-rt`'s docs on extra/uninit sections explain why a `MaybeUninit` buffer still
arrives as file-backed content.

Constraints the planner must respect: this moves the link layout of every binary, and the buffers in question are the
SAI DMA buffers, so "it compiled" is worth nothing here - acceptance needs flashing and booting real images with audio
running. Per CLAUDE.md, split every board-touching criterion into its own `@human` sub-task before this goes anywhere
near Dev Ready. Also decide explicitly whether deleting the six `--only-section` flags is part of this ticket or a
follow-up once the layout is fixed.

Out of scope: the docs (TASK-057) and the docs gate (TASK-058).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A link map (`-C link-arg=-Wl,-Map`) establishes which linker input actually places `.sram1_bss`, and the answer plus the command that produced it go in the ticket notes - the measurement in the description says daisy-embassy's own NOLOAD rule is not what placed it, and that has to be confirmed rather than assumed.
- [ ] #2 Plain `cargo objcopy --release --features seed3 --bin main -- -O binary /tmp/out.bin`, with no `--only-section` flags, yields an image within a few percent of what `make build BINARY=main` produces instead of 469,763,480 bytes, and the same holds for `rig`.
- [ ] #3 All six bin targets still link, and the resulting flash image sizes are recorded before and after with any real difference explained.
- [ ] #4 HUMAN: before this ticket is Dev Ready, every criterion needing the board is split into its own @human sub-task - audio in and out on real hardware for `main` and `rig`, since the buffers being moved are the SAI DMA buffers.
<!-- AC:END -->
