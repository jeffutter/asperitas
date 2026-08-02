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
make flash-all
```

This builds `src/bin/blinky.rs` and flashes it over DFU. The onboard LED (PC7) should blink at ~1.6 Hz. If nothing happens, power-cycle the USB-C cable (`:leave` in dfu-util is unreliable on some STM32H7 devices).

### 2. Flash the main firmware

```bash
cd firmware
make build   # produces firmware.bin
make flash   # flashes via DFU
```

Or combined: `make flash-all`

### Entering DFU Mode

1. Hold **BOOT**
2. Tap **RESET**
3. Release **BOOT**

Verify the board enumerates: `lsusb` should show "STMicroelectronics STM32 bootloader". After flashing, the board may need a power cycle to restart the application.

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
