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

## Flashing the Seed3

Two routes reach the same flash. **DFU over the onboard USB-C** needs no extra hardware and is
the everyday path. **An ST-Link probe on the SWD pads** also streams `defmt`/RTT logs,
symbolicates faults from the release build's DWARF, and can recover a board that dies before
USB enumerates — see *Flashing and logging over an ST-Link probe* below.

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
  in every binary, presenting as a board that simply never booted.
- **A fault before `main` is invisible to both software channels.** Nothing has initialised
  USB or the LED state machine yet, so the board just sits there. The probe is the way to see
  it: attaching under reset halts the core wherever it faulted and the fault status registers
  say why (*Flashing and logging over an ST-Link probe*). Reading the first four bytes of
  `firmware.bin` — the little-endian initial SP — is what's left on a bench with no probe, and
  it is a clue, not a diagnosis.

### Debugging with nothing attached

These two work with no probe on the bench, and they keep working when a probe is attached:
`defmt`/RTT is a third channel beside them, not a replacement for either. *What each channel
loses* covers why you can't read one off the other. In order of usefulness:

1. **USB CDC-ACM serial over the onboard USB-C.** `daisy-embassy` ships an
   `examples/usb_serial.rs`. This gives real text logging from the running application
   with no extra hardware. Flash over DFU, then the application enumerates as a serial
   device.
2. **Pod RGB LEDs as a boot-stage indicator**, for the case where USB itself hasn't come
   up yet. See [daisy-pod.md](./daisy-pod.md).

Neither speaks before USB enumerates, which is exactly where boot faults live. That window is
the probe's own ground, and the reason it earns a place on the bench.

### Flashing and logging over an ST-Link probe

The Seed3 exposes SWD/JTAG pads. The 10-pin connector pinout is identical to earlier
Seeds; there are additional pads matching the 14-pin ST-LINK-V3MINIE, present only for
mechanical alignment — **the extra pins are not wired up**. Whether the 10-pin footprint is
physically reachable with the Seed seated in the Pod is unmeasured; TASK-037 records the
attachment method actually used.

The probe for this bench is an ST-Link V3 MINIE. As of 2026-09-10 no probe has been attached to
this board at all, which is what TASK-037 exists to change.

**What is and isn't established here.** The commands below are copied from `make -n` output in
`firmware/`, the flag semantics from `probe-rs` 0.32.0's own `--help`, the target behaviour
from `defmt-rtt` 1.3.0 source, and the chip entry from `probe-rs chip info` — all of which are
host-side facts, checkable without a board. What has *not* been established is anything about
this board: attach time, flash time, log throughput, whether `--connect-under-reset` works
reliably with an ST-Link V3 MINIE, whether a real panic decodes to a backtrace. Those are
TASK-037's measurements and this document leaves them blank rather than estimating.

```bash
cd firmware
DEFMT_LOG=info make probe-flash FEATURES="seed3 log-defmt" NO_DEFAULT=1
make probe-log
```

Expanded (`make -n probe-flash probe-log FEATURES="seed3 log-defmt" NO_DEFAULT=1`):

```
cargo build --release --no-default-features --features "seed3 log-defmt" --bin main
probe-rs download target/thumbv7em-none-eabihf/release/main --chip STM32H750IBKx --connect-under-reset --verify --reset
probe-rs attach target/thumbv7em-none-eabihf/release/main --chip STM32H750IBKx
```

There is a third target, `probe-run`: flash, then stay attached and stream. The names don't
show the difference that matters, so: **`probe-flash` exits when it's done**, which is what an
unattended loop wants, while `probe-run` and `probe-log` hold the attachment and stream until
you interrupt them. `probe-flash`'s `--verify` is genuine read-back verification by probe-rs —
don't port the `File downloaded successfully` grep from the DFU recipe to this path; that idiom
exists only because dfu-util reports failure on success. `probe-log` compiles nothing and resets
nothing — it reads the ELF from the previous build to find the RTT control block, so rebuild
first if the firmware changed, and don't expect `FEATURES` or `DEFMT_LOG` on that line to do
anything. There is deliberately no
`runner` in `.cargo/config.toml`, even though upstream daisy-embassy ships one: a runner lets
`cargo run` in a script, or an editor action, silently reach for the probe.

`--chip STM32H750IBKx` is the exact string, verified host-side against the pinned toolchain:
`probe-rs chip info STM32H750IBKx` reports NVM `0x08000000..0x08020000` (128 KiB) and AXI SRAM
`0x24000000..0x24080000` (512 KiB), which matches `firmware/memory.x`. Bare `STM32H750IB`
resolves too; use the full string so it reads as the same part `memory.x` describes.

**Why the probe path takes the ELF and DFU takes `firmware.bin`.** `probe-rs` decodes `defmt`
frames from the ELF's `.defmt` section and its symbol table, and unwinds with its DWARF. Flash
a stripped `.bin` while holding some other ELF on the host and the host's metadata cannot match
what is running — you get frames that decode wrong, or addresses that resolve to the wrong
line, which is worse than no symbols because it looks like data. `firmware.bin` stays a
DFU-only artifact. Flashing a raw binary over the probe would also need
`--binary-format bin --base-address 0x08000000`, adding one more way to confuse the two.

Two things gate whether a probe command gets anywhere, both found by running them against
built artifacts rather than at the bench:

- **Only a `log-defmt` ELF loads at all.** `build.rs` passes `-Tdefmt.x` for that feature,
  which consolidates the per-call-site `.defmt_*` sections into the single `.defmt` section
  probe-rs reads. A console-only ELF is rejected during image load with "Failed to parse defmt
  data: no `.defmt` section" — before probe discovery even starts.
- **`DEFMT_LOG` decides what's in the stream, at build time.** With the variable unset,
  defmt-macros compiles every non-ERROR call to nothing (`defmt-macros-1.1.1`,
  `src/function_like/log/env_filter.rs:34`), so an attach against a plain
  `make build FEATURES="seed3 log-defmt" NO_DEFAULT=1` image shows a silent channel and reads
  as a broken probe. The facade's runtime `set_max_level(Info)` cannot compensate: the arm
  that would have shipped the record isn't in the binary. Ask for the level you want when you
  build — `DEFMT_LOG=info` above is load-bearing, not decoration.

**Three regimes an RTT stream runs in,** predictable from the mode bit alone. `defmt-rtt`
inits its up-channel to `NON_BLOCKING_TRIM` (`src/lib.rs:113`); probe-rs flips it to
block-if-full when it attaches, and `defmt_rtt::in_blocking_mode()` tells firmware which it is.

| Host state | Mode in effect | What happens |
|---|---|---|
| No host attached | `NON_BLOCKING_TRIM` | frames truncated or dropped, silently, with no counter anywhere saying so |
| Attached, host keeping up | block-if-full (set by probe-rs on attach) | nothing lost |
| Attached, host stalled | block-if-full | target spins in the write loop with interrupts disabled — the application freezes, and audio goes first |

Row three is probe-rs's own warning, verbatim from `probe-rs attach --help`: "if the
application writes within a critical section, using this mode can cause the application to
freeze if the buffer becomes full and is not read by the host". Every `defmt` frame here *is*
written inside `critical_section::acquire()` (`defmt-rtt` `src/lib.rs:168`), against a ~667 µs
audio block deadline (32 samples at 48 kHz). Hence the rule, absolute: **nothing may log from
the audio callback.** Reentrancy is fatal too — the logger panics rather than nesting.

If the host is going to be slow on purpose (a laptop resuming, a pipe into `grep` that isn't
draining), say so from the host side instead of hoping: `--rtt-channel-mode no-block-skip`
drops a whole frame when it doesn't fit, `no-block-trim` writes what fits and ignores the rest.
Both are values probe-rs 0.32.0 accepts; `block-if-full` is its default.

Two erase-and-debug details that are cheap to state and expensive to rediscover:

- **STM32H750xB internal flash is a single 128 KB sector**, so any probe-initiated erase wipes
  the whole application. Harmless while the application is all that lives there, but it rules
  out keeping resident data in internal flash beside the firmware.
- **Sleep modes break RTT discovery** on several STM32 parts: unless `DBG_SLEEP` /
  `DBG_STANDBY` / `DBG_STOP` are set in `DBGMCU_CR`, the RTT control block is never found once
  the core has slept (probe-rs #350). Nothing here sleeps — the embassy executor busy-loops —
  so the busy loop is the safe state and is not to be "optimised" without reading that issue.

`probe-flash` passes `--connect-under-reset`, which asserts nRESET while attaching and is what
takes the BOOT/RESET handshake out of the loop. It has open reliability reports against the
ST-Link V3 MINIE specifically — probe-rs #3516, where STM32CubeProgrammer succeeds on the same
probe and probe-rs does not, and an ST community thread concludes the V3 MINIE almost never
drives nRESET low. The flag stays the default until measured otherwise. `PROBE_EXTRA` is the
bench escape hatch without editing the Makefile: `make probe-flash PROBE_EXTRA="--speed 1000"`,
or skip the reset entirely by attaching to an already-flashed board with `make probe-log`, which
never needs one. TASK-037 records which of those worked.

### What each channel loses

The device paths keep different books, and a loss figure from one says nothing about the other.
The framed USB console reports what it failed to send: a per-record sequence number the reader
can difference for gaps, plus cumulative `dropped_full`, `bytes_dropped`, `trunc` and `ep_err`
counters carried in band — so a capture that lost records can prove it afterwards, and a capture
that didn't can too. (TASK-030.03 documents that record grammar and field set; this section is
only about which ledger is which.) RTT keeps no ledger at all. Unattached it discards freely and
says nothing; attached it loses nothing *while the host keeps reading*, and silently stops being
the thing under your control the moment the host stalls. So a clean, gapless RTT stream is
evidence that the host kept up, not evidence that the firmware's diagnostics were complete —
and a loss count quoted from the console does not describe an RTT capture, or vice versa. When a
number has to be trustworthy, take it from the console's counters and say which channel it came
from.

## libDaisy (C++) has no Seed3 support

As of libDaisy `v8.1.0` (released 2026-02-23) and `master` as of 2026-06-26, there is
**no** TAC5242 driver and no Seed3 board definition. `src/dev/` contains only
`codec_ak4556`, `codec_pcm3060`, and `codec_wm8731`; `BoardVersion` enumerates Rev4,
Seed 1.1, and Seed 1.2 only. No open issues or PRs reference Seed3 or TAC5242.

This inverts the usual assumption: **falling back to C++ to prove the board works is not
currently an available strategy for audio.** The Rust stack is ahead. (Blinky in C++
would still work, since that touches no codec.)
