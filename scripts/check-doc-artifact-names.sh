#!/usr/bin/env bash
#
# Keep reader-facing docs to image names the build really produces.
#
#   R1  Every `*.bin` token in README.md or docs/**/*.md must be an image firmware/Makefile builds
#       (derived from `make -n`, not a hand-kept list) or an entry in ALLOWED_TOKENS.
#   R2  No `cargo objcopy` invocation appears in those files. The recipe lives in firmware/Makefile's
#       `build:` target only: a prose copy drifts, and a wrong objcopy keep-list exits 0 and writes a
#       truncated image.
#
# The -n on every make invocation is load-bearing: firmware/Makefile's default goal is `all: build`,
# so without it this lint would cross-compile firmware in every tier, including pre-commit.
#
# Scope is README.md and docs/ on purpose: backlog/** quotes stale names as evidence.
#
# Exit codes: 0 clean, 1 violations (all of them, in one run), 2 the check itself could not run.

set -uo pipefail

ROOT=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT" || exit 2

# `*.bin` tokens that R1 permits despite being no firmware image.
#   capture.bin: a console log capture read OFF the board, not an image flashed TO it. Quoted as
#   the dump destination in README.md and docs/reference/flashing.md.
#
#   dsy_bootloader_v6_4-intdfu-2000ms.bin: the stock DaisyBootloader release image, downloaded from
#   electro-smith/DaisyBootloader and flashed to internal flash once. Third-party, so no
#   firmware/Makefile target can produce it. Quoted in docs/reference/flashing.md's bootloader
#   section.
ALLOWED_TOKENS=(capture.bin dsy_bootloader_v6_4-intdfu-2000ms.bin)

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

  # Both link layouts name their images differently (`rig.bin`, `rig-bootloaded.bin`), so the legal
  # set is derived once per layout rather than listing the second spelling here.
  local layout
  for layout in internal bootloaded; do
  for src in "${targets[@]}"; do
    name=${src##*/}
    name=${name%.rs}

    if ! rules=$(make -n -C firmware BINARY="$name" LAYOUT="$layout" build flash 2>&1); then
      printf '%s\n' "check-doc-artifact-names: make -n -C firmware BINARY=$name LAYOUT=$layout build flash failed:" >&2
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
  done
}

is_permitted_token() {
  local tok=$1 permitted
  for permitted in "${LEGAL_BINS[@]}" "${ALLOWED_TOKENS[@]}"; do
    [ "$tok" = "$permitted" ] && return 0
  done
  return 1
}

# --------------------------------------------------------------------------- pass 2: doc tokens

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

# --------------------------------------------------------------------------- pass 3: hand-copied recipes

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
  check_bin_tokens
  check_objcopy_recipes
  report
}

main
exit $?
