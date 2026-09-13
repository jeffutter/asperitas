---
id: TASK-060
title: >-
  Firmware is invisible to every fmt and lint gate; rig.rs has already drifted
  again
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-12 21:26'
updated_date: '2026-09-13 00:24'
labels:
  - planned
dependencies:
  - TASK-060.01
  - TASK-060.02
  - TASK-060.03
  - TASK-060.04
references:
  - .github/workflows/ci.yml
  - lefthook.yml
  - 'firmware/Makefile:264-269'
priority: medium
type: chore
ordinal: 92800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Discovered while checking what TASK-057's host gates would actually cover. `ci.yml` runs `cargo fmt --all --check`
(:24) and four clippy invocations (:27,32,38,41) - all at the repo root, where root `Cargo.toml:5` declares
`exclude = ["firmware"]`. `firmware/` is its own workspace (own `Cargo.lock`, own `target/`, single member
`asperitas-firmware`), so no formatting or lint gate in CI or lefthook ever looks at firmware code, and the pass is
silent about it.

Reproduced read-only 2026-09-12 inside `nix develop .#default`:

    cargo fmt --all --check           -> rc 0
    cd firmware && cargo fmt --check  -> rc 1, real diffs in firmware/src/bin/rig.rs:21, :28, :445

`firmware/Makefile:251-255` already states the situation plainly - "Neither CI nor lefthook invokes make inside
firmware/, so this target [`make clippy`] is the only place firmware clippy runs" - but nothing enforces running it,
and the drift proves it: TASK-044 cleared pre-existing fmt drift in `main.rs` and `podtest.rs` on 2026-09-10 and closed
Done; `rig.rs` arrived afterwards (TASK-038.03.02.03) and drifted immediately, invisibly.

Two separable holes, both needing a decision rather than a mechanical fix, which is why this is unplanned:

1. Formatting. Either add a firmware fmt step (needs cwd handling, the pattern already used by `lefthook.yml:60-63`'s
   `firmware-cross-compile`), or fold firmware into the root workspace so `--all` means all. The second is tempting and
   probably wrong: keeping firmware out of the root workspace is what stops host tooling from trying to build `no_std`
   code for the host.
2. Lints. Cross clippy works today without any sysroot flag because `flake.nix:24-32` folds the
   `thumbv7em-none-eabihf` std into the same sysroot as `clippy-driver` (that was TASK-009's whole point), so
   `cd firmware && cargo clippy --release --features seed3 --bin main -- -D warnings` is viable in CI as-is. Unknowns
   worth measuring first: how long each of the six bin targets takes under clippy, and whether any of them is currently
   red - if `rig.rs` drifted fmt-wise it may well carry warnings too, and the ticket should clear them before turning
   the gate on.

Also worth folding in while the gates are open: lefthook's pre-push is missing three things CI has (the `pod-hw`
clippy/test pair, `dump_reassemble --selftest`, the RTT-only cross-compile), so local-green is not CI-green. Decide
whether to close that gap here or leave pre-push deliberately cheap.

Depends on nothing. Do not create a firmware clippy target - `make clippy` (`firmware/Makefile:252-255`) already exists;
this ticket is about calling it from a gate.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A gate runs `cargo fmt --check` against the firmware workspace and is red on a planted diff today - `firmware/src/bin/rig.rs:21,:28,:445` are already drifted while root `cargo fmt --all --check` exits 0 - and green once they are cleared.
- [ ] #2 A gate runs cross-target clippy over the firmware bin targets with `-D warnings`, with the measured wall time recorded, and whatever warnings it surfaces are either cleared in the same change or filed as their own ticket rather than silenced.
- [ ] #3 The decision on lefthook's pre-push gap (no pod-hw clippy/test, no `dump_reassemble --selftest`, no RTT-only cross-compile) is recorded one way or the other in the ticket notes, not left implicit.
- [ ] #4 Host gates green in `nix develop .#default`, verbatim from ci.yml.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Shape

Four leaves, executed in the order below, plus an integration pass that belongs to this ticket and nowhere else. All four ACs are dischargeable by reading code, running commands, and pasting output - no board, no ears, no instrument, no outward-facing action - so the whole tree stays `@agent` and nothing splits to `@human`. The one thing an agent genuinely cannot do here is observe a GitHub Actions run: `ci.yml:3-7` triggers only on push/PR to main, has no `workflow_dispatch`, and `main` is 84 commits ahead of `origin/main`. That gap is handled by labelling every figure local and leaving observed CI numbers to TASK-052, which is already `@human` and exists for exactly this class of debt. Do not add a `HUMAN:` child for it: a parent inherits its children's strictest assignee, so an `@human` child would hide the remaining agent work in this umbrella while changing nothing about who can push.

Why split at all: the three halves have different costs, different blast radii, and different owners-of-the-next-word. Clearing drift (.01) must precede turning the gate on (.02/.03) or the gate arrives red - TASK-058's plan states the consequence plainly: "a gate that arrives red gets disabled rather than fixed". The pre-push decision (.04) is a judgement with a price tag attached, and bundling it with mechanical wiring is how it got deferred four times (TASK-036.01 "not here", TASK-053 "wiring firmware clippy into CI: not in scope", TASK-058 twice, TASK-044 cleared drift and gated nothing).

## Execution order

1. **TASK-060.01** - clear the four `rig.rs` rustfmt hunks. One file, formatting only, ships alone.
2. **TASK-060.02** - firmware-workspace `cargo fmt --check` in pre-commit, pre-push and CI. `cargo fmt --manifest-path firmware/Cargo.toml --all --check`, never `root:`-scoped. Closes AC #1.
3. **TASK-060.03** - cross-target clippy over all six bins with `-D warnings`, both CI feature sets, three call sites, placed after CI's existing firmware builds. Closes AC #2 (with the wall-time figure labelled local).
4. **TASK-060.04** - land last: takes the free pre-push items, records the priced exception for the 67 s pod-hw test, rewrites the gate prose in `backlog/docs/doc-001`, and produces the check-by-hook matrix. Closes AC #3.

.02 and .03 touch the same two files and should be reviewed as a pair but committed separately, each green on its own.

## Integration pass (this ticket)

Run after all four leaves are in, from a clean tree:

    nix develop .#default --command bash -c 'set -euo pipefail; <the verbatim ci.yml check list>'

Expected exit 0. Measured locally on 2026-09-12 before any of this landed: 138 s warm, of which 133 s is the two `cargo test --workspace` runs. Paste the run and the per-step deltas into these notes, labelled local with machine and warmth, satisfying AC #4 alongside AC #2's figure.

Then prove the gates bite, end to end, once each: plant an unformatted line in a firmware source and a `clippy::no_effect` statement in a firmware bin, confirm pre-commit blocks a real `git commit` (not just `lefthook run`, which skips on a clean tree), revert both, confirm green, and show `git status --porcelain` empty plus an unchanged sha256 for `firmware/target/thumbv7em-none-eabihf/release/main`. The last two exist because `make elf-check` reads source mtimes as staleness and that ELF is the decoder every bench command uses; TASK-058 AC #4 is the precedent for recording digests rather than asserting safety.

Finally map AC #1-#4 to evidence in the notes, one line each, with the artifact that satisfies it.

## Superseding the overlapping clauses (done during planning, recorded here)

Three open tickets already specify a firmware clippy step in ci.yml, and would have fought .03 for the same lines:

- TASK-038.03.02 AC #13, TASK-038.03.02.02 AC #13, TASK-038.03.02.04 AC #10 first clause - all ask for `cd firmware && cargo clippy --release --features seed3 -- -D warnings` in CI. Same intent as .03; TASK-038.03.02.04's Key Decision 5 (whole-package, not `--bin rig`) is honored, and `--bins` is spelled explicitly rather than relying on a package with no lib defaulting to all bins.
- What stays with TASK-038.03.02.04: the stim-variant **builds** (`seed3,stim-ess` / `seed3,stim-pulse` on `--bin rig`) and the rate gates. Not absorbed here. Left as a comment on that ticket: when those build lines land, each should gain a matching `-D warnings` clippy line (~1-2 s warm each), because cfg-gated stimulus code that no default-feature build compiles is the same blind spot CI closed twice for `asperitas-logging`.

A comment pointing here was also left on TASK-052, so whoever eventually pushes knows the new steps join the queue for observed CI figures.

## Risks and rejects

- **Folding firmware into the root workspace** is rejected, and the reasons are now measured, not vibes: `multiple workspace roots found` if `firmware/Cargo.toml:1-3` keeps its `[workspace]` table; `profiles for the non root package will be ignored` if it does not, which silently drops `[profile.release] debug = 2` that TASK-054 proved probe-rs needs to name defmt locations; `.cargo/config.toml` is discovered from the CWD (cargo #9670), so a folded build compiles `no_std` code for the host triple and breaks the Makefile's ELF paths; plus a forced relock that floated daisy-embassy off its pinned `ca9bcc9`. Recorded in .02's description.
- **Lefthook's silent skip** is the trap that could make this ticket look done while doing nothing: `root:` filters, and an empty filtered set exits 0 without running the command. Hence `cd firmware && ...` and `--manifest-path`, and hence the requirement in every leaf to show red output from the actual call site.
- **Pre-commit gets slower.** Warm ~+4 s, cold up to ~+40 s for the two cross-clippy runs on a fresh target dir. Accepted because the loop never pushes, so pre-commit is the only reliably-executed gate; if it ever hurts, demoting `clippy-firmware*` to pre-push is a one-line change with a measured number attached - say so in the hook comment rather than deleting the coverage.
- **Gates must not write to firmware sources.** `--check` only. A fmt-and-fix-at-commit behaviour would make `make elf-check` fail closed on the bench until TASK-056 lands.
- **Docs go stale in the same change.** `firmware/Makefile:264-269` (.03 fixes), `backlog/docs/doc-001:33,117,223-228,234,303` (.04 fixes), plus the "only place this runs" comments in `ci.yml:29-38,65-67` and `lefthook.yml:6-7`. Ticket bodies quoted these as fact for four tickets; they must not do it again.
- **Follow-up filed, not folded in:** TASK-061 defines the gate set once so CI and lefthook cannot diverge again. Out of scope here because it changes the shape of both files at once and deserves its own planning; TASK-060.04's caveat comment is what keeps divergence honest until then.

## Addendum from re-verification (same day)

Two more stale coordinates live in **this umbrella's own description**, and an executor who trusts them will look at the wrong file region: `:41` cites `firmware/Makefile:251-255` for the "Neither CI nor lefthook invokes make in a linting capacity" comment, and `:63` cites `Makefile:252-255` for `make clippy`. Both are at `Makefile:264-269` today (`make clippy` itself at `:269`). Correct those two lines while touching the Makefile comment in TASK-060.03, so the ticket that fixes the stale claim does not itself remain the stalest instance of it. `References` was already repointed at `:264-269` during planning.

Verified-current coordinates for the executors, checked line by line on 2026-09-12: `lefthook.yml` is 73 lines - pre-commit `:4-29` (five commands), pre-push `:31-73`, with `test` at `:66-68` and `firmware-cross-compile` at `:70-72`. That last job is the live precedent for the `root:` hazard named in .02 and .03: it sets `root: "firmware/"`, which both sets cwd and filters paths, so a push touching no firmware file skips it with exit 0. New lint jobs must not copy that shape. `ci.yml`: triggers `:3-7`, root fmt `:24`, the two feature-set clippies `:29-38`, pod-hw clippy `:40-41`, pod-hw test `:57-58`, `dump_reassemble --selftest` `:63`, the only-make-in-CI claim `:65-67`, RTT-only firmware build `:74-79` inside one single-quoted bash string ending `:80`.
<!-- SECTION:PLAN:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-13 00:17
---
Planned 2026-09-12 by backlog-planner. Four leaves, chained 01 -> 02 -> 03 -> 04, all @agent: .01 clears the rustfmt drift, .02 turns on firmware fmt, .03 turns on cross-target clippy, .04 takes the free pre-push items and records the priced exception. This umbrella keeps the integration pass (verbatim ci.yml suite green, plus one end-to-end plant proving each new gate blocks a real commit) and the AC-to-evidence map.

Two corrections to this ticket's own body, both measured in nix develop .#default on 2026-09-12: rig.rs is red at FOUR hunks - rig.rs:21, :28, :445 AND :554 (:554 is a let-chain rustfmt 2024 style edition Cargo.toml:13 sets), not the three named here. And AC #2's premise that firmware clippy 'surfaces' unknown warnings is already resolved: cd firmware && cargo clippy --release --features seed3 --bins -- -D warnings exits 0 today, as do the RTT-only, stim-ess, stim-pulse and slow-boot variants - so there is nothing to clear or silence, and the gate arrives green.

AC coverage: #1 -> .01+.02, #2 -> .03 (wall time recorded as LOCAL only; ci.yml has no workflow_dispatch and main is 84 commits ahead of origin/main, so no agent can observe CI - see TASK-052), #3 -> .04, #4 -> this ticket's integration pass.

Filed separately rather than folded in: TASK-061 (define the gate set once so ci.yml and lefthook stop diverging). It reshapes both files and needs its own planning.
---
<!-- COMMENTS:END -->
