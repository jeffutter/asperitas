---
id: TASK-056
title: >-
  Make elf-check's staleness test content-based, so a bulk mtime refresh cannot
  fail it spuriously
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-12 09:25'
updated_date: '2026-09-14 06:54'
labels:
  - planned
dependencies:
  - TASK-053
priority: medium
type: task
ordinal: 87700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-053 shipped `elf-check`, which makes `make probe-log` and `make probe-rtt-list` refuse to run
when any input to the release ELF is newer than it. The check uses `find -newer`, so it compares
timestamps and therefore cannot tell "the sources moved on" from "something rewrote the filesystem
metadata".

Observed on this machine within minutes of the ticket landing: with a freshly linked ELF at
mtime 1789201869892003000, `src/bin/main.rs`, `memory.x` and `Cargo.lock` carried mtimes around
17892018858xx-9xx (about 16 s later, within 60 ms of each other) while `git status` reported the
tree clean. `make probe-log` then failed with "<ELF> is older than src/bin/main.rs" for a tree that
was byte-for-byte what built that ELF. A bulk metadata refresh of exactly this shape is what
`git checkout`, `git stash pop` and worktree operations produce.

Cost today: one wasted cycle per occurrence, with a printed recovery ("rm -f <ELF> && make
build-elf") that works. Not blocking, but the loop drives these targets unattended and the failure
message points at a cause that is not there.

Content hashing removes the class: hash the same input set the current `find` expression selects,
record it next to the ELF at build time, compare at check time. Immune to mtime churn by
construction.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The false positive is reproduced as a script or host check that fails on today's Makefile: after a successful `make build-elf`, touch an input without changing its bytes and `make elf-check` exits 1.
- [x] #2 With the new mechanism the same sequence exits 0, and `elf-check` still exits 1 when an input's bytes actually differ from the ones that built the ELF (prove by editing a source and NOT rebuilding).
- [x] #3 The whole mechanism lives in one place in `firmware/Makefile`, and its comment says out loud what it cannot see: content equality is not proof the board runs that image.
- [x] #4 A missing stamp file fails with an actionable message rather than passing silently.
- [x] #5 `make -n build flash flash-all check` stays byte-identical to HEAD, and the host gates in ci.yml are green.
- [x] #6 The two prose claims that currently say "newer than that ELF" (`docs/reference/daisy-seed3.md` probe section and `README.md`), plus the stale case row of the exit-code table, are reworded to match the new mechanism.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## What lands, in three pieces

1. **The mechanism** in `firmware/Makefile`: a content digest over `ELF_INPUTS`, written by
   `build-elf` beside the ELF, compared by `elf-check`. No mtime anywhere. Replaces the
   `find ... -newer $(ELF)` clause at `firmware/Makefile:264-266`.
2. **A permanent assertion** in a new `scripts/check-elf-staleness.sh`, following
   `elf-provenance.sh --selftest`'s shape, registered as one commit-tier gate. It drives the real
   Makefile through CLI variable overrides, so the mechanism under test is the shipped one, not a
   reimplementation. This satisfies AC #1 and #2 durably instead of as a hand-run transcript.
3. **The prose** AC #6 names, plus three passages nobody named that become false the moment the
   mechanism changes (table below).

No sub-tickets: pieces 1 and 2 must ship together (2 is red against today's Makefile by design, so
it cannot land alone), and 3 is meaningless without 1. One focused session, like TASK-067's 617-line
single commit.

## Step 1: the mechanism (write it first, red-first proof comes in step 2)

Order of work inside the file: replace the staleness clause, then the stamp write, then the comments.

Verified prototype: `/tmp/t056/Makefile` (throwaway, mirrors the real file's plain-`=` spelling so
CLI overrides win). Every case below was run against it; the numbers are measurements, not estimates.

Variables to add near `ELF_INPUTS` (`firmware/Makefile:212`):

```make
# sha256sum where coreutils provides it, shasum where only perl does. Same dual spelling
# scripts/elf-provenance.sh:505-510 already uses. Both emit byte-identical "<hex>  <path>" lines,
# measured, including for names they escape.
SHA256    = $(shell command -v sha256sum >/dev/null 2>&1 && echo sha256sum || echo 'shasum -a 256')
ELF_STAMP = $(dir $(ELF))$(BINARY).elf-inputs.sha256
```

One `define` holding the digest, used verbatim by both recipes, so the two halves cannot drift:

```make
define ELF_INPUTS_SHA256
	listing=$$(mktemp); \
	trap 'rm -f "$$listing"' EXIT; \
	if ! find $(ELF_INPUTS) \( -name target -prune \) -o \( -type f \
		\( -name '*.rs' -o -name '*.x' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \) \
		-print0 \) > "$$listing"; then \
		echo "could not enumerate the ELF inputs" >&2; exit 1; \
	fi; \
	if [ ! -s "$$listing" ]; then \
		echo "no ELF inputs found under: $(ELF_INPUTS)" >&2; exit 1; \
	fi; \
	LC_ALL=C sort -z < "$$listing" | xargs -0 $(SHA256) | $(SHA256) | cut -d' ' -f1
endef
```

Four things in that block are load-bearing, and each was measured:

- **The `mktemp` listing is not paranoia.** The obvious shape, `list=$$(find ... | ... | xargs
  shasum)`, silently passes when the input set enumerates nothing: `xargs` with no arguments runs
  `sha256sum` on stdin, which yields the digest of the empty string, and a non-empty hex string
  sails through any `[ -z ]` test. Reproduced: `ELF_INPUTS=/tmp/t056/emptydir` wrote a stamp and
  exited 0. With the listing file, `[ ! -s ]` catches it and the recipe fails with a named cause.
  Command substitution cannot hold NUL bytes, so `-print0` has to go to a file, not a variable.
- **`LC_ALL=C sort -z`**, not bare `sort -z`: BSD and GNU collate the same path list differently,
  and the digest covers the order.
- **Sort by path only.** File mtimes never enter the digest; that is the whole point.
- **Per-file digests hashed together, not just the aggregate.** Names ride along in the hashed
  stream, so renames count. `sha256sum -c` was considered and rejected: it is mtime-immune and
  names the culprit, but it is blind to *added* files, which is half of what this must catch.

Measured on the real tree: 52 files, 922 KB, 20-25 ms for the pipeline, ~40-50 ms end to end
including make startup. Cheap enough to run on every `build-elf` and every `elf-check`.

### `build-elf` writes the stamp last

```make
build-elf:
	cargo build --release $(CARGO_DEFAULTS) --features "$(FEATURES)" --bin $(BINARY)
	@$(ELF_INPUTS_SHA256) > $(ELF_STAMP).tmp
	@mv $(ELF_STAMP).tmp $(ELF_STAMP)
```

Keep these as separate recipe lines, not one shell command joined by `;`: make stops at the first
failing line, so a failed compile cannot leave a fresh stamp behind. The stamp is the last thing
touched, which is the rule this ticket exists to enforce. `mv` rather than `>` so a reader never
sees a truncated stamp.

`build` (the DFU objcopy target) stays untouched: AC #5 requires `make -n build flash flash-all
check` byte-identical to HEAD. Say so in the comment, and say the consequence out loud too:
`cargo objcopy` compiles before it objcopies, so `make build` relinks `release/$(BINARY)` without
writing a stamp, exactly as the raw-cargo gates do. See "What it cannot see" below.

### `elf-check` compares, and keeps provenance first

Keep the existing order (provenance, then freshness) and the existing three-line shape per failure:
cause, why it matters, the copy-pasteable fix. Replace the stale clause with:

- no stamp at `$(ELF_STAMP)` -> fail naming the path and saying the ELF predates this check or the
  stamp was removed (`cargo clean` deletes it with the rest of `target/`), then print `$(REMEDY)`.
  Never pass silently: AC #4.
- digest mismatch -> `$(ELF) was not built from the sources on disk`, plus the two hashes, plus
  `$(REMEDY)`. Drop the "If only timestamps moved" line, which becomes unreachable advice once
  timestamps stop mattering.

Do not reintroduce any mtime comparison anywhere in the recipe, and do not "helpfully" fall back to
`find -newer` when the stamp is missing.

### What the comment must say (AC #3)

Three limits, each one sentence, next to the mechanism:

1. Content equality is not proof the board runs that image. It never was, and hashing does not
   change it. Only flashing is proof.
2. The stamp describes the last link that went through `build-elf`. Anything that relinks
   `release/$(BINARY)` without it, which is every raw-cargo gate in `scripts/gates.sh:262,269` and
   `make build` itself, leaves the stamp describing an earlier link. Consequence, bounded: after
   editing a source and running gates without `make build-elf`, `elf-check` goes red against an ELF
   that is actually current. The printed remedy fixes it, and the window is narrower than the one
   this ticket removes, which fires on any `git checkout` or `stash pop` with the tree unchanged.
   Follow-up ticket filed (see the end).
3. `firmware/Cargo.lock` is in the input set and no gate passes `--locked`, while
   `firmware/Cargo.toml:107` pins daisy-embassy to a moving `master` branch. A lock refresh can turn
   this red even when the linked artifacts did not change. That is deliberate conservatism, and the
   remedy is the printed relink.

Also worth one line: one stamp per binary name, shared by both cfg sets, which is fine because the
digest covers sources, not features. Task-062's provenance clause is what tells the two images apart.

## Step 2: prove it, red first

Follow `elf-provenance.sh --selftest` exactly: `run_case`, `spawn_script`, a `fail()` that exits,
one `<prog>: selftest <name> ok` line per case on stderr, `passed (N cases)` summary, exit 0 pass /
1 a case failed / 2 the suite could not run, fixtures generated at runtime under `mktemp -d` and
never under `firmware/target/`. TASK-068's plan calls this "copy the harness, not the fixtures".

Injection is what makes this a test of the Makefile rather than of a copy of its logic, and it works
today with no Makefile change at all: `ELF`, `ELF_INPUTS` and `PROV` are plain `=` assignments, so a
command line wins. Verified against the current `HEAD` Makefile with a fixture tree and an ELF dated
2020: `make -C firmware elf-check PROV=true ELF=<fixture> ELF_INPUTS=<fixture>` reproduced
`... is older than /tmp/.../crates/foo/Cargo.toml` and rc 2, building nothing and touching no tracked
file. `PROV=true` stubs the provenance clause (it is `@$(PROV) check ...`, and `true` ignores its
arguments); that also keeps the whole suite off `cargo`, because `elf-provenance.sh check` shells out
to `cargo metadata`. Say both of those in the header, and say that stubbing hides the clause order,
which `elf-provenance.sh --selftest` does not cover either.

Cases, priced at roughly 70-100 ms each (one make, one find, one hash):

| case | setup | expect |
|---|---|---|
| `bulk-mtime-refresh-is-silent` | stamp recorded, then `touch` every input | 0. Red against HEAD's Makefile: this is AC #1 and #2 in one case |
| `byte-edit-without-rebuild` | append a line to one input, do not build | nonzero, message names the digest mismatch |
| `bytes-restored-is-green` | write the original bytes back | 0 |
| `added-input-detected` | create a new `.rs` under the fixture `crates/` | nonzero (this is the case `sha256sum -c` would miss) |
| `renamed-input-detected` | `mv` one input | nonzero |
| `target-stays-pruned` | touch a file under a fixture `target/` | 0 |
| `missing-stamp-fails-loudly` | delete the stamp | nonzero, stderr names the stamp path (AC #4) |
| `empty-input-set-fails-loudly` | override `ELF_INPUTS` to an empty dir | nonzero, not a vacuous pass |
| `no-mtime-operator-in-elf-check` | read the recipe text | fails if `-newer` or `-nt` ever comes back |

Run the suite before wiring the gate and show it red against the pre-change Makefile, then green
after; record both transcripts in this ticket's notes, the way TASK-067 recorded its mutation proof.

Gate line, one, commit tier, immediately after `=== elf-provenance --selftest ===`
(`scripts/gates.sh:207`), priced under 1 s warm. It builds nothing and touches nothing outside its
own `mktemp -d`, so ordering rule 1 ("no gate builds firmware after the pair") is not engaged even
though the gate sits before the pair anyway. TASK-068 wants the same slot; a coordination comment is
posted there saying ours is planned first and it can land behind us.

Alongside the gate line, in the same commit, per TASK-067's precedent: regenerate the `--list` matrix
block and the counts in `backlog/docs/doc-001 - Asperitas-Project-Plan.md:254-284` (commit 10 -> 11,
push 19 -> 20, ci 20 -> 21) and restate the tier costs, plus the `~3 s commit` figure in
`lefthook.yml:12` if the measurement moves it.

## Step 3: prose, in one pass after both mechanisms exist

| location | what it claims today | what it must say |
|---|---|---|
| `README.md:268-276` | "then they refuse if any source is newer than it" | the ELF was not built from these sources. Keep the `.asp.prov` sentence; the trailing claim that deleting the ELF restores a stale timestamp stays true and stays relevant to the remedy |
| `docs/reference/daisy-seed3.md:414-418` | "refuses to run rather than warning if any source is **newer than that ELF**" | content statement |
| `.../daisy-seed3.md:814` (exit-code table, stale row) | trigger is `<ELF> is older than <path>` | new first line of the stale failure, plus the two hashes; keep the rc=2 and "no probe-rs output whatsoever" points |
| `.../daisy-seed3.md:815` | "Provenance is asked before timestamps" | provenance is asked before the content test |
| `.../daisy-seed3.md:819-823` | rc=2 ambiguity, four cases | keep, and keep the standing advice to match the message rather than enumerate forever |
| `.../daisy-seed3.md:825-832` | justifies the remedy entirely in mtime terms ("moves nothing but the timestamp") | the remedy still forces a real relink, which is what refreshes the stamp; drop the mtime reasoning that no longer applies |
| `.../daisy-seed3.md:561-567` | closing clause: an in-place objcopy rewrite "is enough to defeat any check that asks whether the ELF is older than its sources" | keep the measurement, correct the conclusion: under a content test that rewrite is inert |
| `.../daisy-seed3.md:845-847` | "refuses if the ELF is behind the sources" | reword to match |
| `firmware/Makefile:208-212` | "so a newer one means the ELF on disk is not the one your sources describe" | content wording |
| `firmware/Makefile:238` | target doc: "Fail if any input to $(ELF) is newer than it" | content wording |
| `scripts/elf-provenance.sh:93-101` and its selftest cases at `:655-690` | the `/dev/null` guard exists because a refreshed mtime would "silently neuter firmware/Makefile's staleness test -- elf-check could never go red again" | the guard stays (do not mutate the artifact you are auditing) but its stated reason must move: an in-place byte-identical rewrite no longer defeats anything. Keep the assertions, fix the rationale |
| `scripts/gates.sh:211-215` | fmt stays `--check` so it cannot bump mtimes of elf-check inputs "until TASK-056 lands" | the `--check` rule stands on its own (a commit must not rewrite the tree it is committing); update the cross-reference and drop the expiry date. Line-number references to the Makefile in that comment have already drifted, refresh them |

Backlog ticket bodies are historical records; do not rewrite their measurement logs.

## Evidence to record in the notes

- AC #1: the false positive on the pre-change tree. Already reproduced while planning, on a clean
  tree: `make -C firmware elf-check FEATURES='seed3 log-defmt' NO_DEFAULT=1` -> rc 2, `... is older
  than src/bin/main.rs`, with `git status` clean; ELF mtime 1789358768 against `src/bin/main.rs`
  1789361743, i.e. 2975 s of pure metadata difference. Note also that plain `make elf-check` on this
  machine fails the provenance clause first, because ordering rule 1 leaves the RTT-only image at
  `release/main`; that masks the mtime bug unless you ask for the matching cfg set.
- AC #2: the selftest transcripts, red against HEAD and green after, plus the `added-input-detected`
  case as the proof that a real content change is still caught without rebuilding.
- AC #3: quote the new comment block.
- AC #4: the missing-stamp and empty-input-set transcripts.
- AC #5: `make -n build flash flash-all check` diffed byte-identically against
  `git show HEAD:firmware/Makefile`, and `bash scripts/gates.sh commit` green with the new gate's
  timing line, the way TASK-053 and TASK-067 recorded theirs. Add one live end-to-end: `make
  build-elf`, `touch src/bin/main.rs`, `make elf-check` -> 0, then `make -C firmware elf-check
  FEATURES='seed3 log-defmt' NO_DEFAULT=1` after a real edit -> 1. Restore whatever cfg set
  `release/main` held before you started, and record that you did.
- AC #6: the prose table, ticked row by row.

## Not in scope

- Dependency files, `cargo build --dry-run` (does not exist), or cargo's own freshness view.
- Any change to the `probe-*` recipes, to `build`, or to the gate ordering rules.
- Moving the digest into the image. Filed separately as an owner decision, because it contradicts
  AC #3 as written and extends the `.asp.prov` schema that TASK-062.01 owns. For the record, what
  the research measured about that option, so the decision does not have to be re-researched: the
  parser at `elf-provenance.sh:120-146` already ignores unknown keys by design and asserts it
  (selftest case `unknown-key-ignored`), so a `src_digest=` line is read-compatible with zero
  reader change; `.asp.prov` is non-allocated (`.section .asp.prov, "", %note`, LMA/VMA 0, objdump
  Type column blank) so growing the 57-byte blob by ~77 bytes adds nothing to any `.bin` and is
  invisible to all three rules of `check-image-load-addresses.sh`; no new dependency is needed,
  because build.rs can shell out to the same dual-spelled hasher. The real cost is elsewhere:
  `firmware/build.rs:33-36` opts out of cargo's default change detection, so putting the digest in
  the blob means emitting `rerun-if-changed` for all 52 inputs and keeping a Rust-side enumeration
  in lockstep with the shell expression here, and the blob would then be computed before the link
  rather than after it.
<!-- SECTION:PLAN:END -->

## Implementation Notes

Shipped as planned, with one design change made mid-flight because an experiment refuted a premise the
plan had taken on trust.

### The plan's step-2 shape was wrong, and the fix is smaller

The plan said: leave `build-elf`'s recipe text untouched, hash the ELF before and after cargo, refuse to
stamp when nothing linked. Measured against the real toolchain, that condition does not hold:
`cargo build --release` decides whether to link **from mtimes**, so a source whose bytes changed while
its mtime went backwards (restore from a backup, a sync tool - the class a `git checkout` does not
cover) prints "Finished in 0.29s", relinks nothing, and leaves the old code in `release/main`. Hashing
over that result certifies bytes nobody compiled - the silent pass this ticket must not trade its noisy
false alarm for. So `build-elf` now touches `$(MAIN_SRC)` itself when the stamp disagrees with the
sources, which leaves cargo no choice about linking. One line instead of double-hashing a 9 MB ELF, and
the false-green window closes at the cause rather than being detected downstream. Cost lands only where
a recompile was already owed; a warm forced relink of this crate measures 0.69 s.

### Two holes closed by enumeration during writing, both now asserted

- **A failed compile left a fresh stamp behind.** `define ... endef` expands into one shell command, so
  the plan's "separate recipe lines stop make at the first failure" reasoning did not apply. Worse,
  `want=$$( ... ) || exit 1` sets `$?` to 2, and a bare `$?` guard then stamps the *previous* digest
  over bytes nobody compiled - red turned into green. Fixed by `|| exit 1` on the compile itself, and
  asserted by `failed-compile-leaves-no-stamp`, which covers both halves: the compiler failing alone,
  and the digest computation failing with the compiler succeeding.
- **An empty input set hashed to the digest of nothing.** Reproduced against the shipped recipe: point
  `ELF_INPUTS` at a directory holding no matching files and `xargs` hashes its own stdin, producing a
  well-formed hex string no `[ -z ]` test rejects. Caught by `[ ! -s "$$listing" ]`, asserted by
  `empty-input-set-fails-loudly`.

### AC #1 - reproduced live on HEAD, clean tree

```
$ git status --porcelain   # empty
$ make -C firmware elf-check FEATURES='seed3 log-defmt' NO_DEFAULT=1
main.elf is older than src/bin/main.rs
...
make: *** [Makefile:278: elf-check] Error 1     rc = 2
```

Plain `make elf-check` on this machine dies at the provenance clause first, because ordering rule 1
leaves the RTT-only image at `release/main`; the matching cfg set is what exposes the mtime bug.

### AC #2 - green after, red on a real edit, both on the shipped recipe

Against the working Makefile, same command: rc 0. Live end-to-end after that, on the real tree: append
one line to `src/bin/main.rs`, do not rebuild, `make -C firmware elf-check FEATURES='seed3 log-defmt'
NO_DEFAULT=1` -> rc 2 naming `was not built from the sources on disk` and printing both digests; restore
the bytes -> rc 0. `added-input-detected` and `renamed-input-detected` cover the two changes a
timestamp cannot see at all.

One honest wrinkle recorded while verifying: after the gate tiers had run, the real tree's `elf-check`
went red against a clean worktree, because the stamp on disk had been written when the input set
included an experimental file since deleted. That is the check doing its job, not a defect: recomputing
the digest in a pristine `git archive HEAD` export gives the same value as the worktree
(`a6cba6dffd1ff...`), and one `make build-elf` brings the stamp back in step with the bytes.

### AC #4 - the two loud failures

```
no stamp at target/.../release/main.elf-inputs.sha256, so nothing here records which sources <ELF> came from
Force a real relink: touch src/bin/main.rs && make build-elf BINARY=main FEATURES='<...>' NO_DEFAULT=1

no ELF inputs found under: /tmp/t056-empty
```

Absence is treated as unknowable, never as fresh. The stamp lives inside `target/`, so `cargo clean`
deletes it and every fresh clone starts in this state; `check-elf-staleness.sh` carries its own fixture
and writes its own stamp, so the suite never depends on a cross-toolchain being present.

### AC #5 - dry-runs identical, all three tiers green

`make -n build flash flash-all check` against the working Makefile is byte-identical to the same four
targets at HEAD (diff of captured output: empty). Tiers, warm, on this machine: commit 11 gates 3-4 s
(new gate 0.9 s), push 20 gates 75.0 s, ci 21 gates 146.0 s. `release/main` was the console image
before this work and is the RTT-only image after, which is what the gate pair leaves there by ordering
rule 1 - not a state this ticket introduced; the stamp matches the sources either way.

### AC #3 and #6 - what the comments and prose now say

Quoted in `firmware/Makefile`: "Content equality is not proof the board runs this image. It never was,
and hashing does not change that. Only flashing is proof, which is why `elf-check` stays a check and
never a build." Plus the two other blind spots (raw-cargo relinks by the gates and `make build`'s own
objcopy-before-compile leaving the stamp talking about an earlier link, owned by TASK-069; and
`Cargo.lock` moving under a `master`-pinned dependency with no `--locked` anywhere).

Prose: `README.md` "refuse if any source is newer than it" -> hashes inputs against the recorded
digest; `docs/reference/daisy-seed3.md` probe intro (:416), the exit-code table's stale row rewritten
as a content mismatch, a new row for the no-stamp case, the rc=2 ambiguity paragraph grown to five
cases, the remedy paragraph rewritten (including why deleting the top-level ELF and rebuilding leaves
the same bytes hardlinked back from `target/.../release/deps/` - measured 2026-09-13, sha256 unchanged,
which under the new test goes green and under the old one could not be fixed by retrying), and
`check-image-load-addresses.sh`'s "behind the sources" line. `scripts/gates.sh`'s fmt-gate comment
cross-reference to TASK-056 updated: it names a decision that still stands, for a reason that has
changed. TASK-062's paragraph at `daisy-seed3.md:~788` is left alone - it describes the skip path it
removed, not the staleness test.

### One variable removed rather than left to lie

`ELF_INPUTS` already existed further down the file, with a comment saying "so a newer one means the ELF
on disk is not the one your sources describe". The plan added a second definition in the mechanism
block and stopped there; two identical plain assignments are harmless to make and fatal to a reader, so
the older one is gone and the surviving definition carries the input-set contract once. Re-verified
afterwards: `make -n build flash flash-all check` is still byte-identical to `6dce7d4` (25 lines), and
all ten cases stay green.

### Test placement, since the plan assumed otherwise

`firmware/tests/` does not exist and `firmware/` is excluded from the workspace (`[workspace]
members = ["crates/*"]`), so `cargo test` there builds host binaries against `no_std` sources. The
assertions live in `scripts/check-elf-staleness.sh` beside `check-doc-artifact-names.sh`, driving the
shipped Makefile through CLI overrides (`ELF=`, `ELF_INPUTS=`, `MAIN_SRC=`, `CARGO=true`, `PROV=true`).
The last form is load-bearing: make prioritises command-line assignments over file assignments, so
`make CARGO=true` works while `make --eval` does not - measured, and the reason `CARGO` and `MAIN_SRC`
are plain `=` in the Makefile. Six mutations were run to confirm each case bites the failure mode named
for it: drop the touch, revert `|| exit 1` to `$?`, drop the `-s` listing guard, prune too eagerly,
skip the stamp write, and remove the `-name '*.rs'` term from the find expression (that last one makes
`renamed-input-detected` go green, which is why the term belongs in the expression rather than in
`ELF_INPUTS`).

## Final Summary

`elf-check` asks two questions now, in the same order: does the ELF name the cfg set I am about to
decode with, and are the sources on disk the ones that built it. The second is a sha256 over per-file
digests of 52 inputs, compared against a stamp `build-elf` writes as the last thing it does. Bulk
metadata refreshes are inert; content changes are caught without rebuilding; a missing record is a
failure rather than a pass; and the one case the plan missed - cargo declining to link at all - is
prevented by forcing the relink instead of detecting it afterwards. Permanent coverage is ten cases in
`scripts/check-elf-staleness.sh`, wired fourth in the commit tier at 0.9 s. Prose in `README.md`,
`docs/reference/daisy-seed3.md`, `lefthook.yml`, `scripts/gates.sh`, `scripts/elf-provenance.sh` and
doc-001's gate matrix moved with the mechanism.

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-13 04:16
---
Path update from TASK-061.02: AC #5's "the host gates in ci.yml are green" now means `scripts/gates.sh commit` (2 s warm) or the whole thing via `scripts/gates.sh ci` (139 s warm), because ci.yml holds no list any more. Nothing about your fix changes. Worth recording alongside it: no gate writes to firmware sources - every fmt gate is `--check` precisely so a commit cannot bump mtimes and send the bench's elf-check red - and `make -C firmware elf-check` is red on today's clean tree for exactly the mtime reason this ticket owns.
---

created: 2026-09-13 07:02
---
Coordination note from planning TASK-059 (2026-09-13): TASK-059.02 adds scripts/check-image-load-addresses.sh and one gate line in scripts/gates.sh (push tier, after the doc-artifact gate). It reads the six release ELFs with the bare rust-objdump/rust-objcopy shims precisely so it never rebuilds, and hard-fails on a missing ELF rather than skipping on mtime - the same staleness trap this ticket is about. If you land first, tell .02's executor what the freshness contract became; the two checks should not each invent their own.
---

created: 2026-09-13 13:04
---
Coordination note from planning TASK-062 (2026-09-13). TASK-062 split into three leaves: .01 stamps a non-allocated .asp.prov note section into every firmware ELF from build.rs, .02 makes elf-check read it and fixes the remedy line, .03 gates what the two cross-builds leave in target/.

Overlap with this ticket is exactly one place: the body of elf-check and its printed failure lines. No hard dependency was added, because .02 fixes the remedy independently of the staleness test - measured again while planning, rm -f $(ELF) && make build-elf leaves the mtime unchanged (mtime 1789302036 against wall clock 1789302070 on a scratch crate), whereas cargo clean -p asperitas-firmware or deleting target/<triple>/release/.fingerprint/asperitas-firmware-* really relinks and refreshes it. So .02's remedy becomes the forced-relink form and its AC passes whether or not this ticket has landed.

Consequences for this plan, whichever order we land in. If .02 lands first: elf-check will have grown a provenance clause that calls scripts/elf-provenance.sh and a third named failure line; keep both, replace only the find -newer test with your content digest, and do not reintroduce an mtime comparison. Your AC #6 prose list and .02's overlap at docs/reference/daisy-seed3.md:806 exit-code row, the two-layer rc paragraph at :809-813, README.md:268-270 and the byte-scan paragraph at :555-558 - edit each once, after both mechanisms exist, and note that the rc=2 ambiguity grows to four cases (no probe, stale content, cfg mismatch, no ELF), so the paragraph should say match on the message rather than enumerate forever.

Also worth knowing before you write the sidecar: the sidecar lives beside release/main, which the raw-cargo gates rewrite without going through make, so it can only ever describe the last make build-elf, not the bytes the name currently holds. That is fine for your purpose (you hash sources, not cfg sets) but it is why TASK-062.01 puts cfg provenance inside the image instead, and why your stamp should stay clearly labelled as a source-content stamp so nobody reads it as a description of the artifact.
---

created: 2026-09-14 05:51
---
Planning done 2026-09-14. The plan above replaces the draft that was here before it; the draft's design survives, corrected in three places. (1) TASK-062.02 has since landed, so elf-check now asks provenance first and prints a fourth failure line; the new test must stub that clause with PROV=true rather than fight it, and the prose list grew to include daisy-seed3.md:815's "Provenance is asked before timestamps" and :819-823's four-case rc ambiguity. (2) The pipeline the draft prescribed, find | sort -z | xargs shasum | shasum, passes vacuously when the input set enumerates nothing: xargs runs sha256sum on stdin with no arguments and the digest of empty is a non-empty string, so no [ -z ] test can catch it. Measured. The plan uses a mktemp listing plus [ ! -s ] instead, which fails loudly; command substitution cannot hold NUL bytes, so -print0 has to land in a file. (3) AC #1/#2 become a permanent commit-tier assertion in scripts/check-elf-staleness.sh, not a hand-run transcript, because driving the real Makefile through CLI overrides (ELF=, ELF_INPUTS=, PROV=) works today with no Makefile change and no build: verified against HEAD with a fixture tree and an ELF dated 2020, which reproduced 'is older than .../crates/foo/Cargo.toml' at rc 2 while touching no tracked file.
---

created: 2026-09-14 05:51
---
Reproduced live while planning, on a clean tree: make -C firmware elf-check FEATURES='seed3 log-defmt' NO_DEFAULT=1 exits 2 printing '... is older than src/bin/main.rs', with git status clean and mtimes 1789358768 for the ELF against 1789361743 for src/bin/main.rs. Note for whoever executes: plain 'make elf-check' on this machine never reaches the staleness clause, because gates ordering rule 1 leaves the RTT-only image at release/main and the provenance clause fires first. Ask for the matching cfg set or you will chase the wrong bug.
---

created: 2026-09-14 05:51
---
Filed TASK-069 (@human, blocked on this ticket) for the one decision that is not the executor's: whether the digest belongs inside .asp.prov instead of beside the ELF. It is not a sub-task on purpose, so it does not hold this ticket open. TASK-056 as written is complete and worth shipping without it: the sidecar removes the false red that fires on every checkout, and its residual hole only opens if a source is edited AND the gates run AND nobody runs make build-elf in between. Also recorded there, because it is true and small: make build relinks release/$(BINARY) without writing a stamp, since cargo objcopy compiles first, so the same hole is reachable from inside make, not only from the raw-cargo gates.
---
<!-- COMMENTS:END -->
