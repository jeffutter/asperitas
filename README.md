# Asperitas

A musically-interactive audio effect for acoustic and clean electric instruments, built in Rust for the Electro-Smith Daisy Seed3 in a Daisy Pod.

Target instruments: mandolin, octave mandolin, upright bass, bass guitar, jazz guitar. Inspired by Chase Bliss Mood - effects that observe and react to playing dynamics rather than applying static processing.

## Layout

```
asperitas/
├── firmware/            # Embedded firmware (separate workspace, thumbv7em-none-eabihf)
│   ├── src/bin/         # main, blinky, ledtest, panictest, podtest, rig
│   ├── Makefile         # Build, DFU flash and probe targets
│   └── memory-*.x       # Linker layouts: internal flash (default), bootloaded (QSPI image run from AXI SRAM)
├── crates/              # Host workspace members
│   ├── asperitas-dsp/       # Core DSP (hardware-independent, no Daisy or Embassy)
│   ├── asperitas-stimulus/  # Rig stimuli (sine, sweep, pulse), the effect-stage chain and replay selector
│   ├── asperitas-cli/       # Offline WAV-in/WAV-out CLI (process, generate) and golden-file tests
│   ├── asperitas-wire/      # no_std console and control wire protocol, device and host sides
│   ├── asperitas-replay/    # Replay core shared by firmware and host
│   ├── asperitas-logging/   # Device logging runtime (USB console, defmt, panic handler, LEDs)
│   ├── asperitas-pod/       # Pod control surface drivers (knobs, encoder, LEDs)
│   ├── asperitas-host/      # Host tooling for the wire protocol (dump reassembly, excerpt install)
│   ├── asperitas-rig/       # Capture runner for the framed console, with a pass/fail verdict
│   └── asperitas-measure/   # Analysis of rig loopback captures
├── audio/               # Instrument corpus, golden WAVs, committed hardware captures (see audio/README.md)
├── scripts/             # Gate, provenance and bench scripts
├── docs/reference/      # Hardware and tooling reference docs
├── docs/upstream/       # Notes prepared for upstream projects
└── backlog/             # Tickets (Backlog.md)
```

The key design principle: **DSP logic is decoupled from hardware.** `asperitas-dsp` compiles on your laptop so you can iterate on sound without flashing anything. Only when the effect sounds right does it get wired into `firmware/`.

## Dev shell

Nix provides the whole toolchain (Rust, embedded target, dfu-util, cargo-binutils, probe-rs). Start it from the project root:

```bash
nix develop .
```

You also need a Daisy Seed3 in a Daisy Pod and a USB-C cable for flashing and power. The Pod's jacks are line level, not hi-Z instrument level: feed it from a DI box or interface.

## Blinky (confirm the board works)

Put the board in DFU mode (hold BOOT, tap RESET, release BOOT), then:

```bash
cd firmware
make flash-all BINARY=blinky
```

Blinky drives **Pod LED 1**: red for a flicker during pre-init, then steady green once USB is up. A running board enumerates as a CDC serial device (`ls /dev/tty.usbmodem*` on macOS). `BINARY` defaults to `main`, so a bare `make flash-all` flashes the main firmware.

DFU mode is not sticky, the `dfu-util: Error during download get_status` line is expected, and the `LAYOUT=bootloaded` flow, probe flashing and RTT logging are all covered in [`docs/reference/flashing.md`](docs/reference/flashing.md). For a dead-looking board, the panic path and reading console captures, see [`docs/reference/asperitas-rig.md`](docs/reference/asperitas-rig.md).

## DSP iteration loop

No flashing required. Process a recorded WAV through the effect (mono or stereo in, stereo out):

```bash
cargo run --bin asperitas-cli -- process recording.wav processed.wav \
  --processor filter --params cutoff_hz=800
```

Generate a synthetic signal (`impulse`, `sweep` or `pluck`):

```bash
cargo run --bin asperitas-cli -- generate -o pluck.wav -t pluck -d 2.0
```

`audio/instruments/` holds real mandolin and octave-mandolin recordings to run against; [`audio/README.md`](audio/README.md) explains why those clips must not be individually normalized.

Golden-file regression tests freeze a known-good output:

```bash
cargo test                                      # includes golden comparisons within float tolerance
UPDATE_GOLDENS=1 cargo test -p asperitas-cli    # regenerate, deliberately opt-in
```

A golden diff means *listen to this before accepting it*, not *run the update command*.

## Building

```bash
cargo build --workspace                              # host crates
cd firmware && cargo build --release --features seed3   # firmware
```

Hardware status, measurements and open questions live in the tickets under `backlog/`, not here.

## Reference

- [`docs/reference/daisy-seed3.md`](docs/reference/daisy-seed3.md) - Seed3 hardware, SAI config, capture budgets
- [`docs/reference/flashing.md`](docs/reference/flashing.md) - DFU and probe flashing runbook, bootloaded layout
- [`docs/investigations/`](docs/investigations/) - measured evidence behind the flashing runbook
- [`docs/reference/daisy-pod.md`](docs/reference/daisy-pod.md) - Pod pin map, controls, line-level audio warning
- [`docs/reference/asperitas-rig.md`](docs/reference/asperitas-rig.md) - `rig` firmware workflow, capture runner, analysis, console debugging
- [`docs/reference/rust-daisy-stack.md`](docs/reference/rust-daisy-stack.md) - Crate landscape, daisy-embassy status, DSP library options
- [`docs/investigations/seed3-schematic-search-log.md`](docs/investigations/seed3-schematic-search-log.md) - Record of the Seed3 schematic search
- [`docs/upstream/`](docs/upstream/) - Notes prepared for upstream projects
