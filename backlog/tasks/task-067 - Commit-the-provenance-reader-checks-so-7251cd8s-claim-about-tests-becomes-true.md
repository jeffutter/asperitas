---
id: TASK-067
title: >-
  Commit the provenance-reader checks so 7251cd8's claim about tests becomes
  true
status: Done
assignee:
  - '@agent'
created_date: '2026-09-13 14:51'
updated_date: '2026-09-13 21:40'
labels:
  - planned
dependencies: []
references:
  - scripts/elf-provenance.sh
  - 'scripts/gates.sh:188'
  - firmware/build.rs
  - 'crates/asperitas-logging/examples/dump_reassemble.rs:523-640'
priority: low
ordinal: 117800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-062.02's commit message (7251cd8, `TASK-062.02: teach elf-check which cfg set an ELF was built for`)
says the `/dev/null` guard and the exit-code paths are "asserted in the script's own tests, which also
cover console agreement, an injected third cfg set refused while naming both sides, the
`--no-default-features --features seed3` pair deriving default=0/features=seed3 and refusing the
console ELF, and all four exit-code paths."

No such file exists. `grep -rn elf-provenance scripts/` finds only the script itself, and no gate runs
it beyond the one push-tier line TASK-062.03 added. The behaviours are real - TASK-062.02's
implementation notes record every one of them measured by hand on 2026-09-13 - but they live in a
ticket's notes rather than in something that runs, so the next edit to build.rs's blob encoding or to
`normalize_features` has nothing between it and a silent mislabel at the bench.

Make the claim true by committing the checks. Two shapes are open and the planner should pick one with
its eyes open: reuse the two ELFs the cross-build pair already produced (free, but ties the selftest's
position to sitting after the pair, like the provenance gate does) or build synthetic blob fixtures
(independent of build order, but then it exercises the parser and not the stamp). The `dump_reassemble
--selftest` gate is the precedent to imitate: one binary, one flag, one gate line, ~0.7 s.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `scripts/elf-provenance.sh --selftest` (or an equivalent sibling invoked the same way) prints one prefixed line per case, exits 0 when all pass, 1 on any failure while reporting every failure in one run, and 2 when it cannot run at all. Follow scripts/check-doc-artifact-names.sh's header conventions for that exit-code contract.
- [x] #2 Cases asserted rather than printed for a human to read: console agreement; RTT-only agreement; a third cfg set refused while naming both sides; `--no-default-features --features seed3` deriving default=0 features=seed3; a feature outside the default closure (e.g. stim_ess) present in the derived expectation; missing ELF -> 2; ELF with no .asp.prov section -> 2; blob carrying a foreign format tag -> 2; reading a blob leaves the ELF's mtime AND sha256 unchanged, which is what the trailing /dev/null argument buys.
- [x] #3 It builds nothing and stays cheap. Choose either reuse of the artifacts the two cross-build gates produce, placed after them, or fixture-based parsing placed anywhere, then record the measured warm cost and the reason for the position. Budget: under 1 s warm, no cargo build invocation, no reaching into target/<triple>/release/.fingerprint.
- [x] #4 Registered as exactly one new gate line in scripts/gates.sh; doc-001's matrix block regenerated from `scripts/gates.sh --list` rather than hand-edited, with its counts and measured cost lines updated in the same commit.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Shape: synthetic ELF fixtures generated inside the script, not reuse of build residue

AC #3 leaves two options open. Reuse is closed off by measurement, not taste:

- After the cross-build pair, `firmware/target/thumbv7em-none-eabihf/release/main` holds ONLY the
  RTT-only image (gates.sh ordering rule 1). Asserting "console agreement" against real artifacts
  would need another build, which AC #3 forbids. Fixtures cover both agreements; reuse covers one.
- Reaching into `release/deps/main-*` was tried: 48 non-`.d` candidates there, four sampled, every
  one reporting "has no .asp.prov section" (they predate TASK-062.01's stamp), and picking the right
  one needs `.fingerprint`, which AC #3 names as forbidden.
- Producer fidelity is already gated elsewhere: the existing push-tier
  `=== firmware ELF cfg provenance ===` reads the REAL stamped ELF. Fixtures cover the reader, that
  gate covers the stamp; together the two halves are asserted. Neither alone is.

Fixtures are generated at runtime into a `mktemp -d` (cleaned on exit) rather than committed as
binary blobs: seven variants of a ~330-byte file carry no information that the generator does not
carry more legibly, they would need a regeneration story (the goldens precedent has
`UPDATE_GOLDENS=1`; nothing here earns that), and keeping the bytes next to the parser means a
format change moves both together.

### Verified feasible, measured boardless on this tree (LLVM 22.1.6-rust-1.97.1-stable)

A hand-emitted ELF64 - `ET_EXEC`, `EM_AARCH64`, zero program headers, three section headers
(SHT_NULL, `.asp.prov` as SHT_NOTE with `sh_flags = 0` i.e. non-allocated, `.shstrtab`) - is 336
bytes, is listed by `rust-objdump -h` as `.asp.prov size 00000036 VMA 0`, and is read by
`scripts/elf-provenance.sh` with no change to the script:

| fixture blob | invocation | observed |
|---|---|---|
| `default=1 features=log_usb,seed3` | `check <fx> "seed3" ""` | rc 0 |
| `default=0 features=log_defmt,seed3` | `check <fx> "seed3 log-defmt" 1` | rc 0 |
| `default=0 features=seed3` | `check <fx> "seed3" 1` | rc 0 |
| `default=1 features=log_usb,seed3,stim_ess` | `check <fx> "seed3 stim-ess" ""` | rc 0 |
| same | `check <fx> "seed3" ""` | rc 1, stderr names `log_usb,seed3,stim_ess` vs `log_usb,seed3` |
| tag `asp-prov2` | `show <fx>` | rc 2, "starts with 'asp-prov2', not 'asp-prov1'" |
| blob without `features=` | `show <fx>` | rc 2, "truncated or malformed" |
| section removed (2-section layout) | `show <fx>` | rc 2, both "predates the stamp" lines |
| nonexistent path | `check <path> ...` | rc 2, "no ELF at ..." |
| non-object file | `show <file>` | rc 2, "could not read ... not recognized as a valid object file" |

Byte emission needs no python, no xxd (macOS-only), no `od` decode: `printf '%b'` with `\xHH`
escapes reproduces every byte, verified working on bash 3.2.57 and bash 5. Two traps found the hard
way while prototyping, both worth a comment in the generator: bash CANNOT hold a NUL in a variable
(`$'\0...'` truncates the string, which silently produced an empty `.shstrtab` and an
"SHT_STRTAB ... is empty" rejection), and every width helper must emit BYTES, not hex text (a
helper returning `le32` output unpadded gave "invalid e_shentsize in ELF header: 12336" - ASCII
"00").

The mtime guard premise got sharper, not softer. On a scratch copy of the 336-byte fixture, the
UNGUARDED form `rust-objcopy --dump-section .asp.prov=out FIXTURE` (no trailing `/dev/null`) exits
0, leaves sha256 unchanged, and DOES move the mtime. So at fixture size only the mtime half of
AC #2's guard case can fire; the sha half fires on big images (TASK-062.02's notes and a repeat
measurement saw the real ~9.5 MB ELF shrink to 9,429,948 bytes). Assert BOTH, and assert the
tripwire is not vacuous by running the unguarded form on a throwaway copy and requiring the mtime
to move. Compare mtimes with `touch -t 198001010000` plus `find f -newer marker`, never `stat`:
nix coreutils `stat` shadows BSD stat, so `stat -f %m` fails outright here (`invalid option -- '%'`).

Warm costs measured while planning: defaults-on `check` 0.157 s (rust-objcopy plus
`cargo metadata`), `NO_DEFAULT=1` `check` 0.072 s, `show` 0.08 s. Twelve naive re-invocations per
case therefore land at ~1 s before metadata - over AC #3's budget. In-process function calls are
what buy the difference; see the cost rules below.

## Implementation

### 1. Add a `--selftest` mode to scripts/elf-provenance.sh

Same file, one flag, dispatched from the existing `case $mode in` at scripts/elf-provenance.sh:177
BEFORE the `show` / `check` arms - not a sourced sibling, so no "run only as main" guard is needed
and the selftest calls `normalize_features` (line 62), `read_blob` (74), `parse_blob` (109) and
`expected_from_metadata` (147) directly. This mirrors `dump_reassemble --selftest`
(crates/asperitas-logging/examples/dump_reassemble.rs:523-640), the repo's one precedent.

Output conventions copied from that precedent verbatim in shape, on stderr (stdout stays reserved
for `show`'s payload):
`elf-provenance: selftest <kebab-case-name> ok` per case,
`elf-provenance: selftest <name> FAILED: <reason>` per failure, no short-circuiting, then exactly
one summary line `elf-provenance: selftest passed (N cases)` or
`elf-provenance: selftest FAILED (M of N cases)`. Exit 0 all pass, 1 any failure, 2 cannot run
(missing `rust-objcopy`, `mktemp` failing) - the last sentence of the existing header contract
(lines 29-32) extended to the new mode, following scripts/check-doc-artifact-names.sh:34-35's
wording: "A check that cannot run never reports success". Update `usage()` at line 51 and the
header's exit-code paragraph in the same edit.

Cases accumulate into a failures counter; negative cases must run in a subshell because `die()`
exits the process. A subshell fork is ~2 ms against ~70 ms for a fresh interpreter plus objcopy,
which is why the negative paths are cheap.

### 2. Fixture generator

One function, emitting into the temp dir, parameterized by blob text plus a "no section" flag
(that flag drops the third section header and sets `e_shnum = 2` rather than leaving a dangling
`sh_name`). Build the seven blobs above as literals. Keep the assembler-visible constants adjacent
to the ones the reader already holds (`PROG` line 40, `TAG` line 41) and cite firmware/build.rs:58-60
as the producer of the format. Note in a comment that duplicating the format here is deliberate: a
fixture derived from build.rs at test time could not catch build.rs changing the format, and the
real-ELF gate is what covers producer drift.

### 3. Cases to assert (every AC #2 item, plus the exit-code contract)

Agreement and refusal, run as real child invocations of the script so the codes reaching the shell
are the ones being graded: console agreement (0), RTT-only agreement (0),
`--no-default-features --features seed3` deriving `default=0 features=seed3` (0), third cfg set
refused while naming both sides (1), `stim_ess` accepted when asked and refused when not (0 and 1).

Parser and read-path cases, in-process except where the read itself is the subject: foreign tag
(2), missing `default=` (2), missing `features=` (2), unknown key ignored rather than refused
(forward compatibility, parse succeeds), no `.asp.prov` section (2, both lines), no ELF at the path
(2), non-object input (2), bad arity and unknown mode (2, usage printed). Plus
`normalize_features` table cases (uppercase, `-` to `_`, comma/space mixing, dedupe, the implicit
`default` token dropped, empty input staying empty), and one case pinning
`expected_from_metadata()` to `log_usb` so the reader is tied to the live `[features]` table in
firmware/Cargo.toml rather than to a remembered string.

Guard cases: guarded read leaves sha256 AND mtime unchanged; unguarded objcopy on a throwaway copy
moves the mtime (proving the first assertion can fail).

### 4. Cost rules - write these into the gate comment

C1: call `expected_from_metadata()` at most once and reuse the result. C2: no more than five child
invocations of the whole script - one per distinct exit code plus the guard cases - everything else
in-process. C3: read each distinct fixture once via `read_blob` and run parse assertions on the
cached blob string. C4: never invoke `cargo build`, `cargo clippy`, `cargo objcopy`, `cargo objdump`
or `make`, and never name a path under `firmware/target/`; those either rebuild or re-point the very
artifact the provenance gate audits. Target <= 0.5 s warm, hard ceiling under 1 s; record the number
the gate banner prints.

### 5. Register exactly one gate line

Position: `gate commit`, declared immediately after `=== docs artifact names ===`
(scripts/gates.sh:188) and before `=== cargo fmt ===` (:190), i.e. third gate overall. Reasons, all
of which belong in the comment above it: it builds nothing and needs no artifact, so cheapest-first
applies and there is no reason to pay clippy's price first; commit placement is where the autonomous
loop is actually gated; and placing it near the push-tier provenance gate "for symmetry" would be
misleading, since the two share no state. Ordering rule 1 is untouched because no firmware gets
compiled. Outside `nix develop .#default` there is no `rust-objcopy`, but gates.sh already exits 3
before any gate for the missing thumbv7em std, so the selftest's own exit-2 message is a direct-run
affordance, not a hook failure. Do not add shellcheck, bats or python3 to flake.nix - the devShell
does not declare them and this ticket does not need them.

### 6. Docs bookkeeping in the same commit (AC #4)

- Regenerate the fenced block at `backlog/docs/doc-001 - Asperitas-Project-Plan.md` lines 253-276
  from `scripts/gates.sh --list`; verify mechanically rather than by eye:
  `diff <(scripts/gates.sh --list) <(sed -n '254,275p' "backlog/docs/doc-001 - Asperitas-Project-Plan.md")`
  must print nothing. Counts move from `commit 9, push 17, ci 18` to `commit 10, push 18, ci 19`.
- Update the cost paragraph at :278-280 from one fresh paired measurement, and the two sentences
  that restate the same figures: doc-001:297-300 ("first ~2 s (commit) or ~73 s (push)"),
  scripts/gates.sh:20 and lefthook.yml:12. Leaving those stale contradicts gates.sh's own claim of
  one figure per gate. Prose only, no behaviour.
- If the new gate's position makes the header's ordering narrative read wrong, fix the narrative,
  not the order.

### 7. Verification

1. `bash scripts/elf-provenance.sh --selftest; echo rc=$?` - every case prints its ok line, rc 0.
2. Prove the suite can fail: delete the trailing `/dev/null` from `read_blob` (line ~91), re-run,
   require the guard cases to go red and rc 1, restore. Then break `normalize_features` the same way
   and require the normalization cases to go red. A suite that passes both ways is not a suite.
3. `scripts/gates.sh --dry-run commit | tail` shows the new line; the diff command in step 6 is clean.
4. Timings inside `nix develop .#default --command bash scripts/gates.sh <tier>`: commit (~2 s today,
   expect ~2.5 s), push (~73 s), ci (~140 s). Take the new gate's own `--- N.NNs` line from the run,
   and the tier totals from the same three runs, and quote those in doc-001 rather than extrapolating.
5. `bash scripts/elf-provenance.sh show firmware/target/thumbv7em-none-eabihf/release/main` must
   still print `default=0 features=log_defmt,seed3 defmt_log=` - the selftest must not disturb the
   real artifact (it has no business opening that path at all; see C4).

## Non-goals

No bats/shellcheck/python harness and no flake dependency. No committed binary fixtures. No change to
firmware/build.rs, firmware/Makefile, ci.yml or lefthook.yml. No mechanical assertion that doc-001's
block matches `--list` (that gap is its own ticket's business, and TASK-062 recorded the same wish).
No assertion tying `$TAG` to build.rs's `const TAG` by grep: brittle, and producer drift is already
loud (a bumped producer makes every real ELF refuse with "starts with ...").

## Risks

- **Toolchain strictness about hand-built ELFs.** Accepted on LLVM 22.1.6-rust-1.97.1 here; the CI
  runner gets the same objcopy because flake.nix pins the toolchain, so the two cannot disagree by
  construction. If a future objcopy rejects the layout it rejects it loudly through the exit-2 path,
  not as a silent pass.
- **Cost creep.** The budget survives only if C1-C4 hold; that is why they are required as comments
  in the gate definition rather than as advice here.
- **Self-consistency of a synthetic fixture.** A generator that emits something build.rs would never
  emit would still pass. Mitigated by reading every fixture through the real `read_blob` (so
  objcopy, not the generator, decides whether it is an ELF) and by the existing real-ELF gate.

## Observed while planning, worth repeating to whoever executes this

An unguarded `rust-objcopy --dump-section ... <ELF>` issued during research rewrote this tree's
`firmware/target/thumbv7em-none-eabihf/release/main` in place: 9,496,868 -> 9,429,948 bytes, mtime
refreshed, blob and symbols intact so `show` still reports the RTT-only set and the provenance gate
still passes. That is the damage AC #2's guard case exists to make loud, reproduced inside one
planning session by someone following a reasonable-looking note that the dummy output argument is
unnecessary. The artifact is regenerable and gitignored; restore it with the documented remedy from
firmware/Makefile:236 if a bench session is next: `cd firmware && touch src/bin/main.rs &&
make build-elf FEATURES='seed3 log-defmt' NO_DEFAULT=1`.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Origin: TASK-062's umbrella run, 2026-09-13. The measurements this ticket turns into code are written out in TASK-062.02's implementation notes; do not rediscover them.

## Shape: fixtures, as planned, and what executing it added

Synthetic ELF images generated inside the script, dispatched as `--selftest` ahead of the `show` /
`check` arms, registered as one `gate commit` line third in `scripts/gates.sh`. Reuse of the cross-build
pair stayed closed: ordering rule 1 leaves only the RTT-only image at `release/main`, so console
agreement would have cost a third firmware build. 19 cases, all green.

Four changes to the production half made the suite possible, each one small and each one load-bearing:

- `compare_request <elf> <blob> <FEATURES> <NO_DEFAULT>` came out of the `check` arm. A case can now
  grade a comparison without paying an `objcopy` for the privilege, which is where most of the budget
  goes.
- `expected_from_metadata()` writes the global `EXPECTED_CLOSURE` instead of printing. Printing looked
  fine until it was memoized through a command substitution, which cannot work - see C1 in the script.
- `fail()` EXITS rather than returns. Each case body runs inside `run_case`'s command substitution, so
  exiting ends exactly one case; returning let the last assertion in the body decide the status. Caught
  by mutating `normalize_features` to stop dropping the implicit `default` token: five cases genuinely
  disagreed and the suite still printed nineteen passing. That mutation is now the reason the comment
  above `fail()` exists.
- Dead state went with it: `parse_blob` declared `BLOB_TAG=""` and never filled it. The tag is validated
  and deliberately not returned (one format version says nothing a caller can use), so the variable is
  gone rather than populated.

## Measured cost, and where the plan's target went

| | warm |
|---|---|
| this gate (`--- N.NNs` in the gate banner) | **0.89 s** |
| commit tier | 3 s (was 2 s) |
| push tier | 76 s (was 73 s) |
| ci tier | 141 s (was 140 s) |

All three tiers carry the gate, so all three moved by about its own cost. Inside the budget AC #3 sets
(under 1 s, no `cargo build`, nothing under `firmware/target/`); the plan's <= 0.5 s target is
unreachable while seven `rust-objcopy` invocations (~50 ms each) and two `cargo metadata` (~85 ms) stay
load-bearing - they are the subjects of cases, not scaffolding. Three changes bought the difference from
a first draft of 1.3 s: deriving the closure once in `run_selftest` instead of lazily per case
(-0.35 s, the memo cannot survive a case's subshell), assembling each fixture as hex text in one
variable and converting once instead of 25 command substitutions per image (-0.08 s), and reading blobs
into files or strings exactly once per process.

Position: third gate, immediately after `=== docs artifact names ===`, because it compiles nothing and
needs no artifact. Recorded in the comment above the gate line, along with why sitting beside the
push-tier provenance gate "for symmetry" would misrepresent both - one grades the reader, the other the
stamp.

## Proof the suite can fail, run before committing

| mutation | result |
|---|---|
| trailing `/dev/null` dropped from `read_blob` | `read-leaves-the-elf-untouched` red ("reading the ELF moved its mtime"), rc 1 |
| `normalize_features` stops dropping `default` | 7 of 19 red |
| `normalize_features` stops folding `-` to `_` | 9 of 19 red, table case names `[LOG-USB]: want [log_usb], got [log-usb]` |

Every failure is reported in the one run rather than stopping at the first, per AC #1.

## Other verification

- Matrix block regenerated from `scripts/gates.sh --list` and diffed mechanically against the file
  (clean); counts now commit 10, push 18, ci 19. Cost lines updated in doc-001 and in both places that
  restate them (`scripts/gates.sh:20`, `lefthook.yml:12`).
- `scripts/elf-provenance.sh show firmware/target/thumbv7em-none-eabihf/release/main` still prints
  `default=0 features=log_defmt,seed3 defmt_log=` after a full ci-tier run - the suite never opens that
  path (rule C4).
- Passes on bash 5.3.15 (the devShell, where gates.sh runs it) and on bash 3.2.57 (macOS system shell),
  same toolchain.

## Left open, deliberately

- Nothing checks mechanically that doc-001's matrix matches `--list`; the ticket names that as its own
  ticket's business. Verified by hand-diff this time, which is exactly the gap that stays.
- `=== firmware ELF cfg provenance ===` remains the only thing asserting producer fidelity. If build.rs
  changes the blob encoding, these fixtures keep passing and that gate goes red - by design, stated in
  the selftest's header so nobody reads the fixtures as end-to-end coverage.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-14 00:24
---
Planning left a working prototype of the fixture generator at /tmp/fx/mkelf2.sh (throwaway, not production quality, /tmp may be cleared). It emits the 336-byte ELF64 described in the plan and reproduces every exit-code row of the table; the two traps named there (NUL cannot live in a bash variable, width helpers must emit bytes not hex text) are the bugs it took to get it reading.
---
<!-- COMMENTS:END -->
