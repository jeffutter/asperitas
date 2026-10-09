---
id: TASK-049
title: >-
  Chore: widen the cargo doc warnings-as-errors gate from asperitas-logging to
  the whole workspace
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-10 09:22'
updated_date: '2026-09-11 02:42'
labels:
  - chore
  - planned
dependencies:
  - TASK-043
  - TASK-048
priority: low
ordinal: 78500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Follow-up to TASK-043, which gates `cargo doc` with warnings-as-errors for asperitas-logging only. Narrow on purpose: asperitas-pod and asperitas-dsp carry four pre-existing warnings of their own (TASK-048), and diluting AC #1 of TASK-043 with crates it never touched would have made that ticket uncloseable for reasons unrelated to its own work.

Once TASK-043 and TASK-048 are both done, widen the same two commands to the whole workspace so a new broken link anywhere fails the push rather than accumulating. Also document asperitas-cli, which has no warnings today but sits inside `cargo doc --workspace` anyway.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Root Cargo.toml carries [workspace.lints.rustdoc] denying broken_intra_doc_links and private_intra_doc_links, and all four members (asperitas-cli, asperitas-dsp, asperitas-logging, asperitas-pod) opt in with "[lints] workspace = true". asperitas-logging's own [lints.rustdoc] stanza is deleted in the SAME commit: a member that both inherits and declares its own lints.rustdoc is a hard manifest error that breaks every cargo command in the workspace, fmt/clippy/test included.
- [x] #2 With RUSTDOCFLAGS unset, cargo doc -p <crate> --no-deps exits non-zero once a throwaway bad intra-doc link is added, proven separately for each of the four crates (inheritance reached every member, not just once). The throwaway links are then reverted and the tree confirmed clean with git diff --no-ext-diff.
- [x] #3 The inherited deny perturbs nothing else: cargo check --workspace, cargo clippy --workspace --all-targets -- -D warnings, and cargo test --workspace all pass, and cargo doc -p asperitas-logging --no-deps still exits 0 on the real docs.
- [x] #4 lefthook pre-push and ci.yml each run exactly two doc commands, widened from "-p asperitas-logging" to "--workspace": one at default features and one at --all-features. NOT the --features asperitas-pod/pod-hw variant named originally — measured, it documents asperitas-logging with default features and re-blinds usb, led, panic_handler and set_backend_defmt. Default features is kept as its own run because under --all-features the log-usb-off fn.init() surface disappears; neither run is a superset. No other restructuring of either file.
- [x] #5 Cost recorded in the notes: warm and cold wall time for both widened commands (baseline already measured by planning, confirm it), plus CI's observed time on the first widened run. "lefthook run pre-push --job <name>" passes for each doc command locally, and the exact quoted command strings from ci.yml run green through bash -c.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
One focused change: lift the rustdoc gate TASK-043 built for one crate to the whole workspace, in two layers — a declarative deny that lives in the manifests, and widened `cargo doc` commands in pre-push + CI. Everything below was measured on this tree (`a63ff08`, cargo/rustc 1.97.1, 10-core macOS), not reasoned about.

**Prerequisite check performed during planning:** TASK-043 is genuinely Done, not just marked Done. `RUSTDOCFLAGS="-D warnings" cargo doc -p asperitas-logging --no-deps` exits 0 at default features, at `boot-led,log-usb,log-defmt`, and at `--all-features`. TASK-048 is likewise closed, so pod/dsp are clean. Both deps of this ticket hold.

## Why no sub-tickets

The whole change is ~14 lines across six files (root `Cargo.toml`, four member manifests, `lefthook.yml`, `.github/workflows/ci.yml`) plus a throwaway-bad-link experiment that leaves nothing behind. Both halves are mechanical config edits with an unambiguous path; splitting them would create tickets whose only relationship is that they share a sentence in the same comment. Execute in one sitting.

## Measured facts the plan rests on

Coverage differs per feature set, and **neither set is a superset of the other**:

| Command (`cargo doc --workspace --no-deps …`) | rustdoc pages present | warm | cold, empty target dir |
|---|---|---|---|
| *(default features)* | `logging::init` present; **no** `usb`, `led`, `panic_handler`, `set_backend_defmt`; **no** pod `led`, `pins` | 1.2 s / 0.75 s repeat | 4.6 s |
| `--features asperitas-pod/pod-hw` | pod gains `led`, `pins`; logging **still blind** — no `usb`, `led`, `panic_handler`, `set_backend_defmt` | 0.8 s | — |
| `--all-features` | everything: `usb`, `led`, `panic_handler`, `fn.set_backend_defmt`, `static.LOG_PIPE`; pod `led`, `pins`; **but no `fn.init`** | 1.0 s | 13.8 s |

All three exit 0 today under `-D warnings`, i.e. the workspace is already clean enough to gate. Alternating default ↔ `--all-features` costs the same as repeating either (0.75 s / 0.98 s), so keeping both runs causes no fingerprint thrash.

Two consequences:

1. **AC #1 as originally written is wrong.** Features are per-package, so `--workspace --features asperitas-pod/pod-hw` documents asperitas-logging with *default* features — verified by the absence of `usb/`, `led/`, `panic_handler/` and `fn.set_backend_defmt.html` in that run's output. That variant buys pod's two extra modules and re-blinds precisely the device surface TASK-043 exists to cover (17 of its original 24 warnings lived behind those features). Replaced in the ACs below with `--all-features`, which covers pod-hw *and* the logging trio in one run.
2. **Still two runs, not one.** Under default features rustdoc sees `fn.init()` (compiled only when `log-usb` is off) and loses the device modules; under `--all-features` it gains those and loses `fn.init`. One run of either kind blinds half the `#[cfg]` surface. This mirrors why TASK-043 kept two commands.

## Step 1 — one declarative deny at workspace scope

Root `Cargo.toml` gains (append after `[workspace]`):

```toml
# Read by rustdoc only: `cargo build/check/clippy/test` never see this table, so a plain
# `cargo doc` fails on a broken cross-reference without anyone setting RUSTDOCFLAGS.
# Members opt in individually with `[lints] workspace = true`.
[workspace.lints.rustdoc]
broken_intra_doc_links = "deny"
private_intra_doc_links = "deny"
```

Each of the four members gains:

```toml
[lints]
workspace = true
```

**Delete `crates/asperitas-logging/Cargo.toml`'s existing `[lints.rustdoc]` stanza (lines 54–56) in the same commit**, along with its now-stale comment (the explanation moves to the root table). This is not cosmetic: a member that both inherits and declares its own `[lints.rustdoc]` is a hard manifest error — reproduced verbatim in a scratch workspace as `cannot override \`workspace.lints\` in \`lints\`, either remove the overrides or \`lints.workspace = true\` and manually specify the lints` — and because the failure happens at manifest load it breaks *every* cargo command in the workspace, `fmt` and `clippy` included, not just `doc`. Inheriting is opt-in per member; a member with no `[lints]` table simply keeps whatever it declares itself.

`firmware/` is listed under `exclude` in the root workspace, so it inherits nothing and is unaffected by this table.

Verified in a scratch workspace that the inherited deny bites correctly and narrowly: `cargo doc -p a --no-deps` with **no** `RUSTDOCFLAGS` exits **101** citing `-D rustdoc::broken-intra-doc-links`, while `cargo check --workspace`, `cargo clippy --workspace --all-targets`, `cargo test --workspace` and `cargo build --workspace` all stay at 0.

## Step 2 — prove it bites, per crate, then clean up

For each of `asperitas-cli`, `asperitas-dsp`, `asperitas-logging`, `asperitas-pod`: add one line such as `/// Cross-ref: [`DefinitelyNotAnItem`].` to the crate's top-level `lib.rs`, run `cargo doc -p <crate> --no-deps` with `RUSTDOCFLAGS` unset, and record the non-zero exit. Four proofs, one per crate — the point is that inheritance actually reached every member, not that the mechanism works once.

Then `git checkout -- crates` and confirm the tree is clean with `git diff --no-ext-diff --stat`. Standing repo gotcha: `diff.external` is difftastic, so a plain `git diff` opens a visualiser instead of emitting patch text and will look like "nothing changed".

Regression check on the merged manifest state: `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Doctests cannot newly fail here — every fenced block in all four crates opens with ` ```text ` or ` ```ignore `, so there is no runnable doctest for the rustdoc lint pass to trip over (checked exhaustively; 24 blocks, all in asperitas-logging except pod's one `text` example).

## Step 3 — widen the two gates

`lefthook.yml`, replacing the two existing doc commands under `pre-push` (keep `forward_stderr: true`, and rewrite the comment above them — it currently claims the deny applies "for this crate alone", which step 1 makes false):

```yaml
    doc-links:
      run: RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
      forward_stderr: true

    doc-links-all-features:
      run: RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
      forward_stderr: true
```

Rename `doc-links-device` → `doc-links-all-features`: the feature set it names changed meaning, and a stale name in front of a failing push is a small tax paid forever. Nothing references that name outside `lefthook.yml` (checked).

`.github/workflows/ci.yml`, swapping the two doc blocks for the same pair with matching `echo "=== cargo doc (workspace) ==="` / `=== cargo doc (workspace, all features) ===` banners, in place between the clippy steps and `cargo test`. Stay inside the existing single `nix develop .#default --command bash -c '...'` script so the environment is still built once. Quoting hazard is nil: `RUSTDOCFLAGS="-D warnings"` already sits inside that single-quoted script today and needs no new nesting.

Leave `cargo test --workspace --features asperitas-pod/pod-hw` and the pod-hw clippy step exactly as they are — this ticket is about docs, and those two compile things `--all-features` would also change codegen for.

## Step 4 — verification

1. `lefthook run pre-push --job doc-links` and `… --job doc-links-all-features`. Both `--job` and `--command` are accepted by the installed lefthook 2.1.10 and both filter correctly (measured: each ran alone in 0.5–0.8 s, exit 0); `--job` is what current docs use, so prefer it in the notes.
2. Reproduce CI's quoting locally by running the two doc lines exactly as they appear inside the workflow's `bash -c` string.
3. `cargo doc -p asperitas-logging --no-deps` with no flag still exits 0 — i.e. the workspace-wide deny did not convert some already-present warning elsewhere into a failure outside the two widened commands.
4. Full regression sweep from Step 2. No firmware source is touched, so the cross-compile need not be re-run.

## Out of scope, filed separately if anyone wants them

- **`firmware/` docs are ungated.** Excluded from the root workspace, so it inherits nothing, and no `cargo doc` step covers it. Adding its own `[workspace.lints.rustdoc]` plus a doc job needs a host-target story for `cargo doc` on a cross-compiled crate first — its own ticket.
- **CI has no cargo cache** (only `install-nix-action`), so every CI doc run pays cold. `Swatinem/rust-cache` is the de-facto fix, but where the target dir actually lives inside a nix shell is a question worth its own ticket rather than a side quest here.
- **rust-lang/rust#134904** (still open, dup candidate #119965): the `/// pub mod foo;` + `//!` merge bug that put module-header links in the wrong scope. Unfixed on 1.97.1, so TASK-043.01's convention "module summaries live in `//!`" remains load-bearing with no lint enforcing it. A grep-based guard is possible; not this ticket.
- **rust-lang/rust#114626** (open): a link to an item behind an inactive `#[cfg(feature)]` always reports unresolved, on every stable. The repo's choice — prose/code text for those two sites — is the correct stable-only answer (`--cfg docsrs` + `#[doc(cfg)]` is nightly and tokio/tracing-style). Do not "helpfully" turn those back into links.

## Risks

- `--all-features` becomes a landmine the day someone adds a feature that cannot build for the host target. Today the entire feature surface is four knobs (`boot-led`, `log-usb`, `log-defmt`, `pod-hw`, plus dsp's `std`), no `compile_error!` guard forbids any combination, and the run is clean. If it ever breaks, the measured fallback is the explicit list — `--features asperitas-logging/boot-led,asperitas-logging/log-usb,asperitas-logging/log-defmt,asperitas-pod/pod-hw` — which also exits 0 today at the same ~1 s warm cost.
- CI wall time is unmeasured. Local cold for `--all-features` is 13.8 s on 10 cores against GitHub's smaller runners, and the doc step probably rebuilds host dependencies because clippy-driver artifacts don't share fingerprints with a plain build. Expect a couple of minutes worst case on a cold runner; the pre-push experience stays ~1 s warm either way.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Cost figures measured during planning (`a63ff08`, cargo/rustc 1.97.1, 10-core macOS, `RUSTDOCFLAGS="-D warnings"`), so AC #3 has a baseline to confirm rather than collect from scratch. Warm = shared `target/` already populated; cold = `--target-dir` pointing at an empty directory, i.e. every host dependency compiled from source.

| Command | warm | cold |
|---|---|---|
| `cargo doc --workspace --no-deps` | 1.2 s first, 0.75 s thereafter | 4.6 s |
| `cargo doc --workspace --no-deps --all-features` | 1.0 s | 13.8 s |
| `cargo doc -p asperitas-logging --no-deps` (today's gate, for comparison) | 0.1–0.7 s | — |

Alternating the two widened commands costs the same as repeating either (0.75 s / 0.98 s), so there is no feature-fingerprint thrash penalty for keeping both. Total added pre-push cost: about 2 s warm. That is cheaper than the 13 s figure TASK-043 recorded for its device-feature command, because `--no-deps` plus an already-warm dep graph does almost all the work.

SUPERSEDED 2026-10-09 (see the CI note at the end of this ticket): CI wall time, which this machine could not tell us at the time — ci.yml carries no cargo cache, so each run is effectively cold on a smaller runner. Append it when the first widened run finishes.

Executed at 76eee5e on cargo/rustc 1.97.1, 10-core macOS. Files: root Cargo.toml (+[workspace.lints.rustdoc]), four member manifests (+[lints] workspace = true; logging's own [lints.rustdoc] stanza deleted in the same commit), lefthook.yml, .github/workflows/ci.yml. No source touched.

AC #2 — per-crate proof, RUSTDOCFLAGS unset, one throwaway '/// Cross-ref: [`DefinitelyNotAnItem`].' appended to each crate's lib.rs: asperitas-cli, -dsp, -logging, -pod each exit 101 citing 'error: unresolved link to `DefinitelyNotAnItem`' / 'could not document'. Inheritance reached all four, not just once. Reverted; git diff --no-ext-diff then showed only this ticket's intended edits (used git status --short to confirm, since a plain git diff opens difftastic).

Gotcha hit for real: my cleanup was 'git checkout -- crates', which also reverted the three manifest edits sitting under that same path. Reapplied by script; if you prove via src files, revert those paths specifically.

AC #3 — cargo check --workspace rc=0 (0.4s warm), cargo clippy --workspace --all-targets -- -D warnings rc=0 (1.0s), cargo test --workspace rc=0 (73.8s incl. compile), and env -u RUSTDOCFLAGS cargo doc -p asperitas-logging --no-deps rc=0: the workspace-wide deny added no failure outside the two widened commands.

AC #5 cost, confirmed against planning's baseline (near-identical):
| command | warm | cold (empty --target-dir) |
| cargo doc --workspace --no-deps | 1.3s first, 0.7-0.8s after | 4.9s (planning: 4.6s) |
| cargo doc --workspace --no-deps --all-features | 1.0s | 13.6s (planning: 13.8s) |
Added pre-push cost ~2s warm. lefthook run pre-push --job doc-links -> 0.79s rc=0; --job doc-links-all-features -> 1.04s rc=0. Both ci.yml command strings copied verbatim through bash -c: rc=0, 0.8s / 1.0s.

CI wall time is NOT observed and cannot be from here. main is 61 commits ahead of origin/main — the loop never pushes — and ci.yml triggers only on push/PR to main (no workflow_dispatch), so there is no widened run to read. Last main-push run (#34306590251, 2026-09-09) totaled 2m28s end to end with the old narrow doc steps in it; local cold for the two widened commands sums to 18.5s, so a couple of minutes worst case stands. Owner owes this figure on the next push: gh run list -R jeffutter/asperitas --limit 3. Checking AC #5 on the half that is measurable here; pushing is @human work per CLAUDE.md.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Rustdoc's two intra-doc-link lints are now denied declaratively at workspace scope and every member inherits them; pre-push and CI each run cargo doc over --workspace twice (default features, then --all-features) under RUSTDOCFLAGS=-D warnings. A new broken cross-reference anywhere in the four crates fails the push for ~2s warm. Cost confirmed against planning's baseline; CI's own wall time stays owed until the owner pushes (main is 61 ahead, ci.yml has no workflow_dispatch).
<!-- SECTION:FINAL_SUMMARY:END -->

2026-10-09 CI observation (closes TASK-052): the widened doc steps ran on GitHub Actions in run 37925833149 (push of 04c324d, success). '=== cargo doc (workspace) ===' took 0.51s and '=== cargo doc (workspace, all features) ===' took 2.21s. The whole job took 17m10s, dominated by the nix toolchain step (10m41s) and the two cargo test runs (123.90s and 131.10s). The earlier note that CI wall time was not observed is now answered by this paragraph.
