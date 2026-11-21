# Knob automation file

`asperitas-cli process in.wav out.wav --processor asperitas --automation knobs.txt` renders a clip
in which the knobs move. The file is the one definition of knob motion; it is parsed by
`Automation::parse` in `crates/asperitas-dsp/src/automation.rs`. It works with the `asperitas` and
`resonator` processors (`--processor resonator --automation knobs.txt`) and cannot be combined with
`--params` (put starting values in the file). Any other processor is refused.

## Grammar

One event per line. `#` starts a comment; blank lines are ignored.

```
<seconds> [step|ramp] key=value [key=value ...]
```

- `seconds`: non-negative decimal of at most 3600 (`MAX_SCRIPT_SECONDS`), not earlier than the previous line. Converted to the nearest
  frame at the clip's sample rate, counted from the start of the clip.
- `key=value`: the names and real units of `--params` (`cutoff_hz`, `gain_db`), checked against the
  `ParamSpec` ranges through `Controls::from_values`.
- `step` (the default): the named knobs jump to the value at that time.
- `ramp`: each named knob moves linearly from its previous breakpoint (or its default, from time 0,
  if it has none) and arrives at the value at that time. The interpolation is in knob position,
  what turning the knob does, so a log-taper `cutoff_hz` sweeps by equal ratios and `gain_db`
  by equal dB.
- Hold is the absence of a later line: a knob keeps its value until it is named again, and keeps its
  default until it is first named.

Every error names the line: unknown key, value outside its range or not a number, times that
decrease, a token that is not `key=value`, a line with no key. Nothing is clamped or skipped.

## Example

```
0     cutoff_hz=8000 gain_db=0     # start open
0.5   ramp cutoff_hz=300           # sweep down, arriving at 0.5 s
1.0   step gain_db=-12             # drop the level at once
1.5   ramp cutoff_hz=8000 gain_db=0
```

## Resonator keys

`Automation<ResonatorControls>` (parsed with `Automation::<ResonatorControls>::parse_for`) uses the
same grammar, the same latch and the same device schedule for the resonator's controls. The keys
are `root_hz`, `structure`, `damping`, `spread`, `excitation_preset`, `follower_depth`,
`walk_depth`, `follower_attack_ms`, `follower_release_ms`, `walk_rate_hz`, `root_source`,
`root_offset_semitones` and `root_offset_cents`, with the units, ranges and defaults in
[resonator.md](resonator.md#controls). Keys ramp in knob position: `root_hz` and the log-taper keys
(`follower_attack_ms`, `follower_release_ms`, `walk_rate_hz`) by equal ratios, the others linearly.
`excitation_preset`, `root_source` and `root_offset_semitones` are step-only: `ramp` on one is an
error naming the line. The modulation and root keys are latched like the others.
`Automation::render_tracked` drives a `TrackedResonator`, which uses all of them (the offline
`process` and `trace` commands do); `Automation::render` drives a plain `Resonator` and ignores the
modulation and root keys. `Automation::parse` still means the low-pass and gain patch. `asperitas-cli process --processor resonator --automation
knobs.txt` renders a clip through it (see [resonator.md](resonator.md#command-line)), and
`asperitas-rig session --fx-f 2 ... automation FILE` plays the same file on the rig's resonator stage.

## 32-frame latch

The device applies its knob reading once before each block of 32 frames
(`asperitas_dsp::asperitas::BLOCK_FRAMES`), and so does the render, counting blocks from the first
frame of the clip. Controls are evaluated (`Automation::controls_at`) only at block starts, so a
breakpoint at a frame that is not a multiple of 32 takes effect at the next multiple, and a ramp is
sampled at block starts. `asperitas-live` latches the same way.

## On the device

`asperitas-rig session ... automation FILE` plays the same file on the board over `CTL SET` (see
[asperitas-rig.md](asperitas-rig.md)), and `asperitas-measure analyze --excerpt` judges that session
against this render (for the patch; `analyze` has no resonator reference yet). The device schedule is `Automation::device_schedule`: a step is one update at the
block it lands in, a ramp one update every 480 frames (10 ms) rather than every block.
