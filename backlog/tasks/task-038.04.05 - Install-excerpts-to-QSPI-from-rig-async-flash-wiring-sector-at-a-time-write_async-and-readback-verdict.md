---
id: TASK-038.04.05
title: >-
  Install excerpts to QSPI from rig: async flash wiring, sector-at-a-time
  write_async and readback verdict
status: Blocked
assignee:
  - '@agent'
created_date: '2026-10-08 15:29'
updated_date: '2026-10-08 15:54'
labels:
  - task
  - planned
dependencies:
  - TASK-038.04.04
  - TASK-038.04.02
parent_task_id: TASK-038.04
priority: high
ordinal: 135800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Scope: firmware/src/bin/rig.rs plus Cargo wiring. Build daisy_embassy Flash via build_async with an MDMA channel and bind QUADSPI and MDMA interrupts (see examples/flash.rs in the vendored daisy-embassy). Consume install records from the inbound path, write each sector with write_async only at sector-aligned slot addresses, then read the slot back in chunks, compute length and CRC-16, and emit EXCOK or EXCFAIL. The blocking flash API must not appear anywhere in the binary (unbounded busy loop); async timeouts panic, so state the consequence. Show that erase pauses (about 47 sectors, tens of seconds) do not disturb the audio interrupt executor or the SDRAM capture ring: a host-computable argument plus rate gates where possible. Acceptance: rig builds and CI covers it; no device claim (TASK-038.09 owns the bench). Covers parent AC #2 (device half) and AC #3.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 rig builds Flash<Async> via board.flash.build_async(MDMA_CH0, RigFlashIrqs) with QUADSPI and MDMA bound in a new bind_interrupts struct; the Flash lives in a static embassy_sync Mutex so TASK-038.04.03.02 can share it; no blocking read/write/erase call and no use of FlashBuilder::build anywhere in the binary (grep-enforced in CI)
- [ ] #2 An install task consumes records from usb::inbound() (TASK-038.04.02), drives excerpt::Installer (TASK-038.04.04) and calls write_async only with exactly 4096 bytes at sector-aligned addresses inside the target slot; an assert at the single call site checks alignment, length and slot bounds before the driver can erase anything
- [ ] #3 After EXCEND the task reads the PCM back in bounded chunks with read_async, computes length and CRC-16, calls Installer::verdict, writes the header sector only on success, and emits EXCOK or EXCFAIL through the one whole-record emit path; a truncated or corrupted upload cannot produce EXCOK (covered by the Installer host tests; device path reviewed against them)
- [ ] #4 Erase pauses do not disturb capture: the install task runs on the thread-mode executor only, the SAI1 callback never references the flash, and a written argument in a comment prices the MDMA and QUADSPI ISR preemption of the P6 audio executor against callback headroom (about 590 us of 666 us); install is refused while capture is armed or dumping, with a console record saying so
- [ ] #5 CI builds rig with the install path on thumbv7em and passes cargo clippy; no bench claim is made - TASK-038.09 owns the hardware check
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 6a99698

Depends on TASK-038.04.02 (inbound channel) and TASK-038.04.04 (Installer, verdict bodies); .01 transitively. Edits firmware/src/bin/rig.rs and, if needed, firmware/Cargo.toml (embassy-sync is already a logging dependency; check firmware's own).

Verified facts (daisy-embassy pinned ca9bcc9, src/flash.rs; Cargo.lock matches): board.flash is a FlashBuilder moved by new_daisy_board!, which leaves MDMA_CH0 and MDMA unmoved in p, but rig currently calls new_daisy_board!(p) and then no longer uses p - take p.MDMA_CH0 BEFORE or alongside the macro (macro takes fields by value from p; it does not touch MDMA_CH0). build_async(dma_ch, irqs) needs QUADSPI => qspi::InterruptHandler<QUADSPI> and MDMA => dma::InterruptHandler<MDMA_CH0>, as examples/flash.rs. write_async erases the sector range first then programs 256-byte pages with a 1.6 ms page timeout and 600 ms sector timeout, and PANICS on timeout; read_async asserts address+len <= 0x7FFFFF. So every call must be sector-aligned, 4096 bytes, inside the slot.

Steps:
1. bind_interrupts!(struct RigFlashIrqs {QUADSPI..., MDMA...}) beside RigUsbIrqs; comment why separate from USB (each binary owns its vectors; do not touch SAI1, see existing note).
2. In main after SDRAM bring-up and before audio start: build_async once; put into static FLASH: StaticCell/Mutex<NoopRawMutex, Flash<Async>> (thread-mode only users). Check NVIC priorities for MDMA and QUADSPI after embassy init (embassy default dma_interrupt_priority P0 would preempt the P6 audio executor; ISRs are short but state the cost and read the priority back in the existing nvic info! line).
3. install_task(inbound rx): loop { rec = rx.receive().await; parse_record; Installer step; match Action { InvalidateHeader => write 0xFF sector at slot base; WriteSector => single guarded fn write_sector(slot, index, &[u8;4096]) containing the asserts; Verify => readback loop of e.g. 1024-byte chunks into a stack buffer, running crc16_ccitt_update, then verdict; WriteHeader => write header sector; Reject => emit EXCFAIL } }. Emit via the existing whole-record path (emit_console / console emit used for RIGCFG) with Info level. Add progress throttle: do not emit a record per sector (47 sectors, console pipe has headroom rules in dump::dump_fits) - one EXCPROG every N sectors at most, or none; prefer none and rely on EXCOK.
4. Gate: refuse EXCSTART with a EXCFAIL why=busy while ARMED or FILLING or dumping, so flash traffic cannot overlap a capture; read the flags the existing timeline uses (ARMED, FILLING).
5. Add the task to the existing join/select in main (rig_fut) alongside report_capstat and run_capture, enabling usb::enable_inbound() (from .02) only here so other binaries are unchanged. Under the log-defmt-only build there is no console; compile the install task out with the same cfg shim as emit_console.
6. Docs in code: a header comment stating the blocking API ban and why (wait_for_write busy loop has no timeout; async timeouts panic, which the panic handler turns into a PANIC record), and that install wall-clock is dominated by ~47 sector erases (tens of seconds) with predicted vs observed left to TASK-038.04.06.
7. CI: confirm .github/workflows/ci.yml already builds rig with log-usb and with log-defmt; add a grep step that fails on '.build()' of FlashBuilder, '.read(' / '.write(' / '.erase(' on Flash in rig.rs (cheap, keeps the ban honest).

Verify: cargo build --release for the firmware workspace on thumbv7em-none-eabihf, both feature configurations; cargo clippy; all host tests. Cannot verify on hardware.

Risks: write_async panics on a stuck chip; 4096-byte stack buffers plus the Installer's own sector buffer on a thread-mode stack (check stack headroom; make them statics); MDMA ISR priority versus audio; the Flash mutex must not be held across an await by anyone but the owner - document the rule for the replay ticket.
<!-- SECTION:PLAN:END -->
