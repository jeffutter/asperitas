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
| `processor` | no | `"asperitas"` (default) or `"resonator"`, as `process --processor`. The knob keys in `params` and `automation` are that processor's. |
| `automation` | no | Knob automation file, grammar in [automation.md](automation.md). |
| `params` | no | List of `key=value` knob settings, as `process --params`. Not with `automation`. |
| `reference` | no | `"dry"` (the input clip) or the path of a 48 kHz WAV, such as a previous render. |
| `trace` | no | `true` draws the resonator stage's trace under the case (default `false`); needs `processor = "resonator"`. |

Top-level keys, before the first table:

| Key | Meaning |
|-----|---------|
| `output_format` | `"float32"` (default) embeds each render exactly as `process` writes it. `"pcm16"` embeds it as 16-bit PCM, half the size; it is for listening only, and the checks always run on the float render. |

`[[response]]` tables add the resonator's impulse and frequency response, drawn as
[analyze.md](analyze.md) draws it: `name` (unique), `params` (`key=value` resonator settings, as
`process --params`) and optional `length_s`. The page shows each as an embedded PNG with its
settings and the peaks report, after the summary table.

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
| parameter steps | `asperitas_measure::param_step::detect_param_steps_windowed` with the music window (7 ms), on the left channel | the render has more events than the dry input clip, not counting events at a scripted step (`Automation::step_frames`) |

Not covered: `asperitas-measure` has no peak or clipping check for an offline f32 render, so those
two are computed inline (a max and a count, not an analysis algorithm). Its discontinuity metrics
need a loopback capture and its alignment, so they do not apply to a render. The step detector flags
musical attacks too (the corpus clips have over a hundred events each), so the rule compares the
render against the dry clip rather than against zero; an event within a few detector hops of a scripted
step (any non-ramp breakpoint after frame 0, with or without the `step` keyword) is exempt. The
exemption is by time only, since the detector measures level and cannot say which parameter moved;
it covers those events, not the rest of the script. The check therefore catches added steps only coarsely; treat it as a tripwire
and listen.

## Size

The whole page must stay under 14 MB (the artifact page limit is 16 MB); otherwise the command fails
and suggests shorter clips or fewer cases. A 3 s corpus clip adds about 1.5 MB for its output
(stereo float WAV, base64) plus a few hundred KB of plots.

## Resonator review

[audio/review/resonator.toml](../../audio/review/resonator.toml) is the resonator's manifest: each
corpus clip dry and through the default voicing, a damping sweep and a structure sweep on each
family's chord clip (driven by `audio/review/resonator-*-sweep.txt`), and response plots for the
default voicing, damping 0 and 1, structure 0 and 1 and spread 0. It uses `output_format = "pcm16"`
to stay under the size limit. Regenerate and open it after a voicing change:

```bash
cargo run --release -p asperitas-cli -- review audio/review/resonator.toml /private/tmp/resonator-review.html
open /private/tmp/resonator-review.html
```

The build takes about 15 s in release and is deterministic: two runs give byte-identical pages.

## Modulation and tracking review

[audio/review/modulation.toml](../../audio/review/modulation.toml) holds the cases for the
envelope follower, the random walk and the tracked root, rendered for asynchronous listening. Every
case sets `trace = true`, so the page shows the trace plot ([analyze.md](analyze.md)) under the
audio: follower and walk, confidence against the 0.8 gate, and the tracker estimate with the
effective root. The follower drives damping and the random walk drives spread. Judging the
renders is the listening step, not part of this page.

- Follower to damping: the soft and hard single note and the fast run at depth 0.15 (the
  first-listen default), and the hard note and the fast run at depth 1, all with root 440 Hz.
- Random walk to spread: the mandolin chord at depth 0.5, at 0.1 Hz and at 5 Hz, root 196 Hz.
- Tracked root against manual root: the soft note, the fast run (note changes) and the chord, each
  as a `root_source=1` render and a `root_source=2` render. The chord is the case where the tracker
  is least sure of one pitch, so its parameter-steps check reports the root jumps and can fail;
  that is evidence for the listener, not a broken page.

The page uses `output_format = "pcm16"` and is about 11 MB. Build and open it:

```bash
cargo run --release -p asperitas-cli -- review audio/review/modulation.toml /private/tmp/modulation-review.html
open /private/tmp/modulation-review.html
```

Two builds give byte-identical pages. The regression goldens for the same renders are described in
[audio/README.md](../../audio/README.md).
