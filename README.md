# Asperitas

A musically-interactive audio effect for acoustic and clean electric instruments, built in Rust for the Electro-Smith Daisy Seed3 in a Daisy Pod.

Target instruments: mandolin, octave mandolin, upright bass, bass guitar, jazz guitar. Inspired by Chase Bliss Mood — effects that observe and react to playing dynamics rather than applying static processing.

## Architecture

```
asperitas/
├── firmware/          # Embedded firmware (Seed3 binary)
│   ├── src/bin/       # Binaries (main, blinky, ledtest, panictest)
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
PC1/PA6/PA7) — *not* the Seed's onboard user LED, which it never touches. The LED is
**red** during the pre-init window, then goes **steady green** once USB is up. Boot takes
milliseconds, so in practice the red is a flicker and you'll see steady green almost
immediately. A steady LED is success, not a hang.

The LED-independent check is USB: a running board enumerates as a CDC serial device, so
`ls /dev/tty.usbmodem*` (macOS) or `lsusb` should show it as a serial device rather than
"STM32 bootloader".

LED polarity is settled — the Pod's LEDs are **active-low**, verified on hardware, and
`LED_ACTIVE_LOW = true` in `crates/asperitas-logging/src/led.rs` is correct. If colours
read as their complement (cyan where you expect red), your board differs from the one
characterised in `docs/reference/daisy-pod.md`.

### If the board looks completely dead

A dark Pod LED means the fault is *before* `led::init` — that call lights red on its way
out, so anything after it has a lit LED regardless of whether the executor ever runs.
`src/bin/ledtest.rs` narrows it down further:

```bash
make flash-all BINARY=ledtest
```

It depends on nothing but clock init, board init, and three GPIO pins — no USB, no
logging, no LED singleton, not even the polarity constant — and cycles the RGB channels so
that *something* visibly changes under either polarity. Any motion means the core is
running and the fault is downstream; a dark LED through the whole cycle means it isn't.

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
cargo run --bin asperitas-cli -- process recording.wav processed.wav \
  --processor filter --params cutoff_hz=800
```

Useful for comparing parameter sets on the same source material without re-playing. Mono
or stereo input is accepted; output is always stereo.

`audio/instruments/` holds a corpus of real mandolin and octave-mandolin recordings to run
against — soft and hard plucks, fast runs, and chords. See [`audio/README.md`](audio/README.md),
which also explains why those clips must not be individually normalized.

### Golden-file regression tests

Freeze a known-good output WAV, then assert future changes don't silently alter it:

```bash
cargo test                                      # includes golden comparisons within float tolerance
UPDATE_GOLDENS=1 cargo test -p asperitas-cli    # regenerate, deliberately opt-in
```

A golden diff means *listen to this before accepting it*, not *run the update command*.

## Debugging

No ST-Link? You still have two channels:

1. **USB CDC-ACM serial** — the firmware enumerates as a serial device after booting. Connect with `screen /dev/ttyACM0 115200` (or your terminal program of choice) to see log output. The baud rate in that command is decorative — CDC-ACM has no UART to configure, and neither this firmware nor embassy-usb applies the host's line coding to the hardware.
2. **Pod RGB LEDs** — the firmware uses LED colour to indicate boot stage: red = pre-init, green = running, red = panicked. Check `docs/reference/daisy-pod.md` for the pin map and polarity notes.

Red means both "starting up" and "panicked", which is unambiguous in context — a panic
follows green — but see `slow-boot` below if you need to watch the boot stages closely.

Every line on that serial channel carries its own framing: `~<level> <seq> <t_ms> <body>*<crc>`.
It stays readable by eye and greppable in a raw capture — that legibility is why the protocol is
printable ASCII rather than COBS. The first line off a fresh boot is `BOOT proto=1 …`. `STATUS …`
appears at most once a second and only when a counter moved, so a quiet stream usually means nothing
changed rather than nothing working. Grammar, checksum parameters and what each loss counter means:
`docs/reference/daisy-seed3.md` → *Console protocol v1*.

### Watching the boot stages

Normal boot reaches green in milliseconds, so red → green is a single flicker. The
`slow-boot` feature holds the pre-init stage for ~3 s:

```bash
make flash-all FEATURES="seed3 slow-boot"
```

Bring-up only — don't leave it enabled.

### Checking the panic path

A panic reports through both channels, and `src/bin/panictest.rs` panics on purpose so
you can confirm it:

```bash
make flash-all BINARY=panictest
screen /dev/cu.usbmodem<N> 115200     # attach within the 10 s countdown
```

Expect green with `panictest: panicking in N...` counting down, then steady red plus the panic line.
Both now travel framed, so the countdown reads `~I <seq> <t_ms> panictest: panicking in N...*<crc>` and
the last line reads `~E <seq> <t_ms> PANIC: <msg> at src/bin/panictest.rs:L:C *<crc>`; the stage table in
`src/bin/panictest.rs` is the copy to read. Framing changes the bytes, not the argument below: the
countdown and the panic line travel by *different* mechanisms — the countdown goes through the log
pipe, while the panic line is pushed straight to the endpoint because the executor is dead by then — so
countdown text with no `PANIC:` line is a real failure, not a missed message.

### Reading a saved capture

Save the raw bytes, not a scrollback:

```bash
cat /dev/ttyACM0 > capture.bin
cargo run -p asperitas-logging --example console_decode -- capture.bin > clean.txt
cat capture.bin | cargo run -p asperitas-logging --example console_decode
```

stdout carries the validated records, byte-identical to what the device sent — greppable, and re-feedable
into the same program. stderr carries the integrity counters
(`records=… bad_frames=… resyncs=… discarded_bytes=…`), a byte-accounting line proving every pushed byte
is in one category or the other, and a `seq continuity` line with `gaps=` and
`first_gap=<from8>-><to8>`. The split is the point: the counters say what the wire did to the bytes,
the sequence summary says whether any bytes are missing, and a CRC cannot answer the second question.
The tool **exits 0 whatever the capture contains** — it reports, and deciding whether a capture passed is
TASK-031's job. A gap immediately after a `BOOT` record is a restart, not loss.

### Flashing and logging over an ST-Link probe

The software side is in place: `make probe-*` targets, `probe-rs` 0.32.0 from the flake, release
line tables for symbolication. What has *not* happened is a probe talking to this board —
TASK-037 makes the first attachment and records timings. Treat these commands as ready, not
proven.

```bash
cd firmware
DEFMT_LOG=info make probe-flash FEATURES="seed3 log-defmt" NO_DEFAULT=1  # build, program, verify read-back, reset, exit
make probe-log   # attach and stream RTT; no reflash, no reset
```

Both drive the release **ELF**, never `firmware.bin`: `probe-rs` decodes `defmt` frames from the
ELF's `.defmt` section and unwinds with its DWARF, so the host's copy has to be the one that
built what's running. The chip string (`--chip STM32H750IBKx`) is in the Makefile.
`DEFMT_LOG=info` is load-bearing rather than decorative: unset, defmt compiles every non-ERROR
call to nothing and a silent channel looks exactly like a dead probe.

`docs/reference/daisy-seed3.md` has the rest — why the probe path takes the ELF, what RTT does
when no host is attached versus when the host stalls, and the rule that follows from that:
**nothing logs from the audio callback.**

**Permissions are Linux-only here.** `probe-rs` talks to the ST-Link over `/dev/bus/usb/*`, not
`/dev/ttyACM*`, so being able to reach a serial or DFU device doesn't imply this works. On Linux
the bench needs the udev rules — they ship inside `probe-rs-tools` as
`etc/udev/rules.d/69-probe-rs.rules`, and a NixOS host installs them with

```nix
services.udev.packages = [ pkgs.probe-rs-tools ];
```

Any other Linux, install that file the way your distro does. macOS needs nothing.

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
