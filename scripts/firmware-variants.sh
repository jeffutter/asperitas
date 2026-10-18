#!/usr/bin/env bash
#
# Build, lint and grade every firmware image variant listed in scripts/firmware-variants.table.
#
#   scripts/firmware-variants.sh build | clippy | grade   run that action over every row that lists it
#   scripts/firmware-variants.sh list                     print each row with its resolved binaries
#   scripts/firmware-variants.sh check                    fail unless every firmware/src/bin/*.rs is
#                                                         built, linted and graded by some row
#
# Each row builds into its own target directory (scripts/variant-dir.sh), so no variant replaces the
# ELF another one produced. `grade` only reads those ELFs; it never builds, so a missing one is a
# loud failure from scripts/check-image-load-addresses.sh rather than a stale image graded by luck.
# Run `build` first.
#
# Every row of the requested action is attempted and every failure reported at the end.
#
# Exit codes: 0 all rows passed, 1 a row failed, 2 the script or the table could not be used.

set -uo pipefail

ROOT=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT" || exit 2
TABLE=scripts/firmware-variants.table
PROG=firmware-variants

die() { printf '%s: %s\n' "$PROG" "$*" >&2; exit 2; }

usage() { sed -n '3,8p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//' >&2; }

[ $# -eq 1 ] || { usage; exit 2; }
ACTION=$1
case $ACTION in
  build | clippy | grade | list | check) ;;
  -h | --help) usage; exit 0 ;;
  *) usage; exit 2 ;;
esac

[ -f "$TABLE" ] || die "no $TABLE"

ALL_BINS=()
for src in firmware/src/bin/*.rs; do
  [ -e "$src" ] || continue
  name=${src##*/}
  ALL_BINS+=("${name%.rs}")
done
[ "${#ALL_BINS[@]}" -gt 0 ] || die "no firmware/src/bin/*.rs"

# Row fields, set by load_rows into parallel arrays.
R_LAYOUT=() R_RTT=() R_FEATURES=() R_TAG=() R_ENV=() R_BINS=() R_ACTIONS=() R_LINE=()
# R_RESOLVED holds the resolved binary names, space separated; R_ALLBINS is 1 for a bare `*`.
R_RESOLVED=() R_ALLBINS=()

has_word() { case ",$1," in *,"$2",*) return 0 ;; esac; return 1; }

resolve_bins() {
  local spec=$1 item b known out=() drop=()
  local -a items
  IFS=, read -r -a items <<<"$spec"
  for item in "${items[@]}"; do
    case $item in
      '*') out+=("${ALL_BINS[@]}") ;;
      -*) drop+=("${item#-}") ;;
      *) out+=("$item") ;;
    esac
  done
  for b in "${out[@]}"; do
    known=0
    for item in "${ALL_BINS[@]}"; do [ "$item" = "$b" ] && known=1; done
    [ "$known" = 1 ] || return 1
    for item in "${drop[@]+"${drop[@]}"}"; do [ "$item" = "$b" ] && continue 2; done
    printf '%s\n' "$b"
  done
}

load_rows() {
  local line n=0 layout rtt features tag env bins actions extra resolved a
  while IFS= read -r line || [ -n "$line" ]; do
    n=$((n + 1))
    line=${line%%#*}
    [ -n "${line//[[:space:]]/}" ] || continue
    read -r layout rtt features tag env bins actions extra <<<"$line"
    [ -n "$actions" ] && [ -z "$extra" ] || die "$TABLE:$n: want 7 columns, got: $line"
    case $layout in internal | bootloaded) ;; *) die "$TABLE:$n: layout must be internal or bootloaded" ;; esac
    case $rtt in 0 | 1) ;; *) die "$TABLE:$n: rtt must be 0 or 1" ;; esac
    for a in ${actions//,/ }; do
      case $a in build | clippy | grade | replay) ;; *) die "$TABLE:$n: unknown action '$a'" ;; esac
    done
    if has_word "$actions" replay && [ "$layout" != bootloaded ]; then
      die "$TABLE:$n: replay applies to the bootloaded layout only"
    fi
    resolved=$(resolve_bins "$bins") || die "$TABLE:$n: bins '$bins' names a binary with no firmware/src/bin/*.rs"
    [ -n "$resolved" ] || die "$TABLE:$n: bins '$bins' resolves to nothing"
    R_LAYOUT+=("$layout") R_RTT+=("$rtt") R_FEATURES+=("$features") R_TAG+=("$tag")
    R_ENV+=("$env") R_BINS+=("$bins") R_ACTIONS+=("$actions") R_LINE+=("$n")
    R_RESOLVED+=("$(printf '%s ' $resolved)")
    [ "$bins" = '*' ] && R_ALLBINS+=(1) || R_ALLBINS+=(0)
  done <"$ROOT/$TABLE"
  [ "${#R_LAYOUT[@]}" -gt 0 ] || die "$TABLE has no rows"
}
load_rows

dash_empty() { [ "$1" = - ] && printf '' || printf '%s' "$1"; }

# <row> -> prints the target dir relative to firmware/
row_dir() {
  local i=$1
  scripts/variant-dir.sh "${R_LAYOUT[$i]}" "${R_RTT[$i]}" "${R_FEATURES[$i]}" "$(dash_empty "${R_TAG[$i]}")"
}

row_label() {
  local i=$1 tag
  tag=$(dash_empty "${R_TAG[$i]}")
  printf '%s rtt=%s [%s]%s bins=%s' "${R_LAYOUT[$i]}" "${R_RTT[$i]}" "${R_FEATURES[$i]}" "${tag:+ $tag}" "${R_BINS[$i]}"
}

# Prints the env assignments for a row, one per line, always including the layout.
row_env() {
  local i=$1 pair
  printf 'ASPERITAS_LAYOUT=%s\n' "${R_LAYOUT[$i]}"
  if [ "${R_ENV[$i]}" != - ]; then
    for pair in ${R_ENV[$i]//,/ }; do printf '%s\n' "$pair"; done
  fi
}

failed=0
fail() { printf '%s: %s\n' "$PROG" "$*" >&2; failed=1; }

run_cargo() {
  local i=$1 sub=$2 b dir
  local -a envs=() args=()
  while IFS= read -r b; do envs+=("$b"); done < <(row_env "$i")
  dir=$(row_dir "$i") || return 1
  args=("$sub" --release --target-dir "${dir#firmware/}")
  [ "${R_RTT[$i]}" = 1 ] && args+=(--no-default-features)
  args+=(--features "${R_FEATURES[$i]}")
  if [ "$sub" = clippy ] && [ "${R_ALLBINS[$i]}" = 1 ]; then
    args+=(--bins)
  else
    for b in ${R_RESOLVED[$i]}; do args+=(--bin "$b"); done
  fi
  [ "$sub" = clippy ] && args+=(-- -D warnings)
  (cd firmware && env "${envs[@]}" cargo "${args[@]}")
}

check_replay() {
  local i=$1 elf addr
  elf=firmware/$(row_dir "$i")/thumbv7em-none-eabihf/release/rig
  [ -f "$elf" ] || { fail "no ELF at $elf to check the REPLAY staging in"; return; }
  # The replay staging must sit in the AXI tail above the image (memory-bootloaded.x AXI_TAIL,
  # 0x24078000..0x24080000), never in DTCM where plain .bss would put it.
  addr=$(rust-nm "$elf" 2>/dev/null | awk '$3 ~ /REPLAY/ && $2 ~ /^[dDbB]$/ {print $1; exit}')
  if [ -z "$addr" ]; then
    fail "no REPLAY symbol in $elf (rust-nm missing or symbol stripped)"
  elif [ $((16#$addr)) -lt $((0x24078000)) ] || [ $((16#$addr)) -ge $((0x24080000)) ]; then
    fail "REPLAY at 0x$addr is outside the AXI tail 0x24078000..0x24080000"
  else
    printf '%s\n' "REPLAY staging at 0x$addr (AXI tail)"
  fi
}

case $ACTION in
  list)
    for i in "${!R_LAYOUT[@]}"; do
      printf '%-34s %-26s %s\n' "$(row_label "$i")" "${R_ACTIONS[$i]}" "-> ${R_RESOLVED[$i]}"
    done
    ;;

  check)
    for b in "${ALL_BINS[@]}"; do
      for act in build clippy grade; do
        covered=0
        for i in "${!R_LAYOUT[@]}"; do
          has_word "${R_ACTIONS[$i]}" "$act" || continue
          case " ${R_RESOLVED[$i]}" in *" $b "*) covered=1 ;; esac
        done
        [ "$covered" = 1 ] || fail "firmware/src/bin/$b.rs has no $act row in $TABLE"
      done
    done
    ;;

  build | clippy | grade)
    for i in "${!R_LAYOUT[@]}"; do
      has_word "${R_ACTIONS[$i]}" "$ACTION" || continue
      printf '%s\n' "--- $ACTION: $(row_label "$i")"
      case $ACTION in
        build | clippy)
          run_cargo "$i" "$ACTION" || fail "cargo $ACTION failed for $(row_label "$i")"
          ;;
        grade)
          dir=firmware/$(row_dir "$i")
          # shellcheck disable=SC2086
          bash scripts/check-image-load-addresses.sh --target-dir "$dir" ${R_RESOLVED[$i]} ||
            fail "image check failed for $(row_label "$i")"
          has_word "${R_ACTIONS[$i]}" replay && check_replay "$i"
          ;;
      esac
    done
    ;;
esac

exit "$failed"
