---
id: TASK-061
title: 'Define the gate set once, so ci.yml and lefthook cannot diverge again'
status: Needs Plan
assignee:
  - '@agent'
created_date: '2026-09-13 00:10'
labels: []
dependencies:
  - TASK-060
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
