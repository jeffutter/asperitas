---
id: TASK-070.02
title: >-
  Add scripts/gate-costs.sh: the measurement ledger, recorder, renderer and
  checker
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-14 15:14'
updated_date: '2026-09-14 15:15'
labels:
  - planned
dependencies:
  - TASK-070.01
parent_task_id: TASK-070
priority: high
type: chore
ordinal: 122800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The mechanism that ends TASK-070's drift: gate cost figures become data in one committed ledger (docs/gate-costs.json) written by a command that actually times the tiers, prose cites keys, and a cheap check renders prose from the ledger and fails on any byte of difference - which covers stale values and hand-typed numbers with one rule. Ships un-wired; registering it as a gate is TASK-070.04 and cannot happen before TASK-070.03 removes the literal figures from the prose. Copy the house conventions from scripts/check-doc-artifact-names.sh: root anchoring, all violations reported in one run, and exit 2 when the check itself could not run.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `--refresh` runs each tier once via GATES_TIMINGS_FILE, writes docs/gate-costs.json canonically (stable key order so regeneration is byte-stable) recording per-gate seconds per paying tier, tier totals, measured date, host, environment, sample count, an approximation caveat, a sha256 of each gate's dry-run command line, and dated `components` entries for micro-costs. Refuses to run outside the dev shell with exit 2 and a one-line remedy.
- [ ] #2 `--render` substitutes {{gate:...}}/{{tier:...}}/{{component:...}}/{{meta:...}} keys with fixed formatting rules and rewrites marked generated regions between HTML comment markers; unknown key exits 2 rather than leaving a hole; idempotent on a rendered tree.
- [ ] #3 `--check` renders into temp copies and compares bytes against disk across the eight guarded files, reports every violation in one run naming file and line, never writes, and exits 0/1/2 per the house convention. Measured three times at under 1 s added to the commit tier, with the numbers recorded.
- [ ] #4 `--check` enforces: every live gate key has a ledger entry and every entry names a live gate; each gate's command digest still matches `--dry-run`; no wall-clock duration literal appears in a guarded file outside a generated region except a reasoned, counted exemption; no dead ledger entries. No date-based staleness anywhere.
- [ ] #5 `--selftest` drives the shipped script through fixtures under mktemp -d with an EXIT trap, covering stale value, unknown key, dead entry, gate added without entry, gate removed leaving entry, digest changed, literal duration, reasoned exemption, mismatched region markers, malformed ledger exit 2, missing generator exit 2, and one case asserting every guarded file is byte-identical after `--check`.
- [ ] #6 Depends only on jq, awk and coreutils - python3, hyperfine and git resolve locally solely because nix develop inherits the user PATH and are not in flake.nix's packages, so a gate needing them would be green here and broken on a clean runner.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# Shape

One new script, `scripts/gate-costs.sh`, holding the whole mechanism: where a measurement lives, how
it gets measured, how it reaches prose, and how a stale one turns a commit red. It is not registered
as a gate here - that is TASK-070.04, and it cannot be earlier because the prose still holds literal
figures until TASK-070.03 removes them. Ship this un-wired and let TASK-070.03/04 pull it in.

The design decision worth stating before the steps: **the figures become data, and prose holds
keys.** The alternative considered was one generated table in doc-001 with every number deleted from
`gates.sh`'s comments. It is simpler but it guts `gates.sh`, whose ordering arguments are priced ("a
cfg switch finishes in {{...}} by re-uplifting the cached hardlink, so the check would grade its own
side effect"). Keys keep the argument beside the code while making two files disagreeing structurally
impossible: after TASK-070.03 a numeral appears literally in exactly one file, the ledger.

## Step 1 - the ledger, `docs/gate-costs.json`

First committed machine-readable data file in the repo (`git ls-files` shows only tool config as
JSON today), so its shape has to be justified in the header, not just written down.

    { "schema": 1,
      "measured_utc": "...", "host": {...}, "environment": "nix develop .#default",
      "samples_per_tier": 1,
      "approximation": "one warm sample per tier; the last digit is noise",
      "tiers": { "commit": {"seconds": 4.3, "gates": 12}, "push": {...}, "ci": {...} },
      "gates": [ { "key": "elf-provenance-selftest", "min_tier": "commit",
                   "banner": "=== elf-provenance --selftest ===",
                   "command_sha256": "<64 hex>",
                   "cost": { "commit": 0.89, "push": 0.88, "ci": 0.89 } } ],
      "components": [ { "key": "objcopy-invocation", "display": "~50 ms",
                        "measured_utc": "...", "method": "timed loop of rust-objcopy over 7 images" } ] }

Two facts this schema has to carry, both learned the hard way in this repo:

- **A cost is not intrinsic to a gate.** The two firmware-clippy gates cost ~20 s in a fresh target
  dir, ~2 s straight after a build, and 0.25 s when nothing changed, and ordering rule 1 makes the
  value depend on adjacency, not membership. So `cost` is keyed by which tier's process paid it, and
  the published figure for a gate is defined mechanically as the observation from its own `min_tier`.
  Cross-tier arithmetic is invalid even though tiers are cumulative as sets: each tier is a separate
  process and `SECONDS` restarts.
- **Component micro-costs need somewhere to live too**, or the ban on literals in step 4 forces
  silly prose. They were profiled ad hoc and cannot be re-measured automatically, so they are
  `components`: a display string, the date, and the method that produced it. That is AC #1's second
  branch - explicitly approximate, dated - and it kills the duplicates found while planning
  (`gates.sh:203` says one `cargo metadata` at ~85 ms, `elf-provenance.sh:252,267` say two, and
  `:163` says 82 ms).

Serialize canonically (`jq -S`, fixed indent, LF, gates in declaration order) so byte-stability holds:
a non-empty diff after regeneration must always mean a real change, never key reordering.

## Step 2 - `--refresh` (measure, then publish): the one command AC #2 asks for

Runs `bash scripts/gates.sh <tier>` once per tier with `GATES_TIMINGS_FILE` set (TASK-070.01), takes
each gate's seconds and the tier total from the sink rather than from stdout parsing, computes each
gate's `command_sha256` from `--dry-run`, writes the ledger, then renders. Refuse to run outside the
dev shell with exit 2 and the remedy on one line (`gates.sh` already exits 3 there; do not inherit
its confusion). Default one sample per tier, recorded as such in `samples_per_tier`; an optional
`--repeat N` stores the median and says so. Roughly four minutes total (4 + 77 + 144 s local warm):
that is why this is a human-invoked command and not a gate, which is TASK-068's non-goal restated.

`--record` (ledger only) and `--render` (prose only) exist separately for debugging; `--refresh` is
what the docs name. Keep them composable so a future CI job can measure without publishing.

## Step 3 - rendering: keys and generated regions

Key syntax `{{gate:<key>}}`, `{{tier:<tier>}}`, `{{component:<key>}}`, `{{meta:measured}}`. It has to
survive bash comments, YAML comments and markdown alike; nothing in this repo uses braces today.
Formatting is fixed and stated in the header: below 1 s print two decimals, at or above 1 s print the
nearest integer prefixed `~`, so `0.89` -> `0.89 s`, `4.3` -> `~4 s`. An unknown key is exit 2, not a
silent hole, and a ledger entry no prose cites is a violation - dead figures are how drift comes
back.

Structural blocks in doc-001 (the tier x gate matrix, and a new cost column joined from the ledger)
are delimited by HTML comment markers, `<!-- BEGIN GENERATED: gate-matrix -->` / `END`. doc-001 has
no HTML comments today, so the markers are unambiguous. Everything between belongs to the generator;
everything else passes through untouched.

## Step 4 - `--check`, the rules, and the ban that does the actual work

Render every guarded file into a temp copy and compare bytes against disk. Guarded set:
`scripts/gates.sh`, `lefthook.yml`, `.github/workflows/ci.yml`, `firmware/Makefile`,
`scripts/elf-provenance.sh`, `scripts/check-elf-staleness.sh`,
`scripts/check-image-load-addresses.sh`, `backlog/docs/doc-001 - Asperitas-Project-Plan.md`. Note
this deliberately reaches into `backlog/docs/`, unlike `check-doc-artifact-names.sh:41-42`, which
excludes `backlog/**` - doc-001 is the publisher, so excluding it would guard nothing that matters.

Because the renderer is byte-exact, one rule covers both stale values and hand-typed ones: **no
wall-clock duration literal may appear in a guarded file except inside a generated region.** A
hand-written number simply fails the comparison against the rendered text, and a number that a human
wants to keep must become a key. Where a literal genuinely is not a published cost claim - the quoted
`Finished in 0.29s` cargo output at `firmware/Makefile:284` - allow an inline exemption marker
carrying a reason, count exemptions in the summary line, so creep is greppable rather than silent.
This is the house style already used twice: assert the absence of a construct over the recipe text
(`check-elf-staleness.sh:273-274` forbids `-newer`), and make grep the durable check rather than the
prose (`lefthook.yml:27`).

Plus three structural rules, all cheap because they read `--list` and `--dry-run` only:

1. Every key in `gates.sh --list ci` has a ledger entry, and every ledger entry names a live gate.
   This is what makes adding a gate visible without a person noticing prose disagreed.
2. Each gate's `command_sha256` still matches `--dry-run`. A changed command means the published cost
   is a guess. Per-gate granularity, deliberately: this is the same content-fingerprint idea
   `firmware/Makefile:221-274` runs on ELF inputs, and it is why retiering or renaming a banner does
   not invalidate twenty unrelated measurements. Do **not** date-compare anything - TASK-056 exists
   because a date-based freshness rule went red on 2975 s of pure mtime churn.
3. Ledger gate count equals `counts:` from `--list`.

Report every violation in one run through a single array and one `report()` pass, never early-exit per
finding, copying `check-doc-artifact-names.sh:60,195-210`. Anchor to the repo root first
(`:49-50` shape). Exit codes exactly as the neighbours state them: 0 clean, 1 violations (all of
them), 2 the check could not run - missing `jq`, missing ledger, `gates.sh --list` failing. A check
that cannot run never reports success.

## Step 5 - `--selftest`, in the house pattern

Every other checker here drives the *shipped* script through fixtures under `mktemp -d` with an EXIT
trap (`check-elf-staleness.sh` overrides `ELF`/`ELF_INPUTS`/`CARGO` to keep `make` in a fixture tree).
Copy that: allow `GATE_COSTS_LEDGER` and `GATE_COSTS_GATES_SH` overrides so cases point at a stub
gate list instead of executing the real tier. Cases to cover, at minimum: stale value; unknown key;
dead ledger entry; gate added with no entry; gate removed with an entry left behind; command digest
changed; literal duration outside a generated region; literal duration exempted with a reason;
generated-region markers mismatched; malformed ledger -> exit 2; missing stub gates.sh -> exit 2.
Add one case asserting `--check` leaves every guarded file byte-identical (digest before and after):
this repo has no dirty-tree discipline anywhere and no `git diff --exit-code` precedent, and a hook
that rewrote a tracked file would commit the stale version silently. Use `sha256sum` falling back to
`shasum -a 256`, the spelling at `firmware/Makefile:238`.

## Constraints

- Depend on `jq`, `awk`, coreutils only. `python3`, `hyperfine` and `git` resolve on this machine
  solely because `nix develop` inherits the user PATH - none is in `flake.nix:37-58` - so a gate that
  needs them is green locally and broken on a clean runner.
- `--check` must stay under about 1 s. Budget: `--list` 64-70 ms, `--dry-run` similar,
  `check-doc-artifact-names.sh` at 169-190 ms is the nearest comparable gate, and the commit tier's
  whole published budget is ~4 s. Measure it three times and record the numbers.
- No path filters, no skip paths, no tier exceptions in YAML: `lefthook.yml:23-46` records why, and
  any tier asymmetry gets written at the `gate` line with its price.
- Runner-side figures stay out. `ci.yml` has no `workflow_dispatch`, no artifact upload and no
  `permissions:`, so a runner cannot write a record back; those figures belong to TASK-052/TASK-063.
<!-- SECTION:PLAN:END -->
