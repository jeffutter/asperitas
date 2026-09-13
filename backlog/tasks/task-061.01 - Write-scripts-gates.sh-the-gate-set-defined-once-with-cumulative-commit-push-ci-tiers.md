---
id: TASK-061.01
title: >-
  Write scripts/gates.sh: the gate set defined once, with cumulative
  commit/push/ci tiers
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-13 03:09'
updated_date: '2026-09-13 03:52'
labels:
  - planned
dependencies: []
parent_task_id: TASK-061
priority: medium
type: chore
ordinal: 100800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add scripts/gates.sh as the single definition of the gate set: every fmt/lint/test/build check named exactly once, each tagged with the cheapest tier that runs it (commit is a strict subset of push, push a subset of ci), with per-gate headers and wall times, plus --dry-run and --list so equivalence with the current three lists can be checked mechanically rather than asserted.

Additive work: nothing invokes the script yet, so this commit cannot break a build. TASK-061.02 switches ci.yml and lefthook over and deletes what they were restating. See the plan for the exact gate table, the ordering rules that outrank cheapest-first, and the equivalence recipe.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 scripts/gates.sh exists and is executable, anchors itself to the repo root via BASH_SOURCE rather than trusting the caller cwd, runs its tier sequentially cheapest-first, fails fast naming the gate that died, defaults to ci when no tier argument is given, and exits non-zero on an unknown tier or on a tier that matched zero gates.
- [x] #2 Tier membership equals the three current lists exactly - 9 pre-commit gates, 16 pre-push, 17 CI - with "cargo test --workspace --features asperitas-pod/pod-hw" the only single-tier item, carrying over the priced exception recorded at lefthook.yml:65-73 including its reopen condition (TASK-061 AC #3).
- [x] #3 --dry-run prints the commands a tier would run without executing anything, and --list prints the tier x gate matrix. Both are used to prove equivalence mechanically against the old lists (extracted run: lines plus ci-steps.sh blocks, normalising the cd-firmware and env-prefix spelling differences); the diffs go in the ticket notes as AC #2 evidence.
- [x] #4 Every gate keeps its existing banner label verbatim - including the two cargo doc banners TASK-052 greps CI output for - and gains a one-line wall-time print, so per-gate cost survives having one call site instead of sixteen.
- [x] #5 Firmware gates run through a scoped subshell so no leaked cd can retarget a later host gate to the wrong workspace with exit 0, and the console cross-build stays immediately before the RTT-only one with nothing building firmware after them (TASK-062: whichever build ran last is what the bench ELF names).
- [x] #6 A thumbv7em preflight fails with a plain instruction to enter nix develop .#default rather than an inscrutable E0463 when the embedded std is missing.
- [x] #7 Evidence: all three tiers green end to end inside nix develop on a clean tree; the pre-change baseline for the same tiers captured first; git status --porcelain empty afterwards; firmware/target/thumbv7em-none-eabihf/release/main restored to its starting sha256.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Shape

Additive only: create `scripts/gates.sh`, the one definition of the gate set. Nothing points at it
yet - `.github/ci-steps.sh` and `lefthook.yml` keep running today, so this commit cannot break a
build. Prove equivalence here, with `--dry-run`, before TASK-061.02 switches the callers over.

Why a *script* and not the hook config as the source of truth: lefthook sorts commands by name, not
by declaration order (`lefthook dump` proves today's pre-commit order is `ci-steps-parse, clippy,
clippy-firmware, clippy-firmware-rtt, clippy-log-defmt, clippy-log-usb, doc-artifact-names,
fmt-check, fmt-check-firmware`), so "cheapest gate first" and "cross-build before cross-clippy so
clippy reuses the artifacts" are unexpressible in YAML. They are also the two things that make CI's
list cheaper than the hook's. A sequential script owns ordering, and both callers just invoke it.

## Step 1 - the interface

    scripts/gates.sh [commit|push|ci]      # run that tier, fail fast
    scripts/gates.sh --dry-run [tier]      # print what would run, execute nothing
    scripts/gates.sh --list [tier]         # print the tier x gate matrix (doc-001 reads this)
    scripts/gates.sh --help

Tiers are cumulative (`commit` is a subset of `push` is a subset of `ci`), which is exactly how the
three lists relate today (measured: 9 / 16 / 17 gates, pre-commit a strict subset of pre-push, and
pre-push vs CI differing by exactly two items, both deliberate). No argument means `ci`: forgetting
the argument must never mean "run less than everything". An unrecognised tier exits 2 with a
message. In `run` mode, if zero gates matched the tier, exit 1 - a tier that silently runs nothing
is the same class of bug as a lint behind an empty path filter.

## Step 2 - the skeleton

    #!/usr/bin/env bash
    set -euo pipefail

    ROOT=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
    cd "$ROOT" || exit 1

Anchoring is load-bearing, not hygiene. Today `.github/ci-steps.sh` assumes cwd == repo root with no
anchor, and run from `firmware/` it produces *wrong-workspace passes with exit 0*: `cargo fmt --all
--check` checks the firmware workspace instead of the host one, `cargo clippy --workspace` means the
firmware workspace, `-p asperitas-logging` errors, and `cd firmware` dies and takes both
cross-compiles with it. `scripts/check-doc-artifact-names.sh:37-38` is the in-repo precedent for
anchoring; copy its shape.

Each gate is one line declaring its minimum tier, its banner, and its command:

    gate <min-tier> "<banner>" <command...>

with `gate()` doing the tier comparison, the timing print, and the exec. Ordering inside the file is
the execution order; put the cheap stuff first. Sketch of the guts (keep it this small):

    RAN=0
    CURRENT=""
    trap '[ -n "$CURRENT" ] && printf "\n*** gate failed: %s\n" "$CURRENT" >&2 || true' ERR

    gate() {                                # gate <min-tier> <banner> <command...>
      local min=$1 banner=$2 rank t
      case $min in commit) rank=1;; push) rank=2;; ci) rank=3;;
                 *) printf 'bad min-tier: %s\n' "$min" >&2; exit 2;; esac
      shift 2
      if [[ $MODE != run ]]; then printf '%-6s %s\n' "$min" "$banner"; return 0; fi
      (( RANK >= rank )) || return 0
      CURRENT=$banner
      printf '\n%s\n' "$banner"
      t=$SECONDS
      "$@"
      printf '--- %ds\n' "$(( SECONDS - t ))"
      RAN=$(( RAN + 1 ))
      CURRENT=""
    }

Do not get clever beyond that: no arrays-of-structs, no eval, no registry file. The whole point is
that adding a check is one readable line.

Firmware commands need their own subshell, because `.cargo/config.toml` supplies the thumbv7em
target and link args from the CWD (cargo #9670) while an inherited `cd` would quietly retarget every
later host gate:

    fw() { ( cd "$ROOT/firmware" && "$@" ); }

and the two rustdoc gates need their env inline: `env RUSTDOCFLAGS="-D warnings" cargo doc ...`.

## Step 3 - the gate list, in this order

Membership must equal today's three lists exactly. Banner text must be copied **verbatim** from
`.github/ci-steps.sh`, because TASK-052 AC #2 greps CI output for `=== cargo doc (workspace) ===`
and `=== cargo doc (workspace, all features) ===`; renaming a banner breaks an open @human ticket's
acceptance criterion for no benefit. Order below is cheapest-first with two hard exceptions noted
after the table.

| tier | banner (verbatim) | command |
|---|---|---|
| commit | `=== gate definition parses ===` | `bash -n "$ROOT/scripts/gates.sh"` |
| commit | `=== docs artifact names ===` | `scripts/check-doc-artifact-names.sh` |
| commit | `=== cargo fmt ===` | `cargo fmt --all --check` |
| commit | `=== cargo fmt (firmware workspace) ===` | `cargo fmt --manifest-path firmware/Cargo.toml --all --check` |
| commit | `=== cargo clippy ===` | `cargo clippy --workspace --all-targets -- -D warnings` |
| commit | `=== cargo clippy (asperitas-logging log-usb) ===` | `cargo clippy -p asperitas-logging --features log-usb --lib -- -D warnings` |
| commit | `=== cargo clippy (asperitas-logging log-defmt) ===` | `cargo clippy -p asperitas-logging --features log-defmt --lib -- -D warnings` |
| push | `=== cargo clippy (asperitas-pod pod-hw feature) ===` | `cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings` |
| push | `=== cargo doc (workspace) ===` | `env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` |
| push | `=== cargo doc (workspace, all features) ===` | `env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features` |
| push | `=== firmware cross-compile ===` | `fw cargo build --release --features seed3` |
| push | `=== firmware cross-compile (RTT-only, log-defmt) ===` | `fw cargo build --release --no-default-features --features "seed3 log-defmt"` |
| commit | `=== firmware clippy (all bins) ===` | `fw cargo clippy --release --features seed3 --bins -- -D warnings` |
| commit | `=== firmware clippy (all bins, RTT-only, log-defmt) ===` | `fw cargo clippy --release --no-default-features --features "seed3 log-defmt" --bins -- -D warnings` |
| push | `=== cargo test ===` | `cargo test --workspace` |
| push | `=== dump_reassemble --selftest ===` | `cargo run -p asperitas-logging --example dump_reassemble -- --selftest` |
| ci | `=== cargo test (asperitas-pod pod-hw feature) ===` | `cargo test --workspace --features asperitas-pod/pod-hw` |

Two ordering rules are load-bearing and outrank cheapest-first:

- The console cross-build must come immediately before the RTT-only cross-build, in that order, and
  nothing may build firmware after them. Whichever build ran last is what
  `target/thumbv7em-none-eabihf/release/main` names (they are hardlinks to different
  `deps/main-<hash>` artifacts), and every `make probe-*` target decodes whatever that file
  currently is. Today both CI and pre-push end with the RTT-only build; preserve that end state
  byte-for-byte. TASK-062 owns the underlying cfg-provenance blindness - do not "fix" it here, and
  do not collapse the pair.
- Cross-clippy must come after the cross-builds in the `push`/`ci` tiers, so clippy reuses their
  artifacts (~2 s warm instead of ~20 s fresh, per `.github/ci-steps.sh:90-92`). Note the tiers stay
  where they are today: the firmware clippies are `commit` tier (as today) even though they sit
  after the builds in declaration order.

Keep every rationale comment that exists today, once, next to its gate: the two `cargo doc` runs are
not supersets of each other (TASK-049), the log-usb/log-defmt clippies are the only gates that
compile those record paths (TASK-047), firmware is a second workspace so `--all` never meant all
(TASK-044/TASK-060), and `--check` is not optional - a bare `cargo fmt` would rewrite
`firmware/src/**/*.rs`, which are `elf-check` inputs (`firmware/Makefile:216,228-230`), and close
the bench's log decoder until TASK-056 lands. Where the two existing files disagree on a cost figure
for the same command (`~1 s` at `lefthook.yml:28` vs `~0.4 s` at `ci-steps.sh:27` for the firmware
fmt gate; measured truth 0.41 s), write one number.

The `ci`-only test gate carries the priced exception (TASK-061 AC #3 - exclusions must be written,
never merely absent). Carry over the substance of `lefthook.yml:65-73`: 67 s local warm, more than
every other pre-push command combined; its compile-time half *is* in the push tier as the pod-hw
clippy, so what stays remote is runtime coverage of pod-hw code paths; CI is the authority for it;
TASK-018.01's fixup made the same split on purpose (commit `c44b9c1`); the loop that writes most
commits here never pushes. Reopen condition, stated as a condition: if pushes become routine or the
loop starts pushing, re-measure and reconsider.

## Step 4 - thumbv7em preflight

Before the first firmware gate, in `run` mode only, check
`[ -d "$(rustc --print sysroot)/lib/rustlib/thumbv7em-none-eabihf" ]` and exit 3 with a sentence
that says what to do ("enter `nix develop .#default`"). Outside the nix shell a user's profile cargo
has no embedded std, so today every firmware gate dies as an inscrutable E0463; the hook shim also
honours `LEFTHOOK=0` as a total bypass, and a confusing red hook is exactly what teaches someone to
set it.

## Step 5 - prove equivalence before anyone depends on it

This is the acceptance evidence, not a smoke test.

1. `bash -n scripts/gates.sh`, then `chmod +x`.
2. For each tier, compare the new command set against the old list, ignoring order and normalising
   the known spelling differences (`cd firmware && X` vs `root: "firmware/"` + `X`, and the `env `
   prefix on the rustdoc gates):
   - old commit tier: the 9 `run:` lines under `pre-commit.commands` in `lefthook.yml`
   - old push tier: the 16 `run:` lines under `pre-push.commands`
   - old CI tier: the 16 labelled blocks in `.github/ci-steps.sh`
   Extract with e.g. `yq '.pre-commit.commands[].run' lefthook.yml` and grep the banners out of
   ci-steps.sh; diff those against `scripts/gates.sh --dry-run <tier>` piped through the same
   normalisation. Expected: identical sets, plus exactly one new item per tier (`=== gate definition
   parses ===`, which replaces `lefthook.yml`'s `ci-steps-parse`) and the `ci` tier keeping the
   pod-hw test that pre-push lacks. Paste the diffs into the ticket notes.
3. Run all three tiers end to end inside `nix develop .#default` on a clean tree; record the per-gate
   timings the script prints. Before touching anything, capture the pre-change baseline for the same
   tiers (old pre-commit ≈ 1.5 s claimed by doc-001:229, old pre-push ≈ 70 s, full suite 138 s warm
   per TASK-060/061/063) so TASK-061.02 has honest numbers to compare against.
4. Record `sha256sum firmware/target/thumbv7em-none-eabihf/release/main` and `git status --porcelain`
   before the runs; after them, confirm the tree is still clean and restore the original digest with
   `rm -f $ELF && make -C firmware build-elf` (note in the ticket that the mtime does not refresh -
   that is TASK-062's bug, not something to fix here).

## Do not

- Do not delete `.github/ci-steps.sh` or edit `lefthook.yml` / `ci.yml` - that is TASK-061.02's job,
  and doing it here would leave a commit in which CI points at a file that no longer exists.
- Do not fold `firmware/` into the root workspace (rejected in TASK-060:116), do not merge the two
  `cargo doc` runs, do not replace the feature-variant clippy gates with `--all-features` or
  `cargo hack --feature-powerset` (powerset is exponential, and this ticket is about one
  definition, not fewer checks).
- Do not add caching, per-gate GitHub jobs, or a matrix fan-out. CI has no cache today
  (TASK-049:130), so 17 jobs would mean 17 cold builds; that idea only becomes sane once
  `Swatinem/rust-cache` or sccache lands, and belongs in its own ticket if anyone wants it.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Evidence (all figures LOCAL warm, aarch64-darwin, inside nix develop .#default, clean tree)

### Baselines taken BEFORE the script existed (old lists, forced so nothing skips)

| tier | what ran | result |
| --- | --- | --- |
| old pre-commit | `lefthook run pre-commit -f` (9 commands, alphabetical) | rc 0, 2 s wall; per-cmd 0.01-1.42 s, docs gate eighth at 0.16 s |
| old pre-push | `lefthook run pre-push -f` (16 commands, alphabetical) | rc 0, **74 s** wall; `test` 66.87 s of it |
| old CI | `bash .github/ci-steps.sh` (16 blocks) | rc 0, **139 s** wall |

The documented 138 s CI baseline re-measures as 139 s in this session, so every comparison below is
against a same-session number rather than against a quote.

### All three new tiers, green end to end

| tier | gates | wall | old equivalent | delta |
| --- | --- | --- | --- | --- |
| commit | 9 | **2.0 s** | 2 s | none (same 9 checks; order now cheapest-first) |
| push | 16 | **74.0 s** | 74 s | none |
| ci | 17 | **139.0 s** | 139 s | none, and one gate more than CI had |

Per-gate seconds are printed by the script itself (`--- 0.43s` under each banner) and were pasted
into the umbrella's matrix. The two `cargo test` invocations are 67.2 s and 67.4 s of the 139 s - the
same 133-of-139 concentration TASK-060 measured, which is why the pod-hw test stays CI-only.

### AC #1 - interface

`--dry-run` with no argument prints 17 commands, so forgetting the tier cannot mean "ran less than
everything". Unknown argument -> usage on stderr, rc 2. Bad `min-tier` in the table (planted
`weekly`) -> `gates.sh: bad min-tier: weekly`, rc 2. Tier that matches zero gates (scratch copy with
every gate raised to `ci`, asked for `commit`) -> `tier "commit" matched no gate. That is a bug in the
tier table, not a pass.`, rc 1. Planted failing gate (`=== cargo fmt ===` replaced by `false`) ->
`*** gate failed: === cargo fmt ===` / `*** tier: commit (2 of 3 gates completed before it)`, rc 1,
and the run stopped there instead of continuing to the lints. Anchoring: a scratch copy placed in
/tmp resolved ROOT to / and died naming `/scripts/check-doc-artifact-names.sh` - the anchor follows
the script, not the caller cwd, which is the point.

### AC #3 - mechanical equivalence (/tmp/equiv.sh)

Old lists extracted as text - `yq '.pre-commit.commands[].run'` / `.pre-push...` (with each job's
`root:` rendered as the `cd <dir> && ` prefix the script uses), and the labelled blocks of
`.github/ci-steps.sh` (comments, `echo`s and the `set` line dropped; commands inside the firmware
subshell given their inherited `cd firmware && `). Both sides normalised identically: drop the `env `
prefix, drop quotes, canonicalise `cd firmware/` to `cd firmware`. Then sorted-set diff against
`scripts/gates.sh --dry-run <tier>`:

    ### tier commit: old 9 vs new 9     IDENTICAL SETS (ci-steps-parse swapped for self-parse)
    ### tier push:   old 16 vs new 16   IDENTICAL SETS (same swap)
    ### tier ci:     old 16 vs new 17   IDENTICAL SETS (self-parse added; CI never had a parse gate)

Zero unexplained differences in any tier. The only membership change anywhere is that the hook's
`bash -n .github/ci-steps.sh` becomes `bash -n "$BASH_SOURCE"` - it can only be that, since the file
it parsed is the file this replaces - and that gate now also runs in CI.

### AC #4 - labels

Every banner is copied verbatim from `.github/ci-steps.sh`, including `=== cargo doc (workspace) ===`
and `=== cargo doc (workspace, all features) ===`, which is what TASK-052 greps runner logs for. Each
gate gained a `--- N.NNs` line, so per-gate cost survives having one call site instead of sixteen.

### AC #5 - the two ordering rules

Firmware commands carry `-C firmware`, which runs them in a subshell: the directory dies with the
gate that needed it, so no leaked `cd` can leave a later host gate pointed at the wrong workspace
where `cargo fmt --all --check` and `cargo clippy --workspace` would both have exited 0 on the wrong
code. In the declaration order the console cross-build sits immediately before the RTT-only one and
nothing builds firmware after them; the cross-clippies follow, so they reuse those artifacts (0.42 s
and 0.22 s here versus ~20 s in a fresh target dir). End state observed: after the `push` and `ci`
runs `release/main` was the RTT image 16dc9e5c..., exactly what `.github/ci-steps.sh` left behind
today, and `rm -f $ELF && make -C firmware build-elf` put it back to 9b60b8ffde4d2270e9a043feb300f50283762cdeee537ca81537f36b937fe80b.

### AC #6 - preflight

With a `rustc` shim that reports a sysroot lacking `lib/rustlib/thumbv7em-none-eabihf`, `ci` exits 3
printing "this rustc has no thumbv7em-none-eabihf standard library ... Enter the project shell first:
nix develop .#default" before any gate runs, instead of nine E0463s. Run-mode only: `--dry-run` and
`--list` answer questions about the list and work outside the shell.

### AC #7 - bench state

`git status --porcelain` empty apart from this ticket's own file. ELF digest restored to
9b60b8ff... as above. `make -C firmware elf-check` was red before the runs (rc 2, "target/...
/main is older than src/bin/podtest.rs") and is red after them with the identical message - mtime
drift owned by TASK-056/TASK-062, untouched here. Note `build-elf` restores the bytes but not the
mtime, which is TASK-062's finding restated, not a new one.

### Deliberate exclusions

One exists: `cargo test --workspace --features asperitas-pod/pod-hw` is `ci`-only. Its comment carries
the price (67 s warm, more than every other push gate combined), the reason the runtime half stays
remote while its compile half runs in `push` as the pod-hw clippy, the precedent (TASK-018.01,
c44b9c1), the "CI is the authority" claim it rests on, and an explicit reopen condition. It reads as a
priced decision at the gate itself, not as an absence (TASK-061 AC #3).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
scripts/gates.sh is now the one place a check is named: 17 gates declared as one `gate <tier> "<banner>" <command...>` line each, tagged with the cheapest tier that runs them, cumulative so commit(9) is a subset of push(16) is a subset of ci(17). Nothing calls it yet - ci-steps.sh and lefthook.yml still run their own lists, so this commit cannot break a build; TASK-061.02 switches the callers and deletes what they restated. Equivalence against all three old lists was proven mechanically rather than asserted: extracting the hook run: lines (with root: rendered as `cd dir && `) and the labelled blocks of ci-steps.sh, normalising both sides the same way and diffing sorted sets against --dry-run, gives IDENTICAL SETS in every tier - the only difference anywhere is that the hooks' `bash -n .github/ci-steps.sh` becomes a self-parse which now also runs in CI. All three tiers green end to end on a clean tree inside nix develop: 2.0 s / 74.0 s / 139.0 s against baselines taken from the old lists minutes earlier at 2 s / 74 s / 139 s - no cost change, one gate more in CI. The ordering rules a script can express and hook YAML cannot are preserved as file positions: cheapest-first fail-fast, console cross-build immediately before RTT-only with nothing building firmware after them, cross-clippy after both builds so it reuses their artifacts; firmware commands run through `-C firmware` subshells so a leaked cwd can never hand a host gate the wrong workspace with exit 0. Banner labels kept verbatim (including the two cargo doc banners TASK-052 greps), each gate gained a wall-time print, the thumbv7em preflight exits 3 telling you to enter nix develop instead of nine E0463s, unknown tier exits 2, a tier matching zero gates exits 1, a failed gate names itself and stops, and the single deliberate exclusion - the 67 s pod-hw test, ci-only - reads at its own gate as a priced decision with a reopen condition. Bench left as found: ELF back to 9b60b8ff, tree clean, elf-check exactly as red as before.
<!-- SECTION:FINAL_SUMMARY:END -->
