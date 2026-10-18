# Daisy Seed3 — Hardware Reference

Facts established by research on 2026-08-01. Hardware facts here are stable; for the
fast-moving software-support picture see [rust-daisy-stack.md](./rust-daisy-stack.md).

Official datasheet:
<https://daisy.nyc3.cdn.digitaloceanspaces.com/products/seed3/Daisy_Seed3_datasheet.pdf>

## What is and isn't different from earlier Seeds

The Seed3 is *not* a new platform. It is the same MCU and memory in the same footprint,
with a new codec and a new USB connector.

| | Seed 1.x / Seed2 DFM | Seed3 |
|---|---|---|
| MCU | STM32H750IB, Cortex-M7F @ 480MHz | same |
| SDRAM | 64 MB | 64 MB (`AS4C16M32SB-6BCN`, 32-bit wide) |
| QSPI flash | 8 MB | 8 MB |
| Pinout | Seed pinout | **pin-for-pin compatible** |
| Audio codec | AK4556 / WM8731 / PCM3060 | **TI TAC5242** |
| Audio spec | up to 24-bit / 96 kHz | up to 32-bit / 192 kHz, −120 dB noise floor |
| USB | micro-B | **USB-C**, data + power |

Practical consequence: everything in the Rust stack that concerns clocks, SAI, SDRAM,
QSPI and GPIO carries over unchanged from the Seed 1.x support. The codec is the only
genuinely new hardware.

## The codec is strapped, not I²C-configured

**This is the single most important thing to know, and it is counterintuitive.**

Of the four codecs across the Seed family, two are configured over I²C (WM8731,
PCM3060) and two are hardware-strapped (AK4556, and now the TAC5242 on Seed3). The
TAC5242 *chip* is fully I²C-programmable — TI's datasheet documents a large register
map — but **the Seed3 board straps it into a fixed configuration**, so firmware never
touches I²C for audio.

Do not go looking for a register init sequence to port. There isn't one. Codec support
is SAI configuration and nothing else.

If a future project ever needs register-level control of a TAC51xx-family part, a
readable, well-commented C reference for the I²C path exists in
[`torvalds/GuitarPedal`](https://github.com/torvalds/GuitarPedal) at
`Software/tac5112.h` — it works through the reset, page select, and the datasheet
§9.2.5 EVM setup script. Not needed for Seed3.

## SAI configuration

Taken from the `seed3` implementation in daisy-embassy PR #80 (`src/codec/tac5242.rs`),
which was verified on real hardware. The codec is strapped as an I²S **target**; the
STM32 is master.

- Peripheral: `SAI1`, split into sub-blocks — **A = transmit (master), B = receive
  (synchronous slave)**
- Sample word type: `u32`
- Frame length: **64 bits** (64-bit stereo frame)
- Data size: **32-bit**, MSB-first, left-justified
- Frame sync: active high, offset on first bit, active-level length 32
- Clock strobe: **falling** on TX, **rising** on RX
- RX sync input: internal (synchronous to the TX sub-block)
- FIFO threshold: quarter
- MCLK divider: derived from the configured sample rate

**Startup delay:** the TAC5242 datasheet requires at least **2 ms** between stable
supplies / mode pins and the start of ASI clocks. The Seed3 powers the codec before
application startup so this is usually already satisfied, but the delay is retained in
firmware to make warm reinitialization deterministic.

## Flashing and logging

Two routes reach the same flash: DFU over the onboard USB-C, and an ST-Link probe on the SWD pads
(which also streams `defmt`/RTT and recovers a board that dies before USB enumerates). The procedure,
the probe wiring, the exit-code contract and the `LAYOUT=bootloaded` flow are in
[flashing.md](./flashing.md). The measurements behind it are in
[docs/investigations/](../investigations/). The framed console protocol is specified in rustdoc; start
at [protocol.md](./protocol.md).

## What each channel loses

The device paths keep different books, and a loss figure from one says nothing about the other.
The framed USB console reports what it failed to send: a per-record sequence number the reader
can difference for gaps, plus cumulative `dropped_full`, `bytes_dropped`, `trunc` and `ep_err`
counters carried in band — so a capture that lost records can prove it afterwards, and a capture
that didn't can too. Those fields are specified in the `asperitas_wire::frame` and `asperitas_wire::console` module docs
(see [protocol.md](./protocol.md)); this section is only about which
ledger is which. RTT keeps no ledger at all. Unattached it discards freely and
says nothing; attached it loses nothing *while the host keeps reading and stays attached*, and
silently stops being the thing under your control the moment the host stalls or goes away. So a clean, gapless RTT stream is
evidence that the host kept up, not evidence that the firmware's diagnostics were complete —
and a loss count quoted from the console does not describe an RTT capture, or vice versa. When a
number has to be trustworthy, take it from the console's counters and say which channel it came
from.

## Host control channel (firmware wiring)

The wire grammar is `asperitas_wire::control` and the firmware wiring is documented on
`firmware/src/host_control.rs`; see [protocol.md](./protocol.md). Hardware verification so far:
verified on the board for `podtest` on 2026-10-15 (`VER`/`RSN` query and `RST` restart with
re-enumeration and `reset_reason=software`, over the probe-flashed image). `main` and `SET`/`REL` on a
live knob path are not covered by that run.

## External SDRAM: address, MPU and caches (measured)

Read off the running board through the probe on 2026-10-08, after `rig` had brought the SDRAM up with
daisy-embassy `ca9bcc9`'s `SdRamBuilder::build` + `init`:

| Register | Value | Meaning |
|---|---|---|
| `SCB_CCR` | `0x00040200` | DC (bit 16) = 0, IC (bit 17) = 0: **both caches off** |
| `FMC_BCR1` | `0x800030db` | FMC enabled, BMAP = 00: SDRAM bank 1 at 0xC000_0000, bank 2 at 0xD000_0000 |
| `FMC_SDCR1` | `0x000019e9` | bank 1 configured: 9 col, 13 row, 32-bit, 4 internal banks, CAS 3, SDCLK = HCLK/2, read burst: 64 MiB |
| `FMC_SDCR2` / `SDTR2` | `0x000002d0` / `0x0fffffff` | reset values: **bank 2 is not configured** |
| `MPU_CTRL` | `0x00000005` | MPU on, PRIVDEFENA on |
| region 0 `RBAR` / `RASR` | `0xd0000000` / `0x03030033` | 64 MiB, full access, C=1 B=1 (write-back cacheable), enabled |
| regions 1-7 | `RASR = 0` | unused |

Probe pattern writes read back intact at 0xC000_0000, 0xC1FF_FFF0 and 0xC3FF_FFF0. Distinct values at
0xC000_0000 and 0xC200_0000 stayed distinct, so there is no aliasing below 64 MiB. A read at
0xD000_0000 fails with an AP bus error, so nothing answers there. On the device side, the 300 s
capture of 2026-10-07 (TASK-038.05) wrote 28.8 MB through this window and read it back with every
block CRC matching.

What that settles:

- **The memory is at 0xC000_0000.** `Sdram::init` returns the right address. The MPU region at
  0xD000_0000 covers an unconfigured FMC bank and is inert.
- **0xC000_0000 is Device memory.** No MPU region covers it, so with PRIVDEFENA the ARMv7-M default
  map applies, and 0xA000_0000-0xDFFF_FFFF is "external device": Device type, execute-never,
  uncacheable. The capture ring is therefore uncached **whatever `CCR.DC` says**. Enabling the D-cache
  does not change the ring's coherence; adding or moving an MPU region onto 0xC000_0000 does. Whoever
  does that owns the coherence argument for the FMC window and must revisit `rig`'s capture hand-off
  ordering in the same change.
- **Device memory does not take unaligned accesses.** ARMv7-M supports unaligned loads and stores
  only to Normal memory, so a misaligned access here is architecturally unsupported. Expect a fault,
  not merely a slow access; this has not been tried on this board. Today's ring code copies whole
  aligned 64-byte callbacks and passed 879 blocks. New code that reaches SDRAM through a misaligned
  pointer or `#[repr(packed)]` data is the risk.
- daisy-embassy's region base looks like a mistaken `0xD000_0000`, perhaps from AN4891's wording for
  the bank-2 window. It lives in a third-party crate, so it is recorded here rather than patched.

### Capture budgets (SDRAM)

Basis labels: measured = a reading from a board, with its date; estimated = a guess with no
measurement behind it. The derived budgets (ring size, headroom, capturable seconds, wire cost per
block and per second, base64 efficiency) are computed from constants and live in the module docs of
`asperitas_wire::capture`; see [protocol.md](./protocol.md). A derived figure is not a reading and must
not be cited as one.

| Quantity | Value | Basis |
|---|---|---|
| SDRAM device | 64 MiB (`FMC_SDCR1` configures 9 col x 13 row x 4 banks x 32-bit) | measured 2026-10-08, register read above |
| Ring filled end to end | 349.536 s on the device clock | measured 2026-10-13 |
| Dump of 300 s of audio over USB CDC, internal-flash layout | 69.4 s, 414.9 kB/s PCM, 731.7 kB/s on the wire | measured 2026-10-08, `DUMPEND elapsed_ms=69421` |
| Same, bootloaded layout | 60.2 s | measured 2026-10-15, `DUMPEND elapsed_ms=60232` |
| Full-speed USB bulk ceiling | not separated from the above; the measured wire rate is a lower bound | the dump may be paced by the device, so it is not the ceiling |
| Sustained FMC write throughput | not measured | the ring kept up with 879 of 879 blocks, which bounds it only from below |

## Instrument excerpts in QSPI

The slot layout, storage per second, flash rules, replay CRC posture and the install and replay
record grammar are owned by the module docs of `asperitas_wire::excerpt`; see
[protocol.md](./protocol.md). This section holds only the cost predictions and what the bench has
observed of them. Cells reading `pending TASK-038.09` have not run on a board. The application itself
is not placed in QSPI by the default layout (see [flashing.md](./flashing.md)); the bootloaded layout puts its
image at `0x90040000`, above the bootloader region, which is why the excerpt area starts well clear of
it.

### Costs, predicted and observed

Basis labels: derived = arithmetic on constants in the code; datasheet = IS25LP064A figure as quoted
in the ticket, not re-read from a sheet in this repo; estimated = a guess with no measurement behind it.

| Quantity | Predicted | Derivation | Observed |
|---|---|---|---|
| Playback rate | 96,000 B/s | derived: 48,000 Hz x 1 channel x 2 B (`replay::BYTES_PER_SECOND`) | pending TASK-038.09 |
| Corpus clip PCM sizes | 190,312 to 288,000 B | derived: WAV file size minus 44-byte header, `audio/instruments/` (octave_mandolin_fast_run 190,312; mandolin_fast_run 207,756; six clips at 288,000) | n/a (file sizes) |
| Sectors for a 288,000 B clip | 71 PCM + 1 header = 72 | derived: ceil(288,000 / 4096) = 71 | pending TASK-038.09 |
| Sectors for a 190,312 B clip | 47 PCM + 1 header = 48 | derived: ceil(190,312 / 4096) = 47 | pending TASK-038.09 |
| Erase time, 190 kB clip | at most 14.4 s (47 PCM sectors alone: 14.1 s) | datasheet max 300 ms x 48 sectors; typical is far lower (tens of ms), so this is an upper bound | pending TASK-038.09 |
| Erase time, 288 kB clip | at most 21.6 s | datasheet max 300 ms x 72 sectors | pending TASK-038.09 |
| Program time per sector | at most 12.8 ms | datasheet max 0.8 ms x 16 pages of 256 B | pending TASK-038.09 |
| Install wall clock, 190 kB clip | at most 15.0 s | 14.4 s erase + 48 x 12.8 ms program (0.61 s); excludes USB and readback | pending TASK-038.09 |
| Install wall clock, 288 kB clip | at most 22.5 s | 21.6 s erase + 72 x 12.8 ms program (0.92 s); excludes USB and readback | measured 2026-10-15: 5.82 s from the first stream byte to `EXCOK` (includes USB transfer, erase, program and readback), slot 0, `mandolin_single_note_soft` |
| Wire bytes, 288 kB install | about 493 kB | derived: 2250 chunks x (191 B body + 28 B frame overhead) = 492,750, plus `EXCSTART` and `EXCEND` frames | measured 2026-10-15: 492,867 bytes written by `excerpt_stream` |
| Wire bytes, 190 kB install | about 326 kB | derived: 1486 full chunks x 219 + one 104 B chunk (159 + 28) = 325,621, plus start and end frames | pending TASK-038.09 |
| USB transfer time | well under 1 s | estimated: full-speed bulk, ceiling unmeasured | not separated from the install wall clock above, which was 5.82 s in total; the ceiling stays unmeasured |
| Replay staging half | 8,192 B, plays for 85,333 us | derived: `replay::HALF_BYTES`, two halves, 16 KiB of AXI SRAM | pending TASK-038.09 |
| Worst-case half refill | 18,192 us | estimated: 10,000 us scheduling (`ASSUMED_SCHEDULING_LATENCY_MICROS`) + 8,192 us transfer at an assumed 1 MB/s QSPI floor | pending TASK-038.09 |
| Replay margin | 4.69x (85,333 / 18,192); build fails below 4x | estimated: the inputs above are assumptions; the QUADSPI kernel clock is unset in the HAL config, so the real read rate is unknown | measured 2026-10-15 only as an outcome: one 3.00 s pass, `underruns=0 clean=1`; the margin itself was not measured |

Erase dominates the install: the USB stream and the programming are small beside it. If the observed
erase time lands near typical rather than maximum, the install takes seconds, not tens of seconds.

## libDaisy (C++) has no Seed3 support

As of libDaisy `v8.1.0` (released 2026-02-23) and `master` as of 2026-06-26, there is
**no** TAC5242 driver and no Seed3 board definition. `src/dev/` contains only
`codec_ak4556`, `codec_pcm3060`, and `codec_wm8731`; `BoardVersion` enumerates Rev4,
Seed 1.1, and Seed 1.2 only. No open issues or PRs reference Seed3 or TAC5242.

This inverts the usual assumption: **falling back to C++ to prove the board works is not
currently an available strategy for audio.** The Rust stack is ahead. (Blinky in C++
would still work, since that touches no codec.)
