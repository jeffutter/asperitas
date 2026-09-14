---
id: TASK-064.01
title: Add the cargo cache layer to ci.yml for both workspaces
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-13 05:36'
updated_date: '2026-09-14 14:12'
labels:
  - planned
dependencies:
  - TASK-061
references:
  - .github/workflows/ci.yml
  - scripts/gates.sh
modified_files:
  - .github/workflows/ci.yml
parent_task_id: TASK-064
priority: medium
type: chore
ordinal: 103800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-064's implementation half. Nothing here needs a board or ears; everything here needs a runner to prove, which is TASK-064.02 and stays human-owned. Do not mark the parent done from this ticket.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 ci.yml gains one cache layer, and its keys cover BOTH target directories - the host workspace's `target` and firmware's own `firmware/target` - because root Cargo.toml excludes firmware/, so a single-workspace key silently caches half the build and reads as green while the cross-builds stay cold.
- [x] #2 The key cannot turn a stale artifact into a false pass: it is restored into the same path cargo would use, and a run whose inputs changed must not reuse fingerprints keyed on something narrower than Cargo.lock plus the sources. State in a comment what the key is made of and why, and name the failure mode a wrong key produces - clippy or test saying "finished" having compiled nothing.
- [x] #3 Works inside the nix shell the step already enters (`nix develop .#default --command bash scripts/gates.sh ci`), where cargo comes from the flake rather than a setup-rust action, so any toolchain-pinning assumption inherited from upstream examples is checked rather than copied. Record which of Swatinem/rust-cache (multi-workspace support, nix-shell support) or `RUSTC_WRAPPER=sccache` was chosen and why, against the alternative.
- [x] #4 Local behaviour is unchanged: `scripts/gates.sh ci` still exits 0 with the same 17 gates, and `--list` is untouched - the cache is a runner concern and must not leak a single gate or env var into the shared definition.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
### What this ticket really has to decide

There are two caches on the table and they are not interchangeable. The one that costs time here
is *build artifacts* (both workspaces compile from cold on every push), and the one people reach
for first, `RUSTC_WRAPPER=sccache`, cannot serve it. sccache never caches a crate whose
compilation invokes the linker, which is all six `firmware/src/bin/*.rs` binaries plus every host
test and example binary -- i.e. most of what the gates spend time on. It also refuses incremental
debug builds, so the host `cargo test --workspace` pair (133 s of the local 144 s) is outside it
too. And it needs `pkgs.sccache` added to flake.nix plus `RUSTC_WRAPPER` exported into the step,
which is precisely the leak AC #4 forbids in spirit. Rejected, with its price: nothing.

Raw `actions/cache` was considered second. Hand-rolling the key means hand-rolling rustc identity,
lockfile normalization, target-dir discovery for two workspaces, and dependency pruning -- about
120 lines of YAML+shell whose only test is a runner nobody can watch. Rejected for that reason
alone; see "Sizing" for why the pruning matters too.

Chosen: `Swatinem/rust-cache`, pinned to v2.9.2's commit. Its behavior below is read from
v2.9.2's `src/config.ts`, `src/cleanup.ts`, `src/workspace.ts` and `action.yml`, not from its
README, which understates two things (it says `.cargo/config.toml` files count only "in the root
of the repository"; `config.ts` globs `${root}/**/.cargo/config.toml` per workspace, so
`firmware/.cargo/config.toml` IS in the key).

Pin the commit, not `@v2`: measured with `git ls-remote`, the floating `refs/tags/v2` points at
`49a0bdc7`, which is not v2.9.2 (`6323deb1`). A moving tag means the action that computes our keys
can change underneath a plan that was written against a specific source tree, and this repo has no
dependabot to notice.

### The edit: `.github/workflows/ci.yml`

Insert after `uses: cachix/install-nix-action@v31` and before the gate step (order is load-bearing:
the step needs `checkout` for the manifests it hashes and `nix` for the `cmd-format` it runs).

```yaml
      # Toolchain identity, printed rather than assumed. Everything under "Cargo cache" derives
      # its key from whatever `rustc` this shell resolves, so state which one that is: the nix
      # sysroot built by rust-overlay (1.97.1 + the thumbv7em-none-eabihf std, flake.nix:31-34),
      # never the runner image's preinstalled Rust. Measured locally that `nix develop --command`
      # prepends the shell's paths to the inherited PATH, so a stray ~/.cargo/bin stays reachable
      # behind them -- whether ubuntu-latest has one is recorded by the line below, because the
      # cache action asks `rustup toolchain list` too and would fold anything it finds into the
      # key. Diagnostic only, deliberately not a gate: AC #4 keeps the shared definition clean of
      # runner concerns.
      - name: Toolchain identity the cache key is built from
        run: |
          nix develop .#default --command bash -c 'printf "rustc  %s\n" "$(command -v rustc)"; printf "cargo  %s\n" "$(command -v cargo)"; printf "rustup %s\n" "$(command -v rustup || echo none)"'

      # Cargo cache. Two workspaces named, because root Cargo.toml:3 declares exclude =
      # ["firmware"]: firmware/ is a separate workspace with its own Cargo.lock and its own
      # firmware/target, so naming only "." would cache the host build, leave all four firmware
      # cross-build/clippy invocations cold, and still report green (AC #1).
      #
      # What the key is made of (v2.9.2 src/config.ts): "v0-rust", then the `key:` input, then the
      # job id ("check"), then runner OS and arch, then sha1(rustc -vV release/host/commit-hash
      # for every toolchain it can name, plus every environment variable whose name starts with
      # CARGO, CC, CFLAGS, CXX, CMAKE or RUST), then sha1(each workspace member's Cargo.toml with
      # versions and path-dependency details normalized away, each Cargo.lock reduced to the
      # packages that have a `source` or `checksum` -- i.e. the external ones -- and every
      # .cargo/config.toml and rust-toolchain file under either workspace root).
      #
      # Sources are NOT in the key. That is not a hole, and this is the part that must not be
      # "simplified": the action does not save workspace-member artifacts at all (`cache-workspace-
      # crates` defaults false, src/save.ts + src/cleanup.ts keep only build/, .fingerprint/ and
      # deps/ entries whose names belong to *dependencies*), so there is nothing stale left for a
      # gate to mistake for a pass. Cargo decides freshness for the crates we do rebuild from
      # their own fingerprints and mtimes, and a restored archive carries artifact mtimes older
      # than a fresh checkout's sources, which errs toward compiling again, never toward skipping
      # it. The failure mode a too-narrow key produces is a gate printing "Finished" having
      # compiled nothing -- clippy reporting no warnings because it never re-ran -- and upstream
      # issue #348 documents exactly that appearing once `cache-workspace-crates: true` is set. So
      # do not set it, and do not add `git-restore-mtime-action` to make unchanged sources look
      # old: that is the mtime-trusting bug TASK-056 removed from elf-check, and docs/reference/
      # daisy-seed3.md:833 already records cargo declining to link on mtimes alone.
      #
      # `key:` carries the nix toolchain identity, which is AC #3. cmd-format is the primary fix:
      # it makes the action ask the same `rustc` and `cargo` the gates use, instead of the runner's
      # preinstalled Rust, so a flake toolchain bump changes the key. It is also the thing upstream
      # examples get wrong for nix repos. `key: nix-${{ hashFiles('flake.lock') }}` is the belt:
      # a cmd-format string without exactly one "{0}" silently falls back to "{0}" (config.ts:60-63),
      # and flake.lock is what actually pins the toolchain -- it pins nixpkgs and rust-overlay, and
      # rust-overlay builds the sysroot. With it, a silent fallback degrades to a cold cache, which
      # is visible, rather than to a key that stops noticing toolchain changes, which is not.
      - uses: Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6 # v2.9.2
        with:
          workspaces: |
            . -> target
            firmware -> target
          cmd-format: nix develop .#default --command {0}
          key: nix-${{ hashFiles('flake.lock') }}
          # Pull requests still READ main's cache -- GitHub serves base-branch caches to PRs -- but
          # do not add their own, so a branch that is never merged cannot churn the 10 GB budget
          # with a key nothing will ever hit again.
          save-if: ${{ github.ref == 'refs/heads/main' }}
          # Without this the post step is skipped on a failed job (action.yml `post-if`), so the
          # first red lint throws away every dependency artifact that run already built and the
          # next run pays full price. Correctness is unaffected: the saved content is pruned to
          # dependencies, and cargo re-fingerprints everything above them regardless.
          cache-on-failure: true
```

Nothing else in the workflow changes, and `scripts/gates.sh` and `lefthook.yml` are not touched at
all. The action exports `CARGO_INCREMENTAL=0` and `CACHE_ON_FAILURE` into the job environment
(restore.ts:29-30); both are runner-side and neither changes any gate's verdict -- incremental
off is what makes the restored dependency artifacts usable in the first place.

### Why the five inputs and not more

Each one answers a question the defaults get wrong for this repo; anything else stays at its
default rather than becoming an undocumented knob.

| Input | Why it is here |
|---|---|
| `workspaces` | Two target dirs, AC #1. Also makes the action hash `firmware/Cargo.lock` (147 packages) alongside the root one (150). |
| `cmd-format` | Runs `rustc -vV` and `cargo metadata` inside the same shell the gates run in, AC #3. Upstream's own `nix.yml` does exactly this. |
| `key` | Flake-lock-derived toolchain identity, as the belt described above. |
| `save-if` | Only main writes; PRs read. |
| `cache-on-failure` | Keeps warm progress across red runs. |

Left alone deliberately: `cache-bin` (nothing installs to `~/.cargo/bin` under nix, so the default
path is a no-op), `cache-all-crates` / `cache-workspace-crates` (both keep stale workspace
artifacts, the named false-pass risk), `cache-targets` (false would defeat the point), `env-vars`
(the default prefixes already cover `RUSTFLAGS`, and firmware's `-C link-arg=-Tlink.x` lives in
`firmware/.cargo/config.toml`, which is globbed into the key anyway), `shared-key` (would drop the
job id and couple future jobs' caches together), `prefix-key`, `lookup-only`, `cache-provider`.

Note `DEFMT_LOG` is not in the key and does not need to be: it selects which defmt frames get
compiled into the firmware crate, and the firmware crate is never restored from cache.

### Sizing, so the 10 GB budget is a measured choice

Local raw sizes: `target` 3.2 GB, `firmware/target` 3.6 GB. Both are compressed before upload and
both are pruned to dependencies first, and the pruning is where the payoff differs: the root
workspace is 5 small crates over ~150 packages, while firmware is one package over ~147, so the
cross-builds are nearly all dependency time and are the better bet. GitHub evicts least-recently-
used caches past 10 GB per repo; with one job writing one entry per distinct lock/toolchain
combination on main only, the steady state should be a handful of entries, and TASK-064.02 records
whether that held.

### Steps

1. Read `.github/workflows/ci.yml` and confirm the insertion point (after `cachix/install-nix-action@v31`).
2. Make the edit above verbatim, comments included. Do not reformat the existing steps.
3. Confirm the shared definition is untouched (AC #4). The change is one file, and that is the
   proof: `git status --porcelain` lists only `.github/workflows/ci.yml`, and
   `git diff --stat scripts/gates.sh lefthook.yml` prints nothing. Then confirm the script still
   answers identically: `bash scripts/gates.sh --list` must end with
   `counts: commit 12, push 21, ci 22`.
4. Confirm the tier still passes end to end locally: `bash scripts/gates.sh ci; echo "exit=$?"`
   must exit 0 and print `tier ci: 22 gates`. **AC #4 says "the same 17 gates" -- that number is
   already stale on main** (`--list` says 22 today, measured before this plan was written), which
   is the figure drift TASK-070 exists for. Assert the real invariant -- `--list` unchanged and
   exit 0 -- and say in the commit message that the criterion's 17 is out of date, rather than
   quietly renumbering it or reporting a mismatch as a failure.
5. Prove the YAML parses and every input name is real, so a typo like `workspace:` -- which the
   action would ignore, silently caching nothing and reading green -- cannot survive. Both `yq`
   forms below were run against this shell's yq (mikefarah yq-go 4.53.3, flake.nix:55, NOT Python
   yq) before this plan was written:
   ```bash
   curl -sS -o /tmp/rust-cache-action.yml \
     https://raw.githubusercontent.com/Swatinem/rust-cache/6323deb102c322ba6fcbdcafc7e3dddab59af2b6/action.yml
   nix develop .#default --command yq '.jobs.check.steps[] | select(.uses // "" | test("rust-cache")) | .with | keys | .[]' \
     .github/workflows/ci.yml | sort > /tmp/used.txt
   nix develop .#default --command yq '.inputs | keys | .[]' /tmp/rust-cache-action.yml | sort > /tmp/supported.txt
   comm -23 /tmp/used.txt /tmp/supported.txt   # must print nothing
   cat /tmp/used.txt                            # expect exactly the five inputs named above
   ```
6. Sanity-check the step ordering by eye: checkout, install-nix-action, identity diagnostic,
   rust-cache, gates.
7. Commit as `TASK-064.01: ...`. If pre-commit complains, fix the cause; do not use `--no-verify`.

### What this ticket cannot prove, and who does

No agent can observe a runner: main sits far ahead of `origin/main` and the workflow has no
`workflow_dispatch`. Every claim above about key composition is source-level (file and line cited),
and every claim about what a warm run actually saves belongs to TASK-064.02. Leave AC #1 through
#4 checked on the strength of the local checks plus the source reading, and say in the final
summary which of the two each rests on. Do not mark TASK-064 done from here; it inherits `@human`
from its other child.

Hand to TASK-064.02, in addition to the timings it already asks for: the `Cache Configuration`
log group's `Cache Key:` line from both runs, the `.. Environment considered: - Rust Versions:`
lines (more than one version listed means the runner's rustup answered and the key contains a
toolchain nobody installed), and whether the second run printed `Restored from cache key "..."
full match: true`. Those three lines are what distinguishes "the cache missed" from "the cache was
never wired to the toolchain", which look identical from wall time alone.

### Out of scope

- Caching the nix store itself (`cachix` or a `nix` store cache action). Real cost, different
  mechanism, unrelated to cargo fingerprints.
- Fanning the gate list out into one job per gate. This ticket is its prerequisite, not it.
- Any new gate, or any change to what `gates.sh` knows about caching. AC #4.
- Runner-side figures. TASK-064.02, and TASK-052/TASK-063 for the pre-existing gap.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
One file changed: .github/workflows/ci.yml. Gains a toolchain-identity diagnostic step and one Swatinem/rust-cache@6323deb1 (# v2.9.2) step between install-nix-action and the gate step. scripts/gates.sh and lefthook.yml untouched (AC #4), verified by git diff --stat printing nothing.

Evidence per AC, and what each rests on. All source-level claims below were re-read from the pinned commit's actual TypeScript this run (config.ts, cleanup.ts, save.ts, restore.ts, utils.ts, workspace.ts, action.yml fetched at 6323deb1), not from the plan or the README.

AC #1 (both target dirs): workspaces names ". -> target" and "firmware -> target", because root Cargo.toml:3 excludes firmware/. Re-measured locally: target 3.2 GB, firmware/target 3.6 GB, root Cargo.lock 150 packages, firmware/Cargo.lock 147. Restores into the same path cargo uses (config.ts:274-277 pushes each Workspace.target into cachePaths), so no redirection is involved.

AC #2 (no stale false pass): the key covers rustc identity + env-var prefixes + normalized Cargo.toml + external-only Cargo.lock entries + every globbed .cargo/config.toml (config.ts:157-168 does glob ${root}/**/.cargo/config.toml per workspace, so firmware/.cargo/config.toml is in the key even though the README implies only repo-root ones count). Sources are deliberately absent because the save side prunes workspace-member artifacts (cache-workspace-crates defaults false, action.yml:43-46; save.ts:39-54 keeps only packages outside each workspace root). Comment in ci.yml states the composition and names the failure mode a too-narrow key produces: a gate printing Finished having compiled nothing. Cross-checked upstream issue #348, which is that class of failure arriving once cache-workspace-crates is set true.

AC #3 (works inside the nix shell): cmd-format routes the action's rustc -vV, rustup toolchain list and cargo metadata through nix develop .#default --command, so keying uses the same compiler the gates use. Verified getCmdOutput (utils.ts:15-20) splits the formatted string and execs it, so the multi-word wrapper works. Also verified the fallback hazard: config.ts:58-67 replaces a cmd-format without exactly one {0} with plain {0}, hence the belt key: nix-${{ hashFiles('flake.lock') }}. Chose rust-cache over RUSTC_WRAPPER=sccache because sccache skips anything that invokes the linker (all six firmware/src/bin/*.rs bins, plus every host test and example binary) and refuses incremental debug builds, so most of the 133 s of test time stays outside it; over raw actions/cache because hand-rolling rustc identity, lockfile normalization, two-workspace target discovery and dependency pruning is ~120 lines whose only test is a runner nobody can watch. Recorded in the ci.yml comment, not just here.

AC #4 (local behaviour unchanged): bash scripts/gates.sh --list still ends counts: commit 12, push 21, ci 22. bash scripts/gates.sh ci exits 0 and prints tier ci: 22 gates, 145.0s (measured twice, /tmp/gates-ci-06401.log and /tmp/gates-ci-06401b.log; the script runs set -euo pipefail, so reaching the summary line means every gate passed). The ticket text says 17 gates: that figure is stale on main, where --list already said 22 before this change. Asserted the real invariant instead (--list byte-identical, exit 0) rather than renumbering the criterion; TASK-070 owns the figure drift. No gate and no env var was added to the shared definition; CARGO_INCREMENTAL=0 and CACHE_ON_FAILURE come from the action's own restore step (restore.ts:29-30) into the job environment only.

Validation beyond the plan's steps: yq (.jobs.check.steps) parses the workflow (5 steps, order checkout -> nix -> identity -> rust-cache -> gates); the five with: keys were diffed against the action's declared inputs at the pinned SHA and comm -23 printed nothing, so no typo like workspace: can silently cache nothing; the diagnostic's run: block was extracted via yq and passed bash -n, then run locally, printing the nix-store rustc 1.97.1 and rustup none.

Two corrections to the plan, both applied: (a) it asserted refs/tags/v2 points at 49a0bdc7 'which is not v2.9.2'. Measured with git ls-remote including the ^{} deref, 49a0bdc7 is the annotated tag object for v2 and it resolves to 6323deb1, i.e. v2.9.2's commit right now. Pinning is still correct, because the tag moves and there is no dependabot here, but the comment says that rather than repeating the wrong comparison. (b) The plan implied the action hashes the nix shell's environment variables; it hashes its own process.env (config.ts:110-124), which is the runner environment, since the cache step itself does not run inside nix develop. That is why cmd-format and the flake-lock key carry the toolchain, and the comment now says so.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
ci.yml now caches both workspaces through Swatinem/rust-cache pinned to 6323deb1 (v2.9.2), with a preceding step that prints the rustc/cargo/rustup the nix shell resolves. scripts/gates.sh and lefthook.yml are untouched; the cache is entirely a runner concern.

What each AC rests on. AC #1, #2 and #3 rest on source reading of the pinned commit's TypeScript (key composition, workspace/target handling, cmd-format fallback, save-side pruning) plus the local YAML and input-name validation; none of them is observable from this machine beyond that. AC #4 rests on measurement: --list byte-identical at counts: commit 12, push 21, ci 22, and gates.sh ci exiting 0 with tier ci: 22 gates, 145.0s, twice.

Two things stated rather than copied from the plan: refs/tags/v2 currently dereferences to v2.9.2's commit (the plan compared an annotated tag object against a commit), and the action hashes its own runner-side process.env, not the nix shell's, which is why cmd-format plus a flake-lock key carry the toolchain identity.

AC #4's "same 17 gates" is stale on main, where --list said 22 before this change; asserted the real invariant and said so in the commit message instead of renumbering it quietly. TASK-070 owns the figure drift.

Not proven here, by design: whether a warm run actually reuses artifacts. That needs a runner and belongs to TASK-064.02, whose comment already names the three log lines that separate 'the cache missed' from 'the cache was never wired to the toolchain'. The parent TASK-064 stays @human and is not touched.
<!-- SECTION:FINAL_SUMMARY:END -->
