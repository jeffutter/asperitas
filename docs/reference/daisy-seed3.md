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

## Flashing without a debug probe

The Seed3's onboard USB-C supports DFU. The STM32H750's built-in system bootloader
exposes DFU at `0x08000000` (128 KB internal flash), which is enough for a
reasonably-sized effect. Larger applications need the Daisy bootloader, which relocates
the application into QSPI.

### Enter DFU mode

Hold `BOOT`, tap `RESET`, release `BOOT` — the board enumerates as an STM32 DFU device.
Verify with `lsusb` (should show **STMicroelectronics STM32 bootloader**).

### Build and flash blinky

From the `firmware/` directory:

```bash
# One-shot build + flash
make flash-all BINARY=blinky

# Or step by step
make build BINARY=blinky   # produces firmware.bin via cargo objcopy
make flash                 # dfu-util -a 0 -s 0x08000000:leave -D firmware.bin
```

`BINARY` defaults to `main`, so plain `make flash-all` flashes the application, not
blinky.

Or manually:

```bash
cd firmware
cargo objcopy --release --features seed3 --bin blinky -- -O binary firmware.bin
dfu-util -a 0 -s 0x08000000:leave -D firmware.bin
```

The blinky binary (`firmware/src/bin/blinky.rs`) drives **Pod RGB LED 1** (D20/D19/D18
= PC1/PA6/PA7) through the shared LED state machine — it does *not* touch the Seed's
onboard user LED. It blinks red at ~1 Hz during the pre-init window, then settles to
**steady green** once USB is up. Boot is fast enough that in practice you see steady
green almost immediately; a steady LED here is success, not a hang. It is kept
permanently as a known-good diagnostic — when something later goes wrong, being able to
flash something that definitely works is worth a lot.

If the LED stays dark, flash `BINARY=ledtest` instead. It depends on nothing but clock
init, board init, and three GPIO pins — no USB, no logging, no LED singleton — and
cycles the RGB channels so something visibly changes under either polarity. Motion means
the core is running and the fault is downstream.

### Notes

- **`:leave` works on Seed3.** Verified on hardware 2026-08-01: the bootloader jumps
  straight to the application, no RESET tap needed. (An earlier revision of this document
  claimed it was unreliable on STM32H7 — that was wrong, and it misdiagnosed a firmware
  hang as a DFU problem. See the RAM note below for what was actually broken.)
- **dfu-util exits 74 on a fully successful `:leave`.** The jump to the application tears
  down the USB connection that the pending GET_STATUS was travelling on, so nothing is
  left to answer it and dfu-util reports `Error during download get_status`. The exit
  code is therefore useless as a success signal — and it can't just be ignored either,
  since 74 is also returned for genuine download I/O errors. `make flash` keys on
  dfu-util's own `File downloaded successfully` marker instead and prints `Flashed and
  started` or `FLASH FAILED`; trust that line, not the dfu-util noise above it.
- **DFU mode is not sticky.** Once a flashed app boots, the bootloader is gone. Repeat
  BOOT+RESET before *every* flash, or dfu-util reports `No DFU capable USB device
  available`.
- **Binary size check:** `ls -la firmware.bin` should show < 128 KB (blinky is ~18 KB).
- **Prerequisites:** `nix develop .` provides rustc, cargo-binutils, and dfu-util.
  No additional setup needed.
- **memory.x RAM length is load-bearing.** AXI SRAM is **512 KB**, not 1 MB — the
  advertised "1 MB" is the total across all domains (AXI 512K + D2 288K + D3 64K + DTCM
  128K + ITCM 64K), and only the AXI region is contiguous at `0x24000000`. `cortex-m-rt`
  derives the initial stack pointer from `ORIGIN + LENGTH`, so declaring 1M put SP past
  the end of physical RAM and the first push after reset hard-faulted — before `main`,
  in every binary, presenting as a board that simply never booted. If a board is dead in
  a way that survives reflashing, check the first four bytes of `firmware.bin` (the
  little-endian SP) before suspecting the DFU path.

### Debugging without a probe

Losing `probe-rs` also loses `defmt`/RTT logging, which is the usual way to see panics
and diagnostics. Two replacement channels, in order of usefulness:

1. **USB CDC-ACM serial over the onboard USB-C.** `daisy-embassy` ships an
   `examples/usb_serial.rs`. This gives real text logging from the running application
   with no extra hardware. Flash over DFU, then the application enumerates as a serial
   device.
2. **Pod RGB LEDs as a boot-stage indicator**, for the case where USB itself hasn't come
   up yet. See [daisy-pod.md](./daisy-pod.md).

### Debug probe, when one is available

The Seed3 exposes SWD/JTAG pads. The 10-pin connector pinout is identical to earlier
Seeds; there are additional pads matching the 14-pin ST-LINK-V3MINIE, present only for
mechanical alignment — **the extra pins are not wired up**.

## libDaisy (C++) has no Seed3 support

As of libDaisy `v8.1.0` (released 2026-02-23) and `master` as of 2026-06-26, there is
**no** TAC5242 driver and no Seed3 board definition. `src/dev/` contains only
`codec_ak4556`, `codec_pcm3060`, and `codec_wm8731`; `BoardVersion` enumerates Rev4,
Seed 1.1, and Seed 1.2 only. No open issues or PRs reference Seed3 or TAC5242.

This inverts the usual assumption: **falling back to C++ to prove the board works is not
currently an available strategy for audio.** The Rust stack is ahead. (Blinky in C++
would still work, since that touches no codec.)
