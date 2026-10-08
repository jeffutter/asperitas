---
id: doc-001
title: Asperitas Project Plan
type: specification
created_date: '2026-08-01 05:43'
---

# Asperitas — Project Plan

A musically-interactive audio effect for acoustic and clean electric instruments
(mandolin, octave mandolin, upright bass, bass guitar, jazz guitar), built in Rust for
the Electro-Smith Daisy Seed3 hosted in a Daisy Pod.

Hardware and ecosystem facts referenced throughout live in `docs/reference/` and are
linked from `CLAUDE.md`. This document is the *plan*; those are the *findings*.

---

## 1. Decisions

Settled through a design interview on 2026-08-01. Recorded here so they don't get
relitigated.

| Question | Decision |
|---|---|
| v1 effect | **Sympathetic resonator** — bank of tuned comb/bandpass resonators excited by the input, Rings-like |
| DSP approach | **From scratch on `dasp` primitives**, with the option to transliterate specific Mutable Instruments algorithms later where their exact character is wanted |
| Board bring-up | **Rust-only.** Blinky first, then audio. C++ is *not* a fallback (libDaisy has no Seed3 support). Contributing Seed3 support upstream is a stretch goal |
| Control surface | **Daisy Pod** — 2 knobs, encoder + click, 2 buttons, 2 RGB LEDs. No soldering, no enclosure decisions yet |
| Debug probe | **None for the first few weeks.** Flash by DFU over USB-C; debug over USB CDC serial with Pod LEDs as boot-stage fallback. `probe-rs` + `defmt` slots in later without rework |
| Desktop tooling | WAV CLI first, then live `cpal` TUI, then analysis output, then a CPU-cost harness |
| Test corpus | Real instrument recordings **committed to git**, alongside synthetic signals |
| CI | `lefthook` locally **and** GitHub Actions, all three executing `scripts/gates.sh` at ascending tiers (`commit` / `push` / `ci`). Pre-commit is the one the commit loop always runs; CI adds one priced gate the hook skips (see §5) |

### Assumptions made without asking

State them so they're cheap to overturn:

- **48 kHz, 48-sample blocks, `f32` internally.** Matches libDaisy's Pod default block
  size and the sample rate PR #80 verified. 1 ms of block latency. gate-costs:exempt reason="audio block latency from the sample rate, not a gate cost"
  The Seed3 codec can
  do 192 kHz/32-bit; there is no musical reason to pay for it here, and every reason to
  keep CPU headroom for resonator voices.
- **Mono in, stereo out.** The instruments are mono sources; a resonator bank wants
  stereo spread on the output. The frame type is stereo throughout so this is a policy,
  not a constraint.
- **`proptest`, not `quickcheck`**, for property tests.
- **Two Cargo workspaces**, host and firmware — see §3.

---

## 2. The one thing that makes this project tractable right now

**daisy-embassy PR #80 already implements Seed3 support, and its author tested it on a
Seed3 in a Daisy Pod** — this project's exact hardware. It is open, mergeable, and
unreviewed as of 2026-08-01.

This eliminates what would otherwise have been the dominant risk. The plan therefore:

1. Pins `daisy-embassy` to PR #80's **commit SHA** (not the branch — it's new and may be
   force-pushed), and revisits when it merges.
2. Treats "reproduce the author's test results and report them on the PR" as an early,
   cheap deliverable. An unreviewed PR whose author wants confirmation is where an
   independent report on identical hardware is worth the most.
3. Keeps a real fallback: if PR #80 doesn't work on our board, we own a `tac5242.rs` we
   can debug, and the codec is strapped so there's no I²C register map to reverse — see
   `docs/reference/daisy-seed3.md`.

The corollary risk is the reverse of the usual one: **libDaisy has no Seed3 support**, so
if audio doesn't work there is no C++ reference implementation to compare against.
Blinky in C++ remains available to prove the board is alive, since it touches no codec.

---

## 3. Architecture

The organising principle is the one already identified in `idea.md`: **the DSP knows
nothing about the hardware.** Everything else follows from that.

```
                      ┌───────────────────────────┐
                      │      asperitas-dsp        │
                      │  no_std, no hardware deps │
                      │  Processor trait,         │
                      │  resonator bank, filters, │
                      │  delay lines, envelopes,  │
                      │  modulation sources       │
                      └─────────────┬─────────────┘
             ┌──────────────┬───────┴───────┬──────────────┐
             │              │               │              │
   ┌─────────▼──────┐ ┌─────▼──────┐ ┌──────▼──────┐ ┌─────▼───────┐
   │ asperitas-cli  │ │ asperitas- │ │ asperitas-  │ │  firmware   │
   │ WAV in/out,    │ │ tui        │ │ bench       │ │ Seed3 + Pod │
   │ analysis       │ │ live cpal  │ │ CPU cost    │ │ thumbv7em   │
   └────────────────┘ └────────────┘ └─────────────┘ └─────────────┘
```

### Crate layout

```
asperitas/
├── crates/
│   ├── asperitas-dsp/     # no_std core. The whole point.
│   ├── asperitas-cli/     # offline WAV processing + analysis
│   ├── asperitas-tui/     # live cpal + ratatui
│   └── asperitas-bench/   # CPU cost harness
├── firmware/              # SEPARATE workspace, thumbv7em-none-eabihf
│   ├── .cargo/config.toml
│   └── src/
├── audio/                 # test corpus (see §6)
└── docs/reference/
```

**Two workspaces, deliberately.** A single workspace cannot cleanly hold both host and
`thumbv7em` targets — `forced-target` is unstable, and a workspace-wide
`[build] target` breaks the host crates. `firmware/` is excluded from the root workspace
and carries its own `.cargo/config.toml`. The cost is that nothing reaches `firmware/` by
default: `cargo fmt --all`, `cargo clippy --workspace`, `cargo doc` and `cargo test` at the
root all stop at that exclusion, so each needs a second invocation naming the firmware
workspace, and those exist now, as gates in `scripts/gates.sh`: firmware fmt, cross clippy over
all six bins in both cfg sets, and two cross-compiles. Tests remain the honest gap: a `no_std` target has no test harness to
link, so over there correctness is carried by the cross-compile, the lints, and the bench.
TASK-060 is what the ungated version looked like - `rig.rs` sat unformatted for days while
root `cargo fmt --all --check` exited 0.

### The `Processor` boundary

This is the interface every other piece is organised around, so it's worth getting
right. Sketch:

```rust
pub type Frame = [f32; 2];

pub trait Processor {
    type Params: Clone + Default;

    fn set_sample_rate(&mut self, hz: f32);
    fn set_params(&mut self, params: &Self::Params);
    fn tick(&mut self, input: Frame) -> Frame;
    fn reset(&mut self);

    /// Block processing, provided. Override only if a block-rate
    /// implementation is genuinely faster.
    fn process_block(&mut self, input: &[Frame], output: &mut [Frame]) { ... }
}
```

Design notes, per the project's design philosophy:

- **Per-sample `tick` is the primitive; `process_block` is provided.** Callers who want
  blocks get them free; implementers write the simple thing. Inverting this would force
  every processor to reimplement buffering.
- **Parameters are an associated type, not a bag of floats.** The knob-to-parameter
  mapping is a *policy* that belongs to the caller (Pod, CLI, TUI), not the DSP. The DSP
  owns the parameter *semantics*.
- **No allocation, no `Result`.** A real-time audio path that can fail or block is a
  design error. Errors are defined out of existence: clamp, saturate, and make
  degenerate parameter values into ordinary ones.
- **`set_sample_rate` is separate from construction** so the same processor instance can
  be reused across the 48 kHz device and whatever rate a WAV file happens to be.

Parameter smoothing lives inside processors, not in callers — pulling that complexity
down is what stops every one of the four hosts from reimplementing it.

---

## 4. Milestones

### M0 — Scaffold (tickets 1–3)

Nix flake dev shell, two-workspace skeleton, lefthook hooks, CI. Nothing runs yet; the
point is that everything after this is cheap.

### M1 — First light on device (tickets 4–6)

Blinky over DFU, then audio passthrough with `--features seed3`, then USB CDC serial
logging. **This is the milestone that de-risks the whole project**, and it depends on
almost nothing — it can run in parallel with M2. Ends with a test report posted to
daisy-embassy PR #80.

### M2 — DSP spine and offline tooling (tickets 7–8)

`Processor` trait, a trivial processor (gain/one-pole), property tests, and the WAV CLI.
Establishes the test harness the real DSP work will lean on.

### M3 — Pod integration

Pod BSP (pin map, ADC knobs, encoder, buttons, RGB LEDs), then wire `asperitas-dsp` into
the firmware audio callback with knobs driving parameters. First time the device makes a
sound you can change with your hands.

### M4 — Live desktop iteration

`cpal` TUI with the same parameter model as the device. From here, sound design stops
requiring a flash cycle. This is the point where the project starts being fun.

### M5 — The resonator

Comb/bandpass resonator bank, excitation shaping, damping and structure controls, stereo
spread. Golden-file regression tests against the committed corpus. Analysis output
(impulse/frequency response) to *see* what the bank does, which matters a lot given no
prior DSP background.

### M6 — Musical interactivity

Envelope follower and the smoothed random-walk modulation source from `idea.md`,
modulating resonator parameters. This is what separates the project from a static filter
bank and delivers the Mood-inspired brief.

### M7 — Polish and hardware

CPU-cost harness and headroom work, preset save/recall to QSPI flash, and only then any
decisions about enclosure, footswitch, jacks and true bypass — deferred until the effect
has told us what controls it actually needs.

---

## 5. Development workflow

### Nix

A single flake provides everything: Rust toolchain with `thumbv7em-none-eabihf`,
`clippy`, `rustfmt`, `rust-analyzer`, plus `dfu-util`, `probe-rs`, `cargo-binutils`,
`lefthook`, and the ALSA/pkg-config deps `cpal` needs. `direnv` is already wired up
(`.envrc` contains `use flake`).

Rust toolchain via `fenix` or `rust-overlay` — nixpkgs' `rustc` doesn't carry the
embedded target.

### lefthook

Both hook stages run one command each - `bash scripts/gates.sh commit` and `bash scripts/gates.sh
push`. The gate set is defined once, in that script: every check named once, tagged with the cheapest
tier that runs it, so neither the hooks nor CI can restate it and drift. Restating it *was* the bug -
`lefthook.yml` held 25 hand-written `run:` lines and CI kept a second copy of the same list, and every
check added since TASK-018 landed on one side only (TASK-060 exists because firmware was invisible to
both).

Nothing filters paths any more: no `root`, `glob`, `files` or `local` key survives in `lefthook.yml`,
and no job carries a staged-files template. A filtered job whose set is empty exits 0 without running,
so a lint behind one reads as green having checked nothing. `firmware-cross-compile` sat behind the
last such filter until TASK-061.02 deleted it; grep the file for those keys and count zero, which is
the durable check rather than this sentence. One skip survives and it is benign: lefthook ignores a
stage whose staged-file set is empty, so `git commit --allow-empty` runs no gates and a manual
`lefthook run <stage>` on an empty index needs `-f`. That one is not configurable - 2.1.10 has no key
for it, its builder skips such a command before reading any, and the only override besides `-f` is
`only:`, which measured worse still: given a failing check it reported "skip by condition" and exit 0.
An empty commit changes no tree, so the only thing it could gate is HEAD's tree - already paid for by
the commit that made it.

The tiers, priced. The block below is generated: `scripts/gate-costs.sh --render` joins
`scripts/gates.sh --list` against `docs/gate-costs.json` and writes the whole region, cost column
included, so it cannot drift from either source. Do not hand-edit a row and do not re-copy
`gates.sh --list` over the block - `scripts/gate-costs.sh --check` compares the rendered bytes with
what is on disk and fails the commit tier on any difference:

<!-- BEGIN GENERATED: gate-matrix -->
```text
key                         | min    | com | psh | ci | cost    | gate
----------------------------+--------+-----+-----+----+---------+---------------------------------------
gate-definition-parses      | commit | yes | yes | yes | 0.01 s  | === gate definition parses ===
docs-artifact-names         | commit | yes | yes | yes | 0.28 s  | === docs artifact names ===
gate-costs-current          | commit | yes | yes | yes | 0.75 s  | === gate costs current ===
elf-provenance-selftest     | commit | yes | yes | yes | ~1 s    | === elf-provenance --selftest ===
elf-staleness-selftest      | commit | yes | yes | yes | ~2 s    | === elf-staleness --selftest ===
load-addresses-selftest     | commit | yes | yes | yes | 0.95 s  | === load-addresses --selftest ===
cargo-fmt                   | commit | yes | yes | yes | 0.26 s  | === cargo fmt ===
cargo-fmt-firmware          | commit | yes | yes | yes | 0.46 s  | === cargo fmt (firmware workspace) ===
cargo-clippy                | commit | yes | yes | yes | 0.31 s  | === cargo clippy ===
cargo-clippy-log-usb        | commit | yes | yes | yes | 0.30 s  | === cargo clippy (asperitas-logging log-usb) ===
cargo-clippy-log-defmt      | commit | yes | yes | yes | 0.25 s  | === cargo clippy (asperitas-logging log-defmt) ===
clippy-pod-hw               | push   | -   | yes | yes | 0.27 s  | === cargo clippy (asperitas-pod pod-hw feature) ===
dump-reassemble-selftest    | push   | -   | yes | yes | 0.55 s  | === dump_reassemble --selftest ===
cargo-doc                   | push   | -   | yes | yes | ~4 s    | === cargo doc (workspace) ===
cargo-doc-all-features      | push   | -   | yes | yes | ~3 s    | === cargo doc (workspace, all features) ===
rig-stim-ess-build          | push   | -   | yes | yes | ~2 s    | === firmware rig build (stim-ess) ===
rig-stim-pulse-build        | push   | -   | yes | yes | 0.86 s  | === firmware rig build (stim-pulse) ===
rig-window-override-build   | push   | -   | yes | yes | 0.90 s  | === firmware rig build (shortened capture window) ===
firmware-cross-compile      | push   | -   | yes | yes | ~2 s    | === firmware cross-compile ===
firmware-cross-compile-rtt  | push   | -   | yes | yes | 0.96 s  | === firmware cross-compile (RTT-only, log-defmt) ===
firmware-elf-provenance     | push   | -   | yes | yes | 0.13 s  | === firmware ELF cfg provenance ===
image-load-addresses        | push   | -   | yes | yes | ~1 s    | === image load addresses ===
firmware-clippy             | commit | yes | yes | yes | 0.57 s  | === firmware clippy (all bins) ===
firmware-clippy-rtt         | commit | yes | yes | yes | 0.28 s  | === firmware clippy (all bins, RTT-only, log-defmt) ===
rig-stim-ess-clippy         | commit | yes | yes | yes | 0.27 s  | === firmware clippy (rig, stim-ess) ===
rig-stim-pulse-clippy       | commit | yes | yes | yes | 0.27 s  | === firmware clippy (rig, stim-pulse) ===
cargo-test                  | push   | -   | yes | yes | ~77 s   | === cargo test ===
cargo-test-pod-hw           | ci     | -   | -   | yes | ~81 s   | === cargo test (asperitas-pod pod-hw feature) ===
counts: commit 15, push 27, ci 28
```
<!-- END GENERATED: gate-matrix -->

#### Who owns each number

Every figure in that block, and every figure quoted anywhere else in this repo, has exactly one owner:

- **`docs/gate-costs.json` owns every measured number**, together with the date, host, environment,
  sample count and approximation caveat that make any of them mean anything. It is the only tracked file
  here that holds a wall-clock numeral.
- **`bash scripts/gate-costs.sh --refresh` is the one command that reproduces them all.** It runs each of
  the three tiers once, reads the seconds out of `GATES_TIMINGS_FILE` rather than off stdout, rewrites the
  ledger canonically, and renders the prose from it. Roughly four minutes of cargo and clippy, which is why
  it is a command a person runs and never a gate: four minutes does not belong in pre-commit, and that is
  TASK-068's non-goal kept rather than ignored. It refuses to record a tier that failed, because costs
  measured from a tier that does not pass are fiction. The one exception is pricing a brand-new gate,
  which can only be measured by a run in which that gate is red for want of its own entry, and it needs
  `GATE_COSTS_BOOTSTRAP=1` said out loud - see the header of the script.
- **Everything else cites keys.** Prose in `scripts/gates.sh`, `lefthook.yml`, `.github/workflows/ci.yml`,
  `firmware/Makefile` and the three checker headers carries a `gate`, `tier`, `component` or `meta` key
  inside its sentences, and this plan's matrix and cost column are a generated region. `--render` puts the
  figure where the key sits, so a priced argument keeps its number while the numeral itself stays in one
  file.
- **`scripts/gate-costs.sh --check` is what keeps the citations true.** It runs as the third gate of the
  commit tier, ahead of every cargo invocation, at 0.75 s {{gate:gate-costs-current}} warm. It renders each
  guarded file into a temp copy and compares bytes against the file on disk, so a stale figure and a
  hand-typed one are one finding rather than two rules, and it fails when a gate has no ledger entry, when
  an entry names no live gate, or when a gate's command digest stopped matching the command that runs. It
  never writes: a hook that rewrote a tracked file would commit the stale version it was meant to grade.
- **A stored figure is meaningless without the tier that paid it.** The two firmware-clippy gates cost tens
  of seconds in a fresh target directory, seconds straight after a build, and a few tenths when nothing has
  changed, because ordering rule 2 makes their price a function of adjacency. So the ledger prices every
  gate per paying tier, and the published figure for a gate is the observation from the tier whose
  `min_tier` runs it. Adding those numbers across tiers is invalid even though the tiers are cumulative as
  sets: each tier is its own process and its clock starts over.
- **Runner-side figures stay owed to TASK-052 and TASK-063.** `ci.yml` has no `workflow_dispatch`, no
  artifact upload and no `permissions:` block, so nobody can start that workflow deliberately or write a
  measurement back from it. Every number in this plan is a local one.

The approximation policy is stated once because it covers every row above: one warm sample per tier on one
machine, measured 2026-09-16 2026-10-08 {{meta:measured}} on aarch64-darwin inside `nix develop .#default`. The last digit is
noise, and a cold cache or a background Spotlight scan moves any figure here by tens of percent. What the
ledger's per-gate command digests buy is the claim that each number was taken against the command that runs
today, not that it repeats to three figures. The tier totals are **commit ~8 s {{tier:commit}}**, **push
~101 s {{tier:push}}**, **ci ~179 s {{tier:ci}}**; the two `cargo test` invocations are ~77 s {{gate:cargo-test}} and
~81 s {{gate:cargo-test-pod-hw}}, which together are most of that `ci` total.

Three selftest gates run ahead of every cargo invocation, at ~1 s {{gate:elf-provenance-selftest}},
~2 s {{gate:elf-staleness-selftest}} and 0.95 s {{gate:load-addresses-selftest}}:
`=== elf-provenance --selftest
===`, `=== elf-staleness --selftest ===` (TASK-056) and `=== load-addresses --selftest ===` (TASK-068).
All three sit there because they compile nothing and read no artifact, which is what cheapest-first does
with a check that has no inputs to wait for; putting any of them beside the push-tier gate it shares a
subject with "for symmetry" would misrepresent both, since each selftest grades the *reading* while the
push-tier gate grades the *artifact*, and they share no state. Their cases and the cost rules that keep
each at its figure are stated in the three scripts' own headers:

- the provenance reader, ~1 s {{gate:elf-provenance-selftest}}: five child invocations of the script at most,
  one `cargo metadata`,
  never a `cargo build` or a path under `firmware/target/`;
- the staleness checker, ~2 s {{gate:elf-staleness-selftest}}: one `make` per case against a fixture tree, with
  the compiler and the
  provenance clause both stubbed;
- the load-address checker, 0.95 s {{gate:load-addresses-selftest}}: seventeen cases, of which only five
  re-execute the script as a child
  process, because even the cheapest such child pays ~70 ms {{component:selftest-child-invocation}} for a fresh
  interpreter and its objdump, against a subshell that costs nothing measurable, and the exit code is
  the subject of just those five. Its two hand-emitted images are generated once per run, both binutils shims are
  checked with `command -v` before anything is emitted, and it names no path under `firmware/target/`
  and calls no `cargo` or `make` at all -- a build there would rebuild, or worse re-point, the artifacts
  the push-tier gate audits. Standing alone it stays within a few hundredths of its published figure
  across eight runs; the one outlier reading came from the first run after the disk cache had been
  displaced, and this is the first gate in
  the tier to exec `llvm-objdump`, so it is where that page-in gets paid.

Order within a tier is whatever the script declares, cheapest-first, with two rules that outrank cost:
the console cross-build comes immediately before the RTT-only one with nothing building firmware after
them, and the two cross-clippy gates follow both builds so they reuse the artifacts. Neither rule is
expressible in hook config - lefthook sorts a stage by priority, then leading digits, then command name,
never by declaration order - which is the reason the definition is a script and not YAML.

Rule one is now checked, not just declared. `release/main` is one name for two
images - the console and RTT-only builds are hardlinks to different `deps/main-<hash>` artifacts, and
whichever ran last owns the path every `make probe-*` decodes with - so "the pair ran in this order"
and "that path holds the RTT-only image" are one claim stated twice. `=== firmware ELF cfg provenance
===` reads the second statement out of the ELF's `.asp.prov` note via `scripts/elf-provenance.sh`, so
swapping the pair or inserting a gate that compiles firmware after it fails the run naming both cfg
sets; measured by swapping the two lines, running the tier, and restoring them. It costs
0.13 s {{gate:firmware-elf-provenance}} warm,
reads only, and asks cargo for nothing - a rebuild there would re-point the very path it audits.

The gate below it grades a different axis of the same six ELFs. `=== image load addresses ===` asserts
that no file-backed section loads outside the FLASH region in `firmware/memory.x`, that `.sram1_bss`
stays NOBITS while `memory.x` claims to place it `(NOLOAD)`, and that each plain `-O binary` image is
exactly as long as the highest flash LMA end implies - the invariant whose absence made `main.bin`
469,763,536 bytes of mostly zeros with every gate green, because objcopy writes from the lowest to the
highest *load* address (TASK-059). It reads rather than builds for the same reason the provenance gate
has, bare `rust-objdump` / `rust-objcopy` only, and costs ~1 s {{gate:image-load-addresses}} warm. Its tier is `push` because
pre-commit builds no firmware, so the ELFs it needs may not exist there; inventing a skip path for that
is the blind spot TASK-062 had to remove from `elf-check`. The commit-tier `=== load-addresses
--selftest ===` gate above is not a second copy of this claim and does not make this one redundant: that
gate reads hand-emitted fixtures to assert the *parser*, and this one reads the six linked images to
assert the *link*, which no fixture can stand in for. Red was demonstrated by disabling the
`SECTIONS` rule in `firmware/memory.x` and relinking: three findings naming `.sram1_bss`, its LMA, and
the image length it would have produced.

What the one-command shape costs: lefthook buffers a command's stdout and replays it when the command
finishes, so a hook prints nothing until the tier it just ran has finished - for as long as that tier
costs, which is the ledger's business and the paragraph above owns the figure. In exchange the log
carries per-gate headers and wall times it never had, and the run stops at the first failure naming the
gate that died.

### CI

GitHub Actions runs the same script at full tilt - `nix develop .#default --command bash
scripts/gates.sh ci` - so the repo holds exactly one list and the runner executes it verbatim. CI used
to keep its own copy inline in a single-quoted `bash -c '...'` string, a shape that cannot carry
comments: an apostrophe closed the quote and the step died at end of file, unnoticed for three days
because nothing had been pushed since the last green run (TASK-060 found it that way). The workflow now
names no checks at all; `.github/ci-steps.sh`, the intermediate fix, is gone, and the parse check that
file needed is now the first gate in the script.

Exactly one gate lives in the `ci` tier alone: `cargo test --workspace --features
asperitas-pod/pod-hw`, ~81 s {{gate:cargo-test-pod-hw}} local warm - more than every push-tier gate combined
- re-running the host
suite under one non-default feature flag whose compile-time half (`clippy --features
asperitas-pod/pod-hw`) does run on push. Priced and argued at its own gate in `scripts/gates.sh`,
taking the split TASK-018.01's fixup made deliberately (`c44b9c1`). Reopen it if pushes become routine
or the commit loop starts pushing. CI stays the authority either way, immune to `--no-verify`, and it
remains the only place these numbers have ever been observed on a real runner: main sits far ahead of
`origin/main` and the workflow has no `workflow_dispatch`, so runner-side figures stay owed to TASK-052
and TASK-063.


---

## 6. Testing strategy

Four layers, each catching what the others can't.

### Property tests (`proptest`)

Invariants that must hold for any processor and any input:

- output is always finite — no `NaN`, no infinity, for any parameter combination
- output is bounded (no runaway feedback under any legal parameter set)
- silence in, silence out, once any tail has decayed
- `reset()` is idempotent, and a reset processor given identical input produces
  identical output
- `process_block` agrees sample-for-sample with repeated `tick`
- parameter changes never produce discontinuities above a threshold (this is the test
  that catches missing smoothing)

Resonator-specific properties: energy decays for damping < 1; a resonator tuned to *f*
responds most strongly to input at *f*.

### Golden-file regression tests

Freeze known-good output for a given input + parameter set; assert future changes don't
silently alter it within float tolerance. This is what catches "it still works, but it
sounds different" — the failure mode property tests structurally cannot see.

**Corpus policy.** Real instrument recordings are committed to git alongside synthetic
signals (impulses, sweeps, plucked-string stubs). To keep this from becoming painful:

- keep clips **short (2–5 s)**, mono, 48 kHz gate-costs:exempt reason="audio clip length in the test corpus policy, not a gate cost"
- goldens are regenerated deliberately via an explicit command, never automatically —
  every regeneration is a reviewable diff
- a golden diff in a PR means "listen to this before accepting it", not "run the update
  command"

### Unit tests

Ordinary tests for the parts with knowable correct answers: filter coefficient
computation, note/frequency conversion, parameter mapping curves, ring buffer indexing.

### Analysis output

Not a test, but the thing that makes DSP learnable without prior background: impulse
response, frequency response, RMS and spectrogram dumps from the CLI. Hearing that a
resonator bank is wrong is much harder than seeing it.

### What is deliberately *not* automated

Anything requiring the device: CPU headroom, ADC/pot jitter, real latency, USB/audio
glitching. These get a manual checklist per firmware ticket instead of a CI job that
would need hardware in the loop.

---

## 7. Risks

| Risk | Severity | Mitigation |
|---|---|---|
| PR #80 doesn't work on our board | High | It was tested on this exact Seed3-in-Pod configuration. If it fails, we own the code and the codec is strapped — no I²C to reverse. Blinky in C++ isolates "board dead" from "our Rust wrong" |
| PR #80 force-pushed or reworked | Medium | Pin to commit SHA `477083b0227d`, not the branch. Revisit on merge |
| No debug probe for weeks | Medium | USB CDC serial gives real text logging with no probe; Pod LEDs cover pre-USB boot stages. `probe-rs`/`defmt` slots in later without rework |
| Gain staging mistaken for DSP bugs | Medium | Pod input is line level, not hi-Z. Documented in `docs/reference/daisy-pod.md`. Use a DI/preamp; compare device against CLI on identical source |
| No prior DSP experience | Medium | From-scratch on `dasp` is the *learning* choice, not the fast one. Analysis output makes behaviour visible. Resonators are the right first algorithm — a comb filter is a delay line plus feedback |
| CPU headroom exhausted late | Medium | CPU-cost harness in M7 is arguably too late; if voice counts start feeling ambitious, pull it forward |
| Committed audio corpus bloats the repo | Low | Short mono clips, deliberate regeneration only |
| Two-workspace friction | Low | Every root-level cargo command stops at the exclusion, so firmware gets its own invocations: fmt, cross clippy over all six bins in both cfg sets, and two cross-compiles, in every tier of `scripts/gates.sh` (TASK-060, TASK-061). Residuals named rather than glossed: firmware docs are ungated - `cargo doc` covers `crates/*` only - pedantic clippy lints are unadopted, and the `pod-hw` *test* runs in CI alone |

---

## 8. Deliberately deferred

Not decided, and not needing to be:

- Enclosure, footswitch, true bypass, jacks, and whether this ends up a pedal or a
  desktop box — deferred to M7, once the effect has revealed what controls it wants
- Whether to transliterate Mutable Instruments algorithms (Rings/Clouds) for their
  specific character, versus staying fully from-scratch
- MIDI, SD card, and preset management beyond simple QSPI save/recall
- Any of the other effect directions in `idea.md` (call-and-response looper,
  chord-aware harmonizer, granular wash). The `Processor` boundary is what keeps these
  cheap to try later — they become new implementations, not new architectures
