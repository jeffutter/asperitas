---
id: TASK-065
title: >-
  HUMAN: Tell daisy-embassy its memory.x is silently ignored when an app ships
  its own
status: To Do
assignee:
  - '@human'
created_date: '2026-09-13 06:59'
labels:
  - planned
dependencies: []
references:
  - 'https://github.com/ElectroSmith/daisy-embassy'
  - firmware/memory.x
  - 'https://github.com/rust-lang/rust/issues/73201'
  - 'https://users.rust-lang.org/t/elf-files-and-initialized-data/109678'
priority: low
type: task
ordinal: 108800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
While planning TASK-059 we established something about daisy-embassy that nobody appears to have
written down, and that costs every app which ships its own `memory.x` - which is most of them.

`daisy-embassy/build.rs:17-22` copies its `memory.x` into `OUT_DIR`, and its `memory.x:24-32` declares
`.sram1_bss (NOLOAD) : { *(.sram1_bss) *(.sram1_bss*) } > RAM_D2`. That rule never runs in a build
where the application also provides a `memory.x`: cortex-m-rt's `link.x.in` performs exactly **one**
`INCLUDE memory.x`, and the first `-L` path wins. In our build ours came first, so daisy-embassy's
entire MEMORY block was dead code, `.sram1_bss` became an orphan output section, rust-lld gave it
LMA == VMA in AXI SRAM, and a plain `cargo objcopy -O binary` of `main` produced 469,763,480 bytes
instead of 88,581. The symptom is a flash image of mostly zeros and no error anywhere.

Two things make this worth reporting rather than just working around locally:

- The failure is invisible. Nothing warns that a provided linker fragment was shadowed. The same
  symptom was asked publicly on users.rust-lang.org ("Elf files and initialized data", STM32H7
  AXISRAM buffers, "several 100MB") with no answer.
- The fix could be one character of naming. If `src/audio.rs:18-22` tagged those statics
  `#[unsafe(link_section = ".bss.sram1_bss")]` instead of `.sram1_bss`, rustc's own section naming
  plus cortex-m-rt's existing `.bss (NOLOAD)` rule would absorb them correctly in *any* app layout,
  with no local `SECTIONS` block required at all. We verified that renaming yields `SHT_NOBITS` and
  absorption by link.x's own rule.

We fixed it locally with an `INSERT AFTER .bss` rule in our own `memory.x` (TASK-059.01), so this
report is not unblocking us. It is offered because upstream gets to choose whether the fix belongs in
the section name or in the documentation, and because the next person to hit it will spend as long on
it as we did.

Posting is outward-facing, so this is `@human` by project convention (same rule as TASK-051). Venue:
an issue on ElectroSmith/daisy-embassy. Do not open a PR on our behalf without deciding the design
question first - the rename is theirs to make, and a PR that renames a public link-section name is a
breaking change they may reasonably decline in favour of a doc note.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 HUMAN: An issue is filed against ElectroSmith/daisy-embassy carrying the reproduction (plain `-O binary` byte count plus the `rust-objdump -h` line showing `.sram1_bss` as DATA with LMA == VMA), the single-`INCLUDE memory.x` cause, and the two candidate fixes framed as their decision.
- [ ] #2 HUMAN: The issue URL is recorded in this ticket's notes and added as a reference on TASK-059, so the local workaround and the upstream discussion point at each other.
- [ ] #3 HUMAN: If the maintainers dispute the diagnosis, what they said is recorded here verbatim rather than argued over several rounds; our build is already fixed and gated, so nothing here needs to be won.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Draft the issue around reproduction, not theory. Suggested body, to be trimmed and re-verified
against current `master` before posting - re-run the two commands below on a fresh checkout so the
figures are yours and current:

    # in an app whose memory.x defines only FLASH and RAM
    cd firmware && cargo build --release --features seed3
    cargo objcopy --release --features seed3 --bin main -- -O binary /tmp/out.bin
    ls -l /tmp/out.bin                     # expect ~n x 10^8 bytes, not ~10^5
    rust-objdump -h target/thumbv7em-none-eabihf/release/main | grep sram1_bss
    # expect: .sram1_bss ... DATA, with LMA equal to VMA inside the RAM region

Body outline:

1. Symptom, with those two numbers and the section-header line.
2. Cause: `link.x.in` does a single `INCLUDE memory.x`, so when an app ships its own `memory.x` the
   copy in daisy-embassy's `OUT_DIR` is not included at all and the `.sram1_bss (NOLOAD) > RAM_D2`
   rule never applies. lld then places the section as an orphan, and an orphan whose preceding section
   uses `AT>FLASH` gets LMA == VMA. Cite MaskRay's "Output section LMA" note and
   rust-lang/rust#73201 (160 MB vs 74 KB) as the same class.
3. Why `MaybeUninit` does not save it: rustc emits custom-named sections as `SHT_PROGBITS`; only a
   name starting `.bss.` becomes `NOBITS`. That is why link.x's own `.bss`/`.uninit` do not blow up -
   those *output* sections are declared `(NOLOAD)`.
4. Two candidate fixes, framed as their decision, not ours: rename the section attribute to
   `.bss.sram1_bss` so any host layout absorbs it (verified locally to produce `NOBITS` and correct
   placement), or keep the name and state plainly in the README that apps supplying their own
   `memory.x` must also supply the `.sram1_bss` output-section rule, since theirs will be ignored.
5. Mention the second-order trap for anyone who does adopt their `memory.x` wholesale: it does
   `REGION_ALIAS(RAM, DTCMRAM)`, and DMA1/DMA2 cannot reach DTCM, so audio dies silently.

Then: post it, put the URL in this ticket's notes, and add the URL as a reference on TASK-059 so the
local workaround and the upstream discussion point at each other. If the maintainers disagree with the
diagnosis, record what they said here rather than arguing it out over several rounds - our build is
already fixed and gated.
<!-- SECTION:PLAN:END -->
