---
id: TASK-038.06
title: Document the measurement rig workflow and the SDRAM and QSPI budgets
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 11:44'
updated_date: '2026-09-10 03:52'
labels: []
dependencies:
  - TASK-038.03
  - TASK-038.04
  - TASK-030.03
modified_files:
  - README.md
  - docs/reference/daisy-seed3.md
parent_task_id: TASK-038
priority: medium
type: docs
ordinal: 60500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A measurement rig nobody can operate from a page is a rig that gets used once, by the person who wrote it. TASK-030.03 set the precedent for documenting the framed transport; this is the same kind of ticket for the audio side, and it has to land **before** the bench session in TASK-038.05 so that session can be run from the documentation rather than from memory.

Two documents, two different jobs.

The README gains the operational narrative: which stimulus mode to build for which question, how to flash `rig`, how to capture the console to a file while the device is dumping, how to install an excerpt, how to reassemble a dump into samples, and where the resulting artifacts belong. Natural position is a new section between "Debugging Without a Probe" and "Important Hardware Gotchas", matching the existing structure.

`docs/reference/daisy-seed3.md` gains the budgets, which is what the parent's criterion #7 asks for by name: total SDRAM, capture ring size, bytes per second by sample format, capturable seconds, unused headroom; and for QSPI, the excerpt area start, slot stride, slot count, reserved regions, and storage cost per second. Tables, with predicted figures labelled as predictions until TASK-038.05 replaces them with readings.

Both documents have to keep saying which numbers are measured and which are arithmetic. Base64 efficiency and wire cost per second are derivations; erase-dominated install time and full-speed CDC throughput are estimates until someone times them. Blurring that distinction is how a project ends up trusting a phantom measurement.

While writing this down, stale statements surface. Fix them in the same change rather than leaving a trap for the next reader — most notably the claim in TASK-019.03's notes that the firmware image lives in QSPI via execute-in-place, which describes libDaisy C++ builds and not this Rust stack.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A README section placed between "Debugging Without a Probe" and "Important Hardware Gotchas" walks the rig workflow end to end — choosing a stimulus mode, flashing `rig`, capturing the console during a dump, installing an excerpt, reassembling a dump into samples, and where artifacts get committed — so the bench session in TASK-038.05 can be run from the page alone.
- [ ] #2 `docs/reference/daisy-seed3.md` carries the SDRAM budget (total, capture ring size, bytes per second by sample format, capturable seconds, unused headroom) and the QSPI budget (excerpt area start, slot stride, slot count, reserved regions including DaisyBootloader's, bytes per second by format) as tables, with every predicted figure explicitly labelled as a prediction until TASK-038.05 supplies observations.
- [ ] #3 The record grammars this epic adds — `RIGCFG`, `CAPSTAT`, `CAPMAX`, `AUDIO`, `AUDEND`, `EXCSTART`, `EXCDATA`, `EXCEND`, `EXCOK`, `EXCFAIL` — are documented in one place with one worked example line each, beside what TASK-030.03 documents for `BOOT` and `STATUS`, each pointing at the host test that pins it.
- [ ] #4 Every claim is attributed to its source: derived arithmetic versus measured reading versus estimate, covering base64 useful-byte efficiency, wire cost per second of capture, erase-dominated install time, and the still-unmeasured full-speed USB CDC ceiling, so a prediction cannot later be cited as a measurement.
- [ ] #5 Stale statements found while writing are corrected in the same change, specifically: TASK-019.03's note that the firmware image occupies QSPI via XIP (this stack links to internal flash and exposes no execute-in-place path); the parent ticket's "48 kHz mono float" footprint wording, replaced by the 16-bit decision together with the dump-bandwidth reason that produced it; `docs/reference/daisy-seed3.md:480-483`'s claim that "nothing here sleeps - the embassy executor busy-loops", which is false for embassy-executor 0.10 as pinned (`platform/cortex_m.rs:104-108` runs `asm!("wfe")` whenever `poll()` finds nothing, with no feature to opt out); and `:504-506`'s "nothing here calls it" about `SdRamBuilder::build`, which stops being true the moment `rig.rs` ships.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
TASK-038.03 also adds a dump-summary verb emitted when a transfer finishes (blocks, chunks, bytes, elapsed milliseconds, loss counters at that instant), because the rig decides capture boundaries itself and the host learns of completion only from the captured stream. It belongs in criterion #3's grammar list alongside RIGCFG, CAPSTAT, CAPMAX, AUDIO, AUDEND and the EXC verbs.

### You own the SDRAM memory-model note that TASK-038.03.02 was told to write (added 2026-09-12)

Parent TASK-038.03.02's criterion #10 asks for "a short factual note in `docs/reference/daisy-seed3.md` section 4". **There is no such section.** The file's sections are What is and isn't different (L9), The codec is strapped (L28), SAI configuration (L47), Flashing the Seed3 (L69); the string `FMC` appears nowhere in `docs/`, and the cache/MPU prose sits inside the ST-Link section at L485-508. That leaf now carries only the rule in a code comment; the document note is yours, because you already own this file, your criterion #5 already tells you to fix stale statements as you find them, and two owners editing one file is how half a correction lands twice.

Suggested position: a new `## SDRAM memory model` sibling after "SAI configuration", matching that file's style - dense prose, bolded lead sentence, every claim sourced to a measured address or file:line, explicit "latent, not present" scoping.

Verified facts to state, all checked locally on 2026-09-12 against daisy-embassy `ca9bcc9`:

- `SdRamBuilder::sdram_a13bits_d32bits_4banks_bank1()` resolves through `SdRamTargetBank::Bank1` to `FmcBank::Bank5`, so `init()` returns base `0xC000_0000`, while the driver programs its cacheable MPU region over `0xD000_0000` (Bank6). The mismatch is real and harmless today only because `MPU_DEFAULT_MMAP_FOR_PRIVILEGED` is set, so unlisted addresses fall into the default map. AN4891's `0xD000_0000` wording is itself part of why people get this wrong.
- Caches are enabled nowhere in the stack (no cache or MPU call in embassy-stm32 0.6.0's `src/`, none in daisy-embassy `ca9bcc9`'s boot path, none in cortex-m-rt startup), so every FMC access is uncached and coherent by default rather than by argument.
- The rule that must be written down, not implied: caches stay off until someone owns the coherence argument for the FMC window, and that person revisits the capture hand-off ordering in the same change.
- Mark clearly which figures are arithmetic and which are readings. Ring geometry (`RING_BLOCK_BYTES` 32 768, `RING_BLOCKS` 1 024, `RING_BYTES` 33 554 432 = 32 MiB of the 64 MB device, 96 000 B/s raw, 349 s floor) is arithmetic from `asperitas_logging::capture`; wire cost per second is derived; anything about sustained FMC write throughput or USB FS bulk ceiling is an estimate until TASK-038.05 measures it.
<!-- SECTION:NOTES:END -->
