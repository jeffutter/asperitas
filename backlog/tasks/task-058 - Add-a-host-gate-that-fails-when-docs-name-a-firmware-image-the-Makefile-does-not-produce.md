---
id: TASK-058
title: >-
  Add a host gate that fails when docs name a firmware image the Makefile does
  not produce
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-12 21:24'
updated_date: '2026-09-12 22:34'
labels:
  - planned
dependencies:
  - TASK-057
references:
  - 'firmware/Makefile:101-109'
  - 130-132
  - .github/workflows/ci.yml
  - lefthook.yml
priority: medium
type: chore
ordinal: 90800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-057 fixed prose that named `firmware.bin`, an image `firmware/Makefile` stopped producing when each target
started writing `$(BINARY).bin` (`41f9cae`, 2026-09-12). The same dead name was live in six other places on the
same day (`README.md:92,93,242`, `firmware/Cargo.toml:57,60,68`) and nothing could see it: CI runs cargo only
and lefthook likewise - there is no markdown lint, no docs check, no `make` invocation anywhere in either gate
(verified 2026-09-12 against `.github/workflows/ci.yml`, `lefthook.yml`, `flake.nix`). Prose fixes decay; this
ticket makes the class self-policing with two rules derived from the build rather than from a hand-kept list.

R1 - every `*.bin` token in reader-facing docs (`README.md`, `docs/**/*.md`) must be either an image the Makefile
really produces or an explicit allowlist entry. Legal names come from `make -n -C firmware BINARY=<b> build flash`
for each target in `firmware/src/bin/*.rs` (six today: blinky, ledtest, main, panictest, podtest, rig); the
allowlist needs `capture.bin`, which is a console log capture named at `README.md:214-216` and
`docs/reference/daisy-seed3.md:799`, not a firmware image.

R2 - no `cargo objcopy` invocation appears in those docs at all. The recipe lives only in
`firmware/Makefile:101-109`. A second copy in prose is exactly what lost the `--only-section` keep-list in
TASK-057, and transcription errors there are silent: measured 2026-09-12, `llvm-objcopy -O binary
--only-section=.no_such_section elf out.bin` exits **0** and writes a **0-byte file**, no warning. A doc that
quotes the recipe is a doc that can quietly become a truncated image.

The extraction already works (prototyped 2026-09-12, sub-second for all six binaries, no GNU-only tools):

    rules=$(make -n -C firmware BINARY=$b build flash | tr '\n' ' ')
    built=$(printf '%s' "$rules"   | grep -oE -- "-O[[:space:]]+binary[[:space:]]+[a-zA-Z0-9_./-]+" | head -1 | awk '{print $NF}')
    flashed=$(printf '%s' "$rules" | grep -oE -- "-D[[:space:]]+[a-zA-Z0-9_./-]+\.bin"              | head -1 | awk '{print $NF}')

It reported `-O binary X.bin` and `-D X.bin` agreeing for all six targets, i.e. the Makefile is self-consistent
and the docs were the outlier. Reuse it; do not rewrite the parsing.

Two hazards for whoever implements this. (1) `-n` is load-bearing: the default goal is `all: build`
(`firmware/Makefile:82`), so a dry-run flag that goes missing turns a sub-second docs lint into a real firmware
cross-compile inside a pre-commit hook. (2) Where it lives is a real choice: `scripts/` does not exist yet, the
one tracked shell script (`backlog/unblocked-todo.sh`) opens with a war story about two copies of the same rules
drifting, and the existing self-check precedent (`dump_reassemble --selftest`, `crates/asperitas-logging/examples/
dump_reassemble.rs`) is Rust wired into `cargo test --workspace`. A shell script called from lefthook plus one new
CI step is the recommended shape because the check must run `make`; shelling out to make from a unit test in a
host crate is worse cohesion, not better. Slots: first command in `lefthook.yml`'s `pre-commit.commands` (:6-8)
mirrored in `pre-push` (:25-63), and inside ci.yml's single `nix develop .#default --command bash -c` block
between the `dump_reassemble --selftest` line (:63) and the `=== firmware cross-compile ===` header (:65).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A check exists that derives legal firmware image names from `make -n -C firmware BINARY=<b> build flash` across every target in `firmware/src/bin/*.rs`, and fails with a `file:line` diagnostic naming any other `*.bin` token found in README.md or docs/**; `capture.bin` is allowlisted with a comment saying why.
The same check fails on any `cargo objcopy` line in README.md or docs/**, quoting the rule that the recipe lives only in `firmware/Makefile`.
Both rules are wired into `lefthook.yml` pre-commit (and mirrored in pre-push) and into the single CI job step, and the ticket notes carry the pasted red output of one deliberately planted violation per rule, plus the green run after removing it.
The check never executes a build: every `make` invocation carries `-n`, and the notes show it touching nothing under `firmware/target/` (record the directory listing or mtimes before and after) and finishing in seconds.
Host gates green in `nix develop .#default`: `cargo fmt --all --check`, the four clippy invocations, both `cargo doc` variants with `RUSTDOCFLAGS=-D warnings`, `cargo test --workspace`, `cargo run -p asperitas-logging --example dump_reassemble -- --selftest`, and both firmware cross-compiles from `ci.yml:66,72`.

- [x] #2 The same check fails on any `cargo objcopy` line in README.md or docs/**, quoting the rule that the recipe lives only in `firmware/Makefile`.
- [x] #3 Both rules are wired into `lefthook.yml` pre-commit (and mirrored in pre-push) and into the single CI job step, and the ticket notes carry the pasted red output of one deliberately planted violation per rule, plus the green run after removing it.
- [x] #4 The check never executes a build: every `make` invocation carries `-n`, and the notes show it touching nothing under `firmware/target/` (record the directory listing or mtimes before and after) and finishing in seconds.
- [x] #5 Host gates green in `nix develop .#default`: `cargo fmt --all --check`, the four clippy invocations, both `cargo doc` variants with `RUSTDOCFLAGS=-D warnings`, `cargo test --workspace`, `cargo run -p asperitas-logging --example dump_reassemble -- --selftest`, and both firmware cross-compiles from `ci.yml:66,72`.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Shape

One new script, two wiring edits (lefthook.yml, ci.yml). No firmware logic, no board, no ears: every criterion is
satisfiable by running the check and pasting output. Assignee stays `@agent`, nothing splits to `@human`.

Depends on TASK-057 because this gate must not land while `README.md:92,93,242` still say `firmware.bin` - it would
be red on its own first run, and a gate that arrives red gets disabled rather than fixed.

## Step 1 - `scripts/check-doc-artifact-names.sh`

Bash, ~35 lines, POSIX-ish tools only (`tr`, `grep -oE`, `awk`, `sed`) - the repo has no Python and CI's shell is
whatever `nix develop .#default` provides (GNU make/grep/sed/coreutils, cargo, dfu-util, probe-rs-tools, lefthook,
yq-go, jq; see `flake.nix:38-63`). Run from repo root; resolve paths relative to the script's own directory so it
works from a hook and from CI alike.

Three passes, all failures accumulated and printed as `file:line: token` before exiting 1, so one run reports every
offence:

1. **Legal names.** For each `f in firmware/src/bin/*.rs`, take `b = basename f .rs`, then:

       rules=$(make -n -C firmware BINARY="$b" build flash | tr '\n' ' ')
       built=$(printf '%s' "$rules"   | grep -oE -- "-O[[:space:]]+binary[[:space:]]+[a-zA-Z0-9_./-]+" | head -1 | awk '{print $NF}')
       flashed=$(printf '%s' "$rules" | grep -oE -- "-D[[:space:]]+[a-zA-Z0-9_./-]+\.bin"              | head -1 | awk '{print $NF}')

   Assert `[ -n "$built" ] && [ "$built" = "$flashed" ]` - if the rule that builds an image and the rule that flashes
   it disagree, that is a worse bug than anything the docs could say, and the script should say so and exit 1. This
   turns the check into a Makefile self-consistency test for free, which is the part worth keeping long-term.
   Verified working 2026-09-12 against all six targets (`blinky ledtest main panictest podtest rig`), sub-second total.
2. **Doc tokens.** Scan `README.md` and `docs/**/*.md` for `\b[A-Za-z0-9_-]+\.bin\b`; fail on any token that is not
   in the legal set or the allowlist. Allowlist starts as exactly one entry, `capture.bin`, with a comment naming
   `README.md:214-216` and `daisy-seed3.md:799` as the console-capture uses that motivated it. Do not context-filter
   ("only near `dfu-util`") - that is how a stale name hides.
3. **No hand-written objcopy.** Fail on any line matching `cargo objcopy` in those files. The escape hatch is deleting
   the line and pointing at `firmware/Makefile:101-109` instead; do not add an allow marker, since "the recipe exists
   in exactly one place" is the whole point. Rationale to put in the script header, with the measured silent failure:
   a mistyped `--only-section` name exits 0 and writes a 0-byte file (measured 2026-09-12), so a prose copy of the
   list can rot into a truncated image without any signal at build time.

Header comment must state, in caps where it matters, that `-n` is load-bearing: without it the default goal
`all: build` (`firmware/Makefile:82`) makes this lint cross-compile firmware inside a pre-commit hook.

## Step 2 - wire it

- `lefthook.yml`: add `doc-artifact-names` as the **first** command in `pre-commit.commands` (before `fmt-check` at
  :6-8) - cheapest check, fastest failure - and mirror it in `pre-push.commands`. Commands run from repo root unless
  given `root:`, so no `root:` key here (contrast `firmware-cross-compile` at :60-63, which needs one).
- `.github/workflows/ci.yml`: inside the existing `bash -c` block, between `cargo run -p asperitas-logging --example
  dump_reassemble -- --selftest` (:63) and the `=== firmware cross-compile ===` header (:65), add an
  `echo "=== docs artifact names ==="` plus the script call. It inherits `set -euo pipefail` (:22), so no extra
  error handling.

## Step 3 - prove it bites, then prove it is quiet

Paste all four into the ticket notes:

1. Plant `make flash # dfu-util ... -D firmware.bin` in a scratch edit of README.md -> red, naming `README.md:<line>`
   and the token.
2. Plant a `cargo objcopy ... -O binary main.bin` line in a doc -> red on rule 3 even though the filename is legal.
   This is the case that matters: TASK-057's dangerous half had a correct-looking filename.
3. Remove both -> green.
4. Time the green run and show `ls -l --time-style=full-iso firmware/target/thumbv7em-none-eabihf/release` (or the
   macOS equivalent) unchanged before and after, proving `-n` held and nothing was rebuilt.

Then run the AC#5 gates verbatim from `ci.yml:24,27,32,38,41,49,52,55,58,63,66,72` and note that `cd firmware` at
:66 persists into :72 in the same shell - firmware is a separate workspace excluded at root `Cargo.toml:5`, so
running the second cross-compile from the repo root fails for reasons unrelated to this change.

## Boundaries

- Do not fix any doc here. If the check finds a live violation, TASK-057 (or its successor) owns the prose; a gate
  commit that also edits its own test data hides both changes in one diff.
- Do not extend the check to backlog/**: twelve historical ticket files mention `firmware.bin` as evidence and must
  keep doing so.
- Out of scope and deliberately not folded in: firmware/ being invisible to fmt and clippy entirely (separate
  finding, filed separately), and anything that would require flashing a board.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implemented: scripts/check-doc-artifact-names.sh + wiring

One script (189 lines, bash + `make`/`grep`/`awk`/`find` only), two wiring edits. Exactly one `make`
invocation in it, and it carries `-n`. Exit codes: 0 clean,
1 violations (all of them, one run), 2 the check could not run at all - missing README/docs, no targets,
or make refusing to dry-run. A check that cannot run never reports success.

### Rule 1 red - planted `firmware.bin` in README.md (line 321)

    $ ./scripts/check-doc-artifact-names.sh
    docs artifact-name check FAILED (1 violation(s))
    images firmware/Makefile builds: blinky.bin ledtest.bin main.bin panictest.bin podtest.bin rig.bin
    allowlisted non-image names:     capture.bin

    README.md:321: firmware.bin - no firmware/Makefile target produces this image

    Legal names are derived, not listed: `make -n -C firmware BINARY=<target> build flash`
    for each firmware/src/bin/*.rs. A name that belongs in prose but is not an image
    (a log capture, a fixture) goes in ALLOWED_TOKENS here with a reason.
    $ echo $?
    1

### Rule 2 red - planted objcopy recipe in docs/reference/daisy-seed3.md, legal filename on purpose

The planted line flashes `main.bin`, a name R1 accepts, so rule 3/R2 is the only thing that can catch it.
That is TASK-057's dangerous half: correct-looking filename, hand-copied recipe.

    $ ./scripts/check-doc-artifact-names.sh
    docs artifact-name check FAILED (1 violation(s))
    images firmware/Makefile builds: blinky.bin ledtest.bin main.bin panictest.bin podtest.bin rig.bin
    allowlisted non-image names:     capture.bin

    docs/reference/daisy-seed3.md:933: cargo objcopy --release --features seed3 --bin main -- -O binary main.bin - the objcopy recipe lives only in firmware/Makefile's build: target; delete this copy and link to that file instead
    $ echo $?
    1

Both rules match both spellings (`cargo objcopy`, `cargo-objcopy`). No allow marker by design: the escape
hatch is deleting the line and pointing at the Makefile.

### Both violations accumulate in one run

    docs artifact-name check FAILED (2 violation(s))
    ...
    README.md:321: firmware.bin - no firmware/Makefile target produces this image
    docs/reference/daisy-seed3.md:933: cargo objcopy --release --features seed3 --bin main -- -O binary main.bin - the objcopy recipe lives only in firmware/Makefile's build: target; delete this copy and link to that file instead

### Green after removing both plantings

    $ time ./scripts/check-doc-artifact-names.sh

    real	0m0.154s	user	0m0.095s	sys	0m0.103s
    $ echo $?
    0

Quiet on success: no output at all when clean.

### AC#4 - never builds, measured rather than asserted

Digest over every file under firmware/target plus the built images in firmware/, before and after one run
(`stat -c '%n %Y %s'` per file, sha256 of the sorted list; 13,611 files):

    digest before: 5c541303e1879dc931a434f7ca8aaa7485e536db4d5d06a30025f6365c94e29f  -
    digest after:  5c541303e1879dc931a434f7ca8aaa7485e536db4d5d06a30025f6365c94e29f  -

Same digest taken earlier against a pre-change baseline also matched byte-for-byte. `-n` appears on the only
`make` invocation in the script and the header says in caps why dropping it turns the lint into a
cross-compile inside a pre-commit hook (`all: build` is the default goal). Runtime 0.15-0.18s.

### AC#3 - wiring proven by running the hooks, not by reading the YAML

lefthook v2.1.10, with files staged:

    $ lefthook run pre-commit
    ┃  doc-artifact-names ❯
    ✔️ clippy (0.38 seconds)  ✔️ clippy-log-defmt (0.21)  ✔️ clippy-log-usb (0.24)
    ✔️ doc-artifact-names (0.17 seconds)  ✔️ fmt-check (0.26 seconds)

Red case through the actual hook (planted token staged in README.md):

    README.md:321: firmware.bin - no firmware/Makefile target produces this image
    exit status 1
    🥊 doc-artifact-names (0.19 seconds)
    $ lefthook exit code: 1

pre-push mirror:

    $ lefthook run pre-push --command doc-artifact-names
    ✔️ doc-artifact-names (0.19 seconds)

CI: added as its own step between `dump_reassemble --selftest` and the firmware cross-compile, so it inherits
the block's `set -euo pipefail`. Placed before the cross-compiles because it is the cheapest gate.

### AC#5 - host gates green in nix develop .#default

Ran ci.yml's `bash -c` body extracted verbatim from the workflow (so the commands are CI's, not retyped),
under `bash -x` for provenance, in `nix develop .#default`: cargo fmt, all four clippy invocations (workspace,
log-usb, log-defmt, asperitas-pod/pod-hw), both `cargo doc` variants under `RUSTDOCFLAGS=-D warnings`,
`cargo test --workspace` and the pod-hw variant (34 `test result: ok`, zero failures),
`cargo run -p asperitas-logging --example dump_reassemble -- --selftest`, the new docs step, then both
firmware cross-compiles (`Finished release profile` x2, including the RTT-only `--no-default-features
--features "seed3 log-defmt"` build). No error or warning lines anywhere in the log. The `cd firmware` at
ci.yml's first cross-compile persists into the second, as the plan warned, which matters because firmware/ is
a separate workspace excluded at root Cargo.toml:5.

Measured while doing this: GNU Make comes from the dev shell itself, not from ambient PATH -
`env -u IN_NIX_SHELL PATH="$dir_of_nix:/bin:/usr/bin" nix develop .#default --command bash -c 'command -v make'`
still resolves gnumake 4.4.1, alongside dfu-util 0.11, probe-rs-tools 0.32.0 and cargo 1.97.1. So no flake
change was needed to keep CI able to run this gate.

### Notes for whoever reads next

- The ticket's Makefile line references are stale post-TASK-057: `all: build` is firmware/Makefile:89 (not 82)
  and the objcopy recipe is :116-124 (not 101-109). The script cites the current numbers; the ticket text does
  not, and I did not edit the ticket body's References line since it records what was verified when.
- Scope guards implemented as written: backlog/** is untouched (historical tickets quote `firmware.bin` as
  evidence), and no doc prose was edited here - the check landed green, so nothing needed fixing.
- Token regex uses maximal munch up front and `\b` on the tail, so `firmware.binary` is not mistaken for a
  reference to `firmware.bin`, and `some/path/main.bin` reduces to the basename the Makefile comparison uses.
- Pre-existing lefthook behavior worth knowing (not introduced here, not changed): with nothing staged, every
  pre-commit command reports `(skip) no matching staged files` and the hook exits 0. With files staged they all
  run, as shown above. If that skip is unintended it is a separate finding about the existing commands too.
- Follow-up deliberately NOT folded in: firmware/ remains invisible to the workspace fmt/clippy gates (the
  plan called that out as filed separately), and `docs/**/*.md` is scanned while `firmware/Cargo.toml` comments
  are not - TASK-057 found dead names there too, and a future extension could cover manifest comments.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added scripts/check-doc-artifact-names.sh: a host gate that derives legal firmware image names from the build itself - `make -n -C firmware BINARY=<t> build flash` for every firmware/src/bin/*.rs, comparing the `-O binary X.bin` and `-D X.bin` arguments so it doubles as a Makefile self-consistency test - then fails on any other *.bin token in README.md or docs/**/*.md (capture.bin allowlisted as a console log capture, with why) and on any cargo objcopy line in those files, because a prose copy of the keep-list rots into a truncated image in silence. Wired in as the first lefthook pre-commit command, mirrored in pre-push, and as a CI step ahead of the cross-compiles. Proven red on planted violations of each rule (including one with a legal filename), green once removed, 0.15s per run against a byte-identical firmware/target digest. All ci.yml host gates re-run verbatim inside nix develop .#default and pass. No doc prose edited: the gate landed green.
<!-- SECTION:FINAL_SUMMARY:END -->
