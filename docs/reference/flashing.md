# Flashing the Seed3 - DFU and ST-Link probe runbook

Procedure only. Hardware facts are in [daisy-seed3.md](./daisy-seed3.md); the reasons behind the
rules below are in [docs/investigations/](../investigations/).

Two routes reach the same flash:

- **DFU over the onboard USB-C.** No extra hardware. The everyday path.
- **ST-Link probe on the SWD pads.** Also streams `defmt`/RTT, symbolicates faults from DWARF, and
  is the only way into a board that dies before USB enumerates.

All commands run from `firmware/` inside `nix develop .`, which provides rustc, cargo-binutils,
dfu-util and probe-rs-tools. `lsusb` is not in the shell; use `dfu-util --list`.

## DFU

### Enter DFU mode

Hold `BOOT`, tap `RESET`, release `BOOT`. Check with `dfu-util --list`: one `Found DFU:` line per
device, nothing but the banner when the board is not in DFU mode. DFU mode is not sticky: repeat
BOOT+RESET before every flash, or dfu-util reports `No DFU capable USB device available`.

### Build and flash

```bash
make flash-all BINARY=blinky        # build + flash
make build BINARY=blinky            # writes blinky.bin
make flash BINARY=blinky            # dfu-util -a 0 -s 0x08000000:leave -D blinky.bin
```

- `BINARY` defaults to `main`. Repeat it on both halves of the two-step route: `build` writes
  `$(BINARY).bin` and `flash` reads it, so a bare `make flash` after `make build BINARY=blinky`
  programs `main.bin` with a fully successful transcript.
- Use `make build`; do not hand-type `llvm-objcopy`. Every file-backed section must load in flash
  for a plain `-O binary` image to be right, and `scripts/check-image-load-addresses.sh` checks it.
- **dfu-util exits 74 on a successful `:leave`**: the jump to the application tears down the USB
  connection the final GET_STATUS travelled on, printing `Error during download get_status`. 74 is
  also a genuine I/O error code, so `make flash` keys on `File downloaded successfully` and prints
  `Flashed and started` or `FLASH FAILED`. Trust that line, not the dfu-util noise above it.
- **Size limit:** default `LAYOUT=internal` has one 128 KB sector (131,072 bytes); anything at or
  over cannot fit. `LAYOUT=bootloaded` has 480K (491,520 bytes). The `build` recipe in
  `firmware/Makefile` carries per-binary sizes.

### Known-good diagnostics

- `blinky` drives Pod RGB LED 1: blinks red at about 1 Hz before init, then steady green once USB is
  up. Steady green is success, not a hang. It never touches the Seed's onboard user LED.
- If the LED stays dark, flash `BINARY=ledtest`. It needs only clock init, board init and three GPIO
  pins, and cycles the RGB channels. Motion means the core runs and the fault is downstream.

### Debugging with nothing attached

1. USB CDC-ACM console over the onboard USB-C: flash over DFU, the application enumerates as a serial
   device. Protocol: [protocol.md](./protocol.md).
2. Pod RGB LEDs as a boot-stage indicator: [daisy-pod.md](./daisy-pod.md).

Neither speaks before USB enumerates, which is where boot faults live; that is the probe's job.
With no probe, the first four bytes of the flashed `$(BINARY).bin` are the little-endian initial
stack pointer; treat it as a clue, not a diagnosis. A fault before `main` shows nothing on either
channel. With a probe, a plain attach (no reset) halts the core where it faulted and the fault status
registers say why; attaching under reset clears them but regains control to reflash.

## ST-Link probe

The probe does not power the board: power the Seed over USB-C first.

### Wiring

The 14-pad footprint is two rows of seven; a straight-through 1:1 14-pin ribbon (STDC14 probe end)
is correct. The centre 10 pads carry the old 10-pin Cortex Debug signals; the outer pair at each end
is not connected. With the **USB-C port at the top** and the pad side facing you:

```
              USB-C (top)

 left                                         right
 13     11     9      7      5      3      1      top row (odd pins)
 NC     GNDdet NC     GND    GND    3V3    NC
 14     12     10     8      6      4      2      bottom row (even pins)
 NC     nRESET TDI    SWO    SWCLK  SWDIO  NC
```

Cable pin 1 (red stripe) lands on the **right end of the top row**. Nothing marks pin 1 on the board.
Confirm with continuity to ground on the top row, left to right: `x beep x beep beep x x`. A healthy
`VAPP` does not prove orientation: nRESET has a 10 K pull-up to 3V3, so a cable rotated 180 degrees
still reads about 3.3 V.

### Never name a stock STM32H7 chip

Always use `--chip ASPERITAS_H750IB --chip-description-path asperitas-h750.yaml` (the Makefile's
`CHIP` and `CHIP_DESC`). **Any `--chip STM32H7...` connect, including `STM32H750IB` and
`STM32H750IBKx`, sets `DBGMCU_CR.TRACECLKEN`, kills the debug port, and keeps it dead until the
Seed's USB-C is unplugged and replugged** (RESET, `--connect-under-reset` and replugging the probe do
not help). Generic `probe-rs info --protocol swd` (no `--chip`) is safe. Detail:
[nreset-and-reset-catch.md](../investigations/nreset-and-reset-catch.md).

Because the renamed chip never sets `DBGMCU_CR`, firmware that sleeps in WFI must set the
debug-in-sleep bits itself. `Failed to read component information at 0xe000e000` while attached
means the core was not accessible (held in reset or stuck debug port), not a wiring fault.

### Flash and log

```bash
DEFMT_LOG=info make probe-flash FEATURES="seed3 log-defmt" NO_DEFAULT=1   # build, program, exit
make probe-log                                                            # attach and stream
```

`make -n` shows the exact `probe-rs download ... --connect-under-reset --verify && probe-rs reset`
and `probe-rs attach` lines.

| Target | Behaviour |
|---|---|
| `probe-flash` | Programs with `--verify`, resets, exits. The unattended choice. |
| `probe-run` | Flashes, then stays attached and streams. |
| `probe-log` | Attaches only. Compiles nothing, resets nothing. Reads the ELF from the previous build. |
| `probe-rtt-list` | Reads the RTT control block, prints the channel table, exits. Never reflashes. |

Rules:

- The probe path takes the **ELF**, DFU takes `$(BINARY).bin`. `defmt` frames decode from the ELF's
  `.defmt` section, so the host ELF must be the one on the board.
- `probe-log` and `probe-rtt-list` refuse to run unless the ELF was built from the sources on disk,
  for the same cfg set (digest and provenance in `.asp.prov`; `scripts/elf-provenance.sh show <ELF>`
  prints it). Fix: `touch src/bin/<binary>.rs && make build-elf BINARY=<binary> FEATURES='<features>'
  NO_DEFAULT=1`. `FEATURES` and `DEFMT_LOG` on a `probe-log` line do nothing.
  Cargo decides whether to link from mtimes and can decline to; `build-elf` therefore touches its main
  source itself when the embedded digest disagrees with the sources. Deleting the ELF and rebuilding
  does not force a relink: cargo's per-cfg artifact in `target/.../release/deps/` is still fresh and
  the same bytes are hardlinked back, which the content digest accepts.
- Only a `log-defmt` ELF parses for `attach`/`run`; a console-only ELF fails with `no .defmt section`
  before probe discovery. `make probe-log`, `probe-rtt-list` and `probe-run` always want `log-defmt`.
- **`DEFMT_LOG` is a build-time filter.** Unset, every non-ERROR call compiles to nothing and the
  channel is silent, which reads as a broken probe. Build with the level you want.
- File:line on decoded records needs `[profile.release] debug = 2`. If probe-rs prints
  `Insufficient DWARF info; compile your program with debug = 2`, someone lowered the level. A
  boardless check: `cargo build --release --no-default-features --features "seed3 log-defmt"` then
  `probe-rs attach <ELF> --chip ASPERITAS_H750IB --chip-description-path asperitas-h750.yaml
  --non-interactive --list-rtt` should print only `Error: No connected probes were found.`
- **Nothing logs from the audio callback.** Every `defmt` frame is written inside a critical section;
  with RTT blocking, a stalled or vanished host freezes the application. See
  [rtt-and-dcache.md](../investigations/rtt-and-dcache.md).
- There is deliberately no cargo `runner`: it would let `cargo run` silently reach for the probe.

### Under reset

`probe-flash` and `probe-run` pass `--connect-under-reset` (holds nRESET low across attach, removes
the BOOT/RESET handshake). It needs the flake's probe-rs (built from main, post-0.32.0); stock 0.32.0
fails every under-reset attach on this board with `Unable to wait for 0 halted` or `Timeout while
attaching to target under reset`, while a plain attach works. Details:
[nreset-and-reset-catch.md](../investigations/nreset-and-reset-catch.md) and
[probe-rs-connect-under-reset.md](../upstream/probe-rs-connect-under-reset.md). Move to the released
probe-rs-tools when one ships.

An ST-Link gets a plain probe reset, not a chip-specific one. If a flash works without the flag and
fails with it, that is an expected outcome for this probe class. Try both ways:

```bash
make probe-flash UNDER_RESET=0 FEATURES="seed3 log-defmt" NO_DEFAULT=1
```

`UNDER_RESET=0` drops the flag from `probe-flash` and `probe-run` only. `PROBE_EXTRA="--speed 1000"`
slows the SWD clock. `make probe-log` avoids the question by attaching with no reset. If under reset
fails, suspect the probe first (V3 firmware below 3.2, a V3MINIE still in MassStorage mode).

### Cycle counter after a detach

After `make probe-flash` (which resets and detaches), `DEMCR`, `DWT_CTRL` and `CYCCNT` read 0 and
`rig`'s `CAPSTAT` reports `max_block_us=0 worst_gap_us=0`. That is not a pass; nothing was measured.
Cycle-counter figures are valid only after a RESET-button boot, a power cycle, or with the probe still
attached. A probe memory read halts the core long enough to overrun the SAI ring, so never read memory
during a capture.

## Unattended probe runs

- **No prompts.** Every probe recipe passes `--non-interactive`; without it probe-rs asks which
  probe to use when two are present and waits forever. `probe-rs list` rejects the flag (rc 2).
- **Exit codes** (probe-rs rc; `make` turns any nonzero into 2):

| Situation | probe-rs rc | stderr |
|---|---|---|
| `download`/`attach`, no probe | 1 | `Error: No connected probes were found.` |
| `probe-rs list`, no probe | 0 | `No debug probes were found.` |
| `make probe-rtt-list`, chip unreadable | 1 | `probe-rtt-list: probe-rs could not read 0x<addr>; see its error above.` |
| `probe-log`/`probe-rtt-list`, ELF not from these sources | never runs | `<ELF> was not built from the sources on disk` plus both digests |
| same, ELF with no source digest | never runs | `<ELF> has no source_digest field in its .asp.prov ...` |
| same, ELF from the other cfg set | never runs | `that ELF was built for a different cfg set ...` |
| same, no ELF | never runs | `no <ELF> - run 'make build-elf' ...` |

  Match on the message, not the number: `make` rc 2 covers all of them. Read `list` then `download`:
  rc 1 from `download` plus an empty `list` means the host sees no probe (cable, udev, a V3 counting
  as MassStorage); rc 1 with another message means a probe was found and something else failed.
  Unlike dfu-util's 74, probe-rs rc means what it says.
- **Capture without a TTY.** The ELF must already match the board:

  ```bash
  make probe-log FEATURES="seed3 log-defmt" NO_DEFAULT=1 \
    PROBE_EXTRA="--no-timestamps --log-format oneline --target-output-file defmt=out.txt"
  ```

  `--target-output-file <channel>=<path>` writes probe-rs's formatted output for one channel; prefix
  `rtt:` or `semihosting:` for ambiguous names. It is not a raw byte capture; a rig wanting bytes takes
  stdout. `--disable-progressbars`, `--disable-double-buffering` and `--read-flasher-rtt` exist on
  `download` and `run` only; passing one to `attach` is a usage error (rc 2).
- **Levers when programming goes wrong** (all via `PROBE_EXTRA`):
  - `--cycle-power` (download, run, attach): power-cycle the probe's USB before attaching. Second
    attempt, not first.
  - `--read-flasher-rtt` (download, run): read the flash loader's RTT output.
  - `--disable-double-buffering` (download, run): try when a download times out.
  - `--dry-run`: not a safe rehearsal; whether the erase precedes the skip is unmeasured, and an erase
    takes the whole application.
- **Keep `--verify` on every flash recipe.** STM32H750 flash downloads have been seen to fail silently
  with partial writes; read-back is the only check. Internal flash is one 128 KB sector, so any
  probe-initiated erase wipes the whole application.
- **More than one probe.** Pin it: `--probe VID:PID`, or `VID:PID:Serial` when two share a VID:PID
  (FT2232H channels: `VID:PID-INTERFACE`). Env equivalents: `PROBE_RS_PROBE`, `PROBE_RS_CHIP`,
  `PROBE_RS_SPEED`, `PROBE_RS_CONNECT_UNDER_RESET`, `PROBE_RS_CYCLE_POWER`,
  `PROBE_RS_NON_INTERACTIVE`. A shared rig should use a `[presets]` entry (`--preset NAME` or
  `PROBE_RS_CONFIG_PRESET`). Precedence: CLI flag over preset over environment variable.
- **First command at a bench: `make probe-rtt-list`.** It reads the control block and exits, so it
  cannot erase anything. It needs a current ELF and builds nothing. With nothing attached it produces
  the exit-code contract above, which checks a rig's probe plumbing before anything riskier.

## Which channel loses what

The USB console keeps a loss ledger; RTT keeps none. A loss figure from one says nothing about the
other: see *What each channel loses* in [daisy-seed3.md](./daisy-seed3.md).

## Bootloaded layout (`LAYOUT=bootloaded`)

The app lives in QSPI at `0x90040000` behind the stock Daisy bootloader, giving 480K of image space
instead of 128 KB. Facts and measurements: [bootloaded-layout.md](../investigations/bootloaded-layout.md).

- **Install the bootloader once** (this overwrites internal flash, so reflash the app afterwards):
  enter system DFU, then
  `dfu-util -a 0 -s 0x08000000:leave -D dsy_bootloader_v6_4-intdfu-2000ms.bin`
  (electro-smith/DaisyBootloader v6.4.0, DFU on the onboard USB-C, 2000 ms grace). Fetch with
  `gh api repos/electro-smith/DaisyBootloader/contents/dist/<name> -H "Accept: application/vnd.github.raw"`.
  The board then enumerates as "Daisy Bootloader" with the QSPI flash as its DFU target.
- **Build and flash:** `make flash-all LAYOUT=bootloaded BINARY=<bin>` writes the binary name with a
  `-bootloaded` suffix (`rig-bootloaded.bin`) to `0x90040000` (`dfu-util -a 0 -s 0x90040000:leave`). The layout is stamped into `.asp.prov`, and
  `make elf-check LAYOUT=...` refuses an ELF built for the other layout.
- **Re-enter the bootloader with an app installed:** its DFU interface is visible from about 0.7 s
  after any reset for the 2 s grace period. Tap RESET and flash inside the window (`dfu-util -w`
  waits), or reset with the probe (`probe-rs reset ... --connect-under-reset`).
- **Never `make flash` without `LAYOUT=bootloaded` on a bootloaded board.** It writes to
  `0x08000000` and replaces the bootloader. Recovery: BOOT+RESET and reinstall it.
- **Probe path:** `probe-flash` and `probe-run` refuse `LAYOUT=bootloaded` (the ELF links into AXI
  SRAM, which a reset erases). The probe still reaches internal-layout binaries and can reset the
  board into the bootloader. RTT logging from a bootloaded app is not available through those
  targets.
- **Image budget:** `scripts/check-image-load-addresses.sh` fails if the region or an image reaches
  the excerpt slots at QSPI `0x100000`, if `.sram1_bss` is outside D2, or if less than 2 KB of the
  480K is spare. `scripts/firmware-variants.sh` builds and grades the matrix.
