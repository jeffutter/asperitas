# Review page

`asperitas-cli review <manifest.toml> <out.html>` renders a list of cases through the asperitas
effect and writes one self-contained HTML page: audio to listen to, plots that make a defect
visible, the settings used, and the objective checks. It exists so an agent, which cannot hear, can
hand the owner something to review when convenient. The code is `crates/asperitas-cli/src/review.rs`.

The page opens from a local file with no network and no server: audio is embedded as `data:` WAV
URIs, the waveform is inline SVG, the spectrogram an embedded PNG, and there is no script. The
command does not publish it. The person or agent that runs it publishes the file (for example as an
artifact page) when it wants the owner to see it. No device is needed.

## Manifest

TOML, one `[[case]]` table per case. Unknown keys are an error. Paths resolve relative to the
manifest's directory.

| Key | Required | Meaning |
|-----|----------|---------|
| `name` | yes | Shown on the page; must be unique. |
| `clip` | yes | Input WAV, 48 kHz, mono or stereo, 16-bit PCM or 32-bit float. |
| `automation` | no | Knob automation file, grammar in [automation.md](automation.md). |
| `params` | no | List of `key=value` knob settings, as `process --params`. Not with `automation`. |
| `reference` | no | `"dry"` (the input clip) or the path of a 48 kHz WAV, such as a previous render. |

A case with neither `automation` nor `params` renders with every knob at its default. The example
manifest is [audio/review/example.toml](../../audio/review/example.toml) and runs on the committed
corpus:

```bash
cargo run -p asperitas-cli -- review audio/review/example.toml /private/tmp/review.html
```

The command exits 1 with a message naming the case and field on a manifest that is not valid TOML
or has an unknown key, a duplicate name, no cases, a missing clip, automation or reference, a clip
that is not 48 kHz, or an automation or params error from `process`.

## What the page shows per case

- Settings: the params, or the automation file text verbatim.
- Output: a player for the render (stereo 32-bit float WAV exactly as `process` writes it, not
  downmixed or quantised), its waveform and its spectrogram. The same for the reference when given.
- Checks on the output, each marked passed or FAILED. A case with any failed check is marked FAILED
  in its header and in the summary table at the top; nothing is hidden.

## Checks and pass/fail rules

| Check | Source | Fails when |
|-------|--------|-----------|
| peak level | computed in `review.rs` | never (informational) |
| clipping | computed in `review.rs` | any sample in either channel has magnitude of 0.999 or more |
| parameter steps | `asperitas_measure::param_step::detect_param_steps`, on the left channel | the render has more events than the dry input clip, and the automation has no `step` line |

Not covered: `asperitas-measure` has no peak or clipping check for an offline f32 render, so those
two are computed inline (a max and a count, not an analysis algorithm). Its discontinuity metrics
need a loopback capture and its alignment, so they do not apply to a render. The step detector flags
musical attacks too (the corpus clips have over a hundred events each), so the rule compares the
render against the dry clip rather than against zero; a `step` line in the automation is expected to
flag and is exempt. The check therefore catches added steps only coarsely; treat it as a tripwire
and listen.

## Size

The whole page must stay under 14 MB (the artifact page limit is 16 MB); otherwise the command fails
and suggests shorter clips or fewer cases. A 3 s corpus clip adds about 1.5 MB for its output
(stereo float WAV, base64) plus a few hundred KB of plots.
