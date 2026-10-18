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
rig captures from. The grammar and codec are in `crates/asperitas-wire/src/control.rs`
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

### Bench session (`session`)

One command for the whole bench sequence: optionally reset the board over the probe, attach,
require a fresh `BOOT`, select the effect, wait for the capture to arm, drive one knob on a
timed schedule, release it, and record every byte until `DUMPEND` plus 2 s.

```
asperitas-rig session --out run.console.zst --fx-f 1 --knob 0 --max-seconds 330 [--probe-reset] \
    [--device P] [--fx-v N] [--chip-description firmware/asperitas-h750.yaml] <MOTION>
  sweep --from 0 --to 1 --duration 30 [--start-after-arm 5]
  soak  [--no-snaps] [--snap-level 0.25]
  hold  --level 0.5
```

- `--fx-f` is `0` bypass or `1` patch; `--knob` is decimal (0 filter cutoff, 1 output gain).
  Device selection is the same as the other subcommands (`--device` or the single
  `/dev/cu.usbmodem*` / `/dev/ttyACM*`).
- `sweep` steps linearly at 20 `SET`s per second from `--start-after-arm` seconds after the
  capture arms, then releases 0.1 s after the last step. `soak` is a 250 s triangle ramp
  0.50 to 0.95 to 0.50 starting 10 s after arming (it starts and ends at the base value so the
  ramp itself causes no snap), with two snaps (+`--snap-level` for 0.5 s) at 80 s and 160 s of the
  ramp unless `--no-snaps`. `hold` sends one `SET` before the capture arms and releases when the
  dump ends. Levels outside `0..=1` are a usage error (exit 2), never clamped.
- `--probe-reset` runs `probe-rs reset` with the chip description, waits for the console node to
  vanish, then retries attaching for up to 15 s. The chip description path defaults to the repo's
  `firmware/asperitas-h750.yaml`.
- `FX` is only sent after a `BOOT` record has been read on this attachment (12 s limit). Without
  one the board has been running and may be armed, so the session exits 2 having sent nothing.
- Release guarantee: once anything has been sent, `REL` follows on every way out, including a
  failed write, a dead port and a timeout before the capture armed.
- The schedule runs between reads of a 10 ms port timeout, so step timing jitters by up to 10 ms.

Output: `--out` is the raw stream, zstd-compressed (readable with `asperitas-measure analyze`);
`<out>.session` is the sidecar, `key=value` lines under an `asperitas-session v1` header, parsed
by `asperitas_host::session::Sidecar`. It holds `fx_f`, `fx_v`, `knob`, `mode`, `from`, `to`,
`duration_s`, `start_after_arm_s`, `snaps`, `snap_level`, and the device `t_ms` of the record that
first showed the capture armed (`arm_t_ms`, a `CAPSTAT` with `delivered>0`) and of the first
`CTLR OK SET` (`first_set_t_ms`) when the stream held them. Partial runs are still written.

Exit codes: 0 when the device acknowledged `FX`, every `SET` and the `REL`, rejected nothing and
the dump ended; 1 for incomplete acks or a device `CTLR ERR`; 2 for usage, device, I/O or
timeout errors. The summary line is `session bytes=.. fx_acked=.. sets_sent=.. set_acks=..
rel_acks=.. device_errors=.. armed=.. dump_end=..`. Verified on the host with a fake device;
a bench run against hardware is a separate ticket.

### Replay effect (`CTL FX`)

Every `rig` build runs an effect stage after its stimulus generator: the sine (the default),
`stim-ess`, `stim-pulse` and the `stim-excerpt` replay all carry the same stage,
`asperitas_dsp::fx_chain::FxChain`. Generator first, effect second, then the encoder; the capture is of the
codec input, as before. The stage is bypass or the `asperitas_dsp::Asperitas` patch, the same
low-pass-then-gain chain `main` runs, so the rig measures what ships and no knob mapping lives outside
`asperitas-dsp`. Wire form: `CTL FX f=<2 hex> v=<4 hex>`, answered `CTLR OK FX`.

| `f` | effect | `v` |
|-----|--------|-----|
| `00` | bypass (default) | ignored |
| `01` | `Asperitas` patch at its default knob positions (filter open at 20 kHz, 0 dB) | ignored; kept in the frame so the wire shape stays stable |

Move the patch with `CTL SET`/`REL`: knob `00` is the filter cutoff (20 Hz to 20 kHz, log taper) and knob
`01` the output gain (-60 dB to +12 dB), both through the `asperitas-dsp` parameter tables.

- **Sine and the other periodic generators.** They run from the first callback, so `FX` is accepted
  any time before the capture arms and the effect is in place for the whole capture; `SET`/`REL` move it
  live. A one-shot generator (the excerpt) is the only one with the arm-window timing below.
- **`RIGGEN`.** It is emitted once at boot, before any `FX` can arrive, so it names the effect only if
  the generator's `describe()` is asked later; on the device it reads as the bare generator. Record the
  `FX` you sent with the capture (`analyze --fx`).
- **Default is bit-exact.** Bypass touches no sample, so captures taken without sending `FX`
  are unchanged from before the control existed. Sending `f=00` returns to that state.
- **Replies.** `CTLR ERR bad-value` for an `f` other than `00` and `01`; `CTLR ERR busy` once a capture is armed
  (a change mid-capture would make the recording unrepeatable); `CTLR ERR unknown-verb` for any
  other `CTL` verb, which this binary does not serve. `FX`, `SET` and `REL` are served by every
  stimulus build with the console (`log-usb`); the RTT-only builds have no console to send them on.
- **Timing (excerpt).** The pass is held until the capture's first callback, `ARM_DELAY_MS` (2 s) after audio
  starts. Send `FX` in that window, i.e. right after the console attaches. The `rig` host
  subcommand for this is not written yet; `cargo run -p asperitas-wire --example ctl_frame -- <fx hex> <value hex>` (or `set <knob> <value>` / `rel <knob>`) prints the
  framed command to send, and `scripts/rig-session.py` runs the whole sequence, and a session script that sends it the moment the console attaches (about 3.7 s after a probe reset) lands inside the window.
- **CRC.** The effect runs after the block is taken, so `EXCPLAY`'s CRC still describes the raw
  stored samples and cannot detect the effect.
- **Applied at the next block** after the reply; the processor starts from a clean state with no
  smoothing ramp, so a given selection always produces the same output.
- **Live override.** `CTL SET k=<00|01> v=<4 hex>` and `CTL REL k=<00|01>` override the patch's
  cutoff (`00`) and gain (`01`), and unlike `FX` they are accepted while a capture is armed, so an
  override sweep moves the effect mid-capture. Other knob ids get `bad-knob`. Each block merges the
  overrides over the patch's default knob positions through the shipped `KnobOverride<2>`, upstream of the
  effect's smoothing, as `main` does; bypass ignores them. A `REL` returns that knob to its default.
  Cost: a critical section and a compare per block, plus one `set_controls` (a `powf`, and an `expf`
  for the filter) when a value moved.
- **`unsmoothed-params` feature.** `--features seed3,unsmoothed-params` (sine) or
  `--features seed3,stim-excerpt,unsmoothed-params` (excerpt) builds the effect without smoothing, so an override steps the parameter on the next sample. It exists only
  to prove the discontinuity detector can see a step; never use it for ordinary captures.

To judge a capture taken with a `CTL FX` selection, record it with `asperitas-rig session`: the
sidecar carries the selection, and for a hold session the knob level, so `analyze --excerpt` runs
`ReplayFx` at that selection and the report carries an `fx` object. The limits are the same at every
setting; a capture taken at a different setting than the reference fails on loop gain (gain) or
residual/alignment (cutoff). Host-synthesized captures cover this.

Measured on the board 2026-10-15 (TASK-035.07.03), on the earlier build whose `FX` codes `01` and `02` were separate gain and low-pass effects driven by `v`; the rows below name that build's settings and are not reproducible with the current wire. `mandolin_single_note_soft` in slot 0, 10 s window, each
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
cargo run -p asperitas-measure --release -- analyze <session.console.zst> [--expect-fail]
cargo run -p asperitas-measure --release -- analyze <hold-session.console.zst> --excerpt audio/instruments/<clip>.wav
```

**A capture recorded by `asperitas-rig session` needs no further flags.** `analyze` reads the
sidecar beside it (`<capture>.session`) for the effect selection, the knob, the motion and its
start, and judges the session accordingly; `--stimulus` is refused (exit 2) because the sidecar
already decided. Without a sidecar the capture is a steady-stimulus loopback, judged as the first
two forms above (an excerpt at the device's default bypass).

A **sweep** session is judged as a knob sweep (knob 0 is the low-pass cutoff, knob 1 the gain;
other knobs have no judgement). The knob levels are the sidecar's `from` and `to`. The capture is
split into 20 equal slices of the knob travel (the rig's `SET` schedule is linear in time), each
trimmed at both ends, and the response per slice is measured: block RMS for the gain, RMS of the
first difference (a high-band level that follows the cutoff) for the low-pass. The report lists
the curve against knob value, `direction`, `span_db`, `monotonic` and `max_reversal_db`, and
`flat_regions`. It fails when the response does not rise with the knob by at least the minimum
span, falls back by more than the monotonic tolerance, or holds still for a run of slices (a
plateau or dead zone). The sweep starts at the device-clock span from the capture arming to the
first `SET` when the recorder read both, the planned `--start-after-arm` otherwise, and runs for
the sidecar's duration. Limits are the `SWEEP_*` constants in `thresholds.rs`.

A **soak** session (a 5+ minute sine capture taken while a knob moves) and a **hold** session
(one level for the whole capture, no motion window) are judged as a soak. One verdict over silence runs, discontinuity candidates and parameter steps for the whole
run: any of them, anywhere, fails. The JSON attributes each to the motion window (motion start and
duration, plus 1 s each side) or outside it, as `inside_motion`, `outside_motion` and
`unattributed` (counted but past the detector's position cap). The discontinuity detector refits
the loop gain every 10 ms so a swept gain does not read as a discontinuity. Keep sweep ranges above
the silence floor, since a sweep to zero legitimately produces silence runs. `--expect-fail` (soak and hold sessions only) is for
the unsmoothed-build capture: exit 0 only when the step detector fired and no silence is present
(`expected_failure_observed`), exit 1 when the capture passes or fails some other way; the
detectors' findings are kept under `detector_failures`. Limits are the `SOAK_*` constants in
`thresholds.rs`.

Exit codes match `asperitas-rig`: **0** pass, **1** failed verdict (the JSON lists `failures`,
each also on stderr), **2** usage, load or I/O error. The default stimulus is the sine. Only the
sine is judged against the TASK-034 baseline; `sweep` and `pulse` are reported and checked for
alignment stability only. `--excerpt` instead compares an excerpt replay with the
`asperitas-cli` reference of the same clip; with a sidecar the reference runs at the session's
effect selection and, for a hold session, its knob level (a sweep or soak session is refused, since
the reference cannot follow a moving knob).

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

### Measured on the board (TASK-019.03.04)

Taken on the earlier build in which `CTL FX` chose a lone gain or low-pass and knob `00` moved whichever was chosen. On the current rig the gain is knob `01` and the cutoff knob `00` of the patch.

All on `rig` in the bootloaded layout, each run started by `scripts/rig-session.py` (probe reset, a fresh `BOOT`
record, `CTL FX` on attach, then a `SET` schedule at 20 per second ending in `REL`), console held by that one
reader, 2026-10-15. Captures are `audio/captures/2026-10-15-rig-{sweep,soak,hold}-*.console.zst`, reports
`captures/task-019-03-04-*.json`. The sweep start is `arm_ms` (the median of `t_ms - (delivered + 0.5) * 341.3` over
the `CAPSTAT` rows) against the first `CTLR OK SET` reply's `t_ms`; it was 4.90 s into the capture.

| Run | Build | Result |
|---|---|---|
| Gain sweep 0 to 1 over 30 s | sine, 45 s window | pass: rising, monotonic, no flat region, span 68.2 dB, 3.6 dB per bucket as 72 dB over 20 buckets predicts |
| Low-pass sweep 0 to 1 over 30 s | pulse train, 45 s window | pass: rising, monotonic, no flat region, span 36.9 dB; buckets 1 to 3 (knob 0.025 to 0.125) all read -84.3 dB, the measurement floor, which is under the 3-step flat limit but is a dead zone the pulse train cannot see into |
| Soak, 300 s, triangle 0.50 to 0.95 to 0.50 over 250 s, 5001 `SET` | sine, smoothed | pass: no silence, no discontinuity (largest jump 6.9 LSB against 8), no step (largest 0.016 dB per ms) |
| Soak with two +0.12 snaps (+8.6 dB) | sine, smoothed | step detector: 0 events, largest 2.1 dB per ms; discontinuity detector: 72 candidates, all at the two snaps |
| Same schedule | sine, `unsmoothed-params` | step detector: 4 events at 90.9, 91.4, 170.9, 171.4 s (the snap edges), largest 8.65 dB per ms; `analyze --soak --expect-fail` exits 0 |
| Held gain +6 dB and low-pass 2 kHz through `SET` on the mandolin excerpt | `stim-excerpt` | pass against `asperitas-cli`: loop gain within 0.03 dB of the model, residual RMS -30.7 and -32.9 dB, delay 81 samples |

Callback cost on the sine path with the effect on and the knob moving every 50 ms: `max_block_us` 52, `worst_gap_us`
696, no overruns, no drops; 47 without the effect. The pulse-train build reads 88.

What the runs taught, for whoever plans the next one:

- **A `REL`, or a first `SET` away from the base value, is a snap.** Releasing returns the parameter to the `FX` value, so
  a ramp that ends elsewhere drops by up to 32 dB at the `REL`. The first soak attempt did exactly that and the
  discontinuity detector flagged it (19 events at 260.99 to 261.06 s); the 250 s ramp itself was clean. Run a triangle that
  starts and ends at the base value.
- **A snap is flagged by the discontinuity detector whatever the smoothing.** The gain fit follows over 10 ms, and a
  smoothed 8.6 dB snap still moves faster than that (72 candidates, all at the snaps). It is the step detector that separates
  smoothed from unsmoothed: 2.1 dB per ms against 8.65. An 18 dB snap (+0.25 knob) defeats the smoothing even for the step
  detector (6.3 dB per ms, over the 5 dB limit); that capture's report is `captures/task-019-03-04-exploratory-soak-18db-snaps-smoothed.json`.
- **A low-pass sweep needs a broadband stimulus.** On the 1 kHz sine the first-difference level is flat once the cutoff passes
  1 kHz, so use the pulse train for it. The gain sweep and the soak use the sine.
- **Sweep and soak need the motion start and duration.** The stream cannot supply them; the session sidecar does.
- No threshold was changed to make any of these pass.

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

## Measurement rig firmware

`rig` plays a stimulus out of the Pod, records what comes back through a cable from the Pod's
output jack to its input jack into external SDRAM, then dumps the recording over the framed USB
console. It decides start and end itself: 2 s after audio starts it arms, captures for the build's
window (300 s by default), dumps, and idles until RESET.

Pick the stimulus by the question. Exactly one stimulus feature goes in the image; the default is the sine.

| Question | Build | Why |
|---|---|---|
| What is the loop gain and its noise floor? | `seed3` (sine) or `seed3 stim-pulse` | A steady tone is easy to difference against; the pulse train adds timing edges. |
| What is the frequency response? | `seed3 stim-ess` | The exponential sweep covers the band once; it starts at the first captured callback. |
| How does the loop treat real material? | `seed3 stim-excerpt` | Plays an installed instrument clip once, from the first captured callback. Needs the bootloaded layout. |

Only the first two are available from a plain internal-flash image. Records the build prints at boot
(`RIGCFG`, `RIGGEN`, `CAPMAX`) say what it was configured to do; their field-by-field grammar is in
`daisy-seed3.md`, *Rig record grammar*.

```bash
cd firmware
make probe-flash BINARY=rig FEATURES=seed3                    # sine, -20 dBFS, 1 kHz
make probe-flash BINARY=rig FEATURES="seed3 stim-ess"         # exponential sweep, starts at arm
ASP_RIG_CAPTURE_SECONDS=30 make probe-flash BINARY=rig FEATURES=seed3   # shorter window
ASP_RIG_CAPTURE_SECONDS=ring make probe-flash BINARY=rig FEATURES=seed3 # until the ring is full, ~349.5 s
# then tap RESET: a probe flash stops the cycle counter, and timing would read 0
```

The sine and pulse train play from boot. The sweep plays once, so `rig` holds it silent until the
first captured callback and starts it there: the capture holds the whole sweep, and a
`STIMSTART first_block=<n> offset=<samples>` record after the window says where in the reassembled
PCM playback began (0 by construction; the echo arrives one loop latency later). The gate line
`rig: gate stimulus samples a..b inside the N-sample capture` says whether the window held all of it.

`stim-excerpt` plays an excerpt installed into QSPI instead of a computed waveform, once, from the
first captured callback like the sweep. `ASP_RIG_EXCERPT_SLOT=<n>` picks the slot at build time (default 0;
a value outside `0..=13` fails the build). The staging ping-pong is read from QSPI by a thread-mode task and
the callback never touches flash. After one full pass `rig` sends one
`EXCPLAY proto=1 slot=<n> bytes=<n> crc16=<hex> want=<hex> underruns=<n> clean=<0|1>` record. `crc16` covers
only the samples handed to the DAC encoder, and `want` is the CRC stored at install: equal CRCs with
`clean=1` say the QSPI-to-encoder path delivered the excerpt intact, and say nothing about the codec or
the cable. A slot that is blank or holds no valid header makes `rig` send
`EXCPLAYFAIL proto=1 slot=<n> why=<empty|invalid>`, show red and start no audio. An excerpt install is refused
until the pass completes. Needs the bootloaded layout (`make flash LAYOUT=bootloaded`); The committed excerpt captures under `audio/captures/` are the evidence.

### Installing an excerpt

An excerpt is mono 48 kHz 16-bit WAV (the corpus in `audio/instruments/` already is). Flash the
bootloaded layout once (`make flash LAYOUT=bootloaded`, see `daisy-seed3.md`, *Flashing the bootloaded app, and the probe*), then stream the
clip into a slot (`0..=13`, 14 slots) with a name of 1 to 16 characters of `[a-z0-9_]`, with the board running
`rig` and its console visible on the host:

```bash
cargo run -p asperitas-host --example excerpt_stream -- \
    audio/instruments/mandolin_single_note_soft.wav 0 mandolin_soft > /dev/cu.usbmodem*
```

The tool writes frames and exits 0 without knowing whether they worked. The device reads every byte back
from flash and sends `EXCOK name=... bytes=... crc16=...` or `EXCFAIL got=... want=...` on the console;
that record is the only verdict, so capture the console while you install (next section). Installing
erases the slot's sectors, and erase dominates the time: the datasheet bound is about 22.5 s for a
288 kB clip and the one reading is 5.82 s (derived upper bound versus one measurement, 2026-10-15). An
install is refused until the replay pass of a `stim-excerpt` build has completed. Layout, costs and
the `EXC*` grammar: `daisy-seed3.md`, *Instrument excerpts in QSPI*.

### Capturing the console

Hold the console open with **one** reader for the whole run, started before the board boots or before
you tap RESET so `BOOT` is in the file. Save raw bytes, not scrollback. A reader restarted mid-dump
loses whatever macOS had buffered, which was 19 blocks once.

```bash
cat /dev/cu.usbmodem* > capture.bin        # leave running; interrupt after DUMPEND
cargo run -p asperitas-wire --example console_decode -- capture.bin > clean.txt
grep -E 'RIGCFG|CAPMAX|CAPSTAT|DUMPEND|STIMSTART|EXC(OK|FAIL|PLAY)' clean.txt
```

Judge the run from the records, not from the audio. `DUMPEND` ends the dump and carries
`dropped_full` and `refused`; a nonzero `dropped_full` before the dump started is the host attaching late
and does not matter, one during the dump does. `CAPSTAT` carries `overrun`, `max_block_us` and
`worst_gap_us`. For a fixed-length evidence capture with a verdict, `asperitas-rig capture` does the same
job and is described above.

### Reassembling a dump

The reassembler turns `capture.bin` back into PCM and exits non-zero if any block is incomplete or fails
its CRC:

```bash
cargo run -p asperitas-host --release --bin dump_reassemble -- capture.bin --out capture.pcm
```

The PCM is headerless signed 16-bit little-endian, mono, 48 kHz. For a sweep or excerpt build, the
`STIMSTART` record gives `offset`, the sample index in this PCM where playback began; the echo arrives
one loop latency later.

### Where artifacts go

Compress the raw console bytes and commit them with the ticket whose evidence they are, under
`audio/captures/<date>-rig-<what>.console.zst`; the reassembled PCM is derived and need not be kept. Keep
`clean.txt` out of the repository unless a ticket quotes it. Record in the ticket which build produced the
capture (features, `ASP_RIG_*` variables, layout), since `RIGCFG` and `RIGGEN` in the capture say it only
for the stimulus and window.

### Which numbers are readings

Arithmetic, readings and estimates are labelled as such in `daisy-seed3.md`, *Capture
budgets (SDRAM)*. In short: ring size, seconds, base64 efficiency (0.568) and about 169 kB/s of wire per
second of capture are derived; callback cost, the 60-69 s dump of 300 s, and the 5.82 s install are
readings; the full-speed USB ceiling and sustained FMC write throughput are not measured.

### Observed on the bench

| Quantity | Reading | Source |
|---|---|---|
| Callback cost, worst | 72 us of a 666 us period | 300 s run, `max_block_us` |
| Callback spacing, worst | 714 us (limit 832) | 300 s run, `worst_gap_us` |
| Blocks delivered / expected | 879 / 879, 0 overruns | 300 s run |
| Callback cost, worst, bootloaded layout | 47 us of a 666 us period (internal flash: 72 us) | 300 s run from QSPI behind the Daisy bootloader, 2026-10-15, `max_block_us` |
| Callback spacing, worst, bootloaded layout | 692 us (limit 832; internal flash: 714 us) | same run, `worst_gap_us` |
| Blocks delivered / expected, bootloaded layout | 879 / 879, 0 overruns, `audio_exit=0` | same run |
| Dump of 300 s of audio, bootloaded layout | 60.2 s (internal flash: 69.4 s); 879 blocks, 28,803,072 PCM bytes, every block proved | `DUMPEND elapsed_ms=60232` |
| Ring filled to the last block | 1024 / 1024 blocks in 349.536 s on the device clock (349.626 s on the host, arm record to close record); 1 overrun ended it, 0 blocks overwritten | ring-fill run, 2026-10-13 |
| Dump of 300 s of audio | 69.4 s; 414.9 kB/s PCM, 731.7 kB/s on the wire | `DUMPEND elapsed_ms=69421` |
| Log loss during capture and dump | 0 | `dropped_full=0` throughout |
| Loop gain at -20 dBFS, 1 kHz | -0.41 dB | returned peak -20.41 dBFS |
| Loop flatness, 150 Hz-11 kHz | +/-0.03 dB | ESS capture RMS envelope |
| Noise + distortion after removing the tone | 0.35 LSB RMS (~76 dB SNR, at the 16-bit floor) | 30 s sine |
| Channel mapping | straight (L to L, R to R), no crosstalk above the floor | three one-channel runs |

The 300 s capture is archived as `audio/captures/2026-10-07-rig-sine-300s.console.zst`. The
analysis behind every row is in the notes of the ticket that took the reading, and the loopback setup is in
`daisy-pod.md`. The bootloaded 300 s run is archived as `audio/captures/2026-10-15-rig-sine-300s-bootloaded.console.zst`; its 36 `dropped_full` all fall before the host attached (t=1.2 s to 27 s), none during the dump. The ring-fill run is archived as
`audio/captures/2026-10-13-rig-ring-fill-349s.console.zst`.

To judge a capture, see *Analysis: `asperitas-measure`* above.

## Debugging without a probe

No ST-Link? You still have two channels:

1. **USB CDC-ACM serial** — the firmware enumerates as a serial device after booting. Connect with `screen /dev/ttyACM0 115200` (or your terminal program of choice) to see log output. The baud rate in that command is decorative — CDC-ACM has no UART to configure, and neither this firmware nor embassy-usb applies the host's line coding to the hardware.
2. **Pod RGB LEDs** — the firmware uses LED colour to indicate boot stage: red = pre-init, green = running, red = panicked. Check `daisy-pod.md` for the pin map and polarity notes.

Red means both "starting up" and "panicked", which is unambiguous in context — a panic
follows green — but see `slow-boot` below if you need to watch the boot stages closely.

Every line on that serial channel carries its own framing: `~<level> <seq> <t_ms> <body>*<crc>`.
It stays readable by eye and greppable in a raw capture — that legibility is why the protocol is
printable ASCII rather than COBS. The first line off a fresh boot is `BOOT proto=1 …`. `STATUS …`
appears at most once a second and only when a counter moved, so a quiet stream usually means nothing
changed rather than nothing working. Grammar, checksum parameters and what each loss counter means:
`daisy-seed3.md` → *Console protocol v1*.

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
the last line reads `~E <seq> <t_ms> PANIC: <msg> at src/bin/panictest.rs:L:C*<crc>`; the stage table in
`src/bin/panictest.rs` is the copy to read. Framing changes the bytes, not the argument below: the
countdown and the panic line travel by *different* mechanisms — the countdown goes through the log
pipe, while the panic line is pushed straight to the endpoint because the executor is dead by then — so
countdown text with no `PANIC:` line is a real failure, not a missed message.

### Reading a saved capture

Save the raw bytes, not a scrollback:

```bash
cat /dev/ttyACM0 > capture.bin
cargo run -p asperitas-wire --example console_decode -- capture.bin > clean.txt
cat capture.bin | cargo run -p asperitas-wire --example console_decode
```

stdout carries the validated records, byte-identical to what the device sent — greppable, and re-feedable
into the same program. stderr carries the integrity counters
(`records=… bad_frames=… resyncs=… discarded_bytes=…`), a byte-accounting line proving every pushed byte
is in one category or the other, and a `seq continuity` line with `gaps=` and
`first_gap=<from8>-><to8>`. The split is the point: the counters say what the wire did to the bytes,
the sequence summary says whether any bytes are missing, and a CRC cannot answer the second question.
The tool **exits 0 whatever the capture contains** — it reports, and deciding whether a capture passed is
the job of `asperitas-rig capture`'s verdict. A gap immediately after a `BOOT` record is a restart, not loss.

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
