---
id: TASK-060.02
title: >-
  Gate firmware formatting: cargo fmt --check on the firmware workspace in
  pre-commit, pre-push and CI
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-13 00:09'
updated_date: '2026-09-13 00:12'
labels:
  - planned
dependencies:
  - TASK-060.01
references:
  - .github/workflows/ci.yml
  - lefthook.yml
modified_files:
  - lefthook.yml
  - .github/workflows/ci.yml
parent_task_id: TASK-060
priority: medium
type: chore
ordinal: 94800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a formatting gate that actually reads `firmware/`. Today no gate does: root `cargo fmt --all --check` (`ci.yml:24`, `lefthook.yml:13,38`) stops at `Cargo.toml:3`'s `exclude = ["firmware"]`, and the pass is silent about it - which is how `rig.rs` sat unformatted from TASK-038.03.02.03 to today (TASK-060.01 clears it).

Two shapes were measured red on the same planted diff before choosing:

    cargo fmt --manifest-path firmware/Cargo.toml --all --check     # from repo root
    cd firmware && cargo fmt --all --check                          # needs the cwd dance

Take the first. It needs no cwd handling in either call site, and it avoids lefthook's `root:` key entirely, which is load-bearing here: `root:` sets cwd *and* filters paths, and lefthook skips a command whose filtered file set is empty (`docs/configuration/root.md:11`; verified against the installed 2.1.10 - `lefthook run pre-push --command firmware-cross-compile --file README.md` prints `(skip) no matching push files` and exits 0 without running cargo, while the same probe on a command with no `root:` runs). A lint gate that can silently skip is the exact failure class TASK-060 exists to kill, so this one runs unfiltered from the root.

Rejected alternative, recorded so nobody re-litigates it: folding `firmware/` into the root workspace so `--all` means all. It breaks three things at once - `multiple workspace roots found` if its own `[workspace]` table stays; `profiles for the non root package will be ignored` if it goes, which silently drops `[profile.release] debug = 2` (the level TASK-054 proved probe-rs needs to decode defmt locations); and `.cargo/config.toml` discovery is CWD-based (cargo #9670), so the folded build compiles `no_std` code for the host triple. `backlog/docs/doc-001 - Asperitas-Project-Plan.md:113-117` already records the two-workspace split as deliberate.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 lefthook.yml gains fmt-check-firmware in BOTH pre-commit.commands and pre-push.commands, and ci.yml gains the same command beside the existing root fmt line (:24). All three run cargo fmt --manifest-path firmware/Cargo.toml --all --check from the repo root, and none uses lefthook's root: key - root: sets cwd AND filters paths, and a job whose filtered file set is empty is skipped with exit 0 without running anything (verified against the installed lefthook 2.1.10).
- [ ] #2 Notes carry red-and-green evidence from every call site: one deliberately planted unformatted firmware line makes pre-commit, pre-push and the raw CI invocation each exit non-zero, and all three go green after the plant is reverted.
- [ ] #3 The gate provably writes nothing: git status --porcelain is empty after a green run. --check stays --check - a fix-at-commit gate would bump firmware source mtimes and make make elf-check fail closed on the bench until TASK-056 lands.
- [ ] #4 Comments that describe what the gates cover are corrected in the same change (lefthook.yml:6-7 cost rationale, the new ci.yml step header), naming the measured local cost (~1 s warm) and the Cargo.toml:3 exclude = ["firmware"] hole this closes.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Shape

Three wiring edits, zero new scripts: `lefthook.yml` pre-commit + pre-push, and one line pair in `ci.yml`. ~6 lines total plus comments. Depends on TASK-060.01 landing first so the gate arrives green.

## Step 1 - lefthook

In `lefthook.yml`, add to **both** `pre-commit.commands` and `pre-push.commands`:

    fmt-check-firmware:
      run: cargo fmt --manifest-path firmware/Cargo.toml --all --check
      forward_stderr: true

No `root:` key, deliberately (see the description: `root:` filters, and an empty filtered set skips the job). No `cd`, because `--manifest-path` resolves the firmware workspace from anywhere.

Two mechanics worth knowing before renaming anything: commands are **not** run in yml order - lefthook sorts by `priority`, then leading digits in the name, then name ascending (`internal/config/command.go:36-92`; visible in `lefthook dump`, which prints pre-commit as `clippy, clippy-log-defmt, clippy-log-usb, doc-artifact-names, fmt-check`). And they run **sequentially** unless the hook sets `parallel: true` (`controller.go:100-106`). Neither affects correctness here; the cost is additive, measured ~1 s warm locally.

## Step 2 - CI

In `.github/workflows/ci.yml`, immediately after `cargo fmt --all --check` (:24), inside the existing single-quoted `nix develop .#default --command bash -c '...'` string - no nested single quotes, no new step, matching how every other check is written there:

    echo "=== cargo fmt (firmware workspace) ==="
    cargo fmt --manifest-path firmware/Cargo.toml --all --check

## Step 3 - prove red, prove green, prove it writes nothing

Paste into notes, in this order:

1. Baseline green: both workspaces exit 0 (after .01).
2. Red: plant one unformatted line in a firmware source file (e.g. collapse a `let` onto one line past 100 columns in `firmware/src/bin/podtest.rs`), then show each of the three call sites failing: `lefthook run pre-commit --command fmt-check-firmware --force --file firmware/src/bin/podtest.rs`, `lefthook run pre-push --command fmt-check-firmware --file firmware/src/bin/podtest.rs`, and the raw cargo line as CI would run it. `--force` on the pre-commit probe defeats the "no staged files" skip so CI can exercise the same definition; note that plain `lefthook run pre-commit` on a clean tree is a no-op for *every* command today.
3. Green: revert the plant, re-run all three.
4. Write-free proof: `git status --porcelain` empty after the green runs. This matters beyond tidiness - `make elf-check` compares input mtimes against the ELF (`firmware/Makefile:216,228-230`), so a gate that reformatted firmware sources at commit time would look like staleness on the bench until TASK-056 lands. `--check` is not interchangeable with a bare `cargo fmt`; keep it.

## Step 4 - comments that go stale

- `lefthook.yml:6-7` says the docs check is the cheapest gate "here on purpose". Still true (~0.15 s vs ~1 s), but say what the new command covers.
- Name the hole in the new comment: root `Cargo.toml:3` excludes `firmware/`, so `--all` never meant all, and TASK-044/TASK-060 are the drift that proves it.
- `README.md`, `CLAUDE.md` and `docs/reference/*.md` carry no gate claims (checked 2026-09-12), and `scripts/check-doc-artifact-names.sh` scans only `README.md` and `docs/**/*.md`, so nothing added here can trip that gate. The prose that *is* wrong lives in `backlog/docs/doc-001`; TASK-060.04 owns that rewrite.

## Cost, labelled local

~1 s warm per call site inside `nix develop .#default` on aarch64-darwin. rustfmt reads sources only; no target dir involved, so cold/warm barely differs.
<!-- SECTION:PLAN:END -->
