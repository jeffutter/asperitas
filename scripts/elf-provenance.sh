#!/usr/bin/env bash
#
# Read the cfg provenance out of a firmware ELF, and refuse an ELF that is not the set asked for.
#
# Why this exists: firmware/target/thumbv7em-none-eabihf/release/main is ONE NAME FOR TWO IMAGES.
# scripts/gates.sh builds the console cfg set and the RTT-only one back to back, and the top-level
# name alternates between the two deps/main-<hash> artifacts it hardlinks, so neither the path nor
# the mtime says which image is sitting there. The bench decodes defmt frames with whatever that
# path currently holds, and the wrong choice mislabels every frame without an error.
#
# The answer travels inside the image. firmware/build.rs writes a non-allocated `.asp.prov` note
# section holding `key=value` lines led by the format tag `asp-prov1`; build.rs defines the format,
# and this script is the ONLY thing in the repo that reads it. Nothing here restates more of the
# format than the tag and the three keys, and nothing else should learn to parse the blob.
#
# Two modes:
#
#   show  <elf>                          print the normalized provenance on stdout, exit 0
#   check <elf> <FEATURES> <NO_DEFAULT>  exit 0 on a match, quiet
#
# `check` takes what the operator asked for as the raw FEATURES / NO_DEFAULT strings the Makefile
# holds, not as a pre-digested feature list: firmware/Makefile must not have to know that `default`
# means log-usb, so the closure is derived from cargo metadata here instead.
#
# DEFMT_LOG is reported by `show` and deliberately NOT compared by `check`: it selects which frames
# got compiled in, not which cfg set an image is, and refusing a correct ELF because the shell
# happens to export DEFMT_LOG would block the very command used to read the board.
#
# Exit codes: 0 match (for `show`: blob read), 1 the ELF is a different cfg set than asked, naming
# both, 2 the check itself could not run - no ELF at that path, no .asp.prov section, or a blob this
# version cannot parse. A check that cannot run never reports success, and never stays silent.
#
# Coupling is mutual with firmware/Makefile: its `elf-check` target calls this, and this derives the
# expected set from the same FEATURES / NO_DEFAULT knobs that recipe expands. scripts/check-doc-
# artifact-names.sh already reaches down into the Makefile to enumerate legal image names; this is
# the Makefile reaching up into scripts/, so both headers say so.

set -uo pipefail

PROG=elf-provenance
TAG=asp-prov1

ROOT=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
FIRMWARE_DIR=$ROOT/firmware

die() {
  printf '%s\n' "$PROG: $*" >&2
  exit 2
}

usage() {
  cat <<'EOF'
usage: scripts/elf-provenance.sh show <elf>
       scripts/elf-provenance.sh check <elf> <FEATURES> <NO_DEFAULT>
EOF
}

# --------------------------------------------------------------------------- reading the blob

# Lowercase, '-' to '_', drop the implicit `default` token, sort, dedupe, comma-join. Must match how
# build.rs normalizes CARGO_FEATURE_* or the two sides of a comparison disagree for real reasons.
normalize_features() {
  tr ' \t,' '\n\n\n' \
    | tr 'A-Z' 'a-z' \
    | tr '-' '_' \
    | sed '/^$/d' \
    | { grep -vx default || true; } \
    | sort -u \
    | paste -sd, -
}

# $1 elf -> stdout: the raw blob, NULs stripped. Any failure to read exits 2 with its own message,
# because "no section" and "no file" need different advice.
read_blob() {
  local elf=$1 tmp err rc msg
  [ -f "$elf" ] || die "no ELF at $elf - build one first: make build-elf FEATURES=\"...\" (add NO_DEFAULT=1 for an RTT-only image)"
  command -v rust-objcopy >/dev/null 2>&1 \
    || die "no rust-objcopy on PATH - this reads the ELF with cargo-binutils, available in nix develop .#default"

  tmp=$(mktemp) || die "mktemp failed"
  err=$(mktemp) || die "mktemp failed"
  # The trailing /dev/null is load-bearing, not decoration. Measured here on LLVM 22:
  #
  #   rust-objcopy --dump-section .asp.prov=out ELF      -> rewrites ELF in place, mtime = now
  #   rust-objcopy --dump-section .asp.prov=out ELF /dev/null -> ELF untouched
  #
  # Content comes out byte-identical either way (same sha256 before and after), which is exactly why
  # this is easy to miss: the only damage is the timestamp. A reader that refreshes the ELF's mtime
  # silently neuters firmware/Makefile's staleness test -- elf-check could never go red again -- and
  # the ELF is the host's only copy of the symbols describing whatever the bench is running.
  if ! rust-objcopy --dump-section ".asp.prov=$tmp" "$elf" /dev/null 2>"$err"; then
    msg=$(sed -n '1p' "$err")
    rm -f "$tmp" "$err"
    if printf '%s' "$msg" | grep -q "section '\.asp\.prov' not found"; then
      printf '%s\n' "$PROG: $elf has no .asp.prov section, so it predates the provenance stamp or came from a tree older than it" >&2
      printf '%s\n' "$PROG: nothing about its cfg set is knowable from it - rebuild it: make build-elf FEATURES=\"...\" (add NO_DEFAULT=1 for an RTT-only image)" >&2
      exit 2
    fi
    die "rust-objcopy could not read $elf: ${msg:-no output}"
  fi
  rm -f "$err"
  # The blob is newline-separated with one trailing NUL from the assembler's `.asciz`.
  tr -d '\0' <"$tmp"
  rm -f "$tmp"
}

# Blob -> three shell vars. Anything missing, or a leading tag that isn't ours, is exit 2: guessing
# at a format change is worse than refusing to answer.
parse_blob() {
  local blob=$1 line key value first=1
  BLOB_TAG=""
  PROV_DEFAULT=""
  PROV_FEATURES=""
  PROV_DEFMT_LOG=""
  local seen_default=0 seen_features=0

  while IFS= read -r line || [ -n "$line" ]; do
    if [ $first -eq 1 ]; then
      first=0
      [ "$line" = "$TAG" ] || die "blob in this ELF starts with '${line:-nothing}', not '$TAG': it was stamped by a build.rs this script does not understand"
      continue
    fi
    [ -n "$line" ] || continue
    key=${line%%=*}
    value=${line#*=}
    case $key in
      default) PROV_DEFAULT=$value; seen_default=1 ;;
      features) PROV_FEATURES=$(printf '%s' "$value" | normalize_features); seen_features=1 ;;
      defmt_log) PROV_DEFMT_LOG=$value ;;
      *) : ;; # unknown key: forward-compatible, ignore it rather than refuse a newer stamp
    esac
  done <<<"$blob"

  [ "$seen_default" = 1 ] || die "stamped ELF has no default= line: the blob is truncated or malformed"
  [ "$seen_features" = 1 ] || die "stamped ELF has no features= line: the blob is truncated or malformed"
}

# --------------------------------------------------------------------------- what was asked for

# cargo metadata answers "what does turning defaults on actually enable", so no copy of this repo
# hard-codes that `default` means log-usb. Expanded transitively over this package's own features
# only: entries naming another package ("foo/bar") or a optional dependency ("dep:...") are not
# features of this crate and never appear as CARGO_FEATURE_*.
#
# metadata never links, so unlike `cargo objcopy` / `cargo objdump` it cannot re-point release/main
# mid-gate - the hazard that makes those forms forbidden in scripts/gates.sh.
expected_from_metadata() {
  local json pkg feats queue cur dep out=""
  if ! json=$(cd "$FIRMWARE_DIR" && cargo metadata --format-version 1 --no-deps --offline 2>&1); then
    die "cargo metadata failed in $FIRMWARE_DIR: $json"
  fi
  if ! pkg=$(printf '%s' "$json" | jq -r --arg m "$FIRMWARE_DIR/Cargo.toml" \
    '[.packages[] | select(.manifest_path == $m)] | if length == 1 then (.[0].features | tojson) else error("expected exactly one package with manifest " + $m) end' 2>&1); then
    die "cannot identify the firmware package in cargo metadata output: $pkg"
  fi

  queue=(default)
  while [ ${#queue[@]} -gt 0 ]; do
    cur=${queue[0]}
    queue=("${queue[@]:1}")
    case $cur in
      */* | dep:*) continue ;;
    esac
    printf '%s\n' "$out" | grep -qx -- "$cur" && continue
    out+="$cur"$'\n'
    while IFS= read -r dep; do
      [ -n "$dep" ] && queue+=("$dep")
    done < <(printf '%s' "$pkg" | jq -r --arg k "$cur" '.[$k] // [] | .[]')
  done

  printf '%s' "$out" | normalize_features
}

# --------------------------------------------------------------------------- modes

mode=${1:-}
case $mode in
  show)
    [ $# -eq 2 ] || { usage >&2; exit 2; }
    blob=$(read_blob "$2") || exit 2
    parse_blob "$blob" || exit 2
    printf 'default=%s features=%s defmt_log=%s\n' "$PROV_DEFAULT" "$PROV_FEATURES" "$PROV_DEFMT_LOG"
    ;;

  check)
    [ $# -eq 4 ] || { usage >&2; exit 2; }
    elf=$2 asked_features=$3 asked_no_default=$4

    blob=$(read_blob "$elf") || exit 2
    parse_blob "$blob" || exit 2

    want_default=1
    [ "$asked_no_default" = 1 ] && want_default=0

    if [ "$want_default" = 1 ]; then
      closure=$(expected_from_metadata) || exit 2
    else
      closure=""
    fi
    want_features=$(printf '%s %s' "$asked_features" "$closure" | normalize_features)

    if [ "$PROV_DEFAULT" = "$want_default" ] && [ "$PROV_FEATURES" = "$want_features" ]; then
      exit 0
    fi

    printf '%s\n' "$elf was linked with default=$PROV_DEFAULT features=${PROV_FEATURES:-none}; you asked for default=$want_default features=${want_features:-none} (FEATURES=\"$asked_features\", NO_DEFAULT=${asked_no_default:-unset})" >&2
    exit 1
    ;;

  *)
    usage >&2
    exit 2
    ;;
esac
