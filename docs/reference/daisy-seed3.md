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
Check it from inside the dev shell with `dfu-util --list`, which prints one `Found DFU:` line
per device and nothing but its banner when the board is not in DFU mode (dfu-util 0.11, run
2026-09-12 with nothing attached). `lsusb` names the same device **STMicroelectronics STM32
bootloader**, but the flake ships no usbutils, so that is the form for outside the shell. The
*Prerequisites* bullet below lists what the shell does provide.

### Build and flash blinky

From the `firmware/` directory:

```bash
# One-shot build + flash
make flash-all BINARY=blinky

# Or step by step, with BINARY on both halves
make build BINARY=blinky   # writes blinky.bin
make flash BINARY=blinky   # dfu-util -a 0 -s 0x08000000:leave -D blinky.bin
```

`BINARY` defaults to `main`, so plain `make flash-all` flashes the application, not
blinky. Repeating `BINARY` on the two-line route is not ceremony. `build` writes `$(BINARY).bin`
and `flash` reads `$(BINARY).bin`, so `make build BINARY=blinky` followed by a bare `make flash`
programs `main.bin` while printing a completely successful DFU transcript. Verified against
`make -n build flash BINARY=blinky` and `make -n flash` on 2026-09-12: the first expands to
`-O binary blinky.bin` and `-D blinky.bin`, the second to `-D main.bin`. `firmware/Makefile:11-14`
carries why each image carries its binary's name instead of sharing one file.

Use `make build`. A hand-typed `llvm-objcopy -O binary` also works now, and did not used to, which
is worth knowing before you copy one from an old note. `-O binary` asks for a *memory* image,
spanning the lowest to the highest **load** address, so the image is right only when every
file-backed section is loaded in flash. It is: `.data` runs from a RAM VMA of `0x24000000` but a
flash LMA of `0x08015808`, and `.sram1_bss` - the two `GroundedArrayCell::uninit()` SAI DMA buffers
daisy-embassy `ca9bcc9` tags `#[link_section = ".sram1_bss"]` (`src/audio.rs:20-22`) - gets
`(NOLOAD)` placement from `firmware/memory.x`. Measured 2026-09-13, a plain `-O binary` and
`make build` agree byte-for-size on all six binaries: `main` 88,581, `rig` 106,811, `blinky`
65,638, `ledtest` 17,774, `podtest` 72,689, `panictest` 65,958.

Before TASK-059 landed, `.sram1_bss` was an orphan section rust-lld placed with LMA == VMA in AXI
SRAM, so anything linking the audio module spanned `0x08000000..0x24000598` and came out at
**469,763,480 bytes** of mostly zeros while `blinky`, which links no audio, stayed at 65,638 and
byte-for-byte matched the Makefile's output. That combination is the reason this page spent a week
insisting the Makefile's `--only-section` keep-list was load-bearing: a recipe checked against
`blinky` looked fine and quietly broke on `main`. The keep-list is gone, and so is the trap it
worked around - but the failure mode is silent, not loud. If a section ever comes back loaded
outside flash, the image grows to span the gap rather than erroring. TASK-059.02 files the gate
that reads load addresses off the ELF instead of trusting anyone to notice.

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
- **The probe path's exit code means what it says**, which is the one place the two flashing
  routes agree on nothing else: with no probe attached, `probe-rs download` and `probe-rs attach`
  both exit **1** printing `Error: No connected probes were found.` The full contract, including
  the different string `probe-rs list` prints for the same empty bench, is in
  [*Running the probe path unattended*](#running-the-probe-path-unattended).
- **DFU mode is not sticky.** Once a flashed app boots, the bootloader is gone. Repeat
  BOOT+RESET before *every* flash, or dfu-util reports `No DFU capable USB device
  available`.
- **Binary size check:** internal flash is one 128 KB sector, so anything at or over 131,072 bytes
  cannot fit. Measured 2026-09-12 at `[profile.release] debug = 2`, from `make build`'s output named
  for the binary: `blinky.bin` is 65,638 bytes built with defaults (`FEATURES="seed3"`), 25,176 as
  RTT-only (`NO_DEFAULT=1 FEATURES="seed3 log-defmt"`), and 21,202 with no transport at all
  (`NO_DEFAULT=1 FEATURES="seed3"`). `main` and `rig` are far larger; the `build` recipe in
  `firmware/Makefile` carries those figures.
- **Prerequisites:** `nix develop .` provides rustc, cargo-binutils, dfu-util and probe-rs-tools,
  matching `firmware/Makefile:4`; `flake.nix:38-63` is the list. No additional setup needed. What
  the shell leaves out is usbutils, so `lsusb` (*Enter DFU mode*) has to come from outside it, and
  `dfu-util --list` is the equivalent that works inside.
- **memory.x RAM length is load-bearing.** AXI SRAM is **512 KB**, not 1 MB — the
  advertised "1 MB" is the total across all domains (AXI 512K + D2 288K + D3 64K + DTCM
  128K + ITCM 64K), and only the AXI region is contiguous at `0x24000000`. `cortex-m-rt`
  derives the initial stack pointer from `ORIGIN + LENGTH`, so declaring 1M put SP past
  the end of physical RAM and the first push after reset hard-faulted — before `main`,
  in every binary, presenting as a board that simply never booted.
- **A fault before `main` is invisible to both software channels.** Nothing has initialised
  USB or the LED state machine yet, so the board just sits there. The probe is the way to see
  it: a plain attach (no reset) halts the core wherever it faulted and the fault status registers
  say why (*Flashing and logging over an ST-Link probe*). Attaching under reset does not show the
  fault - the reset clears those registers and the core stops at the reset vector - but it is how
  you regain control to reflash. On a bench with no probe, the first four
  bytes of the image you flashed, `main.bin` or whichever `$(BINARY).bin` it was, hold the
  little-endian initial stack pointer. Read that, and treat it as a clue rather than a diagnosis.

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

#### Console protocol v1: every record verifies itself

CDC-ACM delivers an undifferentiated byte stream: no boundaries, no length, no sequence. A record
whose tail vanished at a ring wrap is therefore indistinguishable from a short log line, which is how
truncated knob lines came to be filed as an ADC glitch (TASK-018.04). v1 puts a sequence number and a
checksum on every line while keeping each one readable in `screen` and greppable in a raw capture.
The normative copy is `crates/asperitas-logging/src/frame.rs`; this is a transcription, and where the
two disagree the code is right.

```text
record := '~' level SP seq SP t_ms SP body '*' crc CRLF
```

- **`~`** — start marker, device→host only (`>` is reserved the other way).
- **`level`** — one of `I W E D T` (`level_letter`); the decoder accepts exactly those five letters.
- **`seq`** — exactly 8 lowercase hex digits, a `u32` incremented once per record within a boot.
  Assigned *inside* the record lock (`take_seq` called from `emit` in `lib.rs`), which is what makes
  numeric order equal wire order. It wraps mod 2³² on purpose; the loss counters do the opposite.
- **`t_ms`** — exactly 8 decimal digits, milliseconds since boot, transmitted modulo 100 000 000
  (`T_MS_WRAP`) so the field never widens and invalidates the fixed offsets behind it. It wraps after
  ~27.8 h, and neither field reveals the wrap on its own — continuity comes from `BOOT` plus `seq`.
- **`body`** — 0–200 bytes (`MAX_BODY`), sanitised on the way out.
- **`crc`** — `*`, then 4 lowercase hex digits, then `CR LF` (`TRAILER_LEN` 7).

The prefix is 21 bytes (`PREFIX_LEN`), so overhead is 28 bytes per record — and 28 bytes is also the
shortest legal record, the one with an empty body. Longest is 228 (`MAX_FRAME`).

Example records. These are the shipped encoder's own bytes, pinned by the `encode_golden_*` tests in
`frame.rs`, not a bench capture:

```text
~I 00000042 00004567 ENC +1*9c17
~D 00000000 00000000 *91d4
~W deadbeef 76980377 knob r2=298 ✓*b321
```

34, 28 and 43 bytes counting the closing CRLF. The second is pure overhead. The third carries a
wrapped `t_ms` (0x9999_9999 ms → `76980377`) and is the truncated knob line that nearly became an ADC
bug report; the checkmark crosses the wire as its three UTF-8 bytes.

**Checksum: CRC-16/CCITT-FALSE.** Width 16, poly `0x1021`, init `0xFFFF`, `refin=false`,
`refout=false`, `xorout=0x0000`, check `0x29b1` for the 9-byte ASCII string `123456789`, alias
CRC-16/IBM-3740, catalogue <https://reveng.sourceforge.io/crc-catalogue/16.htm>. Copy the parameters,
not the name: "CRC-16/CCITT" is routinely misidentified — the reflected form is KERMIT, check `0x2189`,
and XMODEM is the init-`0x0000` variant — so anyone who implements whichever function their library
calls CCITT gets a capture in which every record fails. The covered range is
`level SP seq SP t_ms SP body`: every byte after the `~` up to but excluding the `*`, the separator
spaces included — 20, 26 and 220 bytes for the three lines above. Deliberately a table-less bit loop
(≤ 220 × 16 iterations per record) so both ends of the link can share identical source. Integrity is
not authenticity: the checksum is affine over GF(2), so two edits whose contributions cancel leave it
valid while changing the payload. That sets the ceiling on what a clean CRC proves.

**CRLF is a trustworthy delimiter because the producer sanitises the body.** `sanitize_byte` replaces
every byte `< 0x20` — CR and LF among them — and DEL (`0x7F`) with `'_'`, while bytes ≥ 0x80 pass
through untouched so UTF-8 survives. The fixed-width prefix contains no CR or LF either, so the first
complete CRLF after a candidate start *must* be that record's terminator. Printable punctuation
deliberately survives, `~`, `*` and `|` included: framing strength comes from the grammar plus the CRC
and above all the no-CR/LF invariant, not from a supposedly tilde-free payload, so a stray `~` or `*`
inside a body can only cause a CRC mismatch, never a false-valid record. SLIP (RFC 1055) and COBS buy
delimiter uniqueness by escaping bytes; this transport buys it at the producer, which is cheaper and
keeps every line legible. The same sanitisation is the log-injection fix (CWE-117).

**Locate the `*` backwards from the CRLF, never forwards from the `~`.** Sanitisation only neutralises
control bytes and DEL, so a body may legitimately contain `*`, and a forward search mis-decodes exactly
those records. The `*` sits 5 bytes before CR and the CRC digits start 4 bytes before it.

**Resynchronisation: advance strictly past the disqualified start byte.** On any failure — a level
letter off `I W E D T`, a misplaced separator space, a non-hex `seq`, a non-decimal `t_ms`, a missing
`*`, a CRC mismatch, a CRLF landing where no legal body length fits, or a window that fills to
`MAX_FRAME` without ever finding a terminator — drop that one `~`, charge the skipped bytes once, and
retry at the next `~`; never repair a record, never guess a shorter body. The hazard is documented
upstream: ArduPilot's C MAVLink parser desynchronised permanently when a bad-CRC message happened to end
in a byte equal to the STX magic, because resuming at "the next plausible-looking byte" can land *inside*
the next real frame (<https://github.com/ArduPilot/pymavlink/issues/881>). What the rule buys is stated
by the code: corruption costs one record, not the capture, and because records come only from fully
validated frames, **no record is ever invented**. Two limits belong in the same breath as that promise.
A record whose leading `~` was lost produces no integrity failure at all — the decoder never saw a
candidate start — and only a `seq` gap or a `STATUS` counter reveals it. And a byte-level splice between
two producers legitimately decodes as two good records plus one integrity failure, because the frames
around the splice really are intact.

#### `BOOT` and `STATUS`: the reserved body prefixes

Two bodies are wire contract, rendered in `crates/asperitas-logging/src/console.rs` and riding ordinary
records — same framing, same CRC, same chance of being dropped:

```text
BOOT proto=1 fw=0.1.0 pipe=2048 maxbody=200
STATUS proto=1 sent=12 dropped_full=3 bytes_dropped=4096 trunc=1 ep_err=2 seq_next=18 pipe_free=2048
```

`proto` comes first so a reader can handshake before trusting anything. `fw` is the
`asperitas-logging` crate version, which is the version that ships the console — the firmware binaries
have no separate release process. `pipe` is the log ring in bytes (`LOG_PIPE_SIZE`) and `maxbody` the
encoder's cap. `BOOT` consumes `seq 0` and is emitted only once the USB backend is installed (`usb::init`
installs the logger, switches the backend, then announces), so ordering it before the switch discards it
with no trace in any counter.

`STATUS` fields and their order *are* the contract: TASK-031 parses them, and a unit test fails if either
moves. Values are absolute since-boot counts, never deltas — the host owns the differencing.

| Field | Counts |
|---|---|
| `sent` | Records committed to the ring whole. Cross-check against the host's decoded count. |
| `dropped_full` | Records refused for lack of space — never partially written. |
| `bytes_dropped` | Bytes those refused records would have occupied. |
| `trunc` | Bodies shortened by the 200-byte cap. **Those records shipped**, which is why they are not in `dropped_full`. |
| `ep_err` | Endpoint writes that failed: link loss or stalls. |
| `seq_next` | The value the next record will carry. |
| `pipe_free` | Ring free bytes at the instant of rendering — how close to full the ring was, which is what makes a drop storm diagnosable after the fact rather than merely deniable. |

Pacing: at most once per second (`STATUS_MIN_INTERVAL_MS`) and only when a counter actually moved,
emitted from the drain loop only when the ring is empty. Debouncing is not politeness — during a
full-ring condition the `STATUS` record competes for the very space that is missing, so an undebounced
emitter starves the logs it is reporting on.

**Counters saturate at `u32::MAX`; `seq` wraps.** A wrapped byte counter becomes an enormous negative
rate and reads as a decoder bug, and roughly 24 days of sustained dropping at ring capacity is
reachable on a rig left running. `seq` does the opposite because a saturated sequence number would emit
the *same* `seq` twice and invent a record, while a wrapped one is recoverable by modular subtraction —
a jump ≥ 2³¹ means restart or corruption, not loss. A counter describes an amount, a sequence number
describes a position.

**Why `seq` alone cannot tell a reboot from a loss:** CRC and `seq` detect damage, never absence. A hole
in the stream leaves no bytes behind, so a gap is equally consistent with "bytes lost" and "board
rebooted and restarted numbering at 0". The second `BOOT` is what breaks the tie — or a `seq`
regression. There is deliberately no boot-id in v1: it would need `.noinit` persistence or a peripheral
read for no gain over the banner's presence, and reset *reason* is TASK-032's. The honest limit is that a
lost `STATUS` record is indistinguishable from nothing having changed, except through the `seq` gap it
leaves behind — silence means "probably nothing changed", not "nothing changed".

`AUDIO` and `AUDEND` are a third reserved pair carrying base64 audio dumps over the same framing
(`crates/asperitas-logging/src/dump.rs`); their grammar and the rig workflow around them belong to
TASK-038.06, not here.

**Direction:** `~` is device→host only. `>` (0x3E) is reserved host→device for TASK-032's commands, with
identical field and CRC rules so one parser serves both directions. The audio dump rejected Ascii85 and
Z85 partly because those alphabets contain `~` and `<>`, which would let a corrupted body reassemble into
a fake record boundary — the same reason the reservation is worth honouring before the first command
exists.

**What this replaced:** roughly 8.8 % of log lines were truncated over USB CDC — 1226 of 13968, counted
by hand across a 240 s `podtest` capture on 2026-08-08 (TASK-018.04's notes). One of them read
`r2=298`, cut mid-number, and nearly got reported as an ADC glitch. Root cause: `Pipe::try_write`
short-writes at every ring wrap even when the ring is empty, and the old path treated a short write as
success. Treat 8.8 % as a baseline, not a reproducible measurement — the raw capture no longer exists,
which is itself part of the case for TASK-031.

#### The USB short-packet rule, for whoever writes the reader

Bulk transfers must end with a short packet. If the final packet of a transaction is exactly
`max_packet_size` — 64 here — the host driver holds it, and everything it carried, until something
shorter follows. embassy-usb says so verbatim in its own `CdcAcmClass` docs: "If you write a packet that
is exactly `max_packet_size` bytes long, it won't be processed by the host operating system until a
subsequent shorter packet is sent. A zero-length packet (ZLP) can be sent if there is no other data to
send." USB 2.0 §5.8.3 is the normative statement, and the ST community thread *"STM32U5 USB (CDC) not
transmitting data if in exact multiple of 64 bytes"* is the same MCU family showing the symptom.

The firmware does it in both places that can strand a tail: the drain loop tracks
`last_packet_was_full` and emits a ZLP before parking, and only when the ring is empty, because that
pair is what means "I am about to stop sending"; and `emit_blocking` sends one when the panic record's
length is a multiple of 64, where the miss would be least forgiving — the withheld record would be the
`PANIC:` line. Without it the end-of-session tail never arrives, and this project's own framing would
report that faithfully as a `seq` gap while the cause sat in the drain loop.

So for the reader: **zero-length reads are legitimate.** Do not treat a 0-byte read as a disconnect or
an EOF; terminate on record validation and `seq` continuity instead. Note too that the endpoint rejects
oversize writes rather than splitting them (`EndpointError::BufferOverflow`), which is why every write
path on the device chunks to 64.

### Flashing and logging over an ST-Link probe

The Seed3 exposes SWD/JTAG pads. The 10-pin connector pinout is identical to earlier
Seeds — including **pin 10 being nRESET**, whose net is traced later in this section; there
are additional pads matching the 14-pin ST-LINK-V3MINIE, present only for mechanical alignment — **the
extra pins are not wired up**. Whether the 10-pin footprint is physically reachable with the Seed seated
in the Pod is unmeasured; TASK-037 records the attachment method actually used.

The probe for this bench is an ST-Link V3 MINIE. As of 2026-09-10 no probe has been attached to
this board at all, which is what TASK-037 exists to change.

**Pad layout and cable orientation.** The 14 pads are two rows of seven, and a straight-through
1:1 14-pin ribbon (STDC14 on the probe end) is correct: pin n on the probe goes to pad n on the
Seed. The outer pair at each end is unconnected, and the centre 10 (pins 3 to 12) carry the same
signals in the same order as the old 10-pin Cortex Debug header (pin 10 nRESET). Nothing on the
board marks pin 1, so it is found by continuity. With the **USB-C port at the top** and the pad
side facing you:

```
              USB-C (top)

 left                                         right
 13     11     9      7      5      3      1      top row (odd pins)
 NC     GNDdet NC     GND    GND    3V3    NC
 14     12     10     8      6      4      2      bottom row (even pins)
 NC     nRESET TDI    SWO    SWCLK  SWDIO  NC
```

The red-stripe wire (cable pin 1) lands on the **right end of the top row**. The ground
pattern is asymmetric, which is what makes the orientation unambiguous: on the top row,
continuity to ground reads `x beep x beep beep x x` left to right (measured on this board,
USB up). A cable rotated 180 degrees puts the probe's NRST on the 3V3 pad and its target-voltage
sense on nRESET. nRESET has a 10 K pull-up to 3V3, so that wrong orientation still reads about
3.3 V on the probe's voltage sense. A healthy `VAPP` therefore does not prove the orientation;
the continuity pattern does.

The STDC14 signal assignment above is from memory of ST's UM2910, not re-checked against the
manual. The Seed-side facts (pin 10 is nRESET, the ground pattern) are the measured ones.

**Setting `DBGMCU_CR.TRACECLKEN` kills the debug port on this board; any `--chip STM32H7...`
connect does it.** Measured on this board with an ST-Link V3 (`V3J15M7`), probe-rs 0.32.0 and
OpenOCD 0.12.0. The generic attach `probe-rs info --protocol swd` (no `--chip`; `info` ignores it)
works every time on a freshly power-cycled board: DPv2, STMicroelectronics, part 0x4500, the
core ROM table at `0xe000e000` readable. Any attach that names the chip fails with `SwdDpError`,
after which **even the generic attach fails until the Seed's USB-C is unplugged and replugged**;
RESET, `--connect-under-reset`, and replugging the probe all leave it stuck.

The cause, found by tracing OpenOCD's `examine-end` hook at `-d3` (it writes `DBGMCU_CR` at
`0xE00E1004` through AP2): the hook's writes of the D1/D3 debug clocks (`0x00600000`) and the
sleep/stop/standby debug bits (`0x3F`) succeed, and so do the watchdog freeze registers. The write
that adds **bit 20, `TRACECLKEN`** (`0x0070003F`) returns `STLINK_SWD_DP_ERROR` and the DP is dead.
Writing `0x00100000` alone to `0x5C001004` on a fresh board reproduces it; writing
`0x00200000` or `0x00400000` alone does not. probe-rs's STM32H7 `debug_device_unlock`
(`probe-rs/src/vendor/st/sequences/stm32h7.rs`) sets all of those bits in one write on connect,
and it is selected purely by `chip.name.starts_with("STM32H7")`. Why the trace clock faults here
is unknown. Neither the cable (a RAM write and flash read at 1 MHz are clean), the probe, the
nRESET path (continuity-checked), nor the firmware image (the system bootloader fails the same
way) is the cause. The remedy that is known to work is to not write `TRACECLKEN`: OpenOCD with
its `examine-end` hook cleared reads and writes memory reliably. `defmt` over RTT does not need
the trace clock. For probe-rs, `firmware/asperitas-h750.yaml` is the stock `STM32H750IB` entry
(internal-flash algorithm only) renamed `ASPERITAS_H750IB`, which falls outside the
`STM32H7` prefix match and so gets the default ARM sequence. The `probe-*` Makefile targets use it
through `--chip-description-path`, so the `--chip STM32H750IBKx` strings elsewhere in this document
now read `--chip ASPERITAS_H750IB`. Measured: `probe-rs read b32 0x08000000 4 --chip
ASPERITAS_H750IB --chip-description-path firmware/asperitas-h750.yaml --protocol swd` returns the
vector table on a freshly power-cycled board without wedging the port. Since measured
(TASK-037): flashing with it, RTT, and `--connect-under-reset`, the last only with the patched
probe-rs described under *Under reset* below. Because that sequence never sets `DBGMCU_CR`, firmware
that sleeps in WFI must set the debug-in-sleep bits itself. `Failed to read component information at 0xe000e000` while attached means the core
was not accessible (held in reset or a stuck debug port), not a wiring fault.

The probe does not power the board: with the Seed unpowered, `VAPP` read 0.58 V and attach failed.

**What is and isn't established here.** The commands below are copied from `make -n` output in
`firmware/`, the flag semantics from `probe-rs` 0.32.0's own `--help`, the target behaviour
from `defmt-rtt` 1.3.0 source, and the chip entry from `probe-rs chip info` — all of which are
host-side facts, checkable without a board. What has *not* been established is anything about
this board: attach time, flash time, log throughput, whether a real panic decodes to a
backtrace. Those are TASK-037's measurements and live in its notes. The one board fact that
changed the tooling, why `--connect-under-reset` failed and how the flake fixes it, is under
*Under reset* below.

```bash
cd firmware
DEFMT_LOG=info make probe-flash FEATURES="seed3 log-defmt" NO_DEFAULT=1
make probe-log
```

Expanded (`make -n probe-flash probe-log FEATURES="seed3 log-defmt" NO_DEFAULT=1`, with the
`elf-check` lines that guard `probe-log` left out):

```
cargo build --release --no-default-features --features "seed3 log-defmt" --bin main
probe-rs download target/thumbv7em-none-eabihf/release/main --chip STM32H750IBKx --non-interactive --connect-under-reset --verify --reset
probe-rs attach target/thumbv7em-none-eabihf/release/main --chip STM32H750IBKx --non-interactive
```

There are two more targets. `probe-run` flashes and then stays attached to stream; `probe-rtt-list`
reads the RTT control block, prints the channel table and exits. The names don't
show the difference that matters, so: **`probe-flash` exits when it's done**, which is what an
unattended loop wants, while `probe-run` and `probe-log` hold the attachment and stream until
you interrupt them. `probe-flash`'s `--verify` is genuine read-back verification by probe-rs —
don't port the `File downloaded successfully` grep from the DFU recipe to this path; that idiom
exists only because dfu-util reports failure on success. `probe-log` compiles nothing and resets
nothing — it reads the ELF from the previous build to find the RTT control block — and it
*refuses to run* rather than warning if that ELF was not built from the sources on disk -- it hashes
every input and compares against the digest `make build-elf` recorded beside the image -- because a
decoder that is not your sources mislabels live output into something that looks like data. Don't expect `FEATURES` or
`DEFMT_LOG` on that line to do anything either: the recipe runs no compiler. There is deliberately no
`runner` in `.cargo/config.toml`, even though upstream daisy-embassy ships one: a runner lets
`cargo run` in a script, or an editor action, silently reach for the probe.

`--chip STM32H750IBKx` is the exact string, verified host-side against the pinned toolchain:
`probe-rs chip info STM32H750IBKx` reports NVM `0x08000000..0x08020000` (128 KiB) and AXI SRAM
`0x24000000..0x24080000` (512 KiB), which matches `firmware/memory.x` (re-run 2026-09-12 against
probe-rs-tools 0.32.0). Bare `STM32H750IB`
resolves too; use the full string so it reads as the same part `memory.x` describes.

**Why the probe path takes the ELF and DFU takes `$(BINARY).bin`.** `probe-rs` decodes `defmt`
frames from the ELF's `.defmt` section and its symbol table, and unwinds with its DWARF. Flash
a stripped `.bin` while holding some other ELF on the host and the host's metadata cannot match
what is running — you get frames that decode wrong, or addresses that resolve to the wrong
line, which is worse than no symbols because it looks like data. The `.bin` stays a DFU-only
artifact. Flashing a raw binary over the probe would also need
`--binary-format bin --base-address 0x08000000`, adding one more way to confuse the two.

Three things gate whether a probe command gets anywhere, all three found by running them against
built artifacts rather than at the bench:

- **Only a `log-defmt` build gets past defmt parsing.** `build.rs` passes `-Tdefmt.x` for that
  feature, which consolidates the per-call-site `.defmt_*` sections into the single `.defmt`
  section probe-rs reads. Give `probe-rs attach` or `probe-rs run` a console-only ELF and it is
  refused before the tool so much as looks for a probe, rc=1, quoted verbatim from probe-rs-tools
  0.32.0 on 2026-09-12:

  ```text
  Error: Some uncategorized error occurred.

  Caused by:
      0: Failed to parse defmt data
      1: defmt version found, but no `.defmt` section - check your linker configuration
  ```

  A `log-defmt` ELF parses cleanly and stops at `Error: No connected probes were found.` instead,
  so `make probe-log`, `make probe-rtt-list` and `make probe-run` want `log-defmt` in `FEATURES`
  without exception. `probe-rs download`, which is what `make probe-flash` runs, never reaches the
  defmt parse on an empty bench: it exits 1 on the missing probe whichever ELF it is given, so this
  boardless check separates the two commands rather than the two images.
- **`DEFMT_LOG` decides what's in the stream, at build time.** With the variable unset,
  defmt-macros compiles every non-ERROR call to nothing (`defmt-macros-1.1.1`,
  `src/function_like/log/env_filter.rs:35`), so an attach against a plain
  `make build FEATURES="seed3 log-defmt" NO_DEFAULT=1` image shows a silent channel and reads
  as a broken probe. The facade's runtime `set_max_level(Info)` cannot compensate: the arm
  that would have shipped the record isn't in the binary. Ask for the level you want when you
  build — `DEFMT_LOG=info` above is load-bearing, not decoration.
- **Locations need the release profile at `debug = 2`.** Symbols and unwinding come from any
  DWARF, but the file:line on each decoded `defmt` record does not: at `false`, `1` or
  `line-tables-only`, probe-rs decodes the stream anyway and announces what it left out
  (measured 2026-09-12, probe-rs 0.32.0, against a boardless `log-defmt` ELF):

  ```text
  WARN probe_rs::util::rtt::processing: Insufficient DWARF info; compile your program with `debug = 2` to enable location info.
  Error: No connected probes were found.
  ```

  The `WARN` comes *before* probe discovery, so that ordering is what makes the level checkable
  with no board: the warning present means locations absent, and rc=1 afterwards is just the
  missing probe. `firmware/Cargo.toml`'s `[profile.release]` comment carries the measured cost
  of the four levels and why this one is chosen; if that `WARN` ever reappears, someone lowered
  the level. Check it in one line:

  ```bash
  cd firmware && cargo build --release --no-default-features --features "seed3 log-defmt" \
    && probe-rs attach target/thumbv7em-none-eabihf/release/main --chip STM32H750IBKx --non-interactive --list-rtt
  ```

  Expect only `Error: No connected probes were found.` Anything else about DWARF is a profile
  regression, not a bench problem.

**Four regimes an RTT stream runs in.** The first three are predictable from the mode bit alone; the
fourth is what the mode bit cannot see. `defmt-rtt` inits its up-channel to `NON_BLOCKING_TRIM`
(`src/lib.rs:113`); probe-rs flips it to block-if-full when it attaches, and
`defmt_rtt::in_blocking_mode()` tells firmware which it is.

| Host state | Mode in effect | What happens |
|---|---|---|
| No host attached | `NON_BLOCKING_TRIM` | frames truncated or dropped, silently, with no counter anywhere saying so |
| Attached, host keeping up | block-if-full (set by probe-rs on attach) | nothing lost |
| Attached, host stalled | block-if-full | target spins in the write loop with interrupts disabled — the application freezes, and audio goes first |
| Host detached mid-run | still block-if-full, as far as the target knows | the same freeze as row three, from a far more ordinary cause: someone closed the terminal |

Row three is probe-rs's own warning, verbatim from `probe-rs attach --help`: "if the
application writes within a critical section, using this mode can cause the application to
freeze if the buffer becomes full and is not read by the host". Every `defmt` frame here *is*
written inside `critical_section::acquire()` (`defmt-rtt` `src/lib.rs:168`), against a ~667 µs
audio block deadline (32 samples at 48 kHz). Hence the rule, absolute: **nothing may log from
the audio callback.** Reentrancy is fatal too — the logger panics rather than nesting.

Row four is `defmt-rtt`'s own admission, verbatim from its crate documentation
(`defmt-rtt-1.3.0/src/lib.rs:15-21`):

> `probe-rs` puts RTT into blocking-mode, to avoid losing data.
>
> As an effect this implementation may block forever if `probe-rs` disconnects
> at runtime. This is because the RTT buffer will fill up and writing will
> eventually halt the program execution.
>
> `defmt::flush` would also block forever in that case.

The mechanism is one assumption deep. `host_is_connected()` (`channel.rs:151-154`) decides
attachment from the mode bits alone - its own comment reads "we assume that a host is connected if we
are in blocking-mode. this is what probe-run does." - so a host that set `BLOCK_IF_FULL` and then
vanished leaves the target writing as though someone were still draining, and `Channel::flush`
(`channel.rs:139-149`) returns early only when the mode says non-blocking. Whether probe-rs restores
the flags on detach is **not verified here**, upstream or at the bench, and this section claims
neither direction; TASK-037 is where to measure it, by attaching, streaming, killing the reader, and
reading `_SEGGER_RTT.up_channel.flags` afterwards. Of the two levers below, only the target-side one
reaches this regime: there is no host left to hand `--rtt-channel-mode` to.

If the host is going to be slow on purpose (a laptop resuming, a pipe into `grep` that isn't
draining), say so from the host side instead of hoping: `--rtt-channel-mode no-block-skip`
drops a whole frame when it doesn't fit, `no-block-trim` writes what fits and ignores the rest.
Both are values probe-rs 0.32.0 accepts; `block-if-full` is its default.

The target holds the matching lever, and it has a name: defmt-rtt 1.3.0's **`disable-blocking-mode`**
cargo feature forces the non-blocking write path *even after* probe-rs has set the channel to
BlockIfFull (`src/channel.rs:33` picks `nonblocking_write` ahead of the connection test;
`src/lib.rs:23-24` documents it). It moves rows three and four out of the audio deadline's way - row
four is otherwise unreachable, since a detached host cannot be passed a flag - and pays for it
exactly where this channel earns its keep: attached no longer means lossless, so frames vanish while
you are watching them. Treat it as insurance under consideration, not a default — the standing rule,
nothing logs from the audio callback, is the primary defence, and it costs nothing.

**"Nothing logs except the facade" is not true of the linked image.** Selecting `log-defmt` turns
defmt on across `daisy-embassy`'s whole dependency stack - the `cargo tree -e features -i
defmt@0.3.100 --features seed3,log-defmt` receipt quoted in `firmware/Cargo.toml` is the evidence -
so driver frames are compiled in whether or not we write them. Counted 2026-09-12 on the RTT image:
82 defmt frame symbols, 75 of them tagged `defmt_error`:

```bash
cd firmware && CARGO_TARGET_DIR=$(mktemp -d) cargo nm --release --no-default-features \
  --features "seed3 log-defmt" --bin main -- | grep -c '"package"'
```

The flags are load-bearing, because `cargo nm` re-runs the build: omit `--no-default-features` and
`--features` and it rebuilds and overwrites `target/.../release/main` with the console image, which
links the `#[cfg(not(feature = "log-defmt"))]` no-op logger and contains no `SEGGER` magic at all -
symptoms that read exactly like "RTT is missing from the firmware". Measured contrast: 99 frame
symbols and zero `SEGGER` strings in the console image, 82 and the magic in the RTT one, so the byte
scan (`strings -a <ELF> | grep -c SEGGER`), not the symbol count, is what tells the two images apart.

The build now answers that question about itself, so nobody has to scan for magic again: every ELF
carries a non-allocated `.asp.prov` note section stamped by `firmware/build.rs` naming the exact cfg
set it was compiled for, and `scripts/elf-provenance.sh show <ELF>` prints it. One caveat when you
read that section by hand: pass an explicit output-file argument, as
`rust-objcopy --dump-section .asp.prov=/dev/stdout <ELF> /dev/null` does. Measured on LLVM 22, the
same dump *without* that argument rewrites the input ELF in place - identical bytes, fresh mtime.
Against the timestamp test this repo used until TASK-056 that was enough to defeat the check outright;
against a content digest it is inert, because nothing that compares bytes can be fooled by a rewrite
that changes no bytes. The `/dev/null` guard in `scripts/elf-provenance.sh` stays for the plainer
reason: a read command does not get to rewrite the artifact it is reading.

One site worth knowing by name: `{"package":"embassy-stm32","tag":"defmt_error","data":"Ringbuffer
broken invariants detected!",...}` comes from `embassy-stm32-0.6.0/src/sai/mod.rs:33`, inside
`impl From<ringbuffer::Error> for Error`, which fires only on `ringbuffer::Error::DmaUnsynced` and
returns `Self::Overrun`. `daisy-embassy` reaches it from `src/audio.rs:166-178`'s `start_callback`
`codec.read(...)` / `codec.write(...)` loop, so it runs in **task context, not the audio callback** -
that callback is `FnMut(&[u32], &mut [u32])` and cannot fail - but it is still an ERROR-level frame,
the one level that survives an unset `DEFMT_LOG` (`env_filter.rs:35`), written inside `defmt-rtt`'s
critical section at the moment audio is already going wrong. No filter is proposed for it:
`DEFMT_LOG=off,crate=off` is recorded in TASK-036.03's notes as roughly 300 proc-macro errors, and
finding a path filter that works belongs to whoever actually needs one.

Two erase-and-debug details that are cheap to state and expensive to rediscover:

- **STM32H750xB internal flash is a single 128 KB sector**, so any probe-initiated erase wipes
  the whole application. Harmless while the application is all that lives there, but it rules
  out keeping resident data in internal flash beside the firmware.
- **Sleep modes break RTT discovery** on several STM32 parts: unless `DBG_SLEEP` /
  `DBG_STANDBY` / `DBG_STOP` are set in `DBGMCU_CR`, the RTT control block is never found once
  the core has slept (probe-rs #350). Nothing here sleeps — the embassy executor busy-loops —
  so the busy loop is the safe state and is not to be "optimised" without reading that issue.

**The D-cache is the other way to lose the control block, and it is armed by a future change, not
by anything today.** `_SEGGER_RTT` lands wherever the linker puts `.data`: measured at
`0x24000008` (48 bytes) in a `log-defmt` release image, the first words of AXI SRAM — the same
`0x24000000..0x24080000` region probe-rs's own `STM32H750IBKx` entry reports and `memory.x`
describes. Enable the Cortex-M7 D-cache over that region and the host stops seeing what the core
wrote: the core's writes live in the cache until they're evicted, while the debug port reads go to
the memory system and never consult the cache. Either the `SEGGER RTT` magic string isn't there yet,
so discovery fails, or the block looks present but its write index and ring contents are stale, so
the host attaches to a stream that says nothing. SEGGER's thread 5360 is the symptom record — a block
at `0x24000000` that auto-search missed, that a manually-set address found but got no data from, and
that worked once moved to DTCM at `0x20000000`. Read it for the symptom, not the explanation: that
thread never mentions caching (SEGGER's answer blames AHB reachability and then edits itself), and the
mechanism above is the architecture's, not theirs.

What "put the block where no cache can hold a write" actually takes is longer than a linker tweak.
SEGGER's RTT knowledge-base page, section *Cortex-M specifics*, lists it under "If the CPU implements
caches:" - quoted verbatim, its grammar included:

> - The RTT control block as well as all RTT buffers must start cache line aligned
> - The RTT control block as well as all RTT buffers must be the multiple of a cache line in size
> - In case the system provides multiple cache levels, the alignment and sizes of the control block and
>   buffers must take the cache with the largest line size as the reference point.
> - It is **user application's responsibility** to call a cache clean + invalidate on the RTT control
>   block + all RTT buffers after segment init is complete but before RTT is used for the first time.
> - In the application, the RTT control block, buffers and pointers to their names must be linked with
>   virtual address == physical address
> - The application must provide a uncached address alias to the memory where the control block +
>   buffers are located in.

Measured against that list on the `log-defmt` release `main` image on 2026-09-12 - these two numbers
belong to that binary at this commit, other binaries land elsewhere, and this is the command:

```bash
cd firmware && CARGO_TARGET_DIR=$(mktemp -d) cargo nm --release --no-default-features \
  --features "seed3 log-defmt" --bin main -- | grep -Ei "SEGGER_RTT|defmt_rtt.*BUFFER"
```

The temporary target dir earns its place: `cargo nm` re-runs the build, so without it this line
replaces the ELF sitting in `target/` with the image built by whatever flags you happened to type,
which is the trap spelled out under *"Nothing logs except the facade" is not true of the linked
image*. Re-running the command as written reproduces both addresses - on 2026-09-12 and again on
2026-09-13 after TASK-059 moved `.sram1_bss` from directly after `.data` to just below `.uninit`.
That move pushed everything it sat above down by 1 KiB and pulled `.uninit` up to fill the hole, so
these two objects happen to land where they always did; an object in `.bss` does not, which is why
this table is a measurement and not a derivation.

| Object | Address | Size | Output section | Bytes into its 32-byte cache line |
|---|---|---|---|---|
| `_SEGGER_RTT` | `0x24000008` | 48 (`0x30`) | `.data`, which starts at `0x24000000` | 8 |
| `defmt_rtt::BUFFER` | `0x240010e4` | 1024 (`0x400`) | `.uninit`: the ring itself | 4 |

Add `--print-size` to that command for the sizes. Read against SEGGER's first two bullets the layout
fails on both counts differently: the control block is 48 bytes, not a multiple of 32, and runs
`0x24000008..0x24000038`, so it straddles the line at `0x24000020` and shares lines with unrelated
`.data`; the ring's 1 KiB *is* a multiple of a cache line but starts 4 bytes into its first one.
Neither starts cache-line aligned. It gets away with that only because the cache is off.

`defmt-rtt` offers no handle on any of it: 1.3.0 ships exactly two features,
`disable-blocking-mode` and `drop-on-contention`, and neither touches alignment or placement. The
nearest hook is that the ring is emitted into a named input section, `.uninit.defmt-rtt.BUFFER`, and
the channel name into `.data.defmt-rtt.NAME` (`src/lib.rs:128-139`) - a linker script can select
those, which is what makes an upstream align-and-section patch a plausible route rather than a fork.
The wrinkle that argues against the naive version of either route: the two objects sit ~4 KB apart
with unrelated data in between (`.data` at `0x24000000`, `.bss` at `0x240005d0`, `.uninit` at
`0x240010e4`), so one MPU window made non-cacheable to cover both makes those neighbours uncached
too, and partial-line sharing defeats invalidate-by-address on an M7 regardless. Two credible routes
survive that: an MPU non-cacheable window over the start of AXI SRAM, or the upstream patch.

One trap on the option people reach for first, hand-moving the block to DTCM: a `NOLOAD` section
placed naively outside DTCM drags `__ebss` across an unmapped gap and bus-faults before `main`, which
passes simulation and fails only on silicon - daisy-rs documents exactly this on the same H750 in
`docs/memory-placement.md`, including that Renode backs the gap and so "passes". Discoverability is
not the constraint on any of these routes: `probe-rs chip info STM32H750IBKx` (run 2026-09-12;
command above) lists DTCM `0x20000000..0x20020000` among the RAM regions, so a relocated block stays
findable without an auto-search scan.

So "it is linker work, not a config bit" understates it. Against SEGGER's list, linker work is
necessary and not sufficient - the cache clean + invalidate and the uncached alias are application
work too. Implementing either route is **not scheduled, and deliberately not filed**: caching is off
today, as the verification directly below records, and the day TASK-038's SDRAM/DSP work wants it on
is the day that ticket plans this.

Latent, not present: nothing enables I- or D-cache anywhere in this stack. Verified locally — no
cache or MPU call in embassy-stm32 0.6.0's `src/`, none in daisy-embassy `ca9bcc9`'s boot path, none
in cortex-m-rt's startup, and no cache-related symbol in the linked image. The near miss worth
naming is daisy-embassy's SDRAM builder (`sdram.rs:16`), which switches on the MPU with a cacheable
region over the SDRAM window; nothing here calls it, and an MPU region is not the D-cache, so even
that leaves RTT alone. Whoever enables caching for DSP headroom is the one who breaks RTT silently,
and will not suspect the cache. TASK-038.03 reached the same caches-off finding independently, from
the SDRAM-coherence side.

**Under reset only works with the flake's patched probe-rs.** Measured 2026-10-07: stock probe-rs
0.32.0 fails every `--connect-under-reset` attach on this board with `Unable to wait for 0 halted` /
`Timeout while attaching to target under reset`, while a plain attach always works. The cause is
probe-rs, not the reset net or the probe. Since probe-rs #3485 it arms reset vector catch
(`DEMCR.VC_CORERESET`), releases nRESET, and only then sets `DHCSR.C_DEBUGEN`. Vector catch fires
only if C_DEBUGEN is already set as the core leaves reset, so the core runs past it. C_DEBUGEN
survives nRESET and is cleared only by power-on, which is why an OpenOCD session (which leaves it
set) let exactly one probe-rs attach through. The stock ST sequences leave it set after a failed
attach, so stock chip names fail only once after power-up (probe-rs #4113). Our renamed chip gets
`DefaultArmSequence`, whose session teardown writes `DHCSR = 0`, so it failed every time.
`nix/probe-rs-cortex-m-reset-catch.patch`, applied to `probe-rs-tools` in `flake.nix`, sets
C_DEBUGEN while reset is still held. With it, 5 of 5 reads and 3 of 3 `make probe-flash` runs passed
under reset, against 0 of 4 stock runs interleaved with them. Full trace and the controlled test:
[docs/upstream/probe-rs-connect-under-reset.md](../upstream/probe-rs-connect-under-reset.md). Drop
the patch when upstream ships a fix. The #3516 discussion below still describes the probe class in
general, but it was not what failed here.

**Under reset, an ST-Link gets a plain reset, not a chip-specific one.** `probe-flash` passes
`--connect-under-reset`, which holds nRESET low across attach and is what takes the BOOT/RESET
handshake out of the loop. On any ST-Link it does *not* play the target's custom reset sequence:
probe-rs 0.32 runs that sequence only when the probe exposes a DAP interface (`session.rs:242-254`)
and the native ST-Link driver doesn't (`stlink/mod.rs:1404-1410` return `None`), so the attach falls
back and logs two `INFO` lines from `probe_rs::session` — quoted from #3516's own debug log, where
the interpolated name is the probe's:

```
Custom reset sequences are not supported on ST-Link V3.
Falling back to standard probe reset.
```

What replaces the sequence is the probe's generic reset-pin drive — for ST-Link, a
`JTAG_DRIVE_NRST_LOW` command (`stlink/mod.rs:246`) — so the flag still means "hold the chip in reset
while I attach", just with less chip knowledge behind it. Two things follow. First, this is not a
V3-only quirk: it is the ST-Link driver, so an ST-Link V2 on the same pads takes the same fallback,
and the sentence above should be read as "any ST-Link behind probe-rs's native driver". Second, the
family-specific work is *not* all lost — H7's DBGMCU debug-component enable goes through the memory
interface (`vendor/st/sequences/stm32h7.rs:63`, called from `session.rs:284`), which the fallback
leaves alone. (That file is `stm32h7.rs`; there is no `stm32cm7.rs` in 0.32.)

So what remains is electrical: whether the probe actually pulls this board's nRESET net down far
enough, long enough. That is the shape of probe-rs #3516, and reading it as "one reporter's bad reset
circuit" oversells it and undersells it at once. Oversells it because their board really was the
problem — an oscilloscope trace showed the ST-Link's nRESET output partly fighting a MIC6315 reset
supervisor and a 74-series buffer, and revising that circuit is what finally made
`--connect-under-reset` work; CubeProgrammer had managed all along. Undersells it because the thread
is still open and gained a second report in 2026-04 on different hardware — STLINK-V3MINIE against
several STM32U5 parts, `cubeprogrammer-cli` working flawlessly on the same bench, failures
intermittent — so the probe class is a live suspect even on a board nobody has modified.

Practically: **"flashes fine without the flag, fails with it" is an expected outcome of this probe
class, not evidence that the bench is broken.** probe-rs's FAQ puts the remedy in one line — "Make
sure you try with and without the connect-under-reset argument. Some chips need it and others don't
support it at all." Run both ways without editing anything, since `PROBE_EXTRA` can only add flags:

```bash
cd firmware
make probe-flash UNDER_RESET=0 FEATURES="seed3 log-defmt" NO_DEFAULT=1
```

Expanded (`make -n probe-flash UNDER_RESET=0 FEATURES="seed3 log-defmt" NO_DEFAULT=1`):

```
cargo build --release --no-default-features --features "seed3 log-defmt" --bin main
probe-rs download target/thumbv7em-none-eabihf/release/main --chip STM32H750IBKx --non-interactive  --verify --reset
```

`UNDER_RESET=0` drops the flag from `probe-flash` and `probe-run` only; the default expansion is
unchanged. Other options stay open: `PROBE_EXTRA="--speed 1000"` slows the SWD clock, and
`make probe-log` sidesteps the question entirely by attaching to an already-flashed board with no
reset at all. TASK-037 records which of those worked here.

**What actually hangs off nRESET.** The question that decides how much the paragraph above matters to
this board is whether the Seed drives nRESET through a reset supervisor or a logic buffer, or just a
pull-up plus the front-panel button. Part of the answer is a dead end worth recording: **there is no
Seed3 schematic to read.** Electrosmith publishes a databrief, a pinout PDF and CSV, 3D models and a
compliance zip for the Seed3 and nothing else; every plausible schematic key against their CDN returns
the bucket's key-absent response, and archived copies of the documentation page list the same five
assets, so a drawing was never published and quietly withdrawn. The databrief carries no reset content
either — extraction finds `TAC5242` once and `STM32H750`, and zero occurrences of `RESET`, `NRST` or
`supervisor` in its 38 pages. The full search, including the leads that came up empty, is kept in
[seed3-schematic-search-log.md](./seed3-schematic-search-log.md) so nobody re-runs it.

So the net gets traced on the drawings that *are* public, each cited by the revision and date printed
in its own title block. All four name the net `RESET`:

| Drawing | Sheet date | Everything drawn on the `RESET` net |
|---|---|---|
| `ES_Daisy_Seed_Rev4.pdf` — full, 4 sheets | 2020-06-08 | `R19` 10 K → `+3V3_D`; `S3` PTS815 button → GND; MCU `NRST`; `P6` mini-JTAG header pin 10. No capacitor. |
| `ES_Daisy_Seed_Rev7.pdf` — reduced | 2024-02-01 | The same three (`R19` 10 K, `S3` PTS815, `P6` pin 10) **plus `C32` 100 nF → GND**. |
| `ES_Daisy_Seed2_DFM_Rev5-REDUCED.pdf` | 2025-02-17 | `R1` 10 K → `+3V3_D`; `S1` button → GND; `P8` `M05X2MINIJTAG` pin 10; MCU `NRST`. No capacitor. |
| `ES_Daisy_Patch_SM_Schematic.pdf` | 2024-02-08 | `R1` 10 K → `+3V3_D`; `S1` button → GND; `P8` pin 10. |

In every one of them the MCU's `NRST` pin runs straight to that net name with nothing drawn in between,
and the debug header's pin 10 — standard Cortex Debug nRESET — is the same net as the front-panel
button. Two traps along the way. The tokens printed on the `NRST` wire beside the BGA symbol (`J1`, with
`C6` and `D6` on the neighbouring `PDR_ON` and `BOOT0`) are UFBGA-169 ball coordinates, not jumpers or
components; a `J`-looking designator on a reset wire here means "ball J1", not "jumper". And the Pod
adds nothing: the Daisy Pod schematic (2022-10-27) contains no reset net at all, so doing this bench
work with the module seated in a Pod changes none of the analysis below.

Electrically that is the benign case. #3516's MIC6315 was a second *driver* on the line, actively
fighting the probe, with a 74-series buffer behind it that the STM32 input can't tolerate being driven
against; revising that circuit is what made under-reset work there. Here the probe's nRESET driver has
to sink nothing but the pull-up — 3.3 V across 10 KΩ is ≈0.33 mA — plus one RC time constant while it
discharges the 100 nF Rev7 cap through that same 10 KΩ (τ ≈ 1 µs, and the fall is the probe's own drive,
not a resistor divider). There is no second driver to fight and no series element keeping the probe from
winning.

**Plausible or ruled out?** On the circuit that is actually drawn, ruled out: none of these boards has a
supervisor, buffer, gate, diode or transistor on nRESET, in any of four drawings spanning five years.
On the Seed3 itself the finding is an inference, not a measurement, and two things limit it. The Seed3
drawing is unpublished, and three of the four sheets above are explicitly *reduced* — Electrosmith's
public sheets omit parts on purpose, so the absence of a supervisor there is weaker evidence than the
presence of the 10 KΩ, the button and the capacitor. Against that, the Seed3 is documented as the same
MCU in the same footprint with a new codec and a new USB connector, and the reset net belongs to the
part nobody says changed. So: **treat `--connect-under-reset` as likely to work on this board, not as
known to work.** If it fails anyway, suspect the probe side first — V3 firmware below 3.2, or a V3MINIE
still enumerating in MassStorage mode — before suspecting a supervisor that no Seed has ever had.

That does not retire the try-both-ways advice above. probe-rs's FAQ recommends it regardless of circuit,
because some parts simply don't support being attached under reset; a clean bill of electrical health
for the reset net removes one suspect, not the whole class.

#### Running the probe path unattended

Everything above assumes a person types the command and watches the terminal. A driver that isn't a
person needs more of it said out loud: what it must never block on, what success looks like in
numbers, how to capture output with no TTY, which levers exist when programming itself goes wrong,
how to name one probe when the bench has several, and which command is cheap enough to run first.
All of it below was measured against the pinned `probe-rs-tools` 0.32.0 **with nothing
attached to the bench**, so every line is reproducible here without hardware. None of it is a claim
about this board; that stays TASK-037's.

**No prompts, ever.** Every probe recipe passes `--non-interactive` ("Disable interactive probe
selection", env `PROBE_RS_NON_INTERACTIVE`). Without it, probe-rs asks the terminal which probe to
use the moment a second one is present, and an unattended run waits forever on a question nobody is
there to answer. Today exactly one probe exists so nobody has seen the prompt; the flag is there for
the day that stops being true. It is unconditional rather than a knob because prompting is never
useful to a scripted driver, and someone who does want a particular probe names it up front
(`--probe`, below) instead of answering a dialog. `probe-rs list` rejects the flag outright
(`error: unexpected argument '--non-interactive' found`, rc=2), which is why the flag appears in the
recipes and nowhere else.

**Exit codes, measured with nothing attached:**

| Command | probe-rs rc | `make` rc | stderr |
|---|---|---|---|
| `probe-rs download … --non-interactive` (what `make probe-flash` runs) | 1 | 2 | `Error: No connected probes were found.` |
| `probe-rs attach … --non-interactive --list-rtt` (the boardless DWARF check; `make probe-rtt-list` no longer runs this) | 1 | 2 | the same string alone. A ` WARN probe_rs::util::rtt::processing: Insufficient DWARF info; compile your program with `debug = 2` to enable location info.` line one row above it means the release profile has dropped below `debug = 2`, so locations are off: see the third gate under *Flashing and logging over an ST-Link probe*. Measured both ways on 2026-09-12. |
| `make probe-rtt-list` (`scripts/probe-rtt-list.sh`, two `probe-rs read`s) | 1 | 2 | `probe-rtt-list: probe-rs could not read 0x<addr>; see its error above.` after probe-rs's own error. Measured only with an unreadable chip description (exit 1), not with no probe attached; with a board it prints the control block and exits 0 in about a quarter of a second. |
| `probe-rs list` | 0 | n/a | `No debug probes were found.` |
| `make probe-log` or `make probe-rtt-list` with an ELF that is not its sources | never runs | 2 | `<ELF> was not built from the sources on disk`, then the two digests it compared (`stamp <hex>`, `sources <hex>`), then two advisory lines (flash the current build; force a real relink with `touch src/bin/<binary>.rs && make build-elf BINARY=<binary> FEATURES='<features>' NO_DEFAULT=1`), and no probe-rs output whatsoever. Measured 2026-09-14 by appending one line to `src/bin/main.rs` and running the target without rebuilding |
| `make probe-log` or `make probe-rtt-list` with an ELF that has no input stamp beside it | never runs | 2 | `no stamp at <.../release/<binary>.elf-inputs.sha256>, so nothing here records which sources <ELF> came from`, then the same two advisory lines and the same `Force a real relink: …`. Reached by any ELF linked before TASK-056 landed and by every tree since `cargo clean` - the absence of the record is treated as unknowable, never as fresh. Measured 2026-09-14 by moving the stamp file aside |
| `make probe-log` or `make probe-rtt-list` with an ELF from the other cfg set | never runs | 2 | `that ELF was built for a different cfg set than you are asking to decode with, so its symbols and defmt metadata describe a binary that is not on the board.`, then the same `Force a real relink: …` line. Provenance is asked before the content test, so this fires even when the ELF is perfectly current with your sources. Measured 2026-09-13 by building console, then RTT-only, then running `make elf-check FEATURES="seed3"` on the result |

| `make probe-log` or `make probe-rtt-list` with no ELF on disk at all | never runs | 2 | `no <ELF> - run 'make build-elf' with the FEATURES and NO_DEFAULT you mean to flash`, plus make's own `*** [Makefile:<line>: elf-check] Error 1` below it. Measured 2026-09-12 by pointing the recipe at a binary that was never built: `make probe-log BINARY=nope-not-built` |

Two layers of exit code, because `make` turns any nonzero recipe status into its own 2. A driver
that shells out to `make` therefore sees rc=2 for "no probe", for "wrong cfg set", for "not built from
these sources", for "no stamp beside the ELF" and for "no ELF": only the stderr separates them, so
match on the message rather than the number, and expect that list to grow whenever `elf-check` learns
another question. Calling `probe-rs` directly removes the ambiguity, but not the last case: without an
ELF to hand it there is nothing to call it on.

Every refusal that names a fix names a relink that really links, and since TASK-056 `build-elf` forces
one on itself: when the stamp disagrees with the sources it touches its own main source file before
invoking cargo, because cargo decides whether to link from mtimes and can decline to do anything at
all. Measured 2026-09-14 on a source whose bytes had changed while its mtime had been set backwards:
"Finished in 0.29s" and an ELF still holding the old code. The printed remedy leads with its own
`touch` anyway, as the belt to that pair of braces - it is a command whose effect was measured (0.69 s
warm, producing bytes identical to the ones already there) rather than an inference from how the
recipe works inside.

Why none of these remedies is just "delete it and rebuild" is worth keeping, because it survived the
change of mechanism as a different kind of fact. Deleting the top-level name (`rm -f $(ELF) && make
build-elf`) leaves cargo's per-cfg artifact in `target/.../release/deps/` in place, so the next build
considers the unit fresh and hardlinks those same bytes back up: measured 2026-09-13, sha256 unchanged
after delete-and-rebuild. Against the timestamp test that kept `elf-check` red however many times it
was retried; against the content digest it goes green again, which is the better result of the two - it
means the check is asking about the bytes, and a rebuild that produces the same bytes agrees with it.

Two different strings for two different questions, and they are not interchangeable evidence.
`list` reports what enumeration found and exits 0 whether or not anything answered; `download` and
`attach` exit 1 because they cannot do their job without a probe. Read them in that order: rc=1 from
`download` plus an empty `list` means the host sees no probe at all (cable, udev rules, a V3 still
counting as MassStorage), while rc=1 with any other message means it found one and something else
failed. And unlike dfu-util's 74 (*Notes* under *Flashing the Seed3*), rc here means what it says:
nonzero failed, zero worked. Until TASK-054 the `attach` row also carried a `WARN` line naming
the DWARF level; `[profile.release] debug = 2` in `firmware/Cargo.toml` silenced it, so if that
line ever reappears somebody lowered the level. It is quoted above only so that whatever matches
on stderr knows what to match.

**Capturing without a TTY.** No new target; it is `probe-log` with flags composed through
`PROBE_EXTRA`. The ELF has to already exist and match the image on the board, because `probe-log`
builds nothing and refuses if the ELF was not built from the sources on disk.

```bash
cd firmware
DEFMT_LOG=info make probe-flash FEATURES="seed3 log-defmt" NO_DEFAULT=1  # build and program once
make probe-log FEATURES="seed3 log-defmt" NO_DEFAULT=1 \
  PROBE_EXTRA="--no-timestamps --log-format oneline --target-output-file defmt=out.txt"
```

Expanded (`make -n probe-log FEATURES="seed3 log-defmt" NO_DEFAULT=1 PROBE_EXTRA="--no-timestamps --log-format oneline --target-output-file defmt=out.txt"`, again with the `elf-check` lines that guard `probe-log` left out):

```
probe-rs attach target/thumbv7em-none-eabihf/release/main --chip STM32H750IBKx --non-interactive --no-timestamps --log-format oneline --target-output-file defmt=out.txt
```

`--target-output-file <channel>=<path>` writes probe-rs's *formatted* output for one channel to a
file; repeat it per channel, and prefix a name with `rtt:` or `semihosting:` when a channel name is
ambiguous (`semihosting:stdout`). `--no-timestamps` suppresses the leading timestamps and
`--log-format oneline` picks probe-rs's one-line preset rather than a custom format string. Two
things this is not: it is not probe-rs's own debug log, which is separate (`--log-file <path>`, or
`--log-to-folder` for the default location), and it is not a raw byte capture the way
`cat /dev/ttyACM0 > capture.bin` is for the USB console. A rig that wants bytes off this channel
takes the stdout stream, not that file.

Which flags belong where matters, because `PROBE_EXTRA` composes into whichever recipe it is given:
`--disable-progressbars`, `--disable-double-buffering` and `--read-flasher-rtt` exist on `download`
and `run` only. Passing one to `attach` is a usage error (rc=2), so a `probe-log` composition that
copies a `probe-run` flag set dies before it reaches the bench.

**Levers when programming goes wrong.** All reachable today through `PROBE_EXTRA`, none needing an
edit to the Makefile.

| Flag | On | What it is for |
|---|---|---|
| `--cycle-power` | download, run, attach | Cycles the probe's power before attaching (help: "Whether to cycle usb power before run", env `PROBE_RS_CYCLE_POWER`). For a target left in a state a reset alone doesn't clear. Costs a power-on, so it belongs on the second attempt, not the first. |
| `--read-flasher-rtt` | download, run | Also read the RTT output the flash algorithm emits (help: "Whether to read the RTT output from the flash loader, if available") — for when what needs seeing is the programmer's progress rather than the application's. |
| `--dry-run` | download, run, attach | Env `PROBE_RS_DRY_RUN`; 0.32.0 gives it no help text at all. A string in the binary, `Skipping programming, dry run!`, says it stops short of programming. Whether the erase happens before that point is unmeasured, and an erase here takes the whole application: **not a safe rehearsal until someone measures it.** |
| `--disable-double-buffering` | download, run | Help: "Use this flag to disable double-buffering when downloading flash data. If download fails during programming with timeout errors, try this option". Read with pyOCD #1700 below. Passing it makes probe-rs say so out loud (`Disabled double-buffering support for loader via passed option, though target supports it.`), so you can tell the flag took effect. |

**Why `--verify` is load-bearing, not decorative.** pyOCD issue
[#1700](https://github.com/pyocd/pyOCD/issues/1700), "STM32H750 flash corruption with double
buffering enabled" (opened 2024-06-11, closed as completed 2025-08-13), is the closest published
account of what a bad download looks like on this part. A 30 kB image came back corrupted on roughly
one attempt in five "without any type of sign that something went wrong", the diff showing pages only
partially written with the tail left `0xff`; the reporter traced it to double buffering alone, under
either erase mode, and separately measured the same loader failing "after on average 2 programmings"
against 100 consecutive successes with a different flash algorithm for the same chip. A second
reporter hit the same symptoms programming an STM32H750 **on a Daisy Seed**, with a success rate of
"about one out of three tries" across three different probes, while OpenOCD programmed and verified
the same bench flawlessly. pyOCD's maintainer closed it by disabling double buffering for flash
algorithms by default (v0.38.0), blaming stalls from bus-related effects on devices with strict
timing constraints such as STM32H7xx. Keep the inference the size of the evidence: pyOCD runs its own flash
algorithm loader, so this is about the technique and the chip, not about probe-rs's implementation.
What it does establish is the shape of the failure, silent partial writes, and the complaint
underneath it, that nothing in that tool chain checked the flash afterwards. That is the argument
for `--verify` on `probe-flash`: on a part whose internal flash is one 128 KB sector, a short write
is a board that boots wrong or not at all, and read-back is the only thing standing between that and
a wrong conclusion about the firmware. So when a download times out, pull
`--disable-double-buffering` before suspecting the bench, and don't add a flash recipe that drops
`--verify`.

**More than one probe on the bench.** Pin it rather than let the tool choose: `--probe VID:PID`, or
`VID:PID:Serial` when two probes share a VID:PID; multi-channel FTDI parts (FT2232H) take
`VID:PID-INTERFACE` for the channel, e.g. `--probe 0403:6010-1` is channel B. Environment
equivalents cover everything used here (`PROBE_RS_PROBE`, `PROBE_RS_CHIP`, `PROBE_RS_SPEED`,
`PROBE_RS_CONNECT_UNDER_RESET`, `PROBE_RS_CYCLE_POWER`, `PROBE_RS_NON_INTERACTIVE`). A shared rig is
better served by a `[presets]` entry in a probe-rs config file, selected with `--preset NAME` or
`PROBE_RS_CONFIG_PRESET`, because it names the probe-chip pair once instead of scattering IDs through
scripts. Precedence is probe-rs's own sentence, verbatim and broken in the original: "Manually
specified command line arguments take overwrite presets, but presets take precedence over environment
variables." CLI beats preset beats env, so an environment variable cannot override a preset, only a
command-line flag can.

**First thing to run at a bench: `make probe-rtt-list`.** It reads the RTT control block, prints the channel table,
and exits. It doesn't reflash, so it can't erase the single 128 KB sector, and it doesn't sit there
streaming, so it is as close to a read-only look as SWD allows. When RTT shows nothing for one of the
several reasons it goes quiet (core asleep, D-cache over the control block, wrong ELF, no `.defmt`
section), this is the cheapest question worth asking, from a shell or from a script. With nothing
attached it produces the exit-code contract above, which makes it the way to check that a rig's probe
plumbing works before anything that could take the board down. Like `probe-log` it needs a current
ELF and builds nothing.

### What each channel loses

The device paths keep different books, and a loss figure from one says nothing about the other.
The framed USB console reports what it failed to send: a per-record sequence number the reader
can difference for gaps, plus cumulative `dropped_full`, `bytes_dropped`, `trunc` and `ep_err`
counters carried in band — so a capture that lost records can prove it afterwards, and a capture
that didn't can too. Those fields are specified above under
[*Console protocol v1*](#console-protocol-v1-every-record-verifies-itself) and
[*`BOOT` and `STATUS`*](#boot-and-status-the-reserved-body-prefixes); this section is only about which
ledger is which. RTT keeps no ledger at all. Unattached it discards freely and
says nothing; attached it loses nothing *while the host keeps reading and stays attached*, and
silently stops being the thing under your control the moment the host stalls or goes away. So a clean, gapless RTT stream is
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
