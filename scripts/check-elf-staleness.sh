#!/usr/bin/env bash
#
# Assert that firmware/Makefile decides "is this ELF built from these sources?" from the bytes of the
# sources, and never from their timestamps.
#
# Why a gate and not a transcript: the check this replaced compared mtimes (`find -newer`), so it went
# red on a tree nobody had edited -- TASK-056 measured `make probe-log` refusing to run with `git
# status` clean and 2975 s separating a freshly linked ELF from files a checkout had just refreshed.
# A fix for that is only worth what its assertion is worth, and a hand-run transcript asserts nothing
# after the next edit to either side. So every case below drives the SHIPPED recipe through make's own
# command-line overrides, which is also what makes the suite red against the old Makefile and green
# against the new one without either being reimplemented here.
#
# The injection points are all plain `=` assignments in firmware/Makefile, so the command line wins:
#
#   ELF          an ordinary file standing in for the linked image (nothing here parses it)
#   ELF_INPUTS   the fixture directory, so the digest covers fake sources instead of real ones
#   MAIN_SRC     the file `build-elf` touches to force a relink, kept inside the fixture
#   CARGO        `true`, so no cross-toolchain runs and no firmware crate recompiles
#   PROV         `true`, stubbing the cfg-provenance clause, which is scripts/elf-provenance.sh's
#                business and already asserted there (TASK-067). Stubbing it also keeps this suite off
#                `cargo metadata`. It does hide the order the two clauses run in, which neither this
#                suite nor elf-provenance's selftest covers; read that ordering in the recipe itself.
#
# What `CARGO=true` does and does not prove: `build-elf` forces its own relink by touching MAIN_SRC,
# so the stamp it writes is earned by that touch rather than by observing a link, and a stub compiler
# still exercises the whole stamping path -- digest, comparison, temp file, rename, and the refusal to
# stamp when the compiler fails. Whether cargo really links after that touch is a cargo fact, measured
# in firmware/Makefile's comment beside the code that relies on it, not something a fake compiler can
# speak to.
#
# Cost rules, obeyed so this stays a commit-tier gate: fixtures are written at runtime under a
# `mktemp -d` and removed on exit, never under firmware/target/. No case runs cargo, clippy, objcopy,
# objdump or a real build; each runs one `make` against a parsed Makefile (~40 ms warm). Nothing here
# writes a tracked file -- which matters more than usual, because writing a firmware source would bump
# the mtime of an ELF input and send the bench's own `elf-check` red, the exact failure this script is
# about. Measured 0.4 s warm for ten cases.
#
# Exit codes: 0 every case passed, 1 at least one case failed (all failures reported in one run), 2
# the suite could not run at all -- no make, no sha256 tool, or a fixture that could not be staged.

set -uo pipefail

PROG=check-elf-staleness

ROOT=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
MAKEFILE=$ROOT/firmware/Makefile

die() {
  printf '%s\n' "$PROG: $*" >&2
  exit 2
}

usage() {
  cat <<'EOF'
usage: scripts/check-elf-staleness.sh --selftest
EOF
}

# --------------------------------------------------------------------------- assertions

ST_TOTAL=0
ST_FAILED=0

# One assertion failing ends its case and nothing else. It has to EXIT rather than return: run_case
# reads each case through a command substitution, so a function that merely returned 1 had its status
# overwritten by whatever ran last. Same shape as scripts/elf-provenance.sh's harness, copied rather
# than shared -- two small harnesses in two scripts is cheaper than one harness with two callers.
fail() { printf '%s\n' "$*" >&2; exit 1; }

run_case() { # <name> <case-function>
  local name=$1 reason rc
  ST_TOTAL=$((ST_TOTAL + 1))
  reason=$("$2" 2>&1)
  rc=$?
  if [ $rc -eq 0 ]; then
    printf '%s\n' "$PROG: selftest $name ok" >&2
  else
    ST_FAILED=$((ST_FAILED + 1))
    printf '%s\n' "$PROG: selftest $name FAILED: ${reason:-exited $rc without saying why}" >&2
  fi
}

expect_eq() { # <what> <want> <got>
  local what=$1 want=$2 got=$3
  [ "$want" = "$got" ] || fail "$what: want [$want], got [$got]"
}

expect_has() { # <what> <needle> <haystack>   - fixed-string: these contain regex punctuation
  local what=$1 needle=$2 hay=$3
  printf '%s' "$hay" | grep -qF -- "$needle" || fail "$what: [$needle] not present in [$hay]"
}

# The exit code is the assertion most likely to be got backwards, because `make` turns any nonzero
# recipe status into its own 2. Cases therefore ask for zero or nonzero and grade the MESSAGE, which
# is what a driver has to match on anyway (docs/reference/daisy-seed3.md's exit-code table).
expect_zero() { # <what>
  [ "$MAKE_RC" = 0 ] || fail "$1: 'make $MAKE_GOAL' exited $MAKE_RC, wanted 0: $MAKE_OUT"
}

expect_refused() { # <what> <needle-in-output>
  local what=$1 needle=$2
  [ "$MAKE_RC" != 0 ] || fail "$1: 'make $MAKE_GOAL' exited 0, wanted nonzero: $MAKE_OUT"
  expect_has "$1 output" "$needle" "$MAKE_OUT"
}

# --------------------------------------------------------------------------- the fixture
#
# A miniature of the tree the digest walks: sources under src/ and crates/, the three loose files, and
# a target/ that must stay pruned. Deliberately NOT under firmware/target/, so no run of this can be
# mistaken for build residue some other gate might read.

fx_setup() { # <dir-to-create>
  FX=$1
  rm -rf "$FX" || return 1
  mkdir -p "$FX/src/bin" "$FX/crates/foo/src" "$FX/target" || return 1
  printf 'fn main() {}\n' >"$FX/src/bin/main.rs" || return 1
  printf 'pub fn tone() {}\n' >"$FX/crates/foo/src/lib.rs" || return 1
  printf '[package]\nname = "foo"\nversion = "0.1.0"\nedition = "2021"\n' >"$FX/crates/foo/Cargo.toml" || return 1
  printf 'MEMORY_FLASH_ORIGIN = 0x08000000u32;\n' >"$FX/memory.x" || return 1
  printf '[workspace]\nmembers = ["crates/foo"]\n' >"$FX/Cargo.toml" || return 1
  printf 'version = 3\n\n[[package]]\nname = "foo"\nversion = "0.1.0"\n' >"$FX/Cargo.lock" || return 1
  # Present at setup rather than added mid-case: pruning is then a property of the same enumeration
  # every other case uses, not of a directory that only existed for one of them.
  printf '// generated by something that is not a source\n' >"$FX/target/generated.rs" || return 1
  printf 'not-yet-linked\n' >"$FX/main" || return 1
  return 0
}

# Where the mechanism says the stamp lives, spelled independently of the Makefile: `$(dir $(ELF))` plus
# `$(BINARY).elf-inputs.sha256`, with ELF pointed at the fixture and BINARY left at its default. Set
# per case below. If the two spellings ever disagree, the cases stop finding a stamp and say so rather
# than passing quietly.
FX=
FX_MAIN=

fx_touch_everything() {
  find "$FX" -exec touch {} + 2>/dev/null
}

MAKE_GOAL=
MAKE_RC=
MAKE_OUT=

run_make() { # <goal> [extra make overrides...]
  MAKE_GOAL=$1
  shift
  MAKE_OUT=$(make --no-print-directory -C "$ROOT/firmware" "$MAKE_GOAL" \
    "ELF=$FX/main" \
    "ELF_INPUTS=$FX" \
    "MAIN_SRC=$FX/src/bin/main.rs" \
    "PROV=true" \
    "CARGO=true" \
    "$@" 2>&1)
  MAKE_RC=$?
}

# Stage a fixture plus a stamp recorded from it, which is the state every case except two begins from.
fx_linked_clean() {
  fx_setup "$FX" || fail "could not stage the fixture in $FX"
  run_make build-elf
  expect_zero "staging the stamp"
  [ -f "$FX_MAIN.elf-inputs.sha256" ] || fail "build-elf wrote no stamp at $FX_MAIN.elf-inputs.sha256"
}

# --------------------------------------------------------------------------- the cases

# AC #1 and #2 in one case, and the one that is red against the pre-TASK-056 Makefile by design:
# refresh every mtime in the tree without changing a byte and the check must stay out of the way. That
# is what `git checkout`, `git stash pop` and worktree operations do to a tree, which is how the loop
# was hitting it.
case_bulk_mtime_refresh_is_silent() {
  fx_linked_clean
  fx_touch_everything
  run_make elf-check
  expect_zero "after touching every input"
}

# The other half of the pair: content that differs must still be caught with no rebuild in between, or
# the mtime test has been replaced by a check that never goes red.
case_byte_edit_without_rebuild() {
  fx_linked_clean
  printf '\npub fn added()\n{}\n' >>"$FX/crates/foo/src/lib.rs"
  run_make elf-check
  expect_refused "an input whose bytes differ from the stamp" "was not built from the sources on disk"
}

# Distinguishing "the bytes moved" from "the clock moved" cuts both ways: undo the edit and the same
# ELF is current again, with no build in between either.
case_bytes_restored_is_green() {
  fx_linked_clean
  cp "$FX/crates/foo/src/lib.rs" "$FX/lib.rs.saved"
  printf '\npub fn added()\n{}\n' >>"$FX/crates/foo/src/lib.rs"
  mv "$FX/lib.rs.saved" "$FX/crates/foo/src/lib.rs"
  run_make elf-check
  expect_zero "after the original bytes went back"
}

# The case that rules out `sha256sum -c` as the mechanism: a checksum-file comparison walks the list it
# was given, so an input that appeared after the stamp is invisible to it. Enumeration is what catches
# this one.
case_added_input_detected() {
  fx_linked_clean
  mkdir -p "$FX/crates/bar/src"
  printf 'pub fn new_crate() {}\n' >"$FX/crates/bar/src/lib.rs"
  run_make elf-check
  expect_refused "an input added since the stamp" "was not built from the sources on disk"
}

# Names ride in the hashed stream precisely for this: renaming a file changes its digest even though
# no file's contents did.
case_renamed_input_detected() {
  fx_linked_clean
  mv "$FX/crates/foo/src/lib.rs" "$FX/crates/foo/src/renamed.rs"
  run_make elf-check
  expect_refused "an input renamed since the stamp" "was not built from the sources on disk"
}

# Build products must not count as sources. Without the prune this goes red on the first cargo run
# inside the fixture, which is the same trap the mtime version had.
case_target_stays_pruned() {
  fx_linked_clean
  printf '\n// regenerated\n' >>"$FX/target/generated.rs"
  touch "$FX/target/generated.rs"
  run_make elf-check
  expect_zero "after a build product under target/ changed"
}

# AC #4. The absence of a record is not evidence of freshness: an ELF older than this mechanism, or any
# tree that has been `cargo clean`ed, has to be refused by name.
case_missing_stamp_fails_loudly() {
  fx_linked_clean
  rm "$FX_MAIN.elf-inputs.sha256"
  run_make elf-check
  expect_refused "a missing stamp" "$FX_MAIN.elf-inputs.sha256"
}

# An empty input set is the vacuous-pass trap: `find ... | xargs shasum` with nothing to hash runs
# shasum on stdin, and the digest of empty input is a well-formed hex string. The writer has to fail
# rather than record it, or every later check compares against a stamp no sources describe.
case_empty_input_set_fails_loudly() {
  fx_setup "$FX" || fail "could not stage the fixture in $FX"
  mkdir -p "$FX/nothing-here"
  run_make build-elf "ELF_INPUTS=$FX/nothing-here"
  [ "$MAKE_RC" != 0 ] || fail "an empty input set was accepted: $MAKE_OUT"
  expect_has "and said which" "no ELF inputs found under" "$MAKE_OUT"
  [ ! -f "$FX_MAIN.elf-inputs.sha256" ] || fail "an empty input set still wrote a stamp"
}

# Ordering claim: a failed compile must not leave a fresh stamp behind, or the next `elf-check` blesses
# a link that never happened. With the compiler stubbed to fail, the stamp must not move.
case_failed_compile_leaves_no_stamp() {
  fx_setup "$FX" || fail "could not stage the fixture in $FX"
  run_make build-elf "CARGO=false"
  [ "$MAKE_RC" != 0 ] || fail "build-elf succeeded with a failing compiler"
  [ ! -f "$FX_MAIN.elf-inputs.sha256" ] || fail "a failed compile recorded a stamp anyway"
}

# The tripwire under the whole ticket: if a timestamp operator ever comes back into this recipe, the
# false positive it removes comes back with it. Read from the shipped Makefile rather than from memory,
# and scoped to the recipe so the word cannot hide in a comment three targets away.
case_no_mtime_operator_in_elf_check() {
  local recipe
  recipe=$(awk 'f && /^[^\t]/ { exit } /^elf-check:/ { f = 1 } f { print } END {
      if (!f) exit 3 }' "$MAKEFILE") || fail "no elf-check recipe found in $MAKEFILE"
  [ -n "$recipe" ] || fail "the elf-check recipe in $MAKEFILE is empty"
  case $recipe in
    *-newer*) fail "elf-check compares mtimes again (-newer): restarts the false positive TASK-056 ends" ;;
    *-nt\ *|*'-nt '*) fail "elf-check compares mtimes again (-nt): restarts the false positive TASK-056 ends" ;;
  esac
  expect_has "elf-check consults the stamp" 'ELF_STAMP' "$recipe"
  expect_has "and the digest" 'ELF_INPUTS_SHA256' "$recipe"
}

# --------------------------------------------------------------------------- running them

run_selftest() {
  command -v make >/dev/null 2>&1 || die "no make on PATH, so there is no recipe to assert"
  command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1 \
    || die "no sha256 tool on PATH (tried sha256sum and shasum), so the digest cannot be computed"
  [ -f "$MAKEFILE" ] || die "no $MAKEFILE - this suite asserts a recipe, not a memory of one"

  local base
  base=$(mktemp -d) || die "mktemp -d failed, so the selftest has nowhere to write its fixtures"
  trap 'if [ -n "${base:-}" ]; then rm -rf "$base"; fi' EXIT

  # One fixture dir per case, named after the case: mktemp hands out one path here, and reusing one
  # dir across cases would let a case inherit another's stamp or stray edit.
  local i=0
  for spec in \
    "bulk-mtime-refresh-is-silent case_bulk_mtime_refresh_is_silent" \
    "byte-edit-without-rebuild case_byte_edit_without_rebuild" \
    "bytes-restored-is-green case_bytes_restored_is_green" \
    "added-input-detected case_added_input_detected" \
    "renamed-input-detected case_renamed_input_detected" \
    "target-stays-pruned case_target_stays_pruned" \
    "missing-stamp-fails-loudly case_missing_stamp_fails_loudly" \
    "empty-input-set-fails-loudly case_empty_input_set_fails_loudly" \
    "failed-compile-leaves-no-stamp case_failed_compile_leaves_no_stamp" \
    "no-mtime-operator-in-elf-check case_no_mtime_operator_in_elf_check"; do
    i=$((i + 1))
    FX=$base/fixture$i
    FX_MAIN=$FX/main
    run_case "${spec%% *}" "${spec##* }"
  done

  if [ "$ST_FAILED" = 0 ]; then
    printf '%s\n' "$PROG: selftest passed ($ST_TOTAL cases)" >&2
    return 0
  fi
  printf '%s\n' "$PROG: selftest FAILED ($ST_FAILED of $ST_TOTAL cases)" >&2
  return 1
}

mode=${1:-}
case $mode in
  --selftest)
    [ $# -eq 1 ] || { usage >&2; exit 2; }
    run_selftest
    exit $?
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
