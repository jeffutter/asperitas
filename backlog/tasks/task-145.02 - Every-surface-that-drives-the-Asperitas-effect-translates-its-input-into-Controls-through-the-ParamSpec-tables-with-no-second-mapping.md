---
id: TASK-145.02
title: >-
  Every surface that drives the Asperitas effect translates its input into
  Controls through the ParamSpec tables, with no second mapping
status: To Do
assignee:
  - '@agent'
created_date: '2026-10-19 22:08'
updated_date: '2026-10-19 22:08'
labels:
  - m4
  - live
  - dsp
  - planned
dependencies: []
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
  - audio/README.md
  - docs/reference/asperitas-rig.md
parent_task_id: TASK-145
priority: high
type: task
ordinal: 280200
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
M4's exit criterion (live host equals device for the same `Controls`) only means something if `Controls` is the single definition of the effect's parameters. doc-001: "exactly one definition: asperitas-dsp's `Controls` (normalised knobs in [0, 1]) and the `ParamSpec` tables (FILTER_PARAMS, GAIN_PARAMS, ASPERITAS_PARAMS), with every host translating its own input into `Controls` and nothing else." The live host (TASK-145.03, TASK-145.04) will be a fourth surface and must reuse whatever this ticket leaves as the one translation, not copy one.

Surfaces known at ticket creation (verify against HEAD, the list may be incomplete):
- Pod knobs: `firmware/src/bin/main.rs` `KnobChain::process` builds `Controls { knobs }` from `asperitas_pod` knob readings merged with host overrides (`firmware/src/host_control.rs` `merge`, `asperitas_pod::knob_override::KnobOverride`).
- `CTL SET k v`: wire u16 to level via `asperitas_wire::control::wire_to_level`, encoded host-side by `asperitas_wire::control::level_to_wire` (used by `asperitas-rig` `control.rs` and `asperitas-measure` `compare.rs` `FxSetting`). `CTL FX` only selects bypass or patch (`asperitas_stimulus::replay_fx::FxKind`); the rig firmware merges overrides over `Controls::default()` in `asperitas_stimulus::fx_chain::FxChain`.
- `asperitas-cli process --params`: `crates/asperitas-cli/src/process.rs` parses real-unit `key=value` pairs against `ASPERITAS_PARAMS`, range-checks them, and converts each with `ParamSpec::to_normalised` into `Controls`.
- Suspected second mapping: `Gain::params_from_normalised` (`crates/asperitas-dsp/src/gain.rs`) and `OnePoleLowPass::params_from_normalised` (`crates/asperitas-dsp/src/filter.rs`) sit beside the private `filter_params`/`gain_params` in `asperitas.rs`, and `asperitas-pod` knob_override tests call one. Their doc comments restate ranges and midpoints that the tables own.

The live host will take initial values with the same `key=value` real-unit syntax as the CLI and display real units, so the real-unit to `Controls` translation (parse result, range check, `to_normalised`) needs to live in one reusable place rather than inside `process.rs`.

Device-side verification: firmware has no host test harness. Any change that reaches firmware code is verified by (a) the `firmware-clippy` and `firmware-builds` gates (cross-build of every variant), (b) host tests on the no_std crates the firmware links: `asperitas_stimulus::replay_fx` tests that pin `ReplayFx` bit-identical to `Asperitas` for given knobs, and `asperitas_pod::knob_override` tests, and (c) the goldens (`crates/asperitas-cli/tests/golden_tests.rs`, 1e-6 absolute) which pin CLI output unchanged. The board is rechecked by the @human bench comparison ticket in this umbrella, which reruns the rig excerpt comparison after this lands. Prefer a change that leaves `firmware/src` untouched; if firmware code must change, say so in the notes so the bench ticket knows.

Code comments describe current state only: no ticket numbers, dates, measurement history, or line numbers of other files (CLAUDE.md, Code comments).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Ticket notes list every place in crates/ and firmware/src that turns a knob reading, wire value, CLI argument or real-unit value into effect parameters, each marked as routed through Controls and the ParamSpec tables or fixed by this ticket
- [ ] #2 No knob-to-parameter or real-unit-to-knob mapping exists outside asperitas-dsp's ParamSpec tables and Controls; params_from_normalised either is removed (callers moved to the tables or Controls) or is shown in the notes to be a one-line delegation with no restated ranges
- [ ] #3 The real-unit key=value to Controls translation the CLI uses (lookup by ParamSpec name, range check, to_normalised) is a public function in one place that a second host can call, and asperitas-cli process calls it
- [ ] #4 A unit test pins that translation: defaults give Controls::default(), each knob round-trips a real value through to_normalised and from_normalised within 1e-4 relative, unknown key and out-of-range or non-finite value are errors
- [ ] #5 Goldens are unchanged: cargo test -p asperitas-cli passes without regenerating any file under audio/goldens
- [ ] #6 The ReplayFx bit-identity tests and the knob_override tests pass unchanged in what they assert
- [ ] #7 bash scripts/gates.sh push passes (this includes firmware-clippy and firmware-builds); notes say whether firmware/src changed
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Audit: `rg -n "Controls|from_normalised|to_normalised|params_from_normalised|wire_to_level|level_to_wire|ASPERITAS_PARAMS|FILTER_PARAMS|GAIN_PARAMS|knobs" crates firmware/src`. For each hit that converts an input into parameters, record the site and its route in the ticket notes (AC #1).
2. Shared real-unit translation, in `crates/asperitas-dsp/src/asperitas.rs` (no_std, no alloc): a constructor such as `Controls::from_values(&[(&str, f32)]) -> Result<Controls, ControlsError>` (name at the implementer's choice) that starts from `Controls::default()`, finds each key in `ASPERITAS_PARAMS` by `ParamSpec::name`, rejects unknown keys and values that are non-finite or outside `[min, max]`, and sets the knob with `to_normalised`. `ControlsError` is a small enum carrying the key and the spec so the host can format the same messages `process.rs` prints today. Also a `Controls::values(&self) -> [f32; KNOB_COUNT]` (real units via the private `map_knob`) if the live readout needs it, so no host calls `from_normalised` itself.
3. `crates/asperitas-cli/src/process.rs`: the `asperitas` arm keeps `parse_param_pairs` (string splitting, std) and replaces its own range check and `to_normalised` loop with the new constructor; keep error text equivalent. The `gain` and `filter` arms are separate processors with real-unit params and stay as they are.
4. `params_from_normalised` on `Gain` and `OnePoleLowPass`: remove them and move their callers (`asperitas-pod` `knob_override` tests, their own tests) to `GAIN_PARAMS[0].from_normalised` / `Controls`, or keep them as one-line delegations with the range-restating doc comments cut. Prefer removal: one way to do it. Move any test of the taper itself to the `ParamSpec` tests if not already covered.
5. Tests in `asperitas.rs` per AC #4. Existing `default_controls_are_open_unity` stays.
6. Leave `firmware/src` untouched unless the audit finds a real second mapping there; it builds `Controls` directly from normalised readings, which is the intended route.
7. `cargo test`, `bash scripts/gates.sh push`. Commit. Notes: audit table, whether firmware/src changed.
<!-- SECTION:PLAN:END -->
