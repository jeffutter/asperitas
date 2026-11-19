# Knob automation file

`asperitas-cli process in.wav out.wav --processor asperitas --automation knobs.txt` renders a clip
in which the knobs move. The file is the one definition of knob motion; it is parsed by
`Automation::parse` in `crates/asperitas-cli/src/automation.rs`. It works only with the `asperitas`
processor and cannot be combined with `--params` (put starting values in the file).

## Grammar

One event per line. `#` starts a comment; blank lines are ignored.

```
<seconds> [step|ramp] key=value [key=value ...]
```

- `seconds`: non-negative decimal, not earlier than the previous line. Converted to the nearest
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

## 32-frame latch

The device applies its knob reading once before each block of 32 frames
(`asperitas_dsp::asperitas::BLOCK_FRAMES`), and so does the render, counting blocks from the first
frame of the clip. Controls are evaluated (`Automation::controls_at`) only at block starts, so a
breakpoint at a frame that is not a multiple of 32 takes effect at the next multiple, and a ramp is
sampled at block starts. `asperitas-live` latches the same way.
