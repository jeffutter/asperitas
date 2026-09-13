---
id: TASK-059
title: >-
  Place .sram1_bss as a NOLOAD section so plain -O binary stops emitting a 469
  MB image
status: Blocked
assignee:
  - '@human'
created_date: '2026-09-12 21:26'
updated_date: '2026-09-13 08:08'
labels:
  - planned
dependencies:
  - TASK-059.01
  - TASK-059.02
  - TASK-059.03
references:
  - 'firmware/Makefile:101-109'
  - firmware/memory.x
  - 'https://github.com/ARMmbed/mbed-os/issues/14572'
  - firmware/Cargo.toml
  - scripts/gates.sh
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
- [ ] #1 Parent umbrella: TASK-059.01, TASK-059.02 and TASK-059.03 are all Done. The substantive criteria this ticket carried before planning live in those subtasks - the link-map confirmation, the size table and the deletion of the `--only-section` keep-list in .01, the regression gate in .02, and every board-touching criterion in .03.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## What this ticket is now

An umbrella. Planning established the mechanism, chose the fix, and found that the work splits into
three leaves with different owners - two agent-owned and one that only a person at the bench can
close. This plan says how they fit; each leaf carries its own step-by-step plan.

## The fix, in one paragraph

`.sram1_bss` is an orphan output section, so rust-lld placed it and gave it LMA == VMA in AXI SRAM,
which makes `llvm-objcopy -O binary` span flash-to-RAM and pad with zeros. daisy-embassy's rule for
it never applies because cortex-m-rt includes exactly one `memory.x` and ours wins. The fix therefore
goes in *our* `firmware/memory.x`, using the hook cortex-m-rt documents:

    SECTIONS { .sram1_bss (NOLOAD) : ALIGN(4) { *(.sram1_bss) *(.sram1_bss*) } > RAM } INSERT AFTER .bss;

`(NOLOAD)` is what turns it into `SHT_NOBITS`; `INSERT AFTER .bss` is the sanctioned injection point
and forbids changing its load region, which is why the `AT> FLASH` shape this ticket was originally
filed to copy is wrong here. Keep `> RAM`: our AXI region, which DMA1 reaches. daisy-embassy's
regions alias `RAM` to DTCMRAM, and audio would die silently there.

## Execution order

1. **TASK-059.01** (`@agent`) - confirm the placement with a link map, land the `memory.x` rule,
   measure all six bins in both cfg sets before and after, delete the six `--only-section` flags, and
   correct every sentence in `firmware/Makefile`, `firmware/Cargo.toml` and
   `docs/reference/daisy-seed3.md` that presents the 469 MB story as current behavior. One commit on
   purpose: layout, crutch and prose describe one fact, and shipping any subset of them leaves the
   repo either half-fixed or describing a hazard it no longer has.
2. **TASK-059.02** (`@agent`, depends on .01) - add `scripts/check-image-load-addresses.sh` to the
   push tier: no file-backed section may load outside flash. It must land second because a gate that
   fails on today's tree cannot land first, and it must exist before anyone is tempted to re-add a
   keep-list. Its red-run demonstration is what proves the class is actually closed.
3. **TASK-059.03** (`@human`, depends on .01 only) - flash and listen. Sizes stay identical across
   the change while roughly 200 bytes of `.text` content move, so no host-side check can settle this.
   It does not wait on .02, which changes no image bytes.

.01 is the only leaf with real risk and the only one worth reading closely before executing; .02 and
.03 are mechanical once it lands.

## How to tell the whole thing worked

All three leaves Done, plus these read straight off the tickets' notes rather than being asserted: a
link-map answer naming the linker input that placed the section; a before/after byte table for six
bins x two cfg sets with every delta explained; `.sram1_bss` reported as `NOBITS`; `_stack_end`
unchanged; a recorded red run of the new gate; and a bench report with counter values rather than
pass/fail claims.

## Deliberately not in scope

- **Upstream.** TASK-065 reports that daisy-embassy's `memory.x` is silently ignored whenever an app
  ships its own, and offers the one-character alternative fix (rename the attribute to
  `.bss.sram1_bss`). Filed separately and *not* as a child, so an outward-facing conversation cannot
  block a local fix that already works. Delete it if you would rather not file it.
- **The objective audio bench.** TASK-034 and TASK-035 would turn .03's listening into metrics. Until
  they land, ears and console counters are the best available evidence, and .03 says so instead of
  pretending otherwise.
- **elf-check's own weaknesses** (TASK-056, TASK-062). Both touch `scripts/gates.sh` and the ELF
  freshness question; coordination notes are on both tickets, and .02's plan is written to avoid
  colliding with either.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Root cause, as measured during planning (not evidence for any criterion)

Every figure below was taken by the planning session at `80c7276`, rustc 1.97.1, linker
`rust-lld`, llvm tools 21.1.8 via cargo-binutils 0.4.0. It is orientation for whoever executes, not
a substitute for measuring it yourself - TASK-059.01 AC #1 exists precisely because this paragraph
is someone else's run.

- `.sram1_bss` is an **orphan output section**, placed by rust-lld's built-in orphan rules directly
  after `.data`, fed from a `daisy_embassy-*.rlib` member. VMA == LMA == `0x24000198`.
- LMA equals VMA because of lld's documented rule (*Output section LMA*): with no `AT()` or
  `AT>region`, an orphan whose previous section is *not* in the default LMA region gets LMA = VMA.
  `.data` uses `AT>FLASH`, so the orphan lands in RAM. Ordering follows lld's rank-based orphan
  placement, which MaskRay advises never relying on.
- daisy-embassy's `.sram1_bss (NOLOAD) ... > RAM_D2` rule is **dead code here**: `link.x.in` does one
  `INCLUDE memory.x` and ours is first on the `-L` path (order observed on the link line:
  `asperitas-firmware/out`, cortex-m, embassy-stm32, cortex-m-rt, defmt, daisy-embassy,
  stm32-metapac). So their MEMORY block, including `REGION_ALIAS(RAM, DTCMRAM)`, never applies.
- File-backed despite `MaybeUninit::uninit()`: rustc emits custom-named sections as `SHT_PROGBITS`
  whatever the initialiser; only names starting `.bss.` become `NOBITS`. link.x's own `.bss` and
  `.uninit` avoid the blowup solely because those *output* sections are declared `(NOLOAD)`. Hence
  `-R .bss -R .uninit` cannot help, as the description found.

## Two commands in this ticket that do not work as written

- `-C link-arg=-Wl,-Map=out.map` fails outright: `rust-lld: error: unknown argument '-Wl,-Map=...'`.
  Use the lld-native form: `cargo rustc --release --features seed3 --bin main -- -C link-arg=-Map=/tmp/main.map`.
- `--orphan-handling=error` is accepted by rust-lld but names only `.debug_*`, `.comment` and
  `.ARM.attributes` - **not** `.sram1_bss`. It is not a usable guard; do not propose it.

Invariants that do work from a script: `rust-objdump -h` prints VMA and LMA columns (note that
`.defmt.*` section names contain spaces, so parse right-to-left), and comparing a plain `-O binary`
length against the highest flash LMA end catches drops. Both became TASK-059.02.

## Baselines for the before/after table

Plain `-O binary`, console cfg (`--features seed3`), at `80c7276`: main 469,763,480 / rig 469,763,480
/ blinky 65,638 / ledtest 17,774 / podtest 72,689 / panictest 65,958. Only `main` and `rig` link the
audio module, which is why the blowup looks intermittent. `make build` produces main 88,581 and rig
106,811; the RTT-only cfg (`--no-default-features --features "seed3 log-defmt"`) gives 48,360 and
65,696, matching `firmware/Cargo.toml`.

Fix shape chosen: `(NOLOAD)` + `INSERT AFTER .bss` in our `memory.x`, keeping `> RAM` (AXI). Measured
during planning: `.sram1_bss` becomes `NOBITS` at `0x240021b8`, plain images collapse to 88,581 /
106,811 - exactly what `make build` already writes - `_stack_end` unchanged at `0x240025b8`, and ~209
bytes of `.text` *content* differ at identical size because literal pools follow the moved statics.
That last point is why the HUMAN bench session cannot be skipped: byte-identical sizes are not
byte-identical images.

Rejected: `> RAM AT> FLASH` without `NOLOAD` (the ARMmbed/NXP prior art this ticket was filed to
follow) leaves 1,027 bytes of junk zeros in flash that nothing copies, and contradicts
`link.x.in:169-172`, which forbids changing the load region of sections injected after `.bss`. Also
corrected: ARMmbed/mbed-os #14572, cited in the references, is an **issue**, not a PR - NXP fixed it
in `LPC1549.ld`.

## Parked by an agent run that could not execute it (2026-09-13)

The ralph loop handed this umbrella to `/backlog-execute`. It is not executable by an agent, and
nothing here was changed except its status. Evidence read off the tree, not off ticket statuses:

- Assignee is `@human`, and CLAUDE.md forbids an agent picking up or closing such a ticket. The
  parent inherits `@human` because TASK-059.03 is a bench task.
- Its only AC (#1) requires all three leaves Done. Measured state: .01 Done at commit f746e54
  (`firmware/memory.x` now carries the `(NOLOAD)` + `INSERT AFTER .bss` rule; the six
  `--only-section` flags are gone from `firmware/Makefile`). .02 To Do - `scripts/` contains
  only `check-doc-artifact-names.sh` and `gates.sh`, so the load-address gate does not exist.
  .03 To Do and needs a board, ears and a line-level source.
- The parent has no direct work of its own beyond the umbrella criterion, so there was no partial
  agent contribution available to make on it.

AC #1 left unchecked deliberately: checking it would assert two unfinished children are Done.

**Next actionable step.** TASK-059.02 (`@agent`, deps satisfied since .01 landed) is the only
agent-owned work left in this tree; it appears in `./backlog/unblocked-todo.sh`. When .02 and .03
are both Done, `backlog task list -s Blocked --ready` releases this ticket and the flip to Done
belongs to a person.

**Why it reached Dev Ready at all.** Planning promoted the container to Dev Ready, and the pi ralph
extension's execute stage selects it with `findFirstByStatus(cwd, "Dev Ready")`
(~/.pi/agent/extensions/ralph/index.ts:2030-2033), which applies neither the assignee filter nor
the unfinished-child hold-back that `unblocked-todo.sh` implements. The Claude workflow in
`.claude/workflows/ralph-backlog-loop.js:219-227` does check readiness and forces such a ticket
back to Blocked; the pi extension has no equivalent. Filed as TASK-066.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-13 07:01
---
Planning 2026-09-13: the diagnosis landed opposite to the hypothesis this ticket was filed with, so
the title changed from "give `.sram1_bss` a flash load address" to placing it `NOLOAD`. The mbed/NXP
prior art named in the description is the wrong fix here: it works, but writes ~1 KB of zeros into
flash that no startup code copies, and `link.x.in:169-172` explicitly forbids changing the load region
of sections injected after `.bss`. Three children, all planned. `.01` fixes the layout AND deletes the
six `--only-section` flags AND corrects the ~18 sentences across `firmware/Makefile`,
`firmware/Cargo.toml` and `docs/reference/daisy-seed3.md` that present the 469 MB story as current
behavior - deliberately one commit, because splitting them would leave main green while documenting a
hazard it no longer has, and TASK-058's doc gate would then be enforcing prose that lies. `.02` adds
the invariant as a push-tier gate ("no file-backed section may load outside flash"), which is what
makes deleting the keep-list safe to keep deleted; it must land after `.01` because a gate that fails
cannot land first. `.03` is `@human`: only `main` and `rig` carry the buffers, sizes stay identical
while roughly 200 bytes of `.text` content move, and the objective loopback metrics that would make
this digital do not exist yet (TASK-034 and TASK-035 both still To Do), so the evidence is ears plus
the console counters. Parent inherits `@human` from `.03` and stays unclosable until someone listens.
Filed TASK-065 separately, deliberately NOT as a child, so reporting upstream cannot block the local
fix that already works.
---
<!-- COMMENTS:END -->
