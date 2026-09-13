---
id: TASK-060.03
title: >-
  Gate firmware lints: cross-target clippy over all six bins with -D warnings in
  pre-commit, pre-push and CI
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-13 00:09'
updated_date: '2026-09-13 00:22'
labels:
  - planned
dependencies:
  - TASK-060.02
references:
  - .github/workflows/ci.yml
  - lefthook.yml
  - 'firmware/Makefile:264-269'
modified_files:
  - lefthook.yml
  - .github/workflows/ci.yml
  - firmware/Makefile
parent_task_id: TASK-060
priority: medium
type: chore
ordinal: 95800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Cross-target clippy already works with no sysroot flag - `flake.nix:31-34` folds the `thumbv7em-none-eabihf` std into the same sysroot as `clippy-driver`, which was TASK-009's whole point - but nothing calls it from an unattended gate. `firmware/Makefile:264-269` states this itself: "Neither CI (.github/workflows/ci.yml) nor lefthook invokes make inside firmware/, so this target is the only place firmware clippy runs." TASK-036.01 created that target and explicitly declined to wire it up ("Do not wire firmware clippy into CI here - that is a separate judgement about CI minutes"). This ticket makes that judgement.

Measured at HEAD 2026-09-12, local, inside `nix develop .#default` on aarch64-darwin:

    cargo clippy --release --features seed3 --bins -- -D warnings                                exit 0
    cargo clippy --release --no-default-features --features "seed3 log-defmt" --bins -- -D warnings   exit 0
    cargo clippy --release --features "seed3 stim-ess"  --bin rig -- -D warnings               exit 0
    cargo clippy --release --features "seed3 stim-pulse" --bin rig -- -D warnings             exit 0
    cargo clippy --release --features "seed3 slow-boot" --bin main -- -D warnings             exit 0

So the gate turns on green: there is nothing to clear, nothing to silence, and no lint-cleanup ticket to file. (`-W clippy::pedantic` *is* red in every bin - 9 findings in `podtest.rs`, more in `rig.rs` - but pedantic is not what `-D warnings` means here, and adopting it is its own decision, not a side effect of turning a gate on.)

Cost, same machine, all labelled local: cold `--bins` clippy in a fresh target dir 20 s; the same for a single `--bin main` is also 20 s, so per-bin invocations cost what the whole package costs and `--bins` is strictly cheaper (six times cheaper than six per-bin runs). Placing the step *after* CI's existing firmware build reuses compiled dependencies: 11 s after a cold release build in the same `CARGO_TARGET_DIR`, ~2 s warm. A build run after that clippy costs 1 s, i.e. the two do not thrash each other.

Why this belongs in `pre-commit` too, against that hook's "cheapest gate here on purpose" comment: `~/.pi/agent/extensions/ralph/index.ts` touches git only via `rev-parse`, `log` and `rebase --autosquash` - the loop never pushes - and `main` sits 84 commits ahead of `origin/main` with no `workflow_dispatch` trigger on `ci.yml`. Pre-push and CI therefore execute rarely; pre-commit executes on every commit by both the loop and a human. A firmware lint placed only upstream of that would be enforced almost never, which is the disease this ticket treats. Warm cost is ~3 s for both feature sets combined; cold cost is bounded (~40 s) and lands once per fresh target dir.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Three call sites run cd firmware && cargo clippy --release --features seed3 --bins -- -D warnings: lefthook pre-commit.commands, lefthook pre-push.commands, and ci.yml placed AFTER the existing cargo build --release --features seed3 so clippy reuses its compiled dependencies (local: 20 s cold in a fresh target dir, 11 s right after that build, ~2 s warm).
- [ ] #2 A second invocation covers the RTT-only feature set, mirroring ci.yml:79: cd firmware && cargo clippy --release --no-default-features --features "seed3 log-defmt" --bins -- -D warnings. cfg-gated code no default-feature build compiles is the same blind spot ci.yml:29-38 already closed twice for asperitas-logging.
- [ ] #3 Wall time is recorded in the notes explicitly labelled LOCAL, naming machine, command and target-dir warmth. No number is presented as CI's: ci.yml has no workflow_dispatch trigger and main is 84 commits ahead of origin/main, so no agent can observe a CI run - TASK-052 exists because TASK-049 checked an AC citing exactly such an unobservable figure.
- [ ] #4 The zero-warning claim is proven rather than assumed: both invocations' exit codes at HEAD are pasted (measured 0 and 0 on 2026-09-12). If either turns red during implementation, nothing is silenced and nothing is narrowed - the cleanup becomes its own ticket, per TASK-060 AC #2.
- [ ] #5 firmware/Makefile:264-269 is corrected in the same commit: its "this target is the only place firmware clippy runs" claim becomes false once these gates land. The gates call raw cargo, not make - make clippy hard-wires --bin $(BINARY) at :269 and cannot express the whole-package form, and ci.yml:65-67's claim that the docs check is the only place CI invokes make stays true.
- [ ] #6 Non-interference with bench state is evidenced, not asserted: git status --porcelain empty and sha256 of firmware/target/thumbv7em-none-eabihf/release/main identical before and after both invocations (measured unchanged locally). That ELF is the decoder every make probe-* command reads with; TASK-058 AC #4 is the precedent.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Shape

Three call sites, two invocations each, plus one Makefile comment fix. Depends on TASK-060.01 (green tree) and TASK-060.02 (same files, same commit series - land fmt first so each commit is coherent). Assignee `@agent`: every criterion is discharged by running commands and pasting output.

## Step 1 - the two invocations

Both must run with cwd = `firmware/`, because `.cargo/config.toml` (`[build] target = thumbv7em-none-eabihf`, `-C link-arg=-Tlink.x`) is discovered from the CWD, not from `--manifest-path` (cargo #9670/#2930/#10302). No sysroot flag is needed.

    cd firmware && cargo clippy --release --features seed3 --bins -- -D warnings
    cd firmware && cargo clippy --release --no-default-features --features "seed3 log-defmt" --bins -- -D warnings

The second mirrors `ci.yml:79`'s RTT-only build. cfg-gated code the default set does not compile is the same blind spot CI already closed twice for `asperitas-logging` (`ci.yml:29-38`), and TASK-036.03's warnings actually surfaced in the transport-less config, so this is not hypothetical coverage.

`--bins` is the shape. `--all-targets` is wrong on a `no_std` target (there is no test harness to link), and `firmware/` has no lib target, so `--bins` *is* the whole package - all six of `blinky ledtest main panictest podtest rig`. Do not narrow to one binary: TASK-038.03.02.04's Key Decision 5 chose whole-package linting precisely so a future bin cannot quietly skip the gate, and `firmware/Cargo.toml:104-107` records that CI's builds pass no `--bin` for the same reason.

In lefthook use `run: cd firmware && cargo clippy ...` rather than `root: "firmware/"`. Same cwd effect, without the path filter: verified against lefthook 2.1.10 that a `root:`-scoped job prints `(skip) no matching push files` and exits 0 when the filtered set is empty, which would let a toolchain-bump commit (`flake.lock`, outside `firmware/`) through unlinted. Note `lefthook.yml:70-73`'s existing `firmware-cross-compile` does have that property today; leaving it as-is is fine (a host-only commit cannot break the cross-build), but do not copy the pattern into a lint job.

## Step 2 - placement

- `lefthook.yml`: add `clippy-firmware` and `clippy-firmware-rtt` to `pre-commit.commands` and `pre-push.commands`. Remember commands run sorted by name, not by yml order, and sequentially unless the hook opts into `parallel: true`; the cost is additive, so say the measured number in the comment instead of implying cheapness.
- `.github/workflows/ci.yml`: put the two steps **after** the existing firmware builds (`:72` and `:79`), inside the same single-quoted bash string, no nested single quotes. Order matters for wall time, not correctness: clippy reuses the dependency artifacts those builds produced (local: 20 s cold standalone vs 11 s right after the build, ~2 s warm). `:79` already runs with cwd `firmware/` because `:72` cd'd there, so the new lines need no `cd` if appended after it - be explicit anyway if it reads better, since relying on an inherited cwd across an edit is how the next insertion breaks.

## Step 3 - evidence for the notes

1. Exit codes of both invocations at HEAD (expected 0/0, measured 2026-09-12). If either is red when you get here, **stop**: land nothing silenced - no `#[allow]`, no narrowing to `--bin main`, no `cap-lints` - and file the cleanup as its own ticket, per TASK-060 AC #2.
2. Wall times, each labelled as a **local** figure naming machine, command and whether the target dir was cold or warm. Do not present any number as CI's: `ci.yml` triggers only on push/PR to main, has no `workflow_dispatch`, and `main` is 84 commits ahead of `origin/main`, so no agent can observe a CI run. TASK-052 exists precisely because TASK-049 checked an AC that cited an unobservable CI time; TASK-038.03.02's plan also points at TASK-052's interest in CI wall time. Observed CI figures belong to TASK-052, which stays `@human`.
3. Red proof for the gate itself: plant something `clippy` flags at warn level in a firmware bin and show each call site exit non-zero, then revert and show green. A plant that needs neither std nor dependencies, verified against the shape of these bins:

       fn planted_lint() -> u32 {
           let a = 1u32;
           1 + 1;
           a
       }

   Measured 2026-09-12 by appending exactly this to `firmware/src/bin/podtest.rs`: `cargo clippy --release --features seed3 --bin podtest -- -D warnings` exits **101** with `error: statement with no effect` (`clippy::no_effect`), `error: function 'planted_lint' is never used` and `error: unused arithmetic operation that must be used`. Either half of that is enough; the file was restored from a copy and `git status --porcelain` came back empty. Record that planting bumps source mtimes, so `make elf-check` fails closed until the ELF is rebuilt - expected, and not a reason to touch `firmware/target/`.

4. Non-interference with bench state: `git status --porcelain` empty, and `shasum -a 256 firmware/target/thumbv7em-none-eabihf/release/main` identical before and after both invocations (measured unchanged locally on 2026-09-12). That ELF is the decoder every `make probe-*` command reads with; TASK-058 AC #4 is the precedent for recording a digest rather than asserting safety.

## Step 4 - the Makefile comment

`firmware/Makefile:264-269` becomes false as written. Rewrite it to say: the `clippy` target is the interactive, per-binary route (`make clippy BINARY=rig FEATURES="seed3 stim-ess"`), while pre-commit, pre-push and CI now run whole-package `--bins` clippy directly with raw cargo. Keep the raw-cargo choice deliberate: `make clippy` hard-wires `--bin $(BINARY)` at `:269` and cannot express the whole-package form, and `ci.yml:65-67` claims the docs check is the only place CI invokes make - a claim worth keeping true. Fixing the comment in the same commit is not optional: the last four tickets that looked at this quoted that comment as fact (TASK-030.05, TASK-036.01, TASK-053, TASK-060 itself, whose `References` still cite `firmware/Makefile:251-255` for a target that moved to `:264-269`).

## Deliberately not here

Stimulus-variant clippy (`stim-ess`, `stim-pulse`, each ~1-2 s warm) and `slow-boot`: those configs are not built by CI yet, and adding their *builds* is TASK-038.03.02.04 AC #10's job. When that lands, each new variant build should gain a matching `-D warnings` clippy line - leave that instruction as a comment on TASK-038.03.02.04 rather than growing this ticket. Likewise out of scope: `-W clippy::pedantic` adoption, CI caching (no cache exists today; TASK-061 territory), and any change to `make clippy` itself.

## Shape note: `--bins`, and what this leaf deliberately does not absorb

TASK-038.03.02.04 AC #10 asks for the same gate written whole-package without a target selector (`cd firmware && cargo clippy --release --features seed3 -- -D warnings`), inside the existing single-quoted `run: |` bash string at `ci.yml:76-79`, with no new workflow step. That is equivalent work: this package declares no `[lib]` (six entries under `src/bin/`), so the default target selection already means all six bins, and `--bins` only states it out loud. **This leaf uses `--bins` explicitly** and adds the RTT-only second pass; whoever lands .04's stim builds should not also add a second clippy line for the default set. Recorded on that ticket as a supersession comment.

Scope guard, so this leaf does not drift: `.04` AC #10's other half stays there - `cargo build --release --features seed3,stim-ess --bin rig`, the `stim-pulse` pair, the window-override build, and the rate gates. Do not pull any of those in here. What belongs to .04 from this change is pairing each new stim *build* with a matching `-D warnings` *clippy* line (~1-2 s warm each), because cfg-gated stimulus code no default-feature build compiles is otherwise unlinted - the same blind spot `ci.yml:29-38` closed twice for `asperitas-logging`.

Two constraints from that file that bite here: the check list is one single-quoted bash string, so **no nested single quotes** in anything added to it, and `cargo clippy --workspace --all-targets` cannot be used for firmware at all - `--all-targets` builds host-profile tests and benches and fails on `no_std` code. That is why CI's pod-hw pair at `:40-41`/`:57-58` covers `crates/*` only, and why firmware gets `--bins`.
<!-- SECTION:PLAN:END -->
