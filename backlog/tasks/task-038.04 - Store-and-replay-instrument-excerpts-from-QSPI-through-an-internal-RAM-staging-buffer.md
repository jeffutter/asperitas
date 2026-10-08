---
id: TASK-038.04
title: >-
  Store and replay instrument excerpts from QSPI through an internal-RAM staging
  buffer
status: Blocked
assignee:
  - '@agent'
created_date: '2026-09-09 11:40'
updated_date: '2026-10-08 15:54'
labels:
  - planned
dependencies:
  - TASK-038.04.06
documentation:
  - docs/reference/daisy-seed3.md
modified_files:
  - crates/asperitas-logging/examples/excerpt_stream.rs
  - firmware/src/bin/rig.rs
  - firmware/Cargo.toml
parent_task_id: TASK-038
priority: high
type: task
ordinal: 58500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Playing real material should not require a host analog path. The corpus in `audio/instruments/` is eight mono 48 kHz 16-bit WAVs of two to three seconds each (190–288 kB), which is exactly the shape an excerpt store wants: 96,000 bytes per second of stimulus, one named clip at a time, no firmware image displaced.

Two facts make this harder than "read flash in the callback", and both are properties of the Rust stack rather than of the hardware.

**There is no memory-mapped read.** `daisy_embassy::flash::Flash` drives the IS25LP064A indirectly only — `read`, `write`, `erase`, and async variants, all taking an explicit `address: u32`. The HAL underneath can `enable_memory_map`, but `Flash.qspi` is a private field and nothing exposes it. So replay cannot fetch samples on demand from the audio callback; it must stage them into internal RAM ahead of time. A ping-pong pair filled by `read_async` over MDMA gives the callback a buffer that is always already resident, which is also what keeps QSPI traffic off the audio deadline.

**Erase dominates write time and the driver's blocking path spins forever on a stuck chip.** Erase granularity here is 4 KiB sectors only — the driver implements no block erase and no chip erase — with a 600 ms timeout per sector against a 300 ms datasheet maximum, and `write()` erases the sector containing its address before programming, so unaligned writes silently destroy their neighbours. A 190 kB excerpt spans about 47 sectors, i.e. tens of seconds of waiting. The blocking `wait_for_write` is an unbounded busy loop, and the async variants panic on timeout. In a binary whose whole purpose is trustworthy timing, only the async API may be used, and the erase pauses must be shown not to disturb the capture guarantees TASK-038.03 establishes.

Getting bytes *onto* the chip needs no new dependency and no Daisy bootloader: a host example emits the install stream as ordinary framed records on stdout, the operator redirects it to the CDC device node, and USB full-speed bulk provides transaction-level flow control by NAKing when the endpoint buffer is full, so a writer that fills up simply blocks. Correctness is decided afterwards by reading the chip back and comparing a CRC, not by hoping the pipe drained cleanly.

Bit-exactness has to be scoped honestly. What this ticket can prove is that the bytes the device wrote to the DAC path equal the bytes in the host's file. What comes back through the codec and the patch cable is TASK-035's problem, and it already refuses a bit-exactness claim there; keep the same posture rather than implying more.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Excerpt storage layout is expressed as constants owned by one module and mirrored in `docs/reference/daisy-seed3.md`: area starts at offset 0x100000, stride 512 KiB, fourteen slots, top 4 KiB sector reserved because the driver bounds check uses `MAX_ADDRESS = 0x7FFFFF` rather than 0x800000, and DaisyBootloader's region below 0x40000 stays untouched so installing it later does not require moving excerpts.
- [ ] #2 Install arrives as framed console records on the existing CDC OUT endpoint — `EXCSTART name=<tag> bytes=<n> crc16=<c>`, then `EXCDATA i=<j> b64=<payload>`, then `EXCEND` — decoded with the existing `frame::Decoder`; the device buffers exactly one 4 KiB sector at a time and calls only `Flash::write_async` at sector-aligned addresses, so the driver's implicit sector erase can never reach into a neighbouring slot.
- [ ] #3 After install the device reads the slot back and compares length and CRC-16 against `EXCSTART`, reporting `EXCOK` or `EXCFAIL got=<crc> want=<crc>`; a truncated or corrupted upload cannot produce a success record.
- [ ] #4 Replay draws samples only from an internal-RAM ping-pong pair refilled by `read_async` over an MDMA channel with interrupt bindings for QUADSPI and MDMA; the audio callback performs no QSPI access, and the staging depth is justified numerically against refill latency in a comment a reviewer can check rather than chosen by feel.
- [ ] #5 Bit-exactness is claimed only where it can be observed: across one full pass through an excerpt the device reports the CRC-16 of the samples it handed to the DAC encoder, that value equals the CRC the host computed from the source file, and both code and documentation state that this says nothing about what returns through the codec and patch cable, matching the posture TASK-035 AC #4 already takes.
- [ ] #6 Storage and timing costs are documented with predicted and observed columns: 96,000 bytes per second for mono 16-bit, install wall-clock including per-sector erases, and replay margin, alongside the rule that the blocking flash API may not be used anywhere in this binary because its status wait is an unbounded busy loop.
- [ ] #7 Host tests cover slot address arithmetic, slot header validation, the install state machine, and WAV header rejection from synthetic inputs with no board attached, and `examples/excerpt_stream.rs` output is checked against pinned golden frames so the wire format cannot drift.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against 6a99698

Approach: keep every byte-level decision host-testable in asperitas-logging, and keep rig.rs a thin consumer. The device has no inbound console path today (EP OUT is allocated in usb.rs but never read), so the install path is built as a narrow reader that feeds the existing frame::Decoder into a bounded channel; TASK-032 can reuse it later.

Sub-tickets and order:
1. .01 excerpt module (layout constants, slot header, WAV parser, grammar) - no deps.
2. .02 inbound CDC OUT reader - no deps, parallel with .01.
3. .04 install state machine + excerpt_stream example with goldens - after .01.
4. .05 rig flash install (async-only QSPI over MDMA, sector-aligned write_async, readback EXCOK/EXCFAIL) - after .02 and .04.
5. .03 replay via RAM ping-pong, DAC-path CRC - after .05.
6. .06 docs with predicted/observed columns, TASK-019.03 XIP correction - last.

Integration: host tests for .01/.04 prove the wire format and verdict logic; .02/.05/.03 are proven by firmware builds, CI and host-testable accounting only. Bench verification (install a real WAV, observed timings, replay CRC match) is TASK-038.09 (@human) and stays out of scope here; the observed columns in docs remain pending until then.

Final testing: cargo test for asperitas-logging, firmware build of rig and the other log-usb binaries, CI green. Parent ACs map: #1 .01+.06, #2 .04+.05, #3 .04+.05, #4 .03, #5 .03, #6 .06, #7 .01+.04+.03 host tests.

Remaining work not in a sub-ticket: none. Risks: QSPI kernel clock unstated (estimate only); async flash timeouts panic; Device-memory SDRAM forbids unaligned access; callback headroom ~590/666 us.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Driver facts verified in the vendored daisy-embassy checkout (commit ca9bcc9, `src/flash.rs`)

- API: `FlashBuilder { pins, qspi }`, `build(self) -> Flash<'a, Blocking>`, `build_async<D: QuadDma<QUADSPI>, I>(self, dma_ch, irq) -> Flash<'a, Async>`; `read(&mut self, address: u32, buffer: &mut [u8])`, `write(&mut self, address: u32, data: &[u8])`, `erase(&mut self, address: u32, length: u32)`, plus `read_async` / `write_async` / `erase_async` on the Async impl, and `read_uuid()`.
- Geometry: page 256 B, **sector 4,096 B and nothing larger** — the driver implements no 32 K/64 K block erase and no chip erase. `MAX_ADDRESS = 0x7FFFFF`, and the bounds checks are `assert!(address + buffer.len() <= MAX_ADDRESS)`, so the usable span stops one byte short of 8 MiB. Leave the top sector alone rather than discovering that assert on hardware.
- Timeouts: `SECTOR_ERASE_TIMEOUT = 600 ms` (datasheet max 300 ms), `PAGE_WRITE_TIMEOUT = 1.6 ms` (datasheet max 0.8 ms). The **blocking** `wait_for_write` is an unbounded `loop { if status & WIP == 0 { break } }` with no timeout at all; the async waits use hardware status-match with `.with_timeout(..).expect("Flash Timed out ...")`, i.e. a stuck chip becomes a panic, not an error value.
- `write()` calls `erase()` for its own range first, and `erase()` issues a sector erase at the raw address, so writing 100 bytes anywhere inside a sector destroys the rest of it. Every write in this ticket must be exactly one sector, sector-aligned.
- `QuadDma<QUADSPI>` on `stm32h750ib` is implemented only by **MDMA channels** (`MDMA_CH0..CH15`, periph request 22); GPDMA cannot serve QSPI. Binding needs both `QUADSPI => qspi::InterruptHandler<QUADSPI>` and `MDMA => dma::InterruptHandler<MDMA_CHn>`, as in `examples/flash.rs:11-16`.
- Latency trap: `read_dma` / `write_dma` set `CR.DMAEN`, and embassy-stm32 guards the clearing code with `#[cfg(not(stm32h7))]`, so on H7 a blocking call issued after an async one leaves `DMAEN` set. Use one mode for the whole session — async only here.
- `Config` is `memory_size = _8MiB`, `address_size = _24bit`, `prescaler = 1`, `fifo_threshold = _1Bytes`. Blocking transfers move **one byte at a time** through DR, so the MMIO loop, not the flash, would be the throughput limit — another reason the async path is the only acceptable one.
- Kernel clock is unstated in `default_rcc()` (`quadspisel` is never assigned, and embassy's `ClockMux::default()` is zeroed), leaving the reset selection hclk3 = 120 MHz, hence roughly 60 MHz QSPI CLK with `prescaler = 1`. Treat as an estimate and measure.
- No resource conflict with audio: SAI takes `DMA1_CH0`/`DMA1_CH1` and the `SAI1`/`DMA1_STREAM0/1` interrupts; QSPI takes `QUADSPI` and `MDMA`. Shared resources are the AXI fabric and CPU cycles only.
- Running XIP from QSPI would make firmware-side QSPI writes impossible (documented on the Daisy forum). Our app links at 0x08000000 in internal flash — blinky is about 18 kB and main around 65 kB, so there is room — which is precisely why the whole 8 MB can be treated as excerpt store. Do not let anyone move the app into QSPI without shrinking this budget in the same change.

## Layout decision, with the reason

Slots at `0x100000 + n * 0x80000` for n in 0..13, top 4 KiB sector reserved. Starting at 1 MiB keeps everything above the region DaisyBootloader would occupy (its image maps firmware from `0x90040000`, i.e. offset `0x40000`), so installing the bootloader later does not require moving excerpts. Fourteen slots of 512 KiB give 7 MiB of excerpt space: fourteen corpus clips, one per slot, each 186–281 kB with about half a slot spare — far more than TASK-019.03 AC #4 asks for.

Note that TASK-019.03's implementation notes claim "the 8 MB QSPI flash also carries the firmware image via XIP", which is true of libDaisy/C++ builds and **not** of this Rust stack: we link to internal flash and have no execute-in-place path at all. Its conclusion (one required excerpt, extra coverage as budget permits) stays sensible, but the reason should be corrected when this work is reported back, or the next person will size the budget against a phantom image.

## Install transport, and why the redirect trick is legitimate

Full-speed CDC-ACM has no baud rate to overflow, but it does have flow control: when the device's endpoint OUT buffer (256 B today, `usb.rs:150-215`) is full the device NAKs, the host stacks retries, and once the OS tty buffers fill the writer blocks. A plain `cat stream.bin > /dev/cu.usbmodem…` therefore self-paces instead of dropping bytes, provided the device drains EP OUT promptly — which means the inbound reader task must not be starved by other work on its executor. Correctness still rests on the final readback CRC, since nothing here guarantees delivery; what USB flow control buys is that a stalled writer cannot silently truncate the excerpt.

Do not add a serial-port dependency to the workspace for this. Framing belongs to `crates/asperitas-logging` (that crate owns `frame.rs`, the CRC, and the decoder), so the emitter is `examples/excerpt_stream.rs` there, writing bytes to stdout; TASK-031's runner is the ticket that owns actually holding the device open.

## Corpus facts

`audio/instruments/` holds eight **mono 48 kHz 16-bit PCM** WAVs, 186–281 kB each (~2–3 s): `mandolin_{single_note_soft,single_note_hard,fast_run,chord}` and the same four under `octave_`. Levels were scaled once globally with the loudest peak at −3 dBFS and must not be normalized individually — the soft/hard contrast is the signal. Goldens in `audio/goldens/` are always stereo 16-bit 48 kHz, regenerated with `UPDATE_GOLDENS=1 cargo test -p asperitas-cli`, tolerance 1e-4. Storing the clip's PCM verbatim means the excerpt the device replays is bit-for-bit the file already in the repo, which is what makes the CRC comparison meaningful.

`audio/tools/prepare_corpus.py` is the precedent for asset tooling (python3 + numpy + stdlib `wave`, argparse, every transform justified in the module docstring, explicitly not part of the build). The installer is different in kind — it emits wire records, so it lives with the codec in Rust rather than beside the Python asset script.

2026-10-08: unblocked. TASK-038.03 and TASK-038.03.02 are Done. Its bench check moved out to TASK-038.09 (@human, depends on this), so this ticket stays @agent. Queued for planning. Findings for the planner:
- **QSPI driver exists:** daisy-embassy ca9bcc9 src/flash.rs has read/write/erase and async read_async/write_async/erase_async, so storage is wiring, not driver work. Check its MDMA use against AC #4 before assuming.
- **No inbound console path exists:** the CDC OUT endpoint is allocated (usb.rs ep_out_buffer) but never read on device; frame::Decoder runs only in host tests. Install over CDC OUT is the first host-to-device path, overlapping TASK-032 (host control). Decide whether this ticket builds a general inbound path TASK-032 then reuses, or a narrow install-only one.
- **Likely leaves:** slot layout and host tooling (WAV parsing, excerpt_stream); the inbound console path; install with readback EXCOK/EXCFAIL; replay through an internal-RAM ping-pong with DAC-path CRC; docs (shared with TASK-038.06). rig needed two splits after two deadline failures, so size the leaves accordingly.
- **Bench facts to plan around (TASK-038.05):** f64 now runs on the FPU (eed633b); callback headroom is about 590 us of 666; the loop is straight at -0.41 dB; the SDRAM window is Device memory (no unaligned access). A replay path that reads QSPI inside the callback would fight the 666 us budget, which is AC #4's point.
<!-- SECTION:NOTES:END -->
