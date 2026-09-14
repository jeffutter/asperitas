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
  size and the sample rate PR #80 verified. 1 ms of block latency. The Seed3 codec can
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

The tiers, as `scripts/gates.sh --list` prints them. This block is generated - regenerate it with
that command, do not hand-edit rows:

```text
min    | com | psh | ci | gate
-------+-----+-----+----+------------------------------------------------
commit | yes | yes | yes | === gate definition parses ===
commit | yes | yes | yes | === docs artifact names ===
commit | yes | yes | yes | === elf-provenance --selftest ===
commit | yes | yes | yes | === cargo fmt ===
commit | yes | yes | yes | === cargo fmt (firmware workspace) ===
commit | yes | yes | yes | === cargo clippy ===
commit | yes | yes | yes | === cargo clippy (asperitas-logging log-usb) ===
commit | yes | yes | yes | === cargo clippy (asperitas-logging log-defmt) ===
push   | -   | yes | yes | === cargo clippy (asperitas-pod pod-hw feature) ===
push   | -   | yes | yes | === dump_reassemble --selftest ===
push   | -   | yes | yes | === cargo doc (workspace) ===
push   | -   | yes | yes | === cargo doc (workspace, all features) ===
push   | -   | yes | yes | === firmware cross-compile ===
push   | -   | yes | yes | === firmware cross-compile (RTT-only, log-defmt) ===
push   | -   | yes | yes | === firmware ELF cfg provenance ===
push   | -   | yes | yes | === image load addresses ===
commit | yes | yes | yes | === firmware clippy (all bins) ===
commit | yes | yes | yes | === firmware clippy (all bins, RTT-only, log-defmt) ===
push   | -   | yes | yes | === cargo test ===
ci     | -   | -   | yes | === cargo test (asperitas-pod pod-hw feature) ===

counts: commit 10, push 19, ci 20
```

Costs are local warm figures on aarch64-darwin inside `nix develop .#default`: **commit 3 s**, **push
76 s**, **ci 141 s**. The two `cargo test` invocations are 133 of
those 141 s.

The gate that moved all three numbers is `=== elf-provenance --selftest ===`, 0.9 s warm, third in the
list. It sits there because it compiles nothing and reads no artifact, which is what cheapest-first
does with a check that has no inputs to wait for; putting it beside the push-tier provenance gate "for
symmetry" would misrepresent both, since one grades the reader of an ELF's cfg stamp and the other
grades the stamp itself, and they share no state. Its cases and the cost rules that keep it at 0.9 s
(five child invocations of the script at most, one `cargo metadata`, never a `cargo build` or a path
under `firmware/target/`) are stated in `scripts/elf-provenance.sh`'s own header.

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
sets; measured by swapping the two lines, running the tier, and restoring them. It costs 0.09 s warm,
reads only, and asks cargo for nothing - a rebuild there would re-point the very path it audits.

The gate below it grades a different axis of the same six ELFs. `=== image load addresses ===` asserts
that no file-backed section loads outside the FLASH region in `firmware/memory.x`, that `.sram1_bss`
stays NOBITS while `memory.x` claims to place it `(NOLOAD)`, and that each plain `-O binary` image is
exactly as long as the highest flash LMA end implies - the invariant whose absence made `main.bin`
469,763,536 bytes of mostly zeros with every gate green, because objcopy writes from the lowest to the
highest *load* address (TASK-059). It reads rather than builds for the same reason the provenance gate
has, bare `rust-objdump` / `rust-objcopy` only, and costs 0.70 s warm. Its tier is `push` because
pre-commit builds no firmware, so the ELFs it needs may not exist there; inventing a skip path for that
is the blind spot TASK-062 had to remove from `elf-check`. Red was demonstrated by disabling the
`SECTIONS` rule in `firmware/memory.x` and relinking: three findings naming `.sram1_bss`, its LMA, and
the image length it would have produced.

What the one-command shape costs: lefthook buffers a command's stdout and replays it when the command
finishes, so a hook prints nothing for its first ~3 s (commit) or ~76 s (push). In exchange the log
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
asperitas-pod/pod-hw`, 67 s local warm - more than every push-tier gate combined - re-running the host
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

- keep clips **short (2–5 s)**, mono, 48 kHz
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
