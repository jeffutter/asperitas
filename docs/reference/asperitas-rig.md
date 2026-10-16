# asperitas-rig: host capture runner

`asperitas-rig` (crate `crates/asperitas-rig`) captures the device's framed USB console for
a fixed time, validates every frame, writes a deterministic text artifact, and exits
non-zero when the capture cannot be trusted. Use it instead of ad hoc serial logging
whenever a ticket's evidence comes from the console.

## Usage

Subcommands: `capture` (below), `query`, `restart` and `override` (see [Control](#control)).
The flags below belong to `capture`; `--device` is shared by all of them.

```
cargo run -p asperitas-rig -- capture --duration 10 --out captures/example.txt
cargo run -p asperitas-rig -- capture --device /dev/cu.usbmodem1234 --duration 30 --out out.txt --max-drops 2
```

| Flag | Meaning |
|---|---|
| `--duration <secs>` | Required. Capture length in seconds (fractions allowed). |
| `--out <path>` | Required. Artifact path; overwritten if present. |
| `--device <path>` | Serial device. When omitted, `/dev/cu.usbmodem*` and `/dev/ttyACM*` are scanned; exactly one match is required. Zero or several matches is an error, so pass `--device` when more than one board is attached. |
| `--max-drops <n>` | Device-reported drops tolerated before the verdict fails. Default `0`. |

A summary (the artifact header, up to the `--` line) is printed to stdout.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Capture completed and the verdict is `pass`. |
| 1 | Capture completed but the verdict is `fail`. The artifact is still written. |
| 2 | Usage, device selection, open, or I/O error. No trustworthy capture exists. |

## What fails a capture

- any bad frame, or any discarded byte (including garbage before the first valid frame);
- a byte accounting mismatch (`accounting_ok=false`);
- device-reported drops (`dropped_full`, summed over boots from the last `STATUS` record
  of each) above `--max-drops`;
- no parsed `STATUS` record at all (the rig always requires one, so a silent device fails).

Sequence gaps (`seq_gaps`) and restarts (`restarts`, a gap at a `BOOT` record) are
reported but do not fail on their own. Read them when the ticket cares about continuity.

## Artifact format

Line-oriented text, `asperitas-rig capture v1`. It contains no wall-clock time, host path,
or duration, so the same wire bytes always produce the same file and it diffs cleanly.

```
asperitas-rig capture v1
records=412
bytes_pushed=38120
bad_frames=0
resyncs=0
discarded_bytes=0
accounting_ok=true
seq_gaps=0
first_gap=none
restarts=0
status_records=4
unparsed_status=0
device_dropped=0
verdict=pass
--
I 00000000 12 BOOT ...
I 00000001 15 ...
```

Header: fixed-order `key=value` lines, then `verdict=pass` or
`verdict=fail <reason>; <reason>` (reasons: `bad_frames=N`, `discarded_bytes=N`,
`accounting_mismatch`, `device_dropped=N exceeds max=M`, `no_status_record`).
`device_dropped=unknown` means no `STATUS` record parsed; `first_gap` is
`<prev>-><next>` in hex sequence numbers or `none`.

After the `--` separator, one line per validated record: `level seq t_ms body`, where
`level` is `I`, `W`, `E`, `D` or `T`, `seq` is 8 hex digits, `t_ms` is device milliseconds
since boot (modulo 1e8), and `body` is the ASCII-sanitised message.

## Control

`query`, `restart` and `override` send host control commands on the same serial device the
rig captures from. The grammar and codec are in `crates/asperitas-logging/src/control.rs`
(`CTL VER|RSN|RST|SET|REL|FX` out, `CTLR ...` replies back in the ordinary record stream); the
rig uses that codec directly, so host and device cannot drift.

```
asperitas-rig query [--device P]
asperitas-rig restart [--device P] [--reenumerate-timeout 15]
asperitas-rig override --knob 2 --value 0.5   --duration 10 --out o.txt [--device P] [--max-drops N]
asperitas-rig override --knob 2 --sweep 0:1   --duration 10 --out o.txt
```

- `query` prints `fw=<version> reset_reason=<token>`.
- `restart` sends `RST`, waits for the console to vanish and re-enumerate (the USB number can
  change, so it re-scans, or reuses `--device` when given), reopens it and queries the new
  boot. It prints `restarted device=<path> fw=.. reset_reason=.. boot_record=<bool>`.
  `boot_record=false` is not an error: the device may log `BOOT` before the host attaches.
  The query reply is the proof of life. Fails (exit 2) after `--reenumerate-timeout` seconds.
- `override` holds (`--value L`) or sweeps (`--sweep FROM:TO`, linear, 20 steps per second)
  knob `--knob K` while capturing the same port for `--duration`, writes the usual artifact,
  then releases. Levels are `0..=1` and map to the wire range `0x0000..=0xFFFF`; a level
  outside `0..=1` is a usage error (exit 2), never clamped.

**Release guarantee.** `override` always sends `REL` for its knob after the last `SET`: after
a finished sweep, after the capture dies mid-run, and after a failed write. The capture
listens `0.5 s` past `--duration` so the `REL` ack is in the artifact. The run passes only
if the normal capture verdict passes, every `SET` and the `REL` were acknowledged
(`CTLR OK SET|REL`) and no `CTLR ERR` appeared. The override summary line
(`override knob=.. sets_sent=.. set_acks=.. rel_acks=.. device_errors=..`) follows the
artifact summary on stdout.

Exit codes for the control subcommands: 0 success, 1 the device rejected a command
(`CTLR ERR`, for example `bad-knob`) or the override acks were incomplete, 2 usage, device,
I/O or timeout.

Verified on the host with a fake transport, and on hardware 2026-10-15 (TASK-033.01): `query` and `restart` against `podtest` on a Seed3 re-enumerated with no BOOT/RESET and reported `reset_reason=software` (`captures/task-033-01-restart.txt`). A 240 s `capture` had `bad_frames=0` but `device_dropped=4041` in one gap (see `captures/task-033-01-podtest-240s.txt`).

### Replay effect (`CTL FX`)

The `rig` firmware built with `stim-excerpt` can run an `asperitas-dsp` processor over the replayed
excerpt. Wire form: `CTL FX f=<2 hex> v=<4 hex>`, answered `CTLR OK FX`.

| `f` | effect | `v` (`0x0000..=0xFFFF` spans knob `0.0..=1.0`) |
|-----|--------|------------------------------------------------|
| `00` | bypass (default) | ignored |
| `01` | gain | `Gain::params_from_normalised`: -60 dB to +12 dB |
| `02` | one-pole low-pass | `OnePoleLowPass::params_from_normalised`: 20 Hz to 20 kHz, log taper |

- **Default is bit-exact.** Bypass touches no sample, so captures taken without sending `FX`
  are unchanged from before the control existed. Sending `f=00` returns to that state.
- **Replies.** `CTLR ERR bad-value` for an unknown `f`; `CTLR ERR busy` once a capture is armed
  (a change mid-capture would make the recording unrepeatable); `CTLR ERR unknown-verb` for any
  other `CTL` verb, which this binary does not serve. Without `stim-excerpt` every `FX` is
  `unknown-verb`.
- **Timing.** The pass is held until the capture's first callback, `ARM_DELAY_MS` (2 s) after audio
  starts. Send `FX` in that window, i.e. right after the console attaches. The `rig` host
  subcommand for this is not written yet; `cargo run -p asperitas-logging --example ctl_frame -- <fx hex> <value hex>` prints the
  framed command to send, and a session script that sends it the moment the console attaches (about 3.7 s after a probe reset) lands inside the window.
- **CRC.** The effect runs after the block is taken, so `EXCPLAY`'s CRC still describes the raw
  stored samples and cannot detect the effect.
- **Applied at the next block** after the reply; the processor starts from a clean state with no
  smoothing ramp, so a given selection always produces the same output.
- **Live override (TASK-019.03.01).** `CTL SET k=00 v=<4 hex>` and `CTL REL k=00` override the
  selected effect's parameter, and unlike `FX` they are accepted while a capture is armed, so an
  override sweep moves the effect mid-capture. Knob 0 only (`bad-knob` otherwise). Each block merges
  the override with the `FX` value through the shipped `KnobOverride`, upstream of the effect's
  smoothing, as `main` does; bypass ignores it. Callback cost, an estimate only: a critical section
  and a compare per block, plus one `set_params` (a `powf`, and an `expf` for the low-pass) when
  the value moved, order of 1-3 us against the 666 us period. The measurement is TASK-019.03.04.
- **`unsmoothed-params` feature.** `--features seed3,stim-excerpt,unsmoothed-params` builds the
  effect without smoothing, so an override steps the parameter on the next sample. It exists only
  to prove the discontinuity detector can see a step; never use it for ordinary captures.

To judge a capture taken with a `CTL FX` selection, pass the same wire values to `analyze`:
`--fx bypass|gain|lowpass` and `--fx-value <0..65535 or 0xHEX>` (the `v=` sent to the board). The
reference then runs `ReplayFx` at that selection, and the report carries an `fx` object. The
limits are the same at every setting; a capture taken at a different setting than the one passed
fails on loop gain (gain) or residual/alignment (low-pass). Host-synthesized captures cover this;
a device capture at non-identity settings is TASK-035.07.03.

Measured on the board 2026-10-15 (TASK-035.07.03), `mandolin_single_note_soft` in slot 0, 10 s window, each
setting sent right after the console attached and answered `CTLR OK FX`:

| `CTL FX` | `analyze --fx --fx-value` | loop gain vs model | residual RMS | residual peak | verdict |
|---|---|---|---|---|---|
| `f=01 v=eaaa` (+6 dB) | `gain 0xEAAA` | -0.009 dB | -30.8 dB | -21.5 dB | pass |
| `f=01 v=aaaa` (-12 dB) | `gain 0xAAAA` | -0.009 dB | -30.8 dB | -21.5 dB | pass |
| `f=02 v=aaaa` (2 kHz) | `lowpass 0xAAAA` | -0.009 dB | -32.9 dB | -30.0 dB | pass |

Each capture aligned at 81 samples. The wrong setting fails: the +6 dB capture against `bypass` is 5.99 dB off on
loop gain, the -12 dB capture against the +6 dB setting fails, and the low-pass capture against `bypass` fails. The
callback cost with the effect on is `max_block_us=19` against 15 without, 30 of 30 blocks delivered, no overruns. The
captures are `audio/captures/2026-10-15-rig-fx-*.console.zst` and the reports `captures/task-035-07-03-*.json`.

## Convention: commit artifacts with hardware verification tickets

A hardware verification ticket's evidence is the artifact, not a recollection of it. Commit
the artifact (for example under `captures/`) in the same commit that records the
verification, and name its path in the ticket's implementation notes. A `verdict=fail`
artifact is evidence too; commit it with the ticket that explains the failure rather than
re-running until it passes. Exit code 0 only says the transport was clean; it does not
replace the ticket's own `HUMAN:` criteria.

See also [daisy-seed3.md](daisy-seed3.md) for the console channels and what a drop count
does and does not describe (RTT captures carry no loss ledger).

## Analysis: `asperitas-measure`

`crates/asperitas-measure` judges a committed `.console.zst` capture. It loads the capture
strictly (any refused block is an error), regenerates the stimulus from `asperitas-dsp`, aligns
the two, and reports metrics as one JSON document on stdout. Every numeric limit lives in
`src/thresholds.rs`, each with its origin; nothing else writes a limit as a literal.

```sh
cargo run -p asperitas-measure --release -- analyze <capture.console.zst> [--stimulus sine|sweep|pulse]
cargo run -p asperitas-measure --release -- analyze <replay.console.zst> --excerpt audio/instruments/<clip>.wav
cargo run -p asperitas-measure --release -- analyze <replay.console.zst> --excerpt audio/instruments/<clip>.wav --fx gain --fx-value 43690
```

Exit codes match `asperitas-rig`: **0** pass, **1** failed verdict (the JSON lists `failures`,
each also on stderr), **2** usage, load or I/O error. The default stimulus is the sine. Only the
sine is judged against the TASK-034 baseline; `sweep` and `pulse` are reported and checked for
alignment stability only. `--excerpt` instead compares an excerpt replay with the
`asperitas-cli` reference of the same clip.

### Metrics

| Metric | Meaning | Fails when |
|---|---|---|
| `alignment` | Loopback delay per 1 s window by cross-correlation. | A window's delay moves more than 0.25 sample from the first (a slip). |
| `level.deviation_db` | Capture RMS over stimulus RMS, minus the baseline loop gain (-0.41 dB). | More than 0.05 dB. |
| `silence` | Runs of at least 1 ms at or below 2 LSB. | Any run. |
| `discontinuities` | Samples where the capture minus gain-scaled, delayed stimulus jumps more than 8 LSB; nearby ones merge. | Any event. |
| `spurs` | Largest non-fundamental FFT peaks, in dBc. | Worst above -80 dBc. |
| `param_steps` | 1 ms block-RMS changes over 5 dB (an unsmoothed parameter change). | Any event. |
| `comparison` (`--excerpt`, optional `--fx`) | Residual of the replay against the reference after alignment, gain fit and DC removal. | Residual RMS above -25 dB or peak above -15 dB relative to the reference, or loop gain off baseline. |

### Blind spots

- A level, noise or spur deviation belongs to the output and input converters combined; it cannot
  be split between them. A clean report is evidence about the loop, not proof of either converter.
- Faults common to stimulus and capture are invisible: a shared clock error, and a left/right
  swap on a single-lane loopback.
- The spur report assumes a tone; its "fundamental" is the strongest bin, so it means nothing for
  a sweep or pulse (which is why those are not judged).
- `param_steps` sees only level steps. A step in a quiet passage (blocks under 2 LSB RMS are
  ignored) or one that does not change level (a filter cutoff step) goes unseen.
- Excerpt comparison is not bit-exact and cannot be: the path adds two converters, an analog
  noise floor and a fitted gain. See `COMPARE_RESIDUAL_RMS_DB` for the budget.
- Alignment of a sine is determined modulo its period; the smallest delay among the ties is used.

### Fixture workflow

1. Capture with `asperitas-rig` (see above); the artifact is a `.console.zst`.
2. Run `analyze` on it. Commit the capture under `audio/captures/` with the hardware verification
   ticket, as for any rig artifact.
3. A new committed capture becomes a regression fixture by adding a test in
   `crates/asperitas-measure/tests/cli.rs` (or the module tests) naming it.
4. If a limit has to move, change `thresholds.rs` and update its "Origin" note with the capture
   and ticket that justify the new value.
