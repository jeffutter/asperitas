#!/usr/bin/env bash
#
# Gate the one property that makes a plain `objcopy -O binary` image something a board can boot: no
# file-backed section may load outside flash.
#
# Three rules, and every number they compare against comes out of the build rather than from a list
# kept by hand -- because a list kept by hand is exactly what went wrong here:
#
#   R1  Every ALLOC + PROGBITS section's LMA lies inside the FLASH region parsed from
#       firmware/memory.x. This is the general rule, and it does not care which linker input put the
#       section there or what it is called.
#
#   R2  When `.sram1_bss` is present it is NOBITS -- objdump prints `BSS`. daisy-embassy tags its two
#       SAI DMA buffers with that name and firmware/memory.x places them `(NOLOAD)` in AXI SRAM. If a
#       dependency bump renames the section, or the SECTIONS rule stops matching its input sections,
#       the output section becomes an orphan again. Checking the name turns that into a failure named
#       by the thing that changed, instead of a coincidence that happens to hold.
#
#   R3  Each plain `-O binary` image is exactly `highest flash LMA end - FLASH origin` bytes, and the
#       lowest file-backed LMA is `FLASH origin`. Length equality proves nothing between the extremes
#       got dropped, which is what lets the old `--only-section` keep-list stay dead. It is also the
#       assertion that says `.defmt*` survived into the image without grepping the Makefile for a flag.
#
# WHY THIS IS LOAD-BEARING. Before TASK-059 put the `(NOLOAD)` rule in firmware/memory.x, `.sram1_bss`
# was one of those orphans, and `llvm-objcopy -O binary` writes a memory image spanning lowest to
# highest *load* address -- so `main.bin` came out at 469,763,536 bytes of mostly zeros. Nothing was
# red: `make build` succeeded, DFU reported a clean transfer, and the board booted whatever prefix of
# itself fit the flash budget. Arm documents the class as a scatter-file error (KA002145); Zephyr
# gates it with a hand-curated section allowlist (`extra_sections` in `scripts/twister`). We do the
# opposite on purpose -- one address range plus the MEMORY block -- because a hand-kept list of section
# names is precisely the shape that killed the keep-list.
#
# Out of scope, one line each. This validates ONE cfg set per run: whichever cross-build ran last,
# which `=== firmware ELF cfg provenance ===` in scripts/gates.sh names, and in every tier that runs
# the cross-build pair that set is the RTT-only one. Whether an ELF is stale belongs to
# `make -C firmware elf-check`. Which cfg set an ELF is belongs to scripts/elf-provenance.sh. Where its
# sections load belongs here. And the ban on re-adding a keep-list belongs to
# scripts/check-doc-artifact-names.sh pass 2, which greps the expansion of `make -n -C firmware build`
# for `--only-section`.
#
# Reads artifacts and builds nothing. That is why it calls the bare cargo-binutils shims
# (`rust-objdump`, `rust-objcopy`) and never `cargo objdump` / `cargo objcopy`: the cargo forms rebuild,
# and a rebuild after the cross-build pair silently re-points
# firmware/target/thumbv7em-none-eabihf/release/main at a different cfg set -- the hazard
# scripts/gates.sh's ordering rule 1 exists to prevent. A missing ELF is therefore exit 2 and not a
# skip: a skip path is how elf-check's own remedy hint became useless (TASK-062).
#
# Exit codes: 0 clean, 1 violations found (all of them, in one run), 2 the check itself could not run
# - no ELF at a derived path, no binutils shim on PATH, an ELF that is not elf32-littlearm, or a
# section whose objdump Type column this parser has not been told how to read. A check that cannot run
# never reports success, and never stays silent.

set -uo pipefail

PROG=check-image-load-addresses

ROOT=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT" || exit 2

CARGO_CONFIG=$ROOT/firmware/.cargo/config.toml
MEMORY_X=$ROOT/firmware/memory.x

VIOLATIONS=()
TMP=""

die() {
  printf '%s\n' "$PROG: $*" >&2
  exit 2
}

usage() {
  cat <<'EOF'
usage: scripts/check-image-load-addresses.sh [ELF...]

  no arguments  check the image of every firmware/src/bin/*.rs, read from
                firmware/target/thumbv7em-none-eabihf/release/
  ELF...        check just these: a path to an ELF, or a bare binary name resolved in that directory
EOF
}

hex8() { printf '0x%08x' "$1"; }
ltrim() { printf '%s' "${1#"${1%%[![:space:]]*}"}"; }
rtrim() { printf '%s' "${1%"${1##*[![:space:]]}"}"; }

# --------------------------------------------------------------------------- inputs, all derived

# The target triple comes from the same [build] line cargo reads, so the release directory this check
# walks is the one the build writes. A hardcoded path would keep pointing at thumbv7em after a move.
derive_release_dir() {
  local target
  [ -f "$CARGO_CONFIG" ] || die "no ${CARGO_CONFIG#"$ROOT"/} - the target triple lives in it"
  target=$(sed -nE 's/^[[:space:]]*target[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' "$CARGO_CONFIG" | head -1)
  [ -n "$target" ] || die "no [build] target = \"...\" line in ${CARGO_CONFIG#"$ROOT"/}"
  RELEASE_DIR=$ROOT/firmware/target/$target/release
}

# FLASH's extent is parsed from the linker script instead of hardcoded, because moving to the 8 MB QSPI
# region is a live plan in this project and an `0x08020000` constant would then be a gate that passes
# by lying. Accepts the K/M suffix LENGTH is written with.
parse_flash_region() {
  local line origin len unit
  [ -f "$MEMORY_X" ] || die "no ${MEMORY_X#"$ROOT"/} - the FLASH region every rule compares against lives in it"

  line=$(grep -m1 -E '^[[:space:]]*FLASH[[:space:]]*:[[:space:]]*ORIGIN' "$MEMORY_X") ||
    die "no FLASH region definition in ${MEMORY_X#"$ROOT"/}"

  origin=$(printf '%s' "$line" |
    sed -nE 's/.*ORIGIN[[:space:]]*=[[:space:]]*(0[xX][0-9a-fA-F]+).*/\1/p')
  len=$(printf '%s' "$line" |
    sed -nE 's/.*LENGTH[[:space:]]*=[[:space:]]*([0-9]+)([KkMm]?).*/\1 \2/p')
  [ -n "$origin" ] || die "cannot parse an ORIGIN out of: $line"
  [ -n "$len" ] || die "cannot parse a LENGTH out of: $line"

  read -r len unit <<<"$len"
  case $unit in
    [Kk]) len=$(( len * 1024 )) ;;
    [Mm]) len=$(( len * 1048576 )) ;;
    '') ;;
    *) die "unparsable LENGTH unit in: $line" ;;
  esac
  (( len > 0 )) || die "FLASH LENGTH parses as zero in: $line"

  FLASH_ORIGIN=$(( origin ))
  FLASH_LEN=$len
  FLASH_END=$(( FLASH_ORIGIN + FLASH_LEN ))
}

ELF_PATHS=()
ELF_NAMES=()

# One entry per firmware/src/bin/*.rs -- the same derivation scripts/check-doc-artifact-names.sh uses
# for legal image names, so a seventh bin joins this gate without anyone editing a list and "six"
# never appears here as a magic number.
derive_elfs() {
  local src name
  for src in "$ROOT"/firmware/src/bin/*.rs; do
    [ -e "$src" ] || continue
    name=${src##*/}
    name=${name%.rs}
    ELF_NAMES+=("$name")
    ELF_PATHS+=("$RELEASE_DIR/$name")
  done
  [ "${#ELF_PATHS[@]}" -gt 0 ] || die "no targets in firmware/src/bin/*.rs: there would be nothing to check"
}

# An ELF this check cannot read is a hard failure naming the command that produces it. Never
# `test -f ... || true`: a skip path is how elf-check's hint became useless (TASK-062).
require_elf() {
  local path=$1 name=$2
  [ -f "$path" ] || die "no ELF at ${path#"$ROOT"/} - run 'bash scripts/gates.sh push', or 'make -C firmware build-elf BINARY=$name FEATURES=\"seed3 log-defmt\" NO_DEFAULT=1'"
}

# --------------------------------------------------------------------------- objdump parsing

# Right-anchored, because section names legitimately contain spaces: a defmt interned-string section is
# named `.defmt.error.{"package":"...","tag":"a b",...}`, and left-to-right splitting shreds it. Size,
# VMA and LMA are always eight hex digits in an elf32 image, so anchoring on the three triples and
# letting the name be whatever precedes them is the only split that survives. Bash matches
# leftmost-longest, so the name capture keeps objdump's column padding and gets trimmed below --
# untrimmed, the `.sram1_bss` comparison in R2 never fires and the whole rule reports `absent` for
# every binary, which is a green run that checked nothing. Verified identical on bash 5.3.15 and 3.2.57.
ROW_RE='^[[:space:]]*[0-9]+[[:space:]]+(.*)[[:space:]]+([0-9a-fA-F]{8})[[:space:]]+([0-9a-fA-F]{8})[[:space:]]+([0-9a-fA-F]{8})[[:space:]]*(.*)$'

SEC_ROWS=()
SEC_NAMES=()
SEC_SIZES=()
SEC_LMAS=()
SEC_TYPES=()

read_sections() {
  local path=$1 out row name type
  SEC_ROWS=() SEC_NAMES=() SEC_SIZES=() SEC_LMAS=() SEC_TYPES=()

  # --show-lma is explicit, not assumed: llvm-objdump enables the LMA column by default only "unless
  # any section has different VMA and LMAs", so a positional parser otherwise depends on a column that
  # can vanish. Note also there is no readelf here to want instead: cargo-binutils ships objdump,
  # objcopy and readobj, and `readelf -W -S` has no LMA column at all anyway, because ELF section
  # headers carry no load address -- objdump reconstructs the column per section from each PT_LOAD's
  # p_paddr - p_vaddr delta.
  out=$(rust-objdump -h --show-lma "$path" 2>&1) ||
    die "rust-objdump failed on ${path#"$ROOT"/}: $(printf '%s' "$out" | sed -n 1p)"

  # The eight-hex-digit widths above come from this line, and thumbv7em is the only thing this repo
  # links. Refusing beats parsing a wider image as if it were a narrower one.
  printf '%s' "$out" | grep -q 'file format elf32-littlearm' ||
    die "${path#"$ROOT"/} is not elf32-littlearm, so the column widths this parser assumes do not apply to it"

  while IFS= read -r row; do
    [[ $row =~ $ROW_RE ]] || continue
    name=$(rtrim "${BASH_REMATCH[1]}")
    type=$(ltrim "${BASH_REMATCH[5]}")
    SEC_ROWS+=("$row")
    SEC_NAMES+=("$name")
    SEC_SIZES+=("0x${BASH_REMATCH[2]}")
    SEC_LMAS+=("0x${BASH_REMATCH[4]}")
    SEC_TYPES+=("$type")
  done <<<"$out"

  [ "${#SEC_NAMES[@]}" -gt 0 ] ||
    die "no section rows parsed from 'rust-objdump -h --show-lma ${path#"$ROOT"/}'"
}

# objdump's Type column is composed flags, not one token, and non-allocated rows print nothing at all.
# Closed set: TEXT or DATA means loaded and file-backed; BSS means loaded with no file content; empty
# or DEBUG means not loaded. Anything else refuses rather than guesses -- including a multi-word
# combination this script has not been told about -- because guessing here is how a gate starts passing
# vacuously. Today's six ELFs use exactly TEXT, DATA, BSS, DEBUG and empty.
#
# Sets FILE_BACKED. Reads $IDX, $SEC_NAMES, $SEC_ROWS and $CURRENT_ELF for its message.
FILE_BACKED=0
classify_type() {
  local type=$1 w seen_text=0 seen_data=0 seen_bss=0 seen_debug=0 unknown=0
  FILE_BACKED=0
  for w in $type; do
    case $w in
      TEXT) seen_text=1 ;;
      DATA) seen_data=1 ;;
      BSS) seen_bss=1 ;;
      DEBUG) seen_debug=1 ;;
      *) unknown=1 ;;
    esac
  done
  if (( unknown )); then
    die "unrecognized objdump section type '$type' for section '${SEC_NAMES[$IDX]}' in ${CURRENT_ELF#"$ROOT"/} (raw row:${SEC_ROWS[$IDX]})"
  fi
  if (( seen_debug )) && (( seen_text || seen_data || seen_bss )); then
    die "section '${SEC_NAMES[$IDX]}' in ${CURRENT_ELF#"$ROOT"/} is typed '$type': allocated and debug at once (raw row:${SEC_ROWS[$IDX]})"
  fi
  if (( seen_text || seen_data )); then
    if (( seen_bss )); then
      die "section '${SEC_NAMES[$IDX]}' in ${CURRENT_ELF#"$ROOT"/} is typed '$type': both file-backed and NOBITS (raw row:${SEC_ROWS[$IDX]})"
    fi
    FILE_BACKED=1
  fi
}

# --------------------------------------------------------------------------- one ELF, three rules

check_one_elf() {
  CURRENT_ELF=$1
  local name=$2 i lma size end
  local n_loaded=0 n_zero=0 zero_names=""
  local low="" high="" span_low="" span_high="" expected="" image=""
  local sram_note=".sram1_bss=absent" sram_found=0

  read_sections "$CURRENT_ELF"
  R1_FAILED=0

  # R1, and the water marks R3 needs.
  for (( IDX = 0; IDX < ${#SEC_NAMES[@]}; IDX++ )); do
    classify_type "${SEC_TYPES[$IDX]}"
    if (( ! FILE_BACKED )); then
      continue
    fi

    lma=${SEC_LMAS[$IDX]}
    size=${SEC_SIZES[$IDX]}
    end=$(( lma + size ))

    # Zero-size file-backed sections belong to no PT_LOAD and contribute nothing to the image. Today
    # that is `.gnu.sgstubs`, which sits ABOVE the real flash high-water mark: counting it breaks R3's
    # equality by 8 bytes on a correct build. Named in the summary line rather than hidden, so the
    # exclusion stays visible to whoever reads a green run.
    if (( size == 0 )); then
      zero_names+="${zero_names:+ }${SEC_NAMES[$IDX]}"
      n_zero=$(( n_zero + 1 ))
      continue
    fi

    # The span objcopy writes, wherever the sections load. Tracked before the flash test below so R3
    # still has a number once R1 has refused the layout.
    if [ -z "$span_low" ] || (( lma < span_low )); then span_low=$lma; fi
    if [ -z "$span_high" ] || (( end > span_high )); then span_high=$end; fi

    if (( lma < FLASH_ORIGIN || end > FLASH_END )); then
      VIOLATIONS+=("${name}: ${SEC_NAMES[$IDX]}: LMA $(hex8 "$lma")..$(hex8 "$end") is outside FLASH $(hex8 "$FLASH_ORIGIN")..$(hex8 "$FLASH_END") - a file-backed section loading outside flash makes 'objcopy -O binary' span the gap, which is the 469 MB image of TASK-059; give the section '(NOLOAD)' placement in firmware/memory.x, or a load address with AT>")
      R1_FAILED=1
      continue
    fi

    n_loaded=$(( n_loaded + 1 ))
    if [ -z "$low" ] || (( lma < low )); then low=$lma; fi
    if [ -z "$high" ] || (( end > high )); then high=$end; fi
  done

  # R2, reported as present-or-absent rather than implied: only the audio-bearing images carry the
  # section today, so printing it is what stops a reader assuming it held for every binary.
  for (( i = 0; i < ${#SEC_NAMES[@]}; i++ )); do
    [ "${SEC_NAMES[$i]}" = ".sram1_bss" ] || continue
    sram_found=1
    sram_note=$(printf '.sram1_bss=%s@%s(%d)' "${SEC_TYPES[$i]:-none}" \
      "$(hex8 "${SEC_LMAS[$i]}")" "${SEC_SIZES[$i]}")
    case ${SEC_TYPES[$i]} in
      BSS) ;;
      *)
        VIOLATIONS+=("${name}: .sram1_bss: typed '${SEC_TYPES[$i]}' rather than BSS - firmware/memory.x places it '(NOLOAD)', so a file-backed section by that name means the SECTIONS rule stopped matching daisy-embassy's input sections (renamed upstream, most likely) and rust-lld is placing an orphan with LMA == VMA again")
        ;;
    esac
    break
  done
  (( sram_found )) || sram_note=".sram1_bss=absent"

  # R3. Written only while R1 held: on a tree with a section far outside flash, a plain `-O binary`
  # writes several hundred megabytes per binary (469,763,536 bytes and 473 MB peak RSS for the one bad
  # `main` measured during planning), and the gate spends that once, by hand, not six times per push.
  # What it prints instead is the same length computed from the section table, which matched objcopy's
  # actual output to the byte on that ELF.
  #
  # The formula has been checked against reality rather than assumed: it equals the real image size on
  # all six ELFs in both cfg sets, twelve measurements. This branch cannot be provoked by patching a
  # linked ELF, because objdump derives the LMA column from PT_LOAD deltas (see read_sections), so any
  # hand-edited sh_size / sh_offset / p_filesz moves the reconstructed LMA and R1 refuses first.
  # Hand-emitted fixtures are what cover it, which is TASK-068.
  if [ -n "$high" ]; then
    expected=$(( high - FLASH_ORIGIN ))
  fi

  if (( R1_FAILED )); then
    image="not-written(sections-outside-flash)"
    if [ -n "$span_high" ]; then
      local would=$(( span_high - span_low ))
      VIOLATIONS+=("${name}: plain '-O binary' would write $would bytes ($(hex8 "$span_low")..$(hex8 "$span_high")) where the parts inside flash hold ${expected:-nothing} - the image spans from the lowest to the highest LOAD address, so this is the TASK-059 defect: a board flashed with it boots only the prefix that fit. Length computed from the section table; the image was not written here because R1 already named the section responsible.")
      image="not-written(would-be-$would)"
    fi
  elif [ -z "$low" ]; then
    die "no file-backed section inside flash in ${CURRENT_ELF#"$ROOT"/}: nothing here describes a bootable image"
  elif (( low != FLASH_ORIGIN )); then
    VIOLATIONS+=("${name}: lowest file-backed LMA $(hex8 "$low") is not the FLASH origin $(hex8 "$FLASH_ORIGIN") - the image would not begin where DFU loads it, and the length formula below is silently wrong")
    image="not-measured(bad-low-address)"
  else
    local tmp_bin=$TMP/${name}.bin actual
    rust-objcopy -O binary "$CURRENT_ELF" "$tmp_bin" 2>&1 | sed "s|^|$PROG: |" >&2
    [ -f "$tmp_bin" ] || die "rust-objcopy wrote no image for ${CURRENT_ELF#"$ROOT"/}"
    actual=$(wc -c <"$tmp_bin")
    actual=${actual//[[:space:]]/}
    rm -f "$tmp_bin"
    if (( actual != expected )); then
      VIOLATIONS+=("${name}: plain '-O binary' image is $actual bytes, expected $expected (highest flash LMA end $(hex8 "$high") minus the FLASH origin) - the image and the link disagree about how much of flash this image occupies, so what 'dfu-util -D' writes is not what linked")
    fi
    image=$actual
  fi

  # Printed even on a pass: a silent pass cannot be told apart from a pass that checked nothing.
  local low_s=none high_s=none
  [ -z "$low" ] || low_s=$(hex8 "$low")
  [ -z "$high" ] || high_s=$(hex8 "$high")
  printf '%-9s image=%-30s low=%s high=%s expected=%s loaded=%d skipped=%d excluded_zero_size=%s %s\n' \
    "$name" "$image" \
    "$low_s" "$high_s" \
    "${expected:-n/a}" "$n_loaded" \
    "$(( ${#SEC_NAMES[@]} - n_loaded - n_zero ))" \
    "${zero_names:-none}" "$sram_note"
}

# --------------------------------------------------------------------------- reporting

report() {
  [ "${#VIOLATIONS[@]}" -gt 0 ] || return 0

  {
    printf '%s\n' "image load-address check FAILED (${#VIOLATIONS[@]} violation(s))"
    printf '%s\n' "checked ${#ELF_PATHS[@]} ELF(s) against FLASH $(hex8 "$FLASH_ORIGIN")..$(hex8 "$FLASH_END"), $(( FLASH_LEN / 1024 ))K parsed from firmware/memory.x"
    printf '\n'
    printf '%s\n' "${VIOLATIONS[@]}"
    printf '\n'
    printf '%s\n' 'The rule is that a section with file content loads inside flash. Data that exists only in'
    printf '%s\n' 'RAM gets (NOLOAD) placement in firmware/memory.x, the way .sram1_bss has since TASK-059.'
  } >&2
  return 1
}

main() {
  local a i
  while [[ $# -gt 0 ]]; do
    case $1 in
      -h | --help) usage; exit 0 ;;
      --) shift; break ;;
      -*) printf '%s: unknown argument: %s\n\n' "$PROG" "$1" >&2; usage >&2; exit 2 ;;
      *) break ;;
    esac
  done

  parse_flash_region
  derive_release_dir
  derive_elfs

  # Explicit paths override the derived set, so the script is usable ad hoc against one mutated copy.
  if [ "$#" -gt 0 ]; then
    ELF_PATHS=()
    ELF_NAMES=()
    for a in "$@"; do
      if [[ $a == */* ]]; then
        ELF_PATHS+=("$a")
        ELF_NAMES+=("${a##*/}")
      else
        ELF_PATHS+=("$RELEASE_DIR/$a")
        ELF_NAMES+=("$a")
      fi
    done
  fi

  command -v rust-objdump >/dev/null 2>&1 ||
    die "no rust-objdump on PATH - this reads ELFs with cargo-binutils, available in nix develop .#default"
  command -v rust-objcopy >/dev/null 2>&1 ||
    die "no rust-objcopy on PATH - this reads ELFs with cargo-binutils, available in nix develop .#default"

  TMP=$(mktemp -d) || die "mktemp -d failed, so R3 has nowhere to write its test images"
  trap 'if [ -n "${TMP:-}" ]; then rm -rf "$TMP"; fi' EXIT

  printf 'checking %d ELF(s) against FLASH %s..%s (%dK from firmware/memory.x)\n' \
    "${#ELF_PATHS[@]}" "$(hex8 "$FLASH_ORIGIN")" "$(hex8 "$FLASH_END")" "$(( FLASH_LEN / 1024 ))"

  for (( i = 0; i < ${#ELF_PATHS[@]}; i++ )); do
    require_elf "${ELF_PATHS[$i]}" "${ELF_NAMES[$i]}"
    check_one_elf "${ELF_PATHS[$i]}" "${ELF_NAMES[$i]}"
  done

  report
}

IDX=0
R1_FAILED=0
CURRENT_ELF=""
main "$@"
exit $?
