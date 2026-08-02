# Asperitas

A musically-interactive audio effect for acoustic and clean electric instruments, built in Rust for the Electro-Smith Daisy Seed3 in a Daisy Pod.

Target instruments: mandolin, octave mandolin, upright bass, bass guitar, jazz guitar. Inspired by Chase Bliss Mood — effects that observe and react to playing dynamics rather than applying static processing.

## Architecture

```
asperitas/
├── firmware/          # Embedded firmware (Seed3 binary)
│   ├── src/bin/       # Binaries (blinky, main)
│   ├── Makefile       # Build + DFU flash targets
│   └── memory.x       # Linker memory layout
├── crates/
│   ├── asperitas-dsp/     # Core DSP logic (hardware-independent)
│   ├── asperitas-cli/     # Offline WAV-in/WAV-out CLI + golden-file tests
│   └── asperitas-logging/ # Log facade (USB CDC on device, stderr on host)
└── docs/reference/  # Hardware reference docs
```

The key design principle: **DSP logic is decoupled from hardware.** The `asperitas-dsp` crate has no dependency on Daisy or Embassy — it compiles on your laptop so you can iterate on sound without flashing anything. Only when the effect sounds right does it get wired into `firmware/`.

## Prerequisites

- **Nix** — the dev shell provides the entire toolchain (Rust, embedded target, dfu-util, cargo-binutils, ALSA). No manual installs needed.
- **Daisy Seed3 in a Daisy Pod** — the hardware this runs on.
- **USB-C cable** — for both flashing and power.

Start the dev shell from the project root:

```bash
nix develop .
```

This gives you `rustc`, `cargo`, `dfu-util`, `cargo objcopy`, `probe-rs`, and everything else. See `flake.nix` for details.

## Quick Start

### 1. Blinky (confirm the board works)

```bash
cd firmware
make flash-all BINARY=blinky
```

This builds `src/bin/blinky.rs`, flashes it over DFU, and starts it automatically — no
RESET tap needed.

**What you should see:** blinky drives **Pod LED 1** (the RGB LED on D20/D19/D18 =
PC1/PA6/PA7) — *not* the Seed's onboard user LED, which it never touches. The LED blinks
red at ~1 Hz only during the brief pre-init window, then goes **steady green** once USB
is up. Boot is fast, so in practice you'll see a steady green LED almost immediately. A
steady LED is success, not a hang.

If the LED behaviour looks inverted (green when you expect off), that's the unresolved
polarity question — `LED_ACTIVE_LOW` in `crates/asperitas-logging/src/led.rs` is
currently a guess pending TASK-006.02. See `docs/reference/daisy-pod.md`.

The LED-independent check is USB: a running board enumerates as a CDC serial device, so
`ls /dev/tty.usbmodem*` (macOS) or `lsusb` should show it as a serial device rather than
"STM32 bootloader".

### If the board looks completely dead

A dark Pod LED is ambiguous — `led::init` leaves the LED off, and the blink task
doesn't start until the end of boot, so any hang before that point looks identical to
a board that never started. `src/bin/ledtest.rs` removes the ambiguity:

```bash
make flash-all BINARY=ledtest
```

It depends on nothing but clock init, board init, and three GPIO pins — no USB, no
logging, no LED singleton — and cycles the RGB channels so that *something* visibly
changes under either polarity. Any motion means the core is running and the fault is
downstream; a dark LED through the whole cycle means it isn't.

### 2. Flash the main firmware

```bash
cd firmware
make flash-all
```

`BINARY` defaults to `main`, so `make flash-all` with no override builds and flashes
`src/bin/main.rs`. To do it in two steps — for example to build now and flash once the
board is in DFU mode:

```bash
make build   # produces firmware.bin
make flash   # flashes firmware.bin via DFU
```

### Entering DFU Mode

1. Hold **BOOT**
2. Tap **RESET**
3. Release **BOOT**

Verify the board enumerates: `lsusb` should show "STMicroelectronics STM32 bootloader".

**DFU mode is not sticky.** Once a flashed app boots, the bootloader is gone — you must
repeat the button dance before *every* flash. Running `make flash` against a board
that's busy running your app fails with `No DFU capable USB device available`.

### Leaving DFU Mode

Automatic. `make flash` passes `:leave`, so the bootloader jumps straight to the
application.

You will still see `dfu-util: Error during download get_status` on a completely
successful flash. That's expected and harmless: the jump tears down the USB connection
the pending status request was travelling on, so nothing is left to answer it. The
Makefile ignores dfu-util's exit code and keys on its `File downloaded successfully`
marker instead — it prints `Flashed and started` on success and `FLASH FAILED` on a real
error, so trust that line rather than the dfu-util noise above it.

If the app doesn't start, power-cycle the USB-C cable.

## Developing DSP Logic

Iterate on your laptop — no flashing required until you're ready to test on hardware.

### Live desktop testing

Plug an instrument into your audio interface, then run the CLI:

```bash
cargo run --bin asperitas-cli -- live --processor <name>
```

This uses `cpal` for real-time audio I/O. Tweak parameters in code, rerun — no flash cycle.

### Offline batch processing

Process a recorded WAV file through the effect:

```bash
cargo run --bin asperitas-cli -- process --input recording.wav --output processed.wav --processor <name>
```

Useful for comparing parameter sets on the same source material without re-playing.

### Golden-file regression tests

Freeze a known-good output WAV, then assert future changes don't silently alter it:

```bash
cargo test  # includes golden-file comparisons within float tolerance
```

## Debugging Without a Probe

No ST-Link? You still have two channels:

1. **USB CDC-ACM serial** — the firmware enumerates as a serial device after booting. Connect with `screen /dev/ttyACM0 115200` (or your terminal program of choice) to see `defmt` log output.
2. **Pod RGB LEDs** — the firmware uses LED color/pattern to indicate boot stage. Check `docs/reference/daisy-pod.md` for the pin map and polarity notes.

When a probe arrives, `probe-rs` restores `cargo run`-style flashing and RTT logging.

## Important Hardware Gotchas

### Pod audio is line level

The Daisy Pod's 3.5 mm jacks are **line level**, not hi-Z instrument level. Plugging a passive pickup or piezo directly in will produce thin, noisy, low-level audio that reads like a DSP bug. Feed the Pod from a DI box, preamp, or audio interface during development.

### Block size is 32 samples

`daisy-embassy` hardcodes `BLOCK_LENGTH = 32` (not libDaisy's default of 48). This is fixed unless you patch the vendored fork. For most effects this is fine; it matters more for tight feedback/delay paths where latency is critical.

### Codec is hardware-strapped

The TAC5242 codec on Seed3 is configured by board straps, not I²C registers. Firmware only configures the SAI peripheral — there is no codec init sequence to port. See `docs/reference/daisy-seed3.md` for SAI details.

## Reference Documentation

- **`docs/reference/daisy-seed3.md`** — Seed3 hardware, SAI config, DFU flashing, why libDaisy C++ isn't an option yet
- **`docs/reference/daisy-pod.md`** — Pod pin map, controls, line-level audio warning
- **`docs/reference/rust-daisy-stack.md`** — Crate landscape, daisy-embassy status, DSP library options

## Building for Other Targets

The workspace has two halves:

```bash
# Host crates (can cross-compile anywhere)
cargo build -p asperitas-cli -p asperitas-dsp

# Firmware (embedded target)
cd firmware && cargo build --release --features seed3
```

Firmware target: `thumbv7em-none-eabihf` (Cortex-M7F, hard float). Binary size must stay under 128 KB (internal flash limit for DFU without the Daisy bootloader).
