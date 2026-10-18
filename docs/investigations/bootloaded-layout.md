# The stock Daisy bootloader and the bootloaded link layout

Measured on the Seed3 at the bench. Procedure is in [flashing.md](../reference/flashing.md); this is
the evidence behind it. The stock DaisyBootloader works on the Seed3 for the bootloader, DFU and QSPI
parts. The upstream README lists only Seed, Seed2 DFM and PatchSM, so this is outside its stated
support.

## Bootloader facts

- **Binary:** `dist/dsy_bootloader_v6_4-intdfu-2000ms.bin` from electro-smith/DaisyBootloader (v6.4.0;
  DFU on the onboard USB-C, 2000 ms grace period), 130,188 bytes, SHA-256
  `76296b6bd625e5e5dbeba2431619a0ed90ae2c0a3a99258df00a64a0b8b9ec35`. Fits the 128 KB internal flash.
- **Install** overwrites internal flash; dfu-util reports "File downloaded successfully" then the usual
  `Error during download get_status`.
- **Enumerates** as USB product "Daisy Bootloader" with one DFU interface,
  `@Flash /0x90000000/64*4Kg/0x90040000/60*64Kg/0x90400000/60*64Kg`: the QSPI part, so it drives this
  chip. The app region starts at `0x90040000`.
- **Where it runs the app:** it reads the app's reset vector (word at +4). An address in AXI SRAM
  (`0x24xxxxxx`) makes it copy the image from QSPI into SRAM (about 480 KB to `0x24000000`) and jump;
  one in QSPI (`0x90xxxxxx`) runs in place; anything else is an error. In SRAM mode the app's
  `.data`/`.bss` cannot live in AXI SRAM.
- **Re-entry with an app installed:** after any reset the DFU interface is visible from about 0.7 s for
  the 2 s grace period, then it jumps to the app. No button is needed; `probe-rs reset ...
  --connect-under-reset` followed by `dfu-util -w` reflashed the app.

## The link layout

`firmware/memory-bootloaded.x`: `FLASH = 0x24000000, 480K`, `RAM = DTCM 0x20000000, 128K`, and
`RAM_D2 = 0x30000000, 288K` for the `.sram1_bss` SAI buffers. The initial stack pointer is
`0x20020000` and the reset vector `0x24000299`. `firmware/memory-internal.x` stays the default.

- **Selecting it:** `ASPERITAS_LAYOUT=bootloaded`, or `make build LAYOUT=bootloaded BINARY=<bin>`,
  which writes the binary
  name with a `-bootloaded` suffix (`rig-bootloaded.bin`). The layout is stamped into the ELF's `.asp.prov`, and
  `make elf-check LAYOUT=...` refuses an ELF built for the other one, since both share
  `target/.../release/<bin>`.
- **Why no `memory.x`:** lld resolves cortex-m-rt's `INCLUDE memory.x` against the working directory
  before any `-L` path, so a `memory.x` in `firmware/` silently beat the layout `build.rs` selected (an
  ELF stamped bootloaded that linked for internal flash). The crate root holds no file by that name;
  `build.rs` writes the selected one into `OUT_DIR`.
- **QSPI budget:** the image is stored from QSPI offset `0x40000`; excerpt slots start at `0x100000`
  (`EXCERPT_AREA_START`). `scripts/check-image-load-addresses.sh` parses both.
- **Sizes** (`-O binary`; spare is 491,520 minus image): blinky 78,438, main 107,621, rig 125,762
  (sine), 127,586 (`stim-ess`), 125,986 (`stim-pulse`). `.sram1_bss` at `0x30000000` (both buffers,
  1 KB). The install-path `rig` links at 144,137 (sine), 145,929 (`stim-ess`) and 144,489
  (`stim-pulse`), leaving about 347 KB. The 128 KB single-sector limit does not apply to these builds.
- **Probe path:** the app no longer lives in internal flash, so the probe targets refuse the layout (a
  reset erases AXI SRAM).

## `main`'s console in the bootloaded layout

- **Symptom:** `main` flashed to `0x90040000` ran its audio loop but the host never listed "Asperitas
  Debug Console". Probe reads: CFSR/HFSR 0 (no fault), `LED_STATE` Running, USB33RDY set, OTG_FS
  `GCCFG` powered, `DCTL` 0 (not soft-disconnected), `DSTS` `SUSPSTS=1` (the bus saw a reset, then
  nothing). The peripheral was attached; nothing serviced it.
- **Cause:** `main` polled the SAI callback on the thread executor beside the knob task's blocking ADC
  reads and `Console::run()`. The callback has about 1.3 ms per DMA half-transfer; `main` lost that
  race at boot (`AUDIO_EXIT == 3`, `Overrun`), the audio loop ended, and the halt that followed left
  `Console::run()` never polled, so SETUP requests went unanswered. The core is not faulted in that
  state, which is why the register reads looked healthy. `rig` was unaffected because it ran audio on
  its own `InterruptExecutor`.
- **Fix:** the callback runs on an `InterruptExecutor` on SAI1 (priority P6, below the SAI DMA
  streams), as `rig` does, with the knob ADC brought up before the interface starts. The executor, its
  SAI1 vector, `encode_block` and the start/exit policy live once in the lib's `audio` module
  (`firmware/src/audio.rs`); `main` supplies only a processor to `audio::start`. If the interface
  fails to start, `start` records `AUDIO_EXIT`, sets the LED to `Panicked` and returns, so the console
  keeps being served.
- **Verified:** with bootloaded `main`, `asperitas-rig query` answered with the firmware version and
  `reset_reason=software`, and `override --knob 0 --value 0.5` got `set_acks=1 rel_acks=1
  device_errors=0`. `rig` has no inbound channel, so a reply proves the image is `main`.

## Binaries known to run, by layout

| Binary | Internal layout | Bootloaded layout |
|---|---|---|
| `main` | enumerates | enumerates after the interrupt-executor fix; did not before |
| `rig` | does not fit with the install path | enumerates |
| `ledtest` | not recorded | ran (the Pod RGB LED cycled as from internal flash) |
| `podtest` | host control verified | not run |

Host control for `podtest` was verified on the board (`VER`/`RSN` query and `RST` restart with
re-enumeration and `reset_reason=software`, over the probe-flashed image). `main` and `SET`/`REL` on a
live knob path are not covered by that run.

Rejected alternatives to this layout: [rejected-flash-routes.md](./rejected-flash-routes.md).
