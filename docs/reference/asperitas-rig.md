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
(`CTL VER|RSN|RST|SET|REL` out, `CTLR ...` replies back in the ordinary record stream); the
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

Verified on the host with a fake transport only; hardware runs are TASK-033.01.

## Convention: commit artifacts with hardware verification tickets

A hardware verification ticket's evidence is the artifact, not a recollection of it. Commit
the artifact (for example under `captures/`) in the same commit that records the
verification, and name its path in the ticket's implementation notes. A `verdict=fail`
artifact is evidence too; commit it with the ticket that explains the failure rather than
re-running until it passes. Exit code 0 only says the transport was clean; it does not
replace the ticket's own `HUMAN:` criteria.

See also [daisy-seed3.md](daisy-seed3.md) for the console channels and what a drop count
does and does not describe (RTT captures carry no loss ledger).
