---
id: TASK-056
title: >-
  Make elf-check's staleness test content-based, so a bulk mtime refresh cannot
  fail it spuriously
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-12 09:25'
updated_date: '2026-09-12 09:26'
labels: []
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
- [ ] #1 The false positive is reproduced as a script or host check that fails on today's Makefile: after a successful `make build-elf`, touch an input without changing its bytes and `make elf-check` exits 1.
- [ ] #2 With the new mechanism the same sequence exits 0, and `elf-check` still exits 1 when an input's bytes actually differ from the ones that built the ELF (prove by editing a source and NOT rebuilding).
- [ ] #3 The whole mechanism lives in one place in `firmware/Makefile`, and its comment says out loud what it cannot see: content equality is not proof the board runs that image.
- [ ] #4 A missing stamp file fails with an actionable message rather than passing silently.
- [ ] #5 `make -n build flash flash-all check` stays byte-identical to HEAD, and the host gates in ci.yml are green.
- [ ] #6 The two prose claims that currently say "newer than that ELF" (`docs/reference/daisy-seed3.md` probe section and `README.md`), plus the stale case row of the exit-code table, are reworded to match the new mechanism.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Mechanism

Reuse the input set `ELF_INPUTS` already defines (`src build.rs memory.x Cargo.toml Cargo.lock
../crates`, pruned at any `target`) so the two definitions cannot drift:

1. In `build-elf`, after cargo succeeds, compute the digest over that set in a deterministic order
   and write it to `$(dir $(ELF))$(BINARY).elf-inputs.sha256`:
   `find $(ELF_INPUTS) \( -name target -prune \) -o -type f \( -name '*.rs' -o -name '*.x'    -o -name 'Cargo.toml' -o -name 'Cargo.lock' \) -print0 | sort -z | xargs -0 shasum -a 256    | shasum -a 256`. Sort by *path only* so file mtimes never enter the digest. Record the digest in
   a variable, then write the stamp — the stamp must be the last thing touched, mirroring the rule
   this ticket exists to enforce.
2. `elf-check` recomputes the same digest and compares against the stamp. No `-newer`, no mtime
   anywhere. Missing stamp -> fail saying the ELF predates the stamp mechanism (or was removed):
   `rm -f $(ELF) && make build-elf`.
3. Keep the existing three-line failure message shape. Drop the "If only timestamps moved" line,
   which becomes unreachable advice once timestamps stop mattering, and replace it with the stamp
   mismatch reason.

## Guard

`build` (the DFU objcopy target) must stay untouched: `make -n build flash flash-all check` has to
come out byte-identical, so the stamp step belongs in `build-elf` only. Verify with a diff against
`git show HEAD:firmware/Makefile` output, the way TASK-053 recorded it.

## Prose that moves

- `docs/reference/daisy-seed3.md`, the paragraph starting "`probe-log` compiles nothing": "any source
  is newer than that ELF" becomes a content statement.
- `README.md`, the paragraph naming `probe-log` and `probe-rtt-list`: same rewording.
- Same doc's exit-code table, the row for an ELF behind the sources: the trigger changes from mtime
  to digest mismatch. Keep the two-layer rc point intact (probe-rs never runs, `make` exits 2).

## Not in scope

Do not add dependency-file support or ask cargo for its freshness view (`cargo build --dry-run` does
not exist; `--message-format json` recompilation reports are a different ticket's problem if anyone
wants them). Do not touch the `probe-*` recipes.
<!-- SECTION:PLAN:END -->
