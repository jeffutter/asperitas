#!/usr/bin/env bash
#
# Gate the firmware image names that reader-facing documentation is allowed to contain, and the one
# shape the recipe that produces them is allowed to have.
#
# Three rules, and all three derive what is legal from the build itself rather than from a list kept
# by hand, because a list kept by hand is the thing that went stale:
#
#   R1  Every `*.bin` token in README.md or docs/**/*.md must be an image firmware/Makefile really
#       builds, or one of the explicitly allowlisted non-images below. TASK-057 found `firmware.bin`
#       still quoted in six places (README.md, firmware/Cargo.toml) long after each target started
#       writing `$(BINARY).bin`, and nothing could see it: at that point every gate in the repo was a
#       cargo invocation, so make was nobody's input and no check compared a doc string against what
#       the build produces. Prose fixes decay; this makes the class self-policing.
#
#   R2  No `cargo objcopy` invocation appears in those files at all. The recipe belongs in
#       firmware/Makefile's `build:` target (:118-120) and nowhere else, because a second
#       copy in prose fails silently. Measured 2026-09-12:
#
#         llvm-objcopy -O binary --only-section=.no_such_section elf out.bin
#
#       exits 0 and writes a 0-byte file, no warning. A doc that transcribes the keep-list is a doc
#       that can turn into instructions for producing a truncated image, and the only symptom is a
#       board that does not boot. TASK-057's prose copy had already lost the flags.
#
#   R3  firmware/Makefile's `build:` recipe passes no `--only-section` flag. Until TASK-059 landed,
#       six of them were load-bearing: `.sram1_bss` loaded where it runs, so a plain `-O binary`
#       spanned flash to AXI SRAM and padded the gap with zeros (469,763,480 bytes for `main`). That
#       section is `(NOLOAD)` placement now, so the plain image is what flash must hold and a
#       keep-list can only subtract something real -- silently, per R2's measurement. Deleting the
#       flags without forbidding their return would leave the next plausible-looking "optimisation"
#       free to re-add one. TASK-059.02 replaces this proxy with the direct check: no file-backed
#       section may load outside flash.
#
# THE -n FLAG ON EVERY make INVOCATION HERE IS LOAD-BEARING. firmware/Makefile's default goal is
# `all: build` (firmware/Makefile:89). Drop -n and this lint stops reading the build rules and starts
# cross-compiling firmware inside every tier of the gate set, including the pre-commit hook that fires
# on every commit. They are the only make invocations any tier has (`scripts/gates.sh` runs this script
# as the second gate, cheapest-first); everything else in the gate set is raw cargo.
#
# Scope is README.md and docs/ on purpose. It does NOT extend to backlog/**: historical ticket
# files quote `firmware.bin` as evidence of what was broken, and must keep doing so.
#
# Exit codes: 0 clean, 1 violations found (all of them, in one run), 2 the check itself could not
# run - missing files, or make refusing to dry-run. A check that cannot run never reports success.

set -uo pipefail

ROOT=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT" || exit 2

# `*.bin` tokens that R1 permits despite being no firmware image.
#   capture.bin: a console log capture read OFF the board, not an image flashed TO it. Quoted as
#   the dump destination in README.md's console-capture section and in
#   docs/reference/daisy-seed3.md's flashing walkthrough.
ALLOWED_TOKENS=(capture.bin)

DOC_FILES=()
LEGAL_BINS=()
VIOLATIONS=()

die() {
  printf '%s\n' "check-doc-artifact-names: $*" >&2
  exit 2
}

# --------------------------------------------------------------------------- files

collect_doc_files() {
  local f
  [ -f README.md ] || die "no README.md here; expected to run from the repo root"
  [ -d docs ] || die "no docs/ directory here"

  DOC_FILES+=(README.md)
  while IFS= read -r -d '' f; do
    DOC_FILES+=("$f")
  done < <(find docs -type f -name '*.md' -print0 | sort -z)

  [ "${#DOC_FILES[@]}" -gt 1 ] || die "found no markdown under docs/ - nothing to check"
}

# --------------------------------------------------------------------------- pass 1: what is legal

# For every binary target, ask make what image it builds (`-O binary X.bin`) and what image it
# flashes (`dfu-util ... -D X.bin`). Those two agreeing is the definition of a real artifact name,
# so this doubles as a Makefile self-consistency test at no extra cost.
derive_legal_image_names() {
  local src name rules built flashed
  local -a targets=()

  for src in firmware/src/bin/*.rs; do
    [ -e "$src" ] || continue
    targets+=("$src")
  done
  [ "${#targets[@]}" -gt 0 ] || die "no targets in firmware/src/bin/*.rs: the legal set would be \
empty and every name in the docs would look wrong"

  for src in "${targets[@]}"; do
    name=${src##*/}
    name=${name%.rs}

    if ! rules=$(make -n -C firmware BINARY="$name" build flash 2>&1); then
      printf '%s\n' "check-doc-artifact-names: make -n -C firmware BINARY=$name build flash failed:" >&2
      printf '%s\n' "$rules" >&2
      exit 2
    fi
    rules=${rules//$'\n'/ }

    built=$(printf '%s' "$rules" |
      grep -oE -- '-O[[:space:]]+binary[[:space:]]+[a-zA-Z0-9_./-]+' | head -1 | awk '{print $NF}')
    flashed=$(printf '%s' "$rules" |
      grep -oE -- '-D[[:space:]]+[a-zA-Z0-9_./-]+\.bin' | head -1 | awk '{print $NF}')

    if [ -z "$built" ] || [ -z "$flashed" ]; then
      VIOLATIONS+=("firmware/Makefile: target '$name' yields no comparable image name (built=\
'${built:-none}' flashed='${flashed:-none}'); the -O binary and -D arguments are what this check reads")
      continue
    fi

    if [ "$built" != "$flashed" ]; then
      VIOLATIONS+=("firmware/Makefile: target '$name' builds '${built##*/}' but flashes \
'${flashed##*/}'. A build/flash mismatch is worse than anything the docs could get wrong - fix the \
Makefile before touching a document.")
    fi

    # Basename, because that is the shape a doc quotes it in: a path in the recipe would otherwise
    # never match a token grepped out of prose.
    LEGAL_BINS+=("${built##*/}")
  done
}

is_permitted_token() {
  local tok=$1 permitted
  for permitted in "${LEGAL_BINS[@]}" "${ALLOWED_TOKENS[@]}"; do
    [ "$tok" = "$permitted" ] && return 0
  done
  return 1
}

# --------------------------------------------------------------------------- pass 2: recipe shape

# R3. Read what the recipe expands to rather than the text of the file, because what matters is the
# command the build runs.
check_no_keep_list() {
  local rules
  if ! rules=$(make -n -C firmware build 2>&1); then
    printf '%s\n' "check-doc-artifact-names: make -n -C firmware build failed:" >&2
    printf '%s\n' "$rules" >&2
    exit 2
  fi

  if printf '%s' "$rules" | grep -qE -- '--only-section'; then
    VIOLATIONS+=("firmware/Makefile: the build: recipe passes --only-section. Since TASK-059 the \
plain -O binary image is exactly what flash holds, so a keep-list can only remove something real \
- and it removes it silently, per R2's measurement. Delete the flags; if you think one is needed \
again, an ALLOC section is loading outside flash, which is the thing to fix.")
  fi
}

# --------------------------------------------------------------------------- pass 3: doc tokens

check_bin_tokens() {
  local f line tok
  for f in "${DOC_FILES[@]}"; do
    # Maximal munch on the leading class, and \b on the tail so `firmware.binary` is not read as a
    # reference to `firmware.bin`.
    while IFS=: read -r line tok; do
      [ -n "$line" ] || continue
      if ! is_permitted_token "$tok"; then
        VIOLATIONS+=("$f:$line: $tok - no firmware/Makefile target produces this image")
      fi
    done < <(grep -noE '[A-Za-z0-9_-]+\.bin\b' "$f" || true)
  done
}

# --------------------------------------------------------------------------- pass 4: hand-copied recipes

check_objcopy_recipes() {
  local f line text
  for f in "${DOC_FILES[@]}"; do
    # Both spellings: `cargo objcopy` is the command, `cargo-objcopy` is the crate that installs it,
    # and either one in prose means the recipe got copied.
    while IFS= read -r hit; do
      line=${hit%%:*}
      text=${hit#*:}
      text=${text#"${text%%[![:space:]]*}"}
      VIOLATIONS+=("$f:$line: $text - the objcopy recipe lives only in firmware/Makefile's build: \
target; delete this copy and link to that file instead")
    done < <(grep -nE 'cargo([[:space:]]|-)objcopy' "$f" || true)
  done
}

# --------------------------------------------------------------------------- reporting

report() {
  [ "${#VIOLATIONS[@]}" -gt 0 ] || return 0

  {
    printf '%s\n' "docs artifact-name check FAILED (${#VIOLATIONS[@]} violation(s))"
    printf '%s\n' "images firmware/Makefile builds: ${LEGAL_BINS[*]}"
    printf '%s\n' "allowlisted non-image names:     ${ALLOWED_TOKENS[*]}"
    printf '\n'
    printf '%s\n' "${VIOLATIONS[@]}"
    printf '\n'
    printf '%s\n' 'Legal names are derived, not listed: `make -n -C firmware BINARY=<target> build flash`'
    printf '%s\n' 'for each firmware/src/bin/*.rs. A name that belongs in prose but is not an image'
    printf '%s\n' '(a log capture, a fixture) goes in ALLOWED_TOKENS here with a reason.'
  } >&2
  return 1
}

main() {
  collect_doc_files
  derive_legal_image_names
  check_no_keep_list
  check_bin_tokens
  check_objcopy_recipes
  report
}

main
exit $?
