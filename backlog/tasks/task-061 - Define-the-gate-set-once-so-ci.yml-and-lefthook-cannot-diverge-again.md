---
id: TASK-061
title: 'Define the gate set once, so ci.yml and lefthook cannot diverge again'
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-13 00:10'
updated_date: '2026-09-13 03:13'
labels:
  - planned
dependencies:
  - TASK-060
  - TASK-061.01
  - TASK-061.02
references:
  - .github/workflows/ci.yml
  - lefthook.yml
  - scripts/check-doc-artifact-names.sh
priority: medium
type: chore
ordinal: 97800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-060 lands firmware fmt and clippy into three call sites that are already near-duplicates of each other, and the reason it needs three is the bug: `lefthook.yml`'s pre-commit/pre-push lists and `.github/workflows/ci.yml`'s single bash step implement the same idea twice, by hand, with no cross-reference. Every check added since has drifted between them.

The drift record, all verified 2026-09-12:

- TASK-018.01's fixup added the `asperitas-pod/pod-hw` clippy/test pair to **CI only** (`c44b9c1`, now `ci.yml:40-41,57-58`). Still missing from pre-push after this ticket, deliberately, with a reason - see TASK-060.04.
- `dump_reassemble --selftest` and the RTT-only cross-compile: **CI only**.
- TASK-049 widened doc links on **both** sides, which is the exception that proves the rule - it took a ticket to remember to do it.
- TASK-060 itself exists because firmware was invisible to both lists, and four closed tickets each declined to fix that.

Two consolidation shapes, both measured as viable, neither decided here on purpose:

1. **One script, two callers.** Extract ci.yml's ordered check list into `scripts/check-all.sh`; ci.yml calls it inside `nix develop .#default`, and lefthook's pre-push runs the same file. Precedent in-repo: `scripts/check-doc-artifact-names.sh` (TASK-058) is already shared this way, and `backlog/unblocked-todo.sh` opens with a war story about exactly this failure mode ("the same rules implemented twice drift; only one of the two copies gets noticed when that happens"). Deep-module shape: one command, no configuration, callers cannot get it wrong.
2. **CI delegates to the hooks.** `lefthook run pre-push --all-files` inside the nix shell makes CI execute the hook definitions instead of restating them. Caveats found: commands with no file template can still be skipped when their filtered file set is empty (lefthook #1038, #554), `--all-files` does *not* bypass `root:` filtering (verified against 2.1.10 source and probes), and `--force/-f` is the escape hatch. Also loses GitHub's per-step output unless the checks stay separate jobs.

Constraints either shape must respect, measured or read from lefthook 2.1.10 today:

- Commands run **sequentially** unless the hook sets `parallel: true` (`controller.go:100-106`), and are sorted by `priority`, then leading digits in the name, then name ascending (`command.go:36-92`) - **not** yml order. `lefthook dump` shows the current pre-commit order as `clippy, clippy-log-defmt, clippy-log-usb, doc-artifact-names, fmt-check`.
- `root:` sets cwd *and* filters paths; an empty filtered set skips the job silently with exit 0 (`docs/configuration/root.md:11`, plus probes). Any consolidated definition must not put a lint behind a filter.
- A manual `lefthook run pre-commit` on a clean tree is a no-op for every command, so a CI-side invocation must pass `--force` or `--all-files` explicitly.
- Local cost baseline for whatever replaces the current lists: the verbatim ci.yml suite is 138 s warm, 133 s of which is the two `cargo test --workspace` runs. CI itself has **no cache at all** today (no `actions/cache`, no `Swatinem/rust-cache`, no sccache), so upstream prior art is on the table if wall time becomes the objection: Swatinem/rust-cache supports multiple workspaces (`workspaces: ".\nfirmware -> target"`) and nix shells (PR #290), or `RUSTC_WRAPPER=sccache`. daisy-embassy's own CI runs a separate `cargo fmt -- --check` job plus one `cargo clippy --features X -- deny=warnings` per feature variant with `dtolnay/rust-toolchain` pinning the thumbv7em target; other Embassy-with-firmware repos keep nested workspaces and use `working-directory:` rather than folding them.

Out of scope and owned elsewhere: what the checks *are* (TASK-060 decides that), firmware docs being ungated by `cargo doc` (TASK-049's note), pedantic clippy adoption, and `make elf-check`'s mtime test (TASK-056).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 One definition of the gate set exists, and both .github/workflows/ci.yml and lefthook.yml invoke it rather than restating individual cargo checks.
- [ ] #2 Behaviour is preserved: every check that ran before still runs, measured local warm cost is no worse than the 138 s baseline, and any newly shared item's cost delta is recorded per call site.
- [ ] #3 Deliberate exclusions survive consolidation as explicit commented exceptions rather than silent absences - the pod-hw test pair from TASK-060.04 is the known case.
- [ ] #4 No lint job sits behind a lefthook path filter or any other condition that can skip it silently with exit 0.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Decision: shape 1, one script, three callers

`scripts/gates.sh <commit|push|ci>` becomes the only place a check is named. `.github/workflows/ci.yml`
runs the `ci` tier; lefthook's `pre-commit` runs `commit` and `pre-push` runs `push`. Each caller is
one line, so AC #1 holds and there is nothing left to restate.

Shape 2 (`lefthook run pre-push --all-files` inside CI) was rejected on measured behaviour, not
taste:

- Commands are sorted by priority, then leading digits, then name - **not** YAML order. Verified with
  `lefthook dump`: today's pre-commit really runs `ci-steps-parse, clippy, clippy-firmware,
  clippy-firmware-rtt, clippy-log-defmt, clippy-log-usb, doc-artifact-names, fmt-check,
  fmt-check-firmware`, i.e. the 0.2 s docs gate runs eighth behind every clippy, and in pre-push the
  cross-clippies run *before* the cross-builds whose artifacts they are supposed to reuse
  (`.github/ci-steps.sh:90-92` says otherwise). Two of this repo's cost decisions - cheapest-first
  fail-fast, build-before-lint artifact reuse - are simply unexpressible in hook config. A script can
  say them.
- Path filtering skips silently with exit 0. One job still uses it today (`lefthook.yml:143-146`), so
  AC #4 is already violated at HEAD and `doc-001:231-233`'s claim that no lint job does is false.
  `--all-files` does not bypass `root:`; only `-f` does.
- `lefthook run` re-syncs `.git/hooks` unless told not to, which would have CI rewriting its own
  checkout mid-run (`--no-auto-install` exists, but it is one more thing a caller must remember -
  exactly the class of mistake this ticket is meant to end).
- Per-step GitHub output disappears either way, and shape 2 buys nothing that shape 1 does not.

What shape 1 gives up, recorded so nobody re-litigates it: lefthook captures a command's stdout and
replays it at the end, so a hook now prints nothing until the tier finishes (~70 s for push). In
exchange the log gets per-gate headers and per-gate wall times that never existed before, plus
deterministic ordering and fail-fast that alphabetical command names could not provide. The escape
hatch (`parallel: true` + `priority: 100000`, streams while preserving declared order, but the first
failing job may not be reported first) is written into `lefthook.yml` as a comment rather than used.

The tier argument is the one knob, and it is genuine rather than decorative: the three call sites
have budgets two orders of magnitude apart (≈1 s / ≈70 s / ≈138 s warm local), and a commit loop that
runs nine gates cannot pay for a test suite. Cumulative tiers (`commit ⊂ push ⊂ ci`) match how the
lists actually relate today - 9 / 16 / 17 gates, pre-commit a strict subset of pre-push, pre-push and
CI differing by exactly two items, both deliberate and both commented. No argument means `ci`, so
forgetting the argument can never mean "ran less than everything".

## Sub-tickets

Both are `@agent`, both planned, executed in this order (`.02` depends on `.01`; both block here):

- **TASK-061.01** - write `scripts/gates.sh`: cumulative tiers, repo-root anchoring, verbatim banner
  labels, per-gate timings, scoped firmware subshell, thumbv7em preflight, `--dry-run` / `--list`.
  Additive: nothing calls it yet, so it cannot break a build, and equivalence against today's three
  lists is proven mechanically by diffing `--dry-run` output against the extracted `run:` lines and
  ci-steps.sh blocks. That diff is AC #2's evidence.
- **TASK-061.02** - point `ci.yml` and both hook stages at it, `git rm .github/ci-steps.sh`, delete
  25 duplicated `run:` lines and the last `root:`, prove the hooks really fire and propagate failure,
  measure all three tiers warm, and re-point every live prose reference (doc-001, `firmware/Makefile`,
  `scripts/check-doc-artifact-names.sh`) plus a comment on each open ticket that still names the dead
  path.

Split where the risk changes shape: `.01` is "is the new definition equivalent", `.02` is "does
everything that calls it survive the switch". They are separate commits because a bug found during
`.02` should cost one small revert, not the loss of the equivalence work.

## Integration verification (what closes this ticket)

1. `scripts/gates.sh --list` regenerated into doc-001, and the matrix compared cell-by-cell against
   the census: 9 / 16 / 17 gates, with `cargo test --workspace --features asperitas-pod/pod-hw` the
   only single-tier item and the self-parse gate the only net-new one.
2. All three tiers green end to end inside `nix develop .#default` on a clean tree, with the printed
   per-gate times pasted into the notes and the `ci` tier ≤ 138 s warm.
3. Hook wiring observed, not inferred: `lefthook dump` shows one command per stage, `grep -c 'root:'
   lefthook.yml` prints 0, `lefthook run pre-commit` / `pre-push` execute rather than skip, an empty
   `git commit` shows the gate output, and a deliberately broken gate aborts a commit non-zero.
4. Bench untouched: `git status --porcelain` empty after the runs, and
   `firmware/target/thumbv7em-none-eabihf/release/main` back to its starting sha256. `make -C firmware
   elf-check` is red on a clean tree before this work starts (mtime-only drift, rc=2) and is expected
   to be exactly as red afterwards - TASK-056 and TASK-062 own it.
5. AC #3 durability check: grep the consolidated file for the pod-hw exception and confirm the
   exclusion reads as a priced decision with a reopen condition, not an omission. Also confirm the
   other asymmetry is written down - `bash -n` used to exist only in the hooks and now runs in all
   three tiers as the first gate.

## Deliberate human-owned work, and why it is not a child of this ticket

Nothing here needs hands: every criterion is provable locally, including the wall-time figures
("local warm" is what the baseline itself is labelled as). What cannot be done by an agent is pushing
main and watching GitHub actually execute the thing - main sits ~91 commits ahead of origin/main,
`ci.yml` triggers only on push/PR with no `workflow_dispatch`, and the last real CI run predates the
breakage that TASK-060 found. That observation is already owned by **TASK-063** (@human, filed by
TASK-060's integration pass for exactly this purpose), so filing a second one would duplicate it.
Instead TASK-063 now depends on TASK-061, so the single pending push happens against the final shape,
and its criteria get a comment naming `scripts/gates.sh ci` and asking for the per-gate timings the
new script prints - which also discharges most of TASK-052's long-standing debt of runner-side
figures. This ticket is *not* gated on that push: making agent work wait on a human is how the loop
stalls, and TASK-060 set the same precedent.

## Out of scope

What the checks *are* (TASK-060), firmware docs being ungated by `cargo doc` (TASK-049's note),
pedantic clippy adoption, `elf-check`'s mtime test (TASK-056) and cfg blindness (TASK-062), CI caching
(`Swatinem/rust-cache` multi-workspace or `RUSTC_WRAPPER=sccache` - worth its own ticket, and a
prerequisite for any idea about fanning the gate list out into one job per gate, since CI has no
cache at all today and 17 jobs would mean 17 cold builds), folding `firmware/` into the root workspace
(rejected at TASK-060:116), and `cargo hack --feature-powerset` collapsing the feature-variant lints
(exponential, and this ticket is about one definition, not fewer checks).

One correction to carry forward: this description's drift record cites `ci.yml:40-41,57-58` for the
pod-hw pair. Those coordinates died in 70c6fc6; the pair now lives at `.github/ci-steps.sh:45-46` and
`:62-63`, and will live in `scripts/gates.sh` after TASK-061.02.
<!-- SECTION:PLAN:END -->
