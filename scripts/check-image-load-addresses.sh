#!/usr/bin/env bash
#
# Gate the one property that makes a plain `objcopy -O binary` image something a board can boot: no
# file-backed section may load outside flash.
#
# Three rules, and every number they compare against comes out of the build rather than from a list
# kept by hand -- because a list kept by hand is exactly what went wrong here:
#
#   R1  Every ALLOC + PROGBITS section's LMA lies inside the FLASH region parsed from
#       firmware/memory-internal.x. This is the general rule, and it does not care which linker input put the
#       section there or what it is called.
#
#   R2  When `.sram1_bss` is present it is NOBITS -- objdump prints `BSS`. daisy-embassy tags its two
#       SAI DMA buffers with that name and firmware/memory-internal.x places them `(NOLOAD)` in AXI SRAM. If a
#       dependency bump renames the section, or the SECTIONS rule stops matching its input sections,
#       the output section becomes an orphan again. Checking the name turns that into a failure named
#       by the thing that changed, instead of a coincidence that happens to hold.
#
#   R3  Each plain `-O binary` image is exactly `highest flash LMA end - FLASH origin` bytes, and the
#       lowest file-backed LMA is `FLASH origin`. Length equality proves nothing between the extremes
#       got dropped, which is what lets the old `--only-section` keep-list stay dead. It is also the
#       assertion that says `.defmt*` survived into the image without grepping the Makefile for a flag.
#
#   R4  A bootloaded image (see "Two layouts" below) is stored in QSPI from QSPI_APP_OFFSET, and the
#       excerpt slots start at EXCERPT_AREA_START. The region the linker may fill and the image that
#       came out must both end at or below that address. Both numbers are parsed, never restated:
#       QSPI_APP_OFFSET from the `QSPI_APP_OFFSET = ` marker in firmware/memory-bootloaded.x, and
#       EXCERPT_AREA_START from crates/asperitas-wire/src/excerpt.rs, which is where the slot layout
#       is defined.
#
#   R6  A bootloaded image leaves at least MIN_SPARE bytes of its FLASH region unused. The region is
#       the bootloader's copy budget, and an image that fills it to the byte is one dependency bump from
#       failing to link. 2 KB is the headroom the bootloader needs. Not applied to the internal
#       layout, where the linker's own overflow error is the existing (and tighter) contract.
#
#   R5  A bootloaded image's `.sram1_bss` lies inside the RAM_D2 region. AXI SRAM is image space there
#       and DMA1/DMA2 cannot reach DTCM, so the SAI buffers anywhere else stop audio with no error.
#
# Two layouts. firmware/build.rs links either memory-internal.x (FLASH = internal flash, DFU to
# 0x08000000) or memory-bootloaded.x (FLASH = the AXI SRAM the stock Daisy bootloader copies the QSPI
# image into). One ELF path serves both, so each ELF is graded against the layout its own lowest
# file-backed LMA falls in: a vector table at 0x24000000 is a bootloaded image, anything else is held to
# the internal-flash region and refused by R1 when it does not fit. Which layout an ELF was BUILT for is
# the provenance stamp's business (`make elf-check LAYOUT=...`), not this gate's.
#
# WHY THIS IS LOAD-BEARING. Before the `(NOLOAD)` rule in firmware/memory-internal.x, `.sram1_bss`
# was one of those orphans, and `llvm-objcopy -O binary` writes a memory image spanning lowest to
# highest *load* address -- so `main.bin` came out at 469,763,536 bytes of mostly zeros. Nothing was
# red: `make build` succeeded, DFU reported a clean transfer, and the board booted whatever prefix of
# itself fit the flash budget. Arm documents the class as a scatter-file error (KA002145); Zephyr
# gates it with a hand-curated section allowlist (`extra_sections` in `scripts/twister`). We do the
# opposite on purpose -- one address range plus the MEMORY block -- because a hand-kept list of section
# names is precisely the shape that killed the keep-list.
#
# Out of scope, one line each. This validates ONE build variant per run: the cargo target directory named by
# --target-dir (scripts/gates.sh passes the RTT-only one, the only variant that links all six
# binaries). Whether an ELF is stale belongs to
# `make -C firmware elf-check`. What an ELF's stamp says belongs to scripts/elf-provenance.sh. Where its
# sections load belongs here. And the ban on re-adding a keep-list belongs to
# scripts/check-doc-artifact-names.sh pass 2, which greps the expansion of `make -n -C firmware build`
# for `--only-section`.
#
# Reads artifacts and builds nothing. That is why it calls the bare cargo-binutils shims
# (`rust-objdump`, `rust-objcopy`) and never `cargo objdump` / `cargo objcopy`: the cargo forms rebuild,
# and an audit that builds is grading its own side effect. A missing ELF is therefore exit 2 and not a
# skip: a skip path is how elf-check's own remedy hint became useless.
#
# Three modes:
#
#   <no arguments>                              every firmware image, from the target directory's release dir
#   ELF...                                      just these ELFs
#   --selftest                                  assert this script's own parsing, exit 0/1/2
#
# Exit codes: 0 clean, 1 violations found (all of them, in one run), 2 the check itself could not run
# - no ELF at a derived path, no binutils shim on PATH, an ELF that is not elf32-littlearm, or a
# section whose objdump Type column this parser has not been told how to read. A check that cannot run
# never reports success, and never stays silent.
#
# `--selftest` grades the reading rather than the link: hand-emitted ELF images driven through the real
# rust-objdump and rust-objcopy, plus literal objdump rows for every trap the row regex and the Type
# column offer. Its codes are the same shape - 0 every case passed, 1 at least one case failed (all of
# them reported in the one run), 2 the suite could not run at all, which here means no shim on PATH or
# no firmware/memory-internal.x to parse a FLASH region out of. It builds nothing and writes nothing outside a
# `mktemp -d`.
#
# Cost rules the selftest obeys, because it runs inside the pre-commit hook:
#
#   C1  At most five child invocations of the whole script per run - one per exit code a gate can
#       observe, plus the three read guards. Everything else calls the parsed-section functions
#       in-process, where a fork is far cheaper than
#       what even the cheapest child pays for a fresh interpreter and its objdump.
#   C2  The fixture images are generated once, before the first case, and no case regenerates one.
#   C3  No `cargo objdump`, `cargo objcopy`, `cargo build`, `cargo metadata` or `make`, and no case
#       names a path under firmware/target/: those either rebuild or re-point the artifact the push-tier
#       gate audits. The bare shims only, for the reason already given above.
#   C4  Both shims are checked with `command -v` at the top of run_selftest, before a fixture is
#       written: a missing tool exits 2 rather than reporting zero cases as a pass.
#   C5  One `mktemp -d` per run, removed by the same EXIT-trap shape main() installs.

set -uo pipefail

PROG=check-image-load-addresses

ROOT=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT" || exit 2

CARGO_CONFIG=$ROOT/firmware/.cargo/config.toml
MEMORY_INTERNAL=$ROOT/firmware/memory-internal.x
MEMORY_BOOTLOADED=$ROOT/firmware/memory-bootloaded.x
EXCERPT_RS=$ROOT/crates/asperitas-wire/src/excerpt.rs

# Smallest slack a bootloaded image may leave in its FLASH region (R6).
MIN_SPARE=2048

VIOLATIONS=()
TMP=""

die() {
  printf '%s\n' "$PROG: $*" >&2
  exit 2
}

usage() {
  cat <<'EOF'
usage: scripts/check-image-load-addresses.sh [--target-dir DIR] [ELF...]
       scripts/check-image-load-addresses.sh --selftest

  no arguments  check the image of every firmware/src/bin/*.rs, read from
                <DIR>/thumbv7em-none-eabihf/release/ (DIR defaults to firmware/target; relative
                paths resolve against the repo root)
  ELF...        check just these: a path to an ELF, or a bare binary name resolved in that directory
  --selftest    assert this script's own parsing against hand-emitted fixtures, exit 0/1/2
EOF
}

hex8() { printf '0x%08x' "$1"; }
ltrim() { printf '%s' "${1#"${1%%[![:space:]]*}"}"; }
rtrim() { printf '%s' "${1%"${1##*[![:space:]]}"}"; }

# --------------------------------------------------------------------------- inputs, all derived

# The target triple comes from the same [build] line cargo reads, so the release directory this check
# walks is the one the build writes, inside the cargo target directory given by --target-dir (default
# firmware/target). A hardcoded path would keep pointing at thumbv7em after a move.
derive_release_dir() {
  local target
  local base=$ROOT/firmware/target
  [ -z "$TARGET_DIR" ] || base=$TARGET_DIR
  [ -f "$CARGO_CONFIG" ] || die "no ${CARGO_CONFIG#"$ROOT"/} - the target triple lives in it"
  target=$(sed -nE 's/^[[:space:]]*target[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' "$CARGO_CONFIG" | head -1)
  [ -n "$target" ] || die "no [build] target = \"...\" line in ${CARGO_CONFIG#"$ROOT"/}"
  RELEASE_DIR=$base/$target/release
}

# A region's extent is parsed from the linker script instead of hardcoded, because moving the image
# between internal flash, AXI SRAM and QSPI is a live plan in this project and a `0x08020000` constant
# would then be a gate that passes by lying. Accepts the K/M suffix LENGTH is written with.
# <file> <region-name> -> REGION_ORIGIN, REGION_LEN
parse_region() {
  local file=$1 region=$2 line origin len unit
  [ -f "$file" ] || die "no ${file#"$ROOT"/} - the $region region every rule compares against lives in it"

  line=$(grep -m1 -E "^[[:space:]]*${region}[[:space:]]*:[[:space:]]*ORIGIN" "$file") ||
    die "no $region region definition in ${file#"$ROOT"/}"

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
  (( len > 0 )) || die "$region LENGTH parses as zero in: $line"

  REGION_ORIGIN=$(( origin ))
  REGION_LEN=$len
}

# Both layouts' regions, the two QSPI numbers R4 needs, and the default layout (internal) selected so
# callers that never pick one behave as before. Sets INT_*, BOOT_*, D2_*, QSPI_APP_OFFSET,
# EXCERPT_AREA_START, and through select_layout the FLASH_* the rules read.
parse_flash_region() {
  local line

  parse_region "$MEMORY_INTERNAL" FLASH
  INT_ORIGIN=$REGION_ORIGIN INT_LEN=$REGION_LEN
  parse_region "$MEMORY_BOOTLOADED" FLASH
  BOOT_ORIGIN=$REGION_ORIGIN BOOT_LEN=$REGION_LEN
  parse_region "$MEMORY_BOOTLOADED" RAM_D2
  D2_ORIGIN=$REGION_ORIGIN D2_END=$(( REGION_ORIGIN + REGION_LEN ))

  line=$(grep -m1 -E 'QSPI_APP_OFFSET[[:space:]]*=[[:space:]]*0[xX][0-9a-fA-F]+' "$MEMORY_BOOTLOADED") ||
    die "no 'QSPI_APP_OFFSET = 0x...' marker in ${MEMORY_BOOTLOADED#"$ROOT"/}"
  QSPI_APP_OFFSET=$(( $(printf '%s' "$line" | sed -nE 's/.*QSPI_APP_OFFSET[[:space:]]*=[[:space:]]*(0[xX][0-9a-fA-F]+).*/\1/p') ))

  [ -f "$EXCERPT_RS" ] || die "no ${EXCERPT_RS#"$ROOT"/} - the excerpt area R4 keeps clear of the image is defined there"
  line=$(grep -m1 -E 'pub const EXCERPT_AREA_START:[[:space:]]*u32[[:space:]]*=' "$EXCERPT_RS") ||
    die "no 'pub const EXCERPT_AREA_START: u32 = ...' in ${EXCERPT_RS#"$ROOT"/}"
  line=$(printf '%s' "$line" | sed -nE 's/.*=[[:space:]]*(0[xX][0-9a-fA-F_]+)[[:space:]]*;.*/\1/p')
  [ -n "$line" ] || die "cannot parse a hex value out of EXCERPT_AREA_START in ${EXCERPT_RS#"$ROOT"/}"
  EXCERPT_AREA_START=$(( ${line//_/} ))

  select_layout internal
}

# <internal|bootloaded> -> LAYOUT_SEL and the FLASH_* every rule reads.
select_layout() {
  LAYOUT_SEL=$1
  if [ "$1" = bootloaded ]; then
    FLASH_ORIGIN=$BOOT_ORIGIN FLASH_LEN=$BOOT_LEN FLASH_FILE=firmware/memory-bootloaded.x
  else
    FLASH_ORIGIN=$INT_ORIGIN FLASH_LEN=$INT_LEN FLASH_FILE=firmware/memory-internal.x
  fi
  FLASH_END=$(( FLASH_ORIGIN + FLASH_LEN ))
}

ELF_PATHS=()
ELF_NAMES=()
TARGET_DIR=""

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
# `test -f ... || true`: a skip path is how elf-check's hint became useless.
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
  local path=$1 out

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

  parse_section_table "$out"
}

# The row loop apart from any ELF: objdump's text goes in, the parallel arrays come out. Splitting it
# from the tool call is what lets a case feed this parser output nobody linked, which is the only way to
# assert the four traps in ROW_RE and the Type column at all: none of them can be provoked by patching a
# linked ELF, because objdump derives the LMA column from PT_LOAD deltas, so any hand-edited sh_size /
# sh_offset / p_filesz moves the reconstructed LMA and R1 refuses first.
#
# Takes the text as one argument rather than on stdin because a pipe would run the loop in a subshell,
# and the arrays it fills would die there with it. Reports itself against CURRENT_ELF, which every
# caller sets first, so a refusal names a file whether the text came off a real ELF or off a fixture.
parse_section_table() {
  local text=$1 row name type
  SEC_ROWS=() SEC_NAMES=() SEC_SIZES=() SEC_LMAS=() SEC_TYPES=()

  while IFS= read -r row; do
    [[ $row =~ $ROW_RE ]] || continue
    name=$(rtrim "${BASH_REMATCH[1]}")
    type=$(ltrim "${BASH_REMATCH[5]}")
    SEC_ROWS+=("$row")
    SEC_NAMES+=("$name")
    SEC_SIZES+=("0x${BASH_REMATCH[2]}")
    SEC_LMAS+=("0x${BASH_REMATCH[4]}")
    SEC_TYPES+=("$type")
  done <<<"$text"

  # The door through which a green run means nothing: an objdump whose columns have moved matches no row
  # at all, and a table parsed as empty satisfies every rule below by having nothing to fail. Upstream
  # has broken `--show-lma` before (llvm/llvm-project 66228), so this is the loud death for that, and
  # `no-rows-parsed-is-fatal` is what says so.
  [ "${#SEC_NAMES[@]}" -gt 0 ] ||
    die "no section rows parsed from 'rust-objdump -h --show-lma ${CURRENT_ELF#"$ROOT"/}'"
}

# objdump's Type column is composed flags, not one token, and non-allocated rows print nothing at all.
# llvm-objdump builds it as `Type = Section.isText() ? "TEXT" : ""` then `Type += Type.empty() ? "DATA"
# : ", DATA"`, and the same again for BSS and DEBUG: a composed row arrives separated by COMMA AND SPACE
# (`DATA, DEBUG`), so the delimiter is folded to plain spaces below before the words are read. Two
# consequences worth knowing. DEBUG is decided by the section's NAME (`isDebugSection()` keys off the
# name for ELF, not off SHF_DEBUG), so an allocated section named `.debug_*` prints it. And `TEXT, DATA`
# cannot happen at all, because isData requires EXECINSTR clear, which is why `.text` prints plain TEXT
# despite being PROGBITS + ALLOC.
#
# Closed set: TEXT or DATA means loaded and file-backed; BSS means loaded with no file content; empty
# or DEBUG means not loaded. Anything else refuses rather than guesses -- including a multi-word
# combination this script has not been told about -- because guessing here is how a gate starts passing
# vacuously. Today's six ELFs use exactly TEXT, DATA, BSS, DEBUG and empty.
#
# Out of scope, deliberately: whether an ALLOCATED `.debug_*` should classify as file-backed and fall
# under R1 rather than refusing. Every `.debug_*` in today's images is unallocated and prints an empty
# Type, so the question is hypothetical; refusing loudly is the existing policy, and changing it belongs
# to whoever first meets the build that emits one.
#
# Sets FILE_BACKED. Reads $IDX, $SEC_NAMES, $SEC_ROWS and $CURRENT_ELF for its message.
FILE_BACKED=0
classify_type() {
  local type=${1//,/ } w seen_text=0 seen_data=0 seen_bss=0 seen_debug=0 unknown=0
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

# What one parsed section table says about an image. Globals rather than return values because every
# trap this script has is in the READING: a case fills these by handing parse_section_table objdump text
# nobody linked, then reads the verdict back with no ELF anywhere on disk. evaluate_sections resets each
# one of them, so a caller that parses fresh text inherits nothing from the ELF before it.
#
#   R1_FAILED             a file-backed section loads outside FLASH
#   LOW / HIGH            the extremes that decide the image length, over sections inside flash
#   SPAN_LOW / SPAN_HIGH  the same extremes over every file-backed section wherever it loads, which is
#                         what a plain `-O binary` would span on a layout R1 has refused
#   EXPECTED              HIGH - FLASH_ORIGIN: the byte count R3 compares objcopy against
#   N_LOADED              file-backed sections counted toward the image
#   N_ZERO / ZERO_NAMES   zero-size file-backed sections held out of that arithmetic, and named, so the
#                         exclusion stays visible in a green run rather than becoming a silent hole
#   SRAM_NOTE             R2's `.sram1_bss=...` field
#   IMAGE                 what the `image=` column says: a byte count, or why there is none
R1_FAILED=0
LOW=''
HIGH=''
SPAN_LOW=''
SPAN_HIGH=''
EXPECTED=''
N_LOADED=0
N_ZERO=0
ZERO_NAMES=''
SRAM_NOTE='.sram1_bss=absent'
IMAGE=''
SUMMARY_NAME=''

# The three rules, over whatever parse_section_table last filled in. Appends to VIOLATIONS, fills the
# block above, prints nothing -- one ELF's summary line is format_summary's job, so a case can grade the
# same numbers with no printf in the way.
evaluate_sections() { # <display-name>
  local name=$1 i lma size end sram_found=0

  R1_FAILED=0
  LOW='' HIGH='' SPAN_LOW='' SPAN_HIGH='' EXPECTED=''
  N_LOADED=0 N_ZERO=0 ZERO_NAMES=''
  SRAM_NOTE='.sram1_bss=absent'

  # Pick the layout this image belongs to from where its lowest file-backed section loads, before any
  # rule compares against a region. classify_type is run here once per row and again below; the second
  # pass is the one that reports.
  local first_low=''
  for (( IDX = 0; IDX < ${#SEC_NAMES[@]}; IDX++ )); do
    classify_type "${SEC_TYPES[$IDX]}"
    (( FILE_BACKED )) || continue
    size=${SEC_SIZES[$IDX]}
    (( size == 0 )) && continue
    lma=${SEC_LMAS[$IDX]}
    if [ -z "$first_low" ] || (( lma < first_low )); then first_low=$lma; fi
  done
  if [ -n "$first_low" ] && (( first_low >= BOOT_ORIGIN && first_low < BOOT_ORIGIN + BOOT_LEN )); then
    select_layout bootloaded
  else
    select_layout internal
  fi

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
      ZERO_NAMES+="${ZERO_NAMES:+ }${SEC_NAMES[$IDX]}"
      N_ZERO=$(( N_ZERO + 1 ))
      continue
    fi

    # The span objcopy writes, wherever the sections load. Tracked before the flash test below so R3
    # still has a number once R1 has refused the layout.
    if [ -z "$SPAN_LOW" ] || (( lma < SPAN_LOW )); then SPAN_LOW=$lma; fi
    if [ -z "$SPAN_HIGH" ] || (( end > SPAN_HIGH )); then SPAN_HIGH=$end; fi

    if (( lma < FLASH_ORIGIN || end > FLASH_END )); then
      VIOLATIONS+=("${name}: ${SEC_NAMES[$IDX]}: LMA $(hex8 "$lma")..$(hex8 "$end") is outside FLASH $(hex8 "$FLASH_ORIGIN")..$(hex8 "$FLASH_END") - a file-backed section loading outside flash makes 'objcopy -O binary' span the gap, which is the 469 MB image; give the section '(NOLOAD)' placement in $FLASH_FILE, or a load address with AT>")
      R1_FAILED=1
      continue
    fi

    N_LOADED=$(( N_LOADED + 1 ))
    if [ -z "$LOW" ] || (( lma < LOW )); then LOW=$lma; fi
    if [ -z "$HIGH" ] || (( end > HIGH )); then HIGH=$end; fi
  done

  # R2, reported as present-or-absent rather than implied: only the audio-bearing images carry the
  # section today, so printing it is what stops a reader assuming it held for every binary.
  for (( i = 0; i < ${#SEC_NAMES[@]}; i++ )); do
    [ "${SEC_NAMES[$i]}" = ".sram1_bss" ] || continue
    sram_found=1
    SRAM_NOTE=$(printf '.sram1_bss=%s@%s(%d)' "${SEC_TYPES[$i]:-none}" \
      "$(hex8 "${SEC_LMAS[$i]}")" "${SEC_SIZES[$i]}")
    case ${SEC_TYPES[$i]} in
      BSS) ;;
      *)
        VIOLATIONS+=("${name}: .sram1_bss: typed '${SEC_TYPES[$i]}' rather than BSS - firmware/memory-internal.x places it '(NOLOAD)', so a file-backed section by that name means the SECTIONS rule stopped matching daisy-embassy's input sections (renamed upstream, most likely) and rust-lld is placing an orphan with LMA == VMA again")
        ;;
    esac
    break
  done
  (( sram_found )) || SRAM_NOTE=".sram1_bss=absent"

  # R3's expectation. The formula has been checked against reality rather than assumed: it equals the
  # real image size on all six ELFs in both cfg sets, twelve measurements. It cannot be provoked by
  # patching a linked ELF, because objdump derives the LMA column from PT_LOAD deltas (see
  # read_sections), so any hand-edited sh_size / sh_offset / p_filesz moves the reconstructed LMA and R1
  # refuses first. Hand-emitted fixtures are what cover it, which are the self-test cases below.
  if [ -n "$HIGH" ]; then
    EXPECTED=$(( HIGH - FLASH_ORIGIN ))
  fi

  # R4 and R5 only mean something for the layout that is stored in QSPI and runs from AXI SRAM.
  if [ "$LAYOUT_SEL" = bootloaded ]; then
    if (( QSPI_APP_OFFSET + FLASH_LEN > EXCERPT_AREA_START )); then
      VIOLATIONS+=("${name}: the bootloaded FLASH region ($(( FLASH_LEN / 1024 ))K) stored at QSPI offset $(hex8 "$QSPI_APP_OFFSET") would end at $(hex8 $(( QSPI_APP_OFFSET + FLASH_LEN ))), past the excerpt area at $(hex8 "$EXCERPT_AREA_START") - shrink FLASH LENGTH in firmware/memory-bootloaded.x")
    fi
    if [ -n "$HIGH" ] && (( QSPI_APP_OFFSET + HIGH - FLASH_ORIGIN > EXCERPT_AREA_START )); then
      VIOLATIONS+=("${name}: the image stored at QSPI offset $(hex8 "$QSPI_APP_OFFSET") ends at $(hex8 $(( QSPI_APP_OFFSET + HIGH - FLASH_ORIGIN ))), past the excerpt area at $(hex8 "$EXCERPT_AREA_START") - installing excerpts would overwrite the app")
    fi
    if [ -n "$HIGH" ] && (( FLASH_END - HIGH < MIN_SPARE )); then
      VIOLATIONS+=("${name}: only $(( FLASH_END - HIGH )) bytes of the $(( FLASH_LEN / 1024 ))K bootloaded FLASH region are left (need at least $MIN_SPARE) - the next dependency bump would stop this image linking")
    fi
    for (( i = 0; i < ${#SEC_NAMES[@]}; i++ )); do
      [ "${SEC_NAMES[$i]}" = ".sram1_bss" ] || continue
      lma=${SEC_LMAS[$i]}
      size=${SEC_SIZES[$i]}
      end=$(( lma + size ))
      if (( lma < D2_ORIGIN || end > D2_END )); then
        VIOLATIONS+=("${name}: .sram1_bss at $(hex8 "$lma")..$(hex8 "$end") is outside RAM_D2 $(hex8 "$D2_ORIGIN")..$(hex8 "$D2_END") - AXI SRAM is image space in the bootloaded layout and DMA1/DMA2 cannot reach DTCM, so SAI audio would stop with no error")
      fi
      break
    done
  fi
}

# Everything the `image=` column says that needs no bytes on disk: the two refusals, and the die under
# them. Leaves IMAGE empty when the layout is sound, which is check_one_elf's cue to go measure it. Split
# out of check_one_elf so a case can grade the sentence the 469 MB defect is read from -- `plain '-O
# binary' would write N bytes` -- against rows rather than against a mutated link.
#
# The refusal is written only while R1 held, because on a tree with a section far outside flash a plain
# `-O binary` writes several hundred megabytes per binary (469,763,536 bytes and 473 MB peak RSS for the
# one bad `main` measured during planning). The gate spends that once, by hand, not six times per push;
# what it prints instead is the same length computed from the section table, which matched objcopy's
# actual output to the byte on that ELF.
decide_image() { # <display-name>
  local name=$1 would

  if (( R1_FAILED )); then
    IMAGE="not-written(sections-outside-flash)"
    if [ -n "$SPAN_HIGH" ]; then
      would=$(( SPAN_HIGH - SPAN_LOW ))
      VIOLATIONS+=("${name}: plain '-O binary' would write $would bytes ($(hex8 "$SPAN_LOW")..$(hex8 "$SPAN_HIGH")) where the parts inside flash hold ${EXPECTED:-nothing} - the image spans from the lowest to the highest LOAD address, so this is the 469 MB defect: a board flashed with it boots only the prefix that fit. Length computed from the section table; the image was not written here because R1 already named the section responsible.")
      IMAGE="not-written(would-be-$would)"
    fi
    return
  fi

  if [ -z "$LOW" ]; then
    die "no file-backed section inside flash in ${CURRENT_ELF#"$ROOT"/}: nothing here describes a bootable image"
  fi

  if (( LOW != FLASH_ORIGIN )); then
    VIOLATIONS+=("${name}: lowest file-backed LMA $(hex8 "$LOW") is not the FLASH origin $(hex8 "$FLASH_ORIGIN") - the image would not begin where DFU loads it, and the length formula below is silently wrong")
    IMAGE="not-measured(bad-low-address)"
  fi
}

# The one line this script prints per ELF, and it is printed even on a pass: a silent pass cannot be
# told apart from a pass that checked nothing. Returned rather than echoed by check_one_elf only in the
# sense that a case may capture it -- a child run and an in-process row set answer to the same string.
format_summary() {
  local low_s=none high_s=none spare_s=n/a
  [ -z "$LOW" ] || low_s=$(hex8 "$LOW")
  [ -z "$HIGH" ] || { high_s=$(hex8 "$HIGH"); spare_s=$(( FLASH_END - HIGH )); }
  printf '%-9s layout=%-10s image=%-30s low=%s high=%s spare=%s expected=%s loaded=%d skipped=%d excluded_zero_size=%s %s\n' \
    "$SUMMARY_NAME" "$LAYOUT_SEL" "$IMAGE" \
    "$low_s" "$high_s" "$spare_s" \
    "${EXPECTED:-n/a}" "$N_LOADED" \
    "$(( ${#SEC_NAMES[@]} - N_LOADED - N_ZERO ))" \
    "${ZERO_NAMES:-none}" "$SRAM_NOTE"
}

check_one_elf() {
  CURRENT_ELF=$1
  SUMMARY_NAME=$2
  local tmp_bin actual

  read_sections "$CURRENT_ELF"
  evaluate_sections "$SUMMARY_NAME"

  IMAGE=''
  decide_image "$SUMMARY_NAME"

  # R3 measured, for the one layout that survived every rule above: objcopy writes by LOAD address, so
  # the image it produces is the assertion that the LMA column this script parses positionally is the
  # column the linker meant.
  if [ -z "$IMAGE" ]; then
    tmp_bin=$TMP/$SUMMARY_NAME.bin
    rust-objcopy -O binary "$CURRENT_ELF" "$tmp_bin" 2>&1 | sed "s|^|$PROG: |" >&2
    [ -f "$tmp_bin" ] || die "rust-objcopy wrote no image for ${CURRENT_ELF#"$ROOT"/}"
    actual=$(wc -c <"$tmp_bin")
    actual=${actual//[[:space:]]/}
    rm -f "$tmp_bin"
    if (( actual != EXPECTED )); then
      VIOLATIONS+=("${SUMMARY_NAME}: plain '-O binary' image is $actual bytes, expected $EXPECTED (highest flash LMA end $(hex8 "$HIGH") minus the FLASH origin) - the image and the link disagree about how much of flash this image occupies, so what 'dfu-util -D' writes is not what linked")
    fi
    IMAGE=$actual
  fi

  format_summary
}

# --------------------------------------------------------------------------- reporting

report() {
  [ "${#VIOLATIONS[@]}" -gt 0 ] || return 0

  {
    printf '%s\n' "image load-address check FAILED (${#VIOLATIONS[@]} violation(s))"
    printf '%s\n' "checked ${#ELF_PATHS[@]} ELF(s); each is held to the FLASH region of the layout it loads in: internal $(hex8 "$INT_ORIGIN")..$(hex8 $(( INT_ORIGIN + INT_LEN ))) ($(( INT_LEN / 1024 ))K, firmware/memory-internal.x) or bootloaded $(hex8 "$BOOT_ORIGIN")..$(hex8 $(( BOOT_ORIGIN + BOOT_LEN ))) ($(( BOOT_LEN / 1024 ))K, firmware/memory-bootloaded.x)"
    printf '\n'
    printf '%s\n' "${VIOLATIONS[@]}"
    printf '\n'
    printf '%s\n' 'The rule is that a section with file content loads inside flash. Data that exists only in'
    printf '%s\n' 'RAM gets (NOLOAD) placement in firmware/memory-internal.x, the way .sram1_bss has since the rule landed.'
  } >&2
  return 1
}

# --------------------------------------------------------------------------- selftest
#
# Every rule above rests on one positional read of `rust-objdump -h --show-lma`, and a parser that stops
# working does not turn this gate red. It turns it green for the wrong reason. Four traps were measured
# while planning this gate, each of which produces a pass that checked nothing:
#
#   *  Bash matches leftmost-longest, so the name capture keeps objdump's column padding. Untrimmed, the
#      `.sram1_bss` comparison never fires and R2 answers `absent` for every binary -- which is what both
#      `main` and `rig` really print if the trim is dropped, with exit still 0. A prototype hit exactly
#      this bug during planning.
#   *  Section names legitimately contain spaces, braces and commas: defmt's interned-string sections are
#      named `.defmt.error.` followed by a JSON record. Any left-to-right field split shreds them.
#   *  The Type column is composed flags joined by ", ", and non-allocated rows print nothing at all. An
#      empty Type must mean not-loaded and an unrecognised word must be a hard error; get either wrong
#      and every ELF fails, or a section walks through the gate unnoticed.
#   *  `.gnu.sgstubs` is ALLOC + PROGBITS with size 0 at an address ABOVE the real flash high-water and in
#      no PT_LOAD. Counting it breaks R3's length equality by 8 bytes on a correct build.
#
# Nothing asserted any of them until this suite. The shape follows scripts/elf-provenance.sh's: fixtures
# generated at runtime, cases graded through real child processes where the exit code is the subject, and
# one prefixed line per case on stderr. The harness is copied rather than shared, for the reason that
# script already gives -- two small harnesses cost less than one harness with two callers.
#
# Why the fixtures are hand-emitted rather than the ELFs the cross-builds leave behind: the commit tier
# builds no firmware at all, so grading real artifacts here would cost a build this tier may not pay for. The same limit applies as there:
# fixtures cannot prove the linker emits anything like them, and the push-tier `=== image load addresses
# ===` gate over six linked ELFs stays the backstop for that. What the fixtures DO pin is the column:
# image OK below carries `.data` at VMA 0x24001000 and LMA 0x08000040, so the LMA column this script
# reads positionally decides R1 and R3 there, and `rust-objcopy` -- which writes by load address -- has
# to agree with the length computed from those rows for the case to pass. objdump and objcopy, not this
# generator, decide whether a fixture counts as an ELF at all.
#
# Generated at runtime, never committed as blobs: ~600 bytes of hex carries no information this
# generator does not carry more legibly, blobs would need a regeneration story of their own, and keeping
# the bytes beside the parser means a layout change moves both together.

# The image is assembled as HEX TEXT in one variable and converted to bytes exactly once, at the end, for
# the two reasons scripts/elf-provenance.sh records beside its own generator: a command substitution per
# field costs a fork, which the first draft paid 25 times per fixture before the first case ran, and
# bash cannot hold a NUL in a variable, which `.shstrtab`
# is made of. Hex text never contains a NUL, so only the one printf that writes the file ever produces
# the byte. `fx_emit`'s byte-count check is not ceremony: the ELF64 fixture below first came out 204
# bytes against a layout of 208, which is one `sh_entsize` written 4 wide where ELF64 wants 8 -- a bug no
# assertion in this file would have noticed, because nothing reads that field back.
#
# Verified on both shells this repo runs under, bash 3.2.57 (macOS system) and 5.3.15 (the devShell's):
# everything below is `printf -v`, arrays and `${s:i:1}`.

FX_HEX=''

fx_raw() { FX_HEX+=$1; }   # constant hex, for the fields whose value never changes

fx_put() { # <value> <byte-count>, little-endian
  local v=$1 n=$2 i t
  for ((i = 0; i < n; i++)); do
    printf -v t '%02x' $(((v >> (8 * i)) & 255))
    FX_HEX+=$t
  done
}

fx_pad() { local n=$1; while ((n > 0)); do FX_HEX+='00'; n=$((n - 1)); done; }

fx_text() { local s=$1 i t
  for ((i = 0; i < ${#s}; i++)); do
    printf -v t '%02x' "'${s:i:1}"
    FX_HEX+=$t
  done
}

fx_emit() { # <path> <planned-byte-count>
  local path=$1 want=$2 out='' i
  [ $(( ${#FX_HEX} / 2 )) -eq "$want" ] || die "assembled $(( ${#FX_HEX} / 2 )) bytes, layout says $want"
  for ((i = 0; i < ${#FX_HEX}; i += 2)); do out+="\\x${FX_HEX:i:2}"; done
  printf '%b' "$out" >"$path"
}

# A 616-byte ELF32 little-endian ARM image laid out so the rules above have something to say, with one
# knob: the p_paddr of the segment that covers `.data`, because that delta is exactly what objdump
# reconstructs the LMA column from. With the default the image PASSES every rule; with the paddr moved
# into RAM the same section loads outside flash and R1 refuses it. No linker is involved, and none is
# needed: `rust-objdump -h --show-lma` reads it as `file format elf32-littlearm` and `rust-objcopy -O
# binary` writes the 96 bytes the rules predict.
#
# Two facts baked into the numbers below, both measured rather than assumed. `.shstrtab` is built one
# `fx_pad 1` for index 0 plus name+NUL per entry, with offsets counted arithmetically, because bash
# truncates a string at a NUL -- an off-by-one there surfaces as "SHT_STRTAB string table section [index
# 6] is non-null terminated". And a section covered by no PT_LOAD prints LMA == VMA, which is why
# `.gnu.sgstubs` and `.sram1_bss` need no segment of their own to appear the way the rules see them.
FX_DATA_LMA_IN_FLASH=0x08000040
FX_DATA_LMA_IN_RAM=0x24001040

fx_arm_image() { # <path> [data-paddr]
  local path=$1 data_lma=${2:-$FX_DATA_LMA_IN_FLASH}
  local names=(.text .data .sram1_bss .gnu.sgstubs \
    '.defmt.error.{"package":"x","tag":"a b"}' .shstrtab)
  local nameoff=() n strlong=1 flash=0x08000000
  local eh=52 phsz=32 phnum=3 shesz=40 textsz=0x40 datasz=0x20 othersz=4
  local data_vma=0x24001000 bss_addr=0x24001020 sgs_addr=0x0801fff8
  local nsec phoff dataoff sgoff str_off shtoff

  # sh_name offsets into .shstrtab: index 0 is the NUL byte every entry is terminated with, so the
  # first name starts at 1 and each entry costs its length plus one. Counted arithmetically because the
  # table itself cannot live in a variable.
  for n in "${names[@]}"; do
    nameoff+=("$strlong")
    strlong=$(( strlong + ${#n} + 1 ))
  done

  phoff=$eh
  dataoff=$(( eh + phnum * phsz ))
  sgoff=$(( dataoff + textsz + datasz ))          # also the defmt-named section's content
  str_off=$(( sgoff + othersz ))
  shtoff=$(((str_off + strlong + 3) / 4 * 4))
  nsec=$(( ${#names[@]} + 1 ))

  FX_HEX=''
  fx_raw '7f454c4601010100'; fx_pad 8             # e_ident: magic, ELFCLASS32, little, current, SysV
  fx_put 2 2                                      # e_type = ET_EXEC
  fx_put 40 2                                     # e_machine = EM_ARM
  fx_put 1 4                                      # e_version
  fx_put "$flash" 4                               # e_entry: nothing here runs, but it must read as flash
  fx_put "$phoff" 4; fx_put "$shtoff" 4
  fx_put 0x5000200 4                              # e_flags = EF_ARM_ABIMASK | EF_ARM_ABI_VER5 | soft-fp
  fx_put "$eh" 2                                  # e_ehsize
  fx_put "$phsz" 2; fx_put "$phnum" 2
  fx_put "$shesz" 2; fx_put "$nsec" 2; fx_put $((nsec - 1)) 2   # shstrndx: the table is the last header

  # PT_LOAD 1: .text, p_paddr == p_vaddr.
  fx_put 1 4; fx_put "$dataoff" 4; fx_put "$flash" 4; fx_put "$flash" 4
  fx_put "$textsz" 4; fx_put "$textsz" 4; fx_put 5 4; fx_put 4 4
  # PT_LOAD 2: .data, and THE point of this fixture -- p_paddr differs from p_vaddr, so the LMA column
  # exists because of it and only because of it.
  fx_put 1 4; fx_put "$((dataoff + textsz))" 4; fx_put "$data_vma" 4; fx_put "$data_lma" 4
  fx_put "$datasz" 4; fx_put "$datasz" 4; fx_put 6 4; fx_put 4 4
  # PT_LOAD 3: .sram1_bss, NOBITS, so p_filesz is zero and its LMA equals its VMA.
  fx_put 1 4; fx_put "$((dataoff + textsz + datasz))" 4; fx_put "$bss_addr" 4; fx_put "$bss_addr" 4
  fx_put 0 4; fx_put 0x400 4; fx_put 6 4; fx_put 4 4

  fx_pad "$textsz"                                                    # .text content
  fx_pad "$datasz"                                                    # .data initializers
  fx_pad "$othersz"                                                   # the defmt-named section's content
  fx_pad 1; for n in "${names[@]}"; do fx_text "$n"; fx_pad 1; done   # .shstrtab
  fx_pad $((shtoff - str_off - strlong))

  # <name-index> <sh_type> <sh_flags> <sh_addr> <sh_offset> <sh_size>
  fx_sec() {
    fx_put "${nameoff[$1]}" 4; fx_put "$2" 4; fx_put "$3" 4; fx_put "$4" 4
    fx_put "$5" 4; fx_put "$6" 4; fx_put 0 4; fx_put 0 4; fx_put 4 4; fx_put 0 4
  }
  fx_pad 40                                            # section 0: the SHT_NULL reservation
  fx_sec 0 1 $((0x2 | 0x4)) "$flash"          "$dataoff"               "$textsz"    # .text  ALLOC|EXECINSTR
  fx_sec 1 1 $((0x2 | 0x1)) "$data_vma"       "$((dataoff + textsz))"  "$datasz"    # .data  ALLOC|WRITE
  fx_sec 2 8 $((0x2 | 0x1)) "$bss_addr"       "$((dataoff + textsz + datasz))" 0x400 # .sram1_bss NOBITS
  fx_sec 3 1 0x2            "$sgs_addr"       "$sgoff"                 0            # zero-size ALLOC
  fx_sec 4 1 0              0                 "$sgoff"                 "$othersz"   # defmt name, no flags
  fx_sec 5 3 0              0                 "$str_off"               "$strlong"   # .shstrtab

  fx_emit "$path" $((shtoff + shesz * nsec))
}

# A valid ELF that is not elf32-littlearm -- ELF64 little-endian aarch64, two section headers, no
# program headers -- so the width guard is graded by a file this repo produced rather than by whatever
# the host happens to have in /bin. llvm-objdump prints `file format elf64-littleaarch64` for it on every
# platform, which is the sentence the guard greps for the absence of.
fx_elf64_image() { # <path>
  local path=$1
  local strtab='002e736873747274616200' strlen=11 eh=64 shesz=64 stroff shoff nsec=2

  stroff=$eh
  shoff=$(((stroff + strlen + 7) / 8 * 8))

  FX_HEX=''
  fx_raw '7f454c46'; fx_raw '02010100'; fx_pad 8  # e_ident: magic, ELFCLASS64, little, current, then padding
  fx_put 2 2                                      # e_type = ET_EXEC
  fx_put 0xb7 2                                   # e_machine = EM_AARCH64, the thing being refused
  fx_put 1 4                                      # e_version
  fx_put 0 8                                      # e_entry
  fx_put 0 8                                      # e_phoff: no program headers
  fx_put "$shoff" 8                               # e_shoff
  fx_put 0 4                                      # e_flags
  fx_put "$eh" 2; fx_put 0 2; fx_put 0 2          # e_ehsize, e_phentsize/phnum
  fx_put "$shesz" 2; fx_put "$nsec" 2; fx_put 1 2 # e_shentsize, e_shnum, e_shstrndx
  fx_raw "$strtab"
  fx_pad $((shoff - stroff - strlen))
  fx_pad 64                                       # section 0: the SHT_NULL reservation
  fx_put 1 4; fx_put 3 4; fx_put 0 8; fx_put 0 8  # .shstrtab: SHT_STRTAB, unallocated
  # The two 8-byte fields are what makes this ELF64 as far as objdump cares; the class byte above is
  # what the width guard actually refuses.
  fx_put "$stroff" 8; fx_put "$strlen" 8; fx_put 0 4; fx_put 0 4; fx_put 1 8; fx_put 0 8

  fx_emit "$path" $((shoff + shesz * nsec))
}

# One temp dir for the whole run, removed on any exit, and deliberately not under firmware/target/ so a
# selftest can never be mistaken for build residue the push-tier gate might read (rule C5).
fx_make_dir() {
  FX_DIR=$(mktemp -d) || die "mktemp -d failed, so the selftest has nowhere to write its fixtures"
  trap 'if [ -n "${FX_DIR:-}" ]; then rm -rf "$FX_DIR"; fi' EXIT
  FX_SELF=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/$(basename -- "${BASH_SOURCE[0]}")
  FX_OK=$FX_DIR/lma-in-flash.elf
  FX_BAD=$FX_DIR/lma-in-ram.elf
  FX_FOREIGN=$FX_DIR/aarch64.elf
  FX_JUNK=$FX_DIR/not-an-object.bin
  # Row-level cases name themselves this in every refusal message. It is written empty: nothing here
  # reads it, but a message that names a file which never existed reads like a second bug.
  FX_ROWS=$FX_DIR/rows.elf
  : >"$FX_ROWS"
}

# --------------------------------------------------------------------------- assertions

ST_TOTAL=0
ST_FAILED=0

# One assertion failing ends its case and nothing more, and it has to EXIT rather than return: run_case
# reads each case through a command substitution, so a function that merely returned 1 had its status
# overwritten by whatever assertion ran last. Same finding scripts/elf-provenance.sh:456-460 records.
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

expect_has() { # <what> <needle> <haystack>   - fixed-string, because these contain regex punctuation
  local what=$1 needle=$2 hay=$3
  printf '%s' "$hay" | grep -qF -- "$needle" || fail "$what: [$needle] not present in [$hay]"
}

expect_refused() { # <what> <exit-code-seen> <output> <needle-in-output>
  local what=$1 rc=$2 out=$3 needle=$4
  [ "$rc" = 2 ] || fail "$what: exited $rc, wanted 2 (could-not-run), output: [$out]"
  expect_has "$what output" "$needle" "$out"
}

# Run this script the way a gate line does, and keep what a shell would see: the exit code that actually
# reaches scripts/gates.sh, and everything on either stream. Used sparingly -- rule C1.
spawn_script() {
  SPAWN_OUT=$(bash "$FX_SELF" "$@" 2>&1)
  SPAWN_RC=$?
}

# Feed objdump text to the parser as if a real ELF had produced it, with the state a real call would have
# left behind: a current ELF for messages, no stale verdicts, and a display name for the summary.
load_rows() { # <objdump text>
  VIOLATIONS=()
  IMAGE=''
  CURRENT_ELF=$FX_ROWS
  SUMMARY_NAME=fixture
  parse_section_table "$1"
}

violations_text() {
  local v out=''
  [ "${#VIOLATIONS[@]}" -gt 0 ] || return 0
  for v in "${VIOLATIONS[@]}"; do
    [ -z "$out" ] || out+=$'\n'
    out+=$v
  done
  printf '%s' "$out"
}

expect_violations() { # <want-count>
  [ "${#VIOLATIONS[@]}" -eq "$1" ] ||
    fail "wanted $1 violation(s), got ${#VIOLATIONS[@]}: [$(violations_text)]"
}

# --------------------------------------------------------------------------- literal objdump rows

# Real `rust-objdump -h --show-lma` output on the fixture above, with `.sram1_bss` re-addressed to the
# value `main` carries today. Pasted rather than typed because the column padding IS one of the traps
# under test: spacing invented here would drift from what objdump emits, and
# `padded-name-trims-to-exact-match` asserts the padding is really in the row it parses.
ROWS_WIDE='  0                                          00000000 00000000 00000000
  1 .text                                    00000040 08000000 08000000 TEXT
  2 .data                                    00000020 24001000 08000040 DATA
  3 .sram1_bss                               00000400 24000ce0 24000ce0 BSS
  4 .gnu.sgstubs                             00000000 0801fff8 0801fff8 DATA
  5 .defmt.error.{"package":"x","tag":"a b"} 00000004 00000000 00000000
  6 .shstrtab                                00000058 00000000 00000000'

# --------------------------------------------------------------------------- the cases

# --- the fixtures, through the real tools and real child processes --------------------------

# The case that makes the positional LMA parse non-vacuous. `.data` runs from AXI SRAM and loads from
# flash, so every number below comes out of the LMA column: capture the VMA instead and this goes red
# naming `.data` with a 469 MB span, which is the 469 MB defect reproduced by a fixture. The objcopy
# equality pins the same column from the other side, since objcopy writes by load address.
case_real_elf_lma_column() {
  spawn_script "$FX_OK"
  [ "$SPAWN_RC" = 0 ] || fail "the passing fixture exited $SPAWN_RC: $SPAWN_OUT"
  expect_has "image length" 'image=96' "$SPAWN_OUT"
  expect_has "high water comes from .data's LMA, not its VMA" 'high=0x08000060' "$SPAWN_OUT"
  expect_has "objcopy agrees with the section table" 'expected=96' "$SPAWN_OUT"
  expect_has "zero-size ALLOC excluded, and named" 'excluded_zero_size=.gnu.sgstubs' "$SPAWN_OUT"
  expect_has ".sram1_bss located" '.sram1_bss=BSS@0x24001020(1024)' "$SPAWN_OUT"
}

# Same bytes, one different p_paddr. The section still runs from RAM, so only the LOAD address moved, and
# that is the whole rule.
case_real_elf_lma_outside_flash() {
  local would=$(( 0x24001060 - 0x08000000 ))
  spawn_script "$FX_BAD"
  [ "$SPAWN_RC" = 1 ] || fail "the refusing fixture exited $SPAWN_RC, wanted 1: $SPAWN_OUT"
  expect_has "refusal names the section" '.data: LMA 0x24001040..0x24001060' "$SPAWN_OUT"
  expect_has "and the region it left" 'is outside FLASH 0x08000000..0x08020000' "$SPAWN_OUT"
  expect_has "prices the image objcopy would have written" "would write $would bytes" "$SPAWN_OUT"
  expect_has "and proves objcopy never ran" 'image=not-written(would-be-' "$SPAWN_OUT"
}

case_guard_not_an_object_file() {
  spawn_script "$FX_JUNK"
  expect_refused "a file that is not an ELF" "$SPAWN_RC" "$SPAWN_OUT" "rust-objdump failed on"
}

# Not /bin/ls: the host's binaries differ by platform (Mach-O on macOS, ELF64 x86 here), and the point is
# the width guard, which needs an ELF this repo controls to be graded at all.
case_guard_wrong_format() {
  spawn_script "$FX_FOREIGN"
  expect_refused "a valid ELF of another width" "$SPAWN_RC" "$SPAWN_OUT" "is not elf32-littlearm"
}

# The affordance: a missing ELF is a hard failure that names the command producing it,
# never a skip.
case_guard_missing_elf() {
  spawn_script "$FX_DIR/never-written.elf"
  expect_refused "an ELF that was never built" "$SPAWN_RC" "$SPAWN_OUT" "no ELF at $FX_DIR/never-written.elf"
  expect_has "says how to produce it" 'make -C firmware build-elf BINARY=never-written' "$SPAWN_OUT"
}

# --- the four traps, at row level ------------------------------------------------------------

case_padded_name_trims_to_exact_match() {
  local i=-1 s
  load_rows "$ROWS_WIDE"
  for (( i = 0; i < ${#SEC_NAMES[@]}; i++ )); do
    [ "${SEC_NAMES[$i]}" = '.sram1_bss' ] && break
  done
  (( i < ${#SEC_NAMES[@]} )) ||
    fail "no row parsed as exactly .sram1_bss; got [${SEC_NAMES[*]}]"
  # Without this the case could pass on a row that had no padding to lose, which would leave the trim
  # untested and the bug it prevents unfixed.
  expect_has "that row really carries objdump's column padding" '.sram1_bss  ' "${SEC_ROWS[$i]}"
  evaluate_sections fixture
  IMAGE=96
  s=$(format_summary)
  expect_has "so R2 finds the section" '.sram1_bss=BSS@0x24000ce0(1024)' "$s"
  expect_has "and the exclusion still names itself" 'excluded_zero_size=.gnu.sgstubs' "$s"
}

case_defmt_json_name_survives() {
  local rows='  1 .defmt.error.{"package":"x","tag":"a b"} 00000004 00000000 00000000
  2 .defmt.error.{"package":"y","tag":"c d"} 00000004 08000100 08000100 DATA'
  load_rows "$rows"
  expect_eq "json name parsed with its spaces and braces intact" \
    '.defmt.error.{"package":"x","tag":"a b"}' "${SEC_NAMES[0]}"
  expect_eq "an unallocated row prints no type" '' "${SEC_TYPES[0]}"
  IDX=0
  classify_type "${SEC_TYPES[0]}"
  (( ! FILE_BACKED )) || fail "an unallocated defmt row classified as file-backed"
  expect_eq "the same name with a type still parses whole" \
    '.defmt.error.{"package":"y","tag":"c d"}' "${SEC_NAMES[1]}"
  expect_eq "and the type came along with it" 'DATA' "${SEC_TYPES[1]}"
  IDX=1
  classify_type "${SEC_TYPES[1]}"
  (( FILE_BACKED )) || fail "a DATA-typed defmt row classified as not loaded"
}

# A composed Type is a refusal, never a skip, and the refusal says which section and which row. Folding
# the comma delimiter is what lets the semantic branch below fire rather than the unknown-word one; both
# refuse, and this pins that neither one is reachable by accident.
case_composed_type_refused_by_name() {
  local out rc
  load_rows '  1 .debug_alloc                               00000020 08000100 08000100 DATA, DEBUG'
  expect_eq "delimiter kept as objdump prints it" 'DATA, DEBUG' "${SEC_TYPES[0]}"
  IDX=0
  out=$(classify_type "${SEC_TYPES[0]}" 2>&1)
  rc=$?
  expect_refused "the refusal names the file it was reading" "$rc" "$out" "$FX_ROWS"
  expect_refused "and the section responsible" "$rc" "$out" "section '.debug_alloc'"
  expect_has "quotes the raw row it choked on" "raw row:${SEC_ROWS[0]}" "$out"
}

case_unknown_type_word_names_section_and_row() {
  local out rc
  load_rows '  1 .weird                                     00000020 08000100 08000100 FOO'
  IDX=0
  out=$(classify_type "${SEC_TYPES[0]}" 2>&1)
  rc=$?
  expect_refused "unrecognized Type word" "$rc" "$out" "section '.weird'"
  expect_has "names the raw row too" "raw row:${SEC_ROWS[0]}" "$out"
}

case_empty_type_is_not_loaded() {
  local rows='  1 .text                                    00000040 08000000 08000000 TEXT
  2 .note_thing                              00000010 0800ff00 0800ff00'
  load_rows "$rows"
  IDX=1
  classify_type "${SEC_TYPES[1]}"
  (( ! FILE_BACKED )) || fail "an empty Type classified as loaded"
  evaluate_sections fixture
  expect_violations 0
  expect_eq "only the TEXT row counts toward the image" 1 "$N_LOADED"
  expect_eq "the empty-Type row contributes nothing to the length" 64 "$EXPECTED"
  expect_eq "and shows up as skipped" 1 "$(( ${#SEC_NAMES[@]} - N_LOADED - N_ZERO ))"
}

case_zero_size_alloc_above_highwater_excluded() {
  local rows='  1 .text                                    00000040 08000000 08000000 TEXT
  2 .data                                    00000020 08000040 08000040 DATA
  3 .gnu.sgstubs                             00000000 0801fff8 0801fff8 DATA'
  load_rows "$rows"
  evaluate_sections fixture
  expect_violations 0
  # Count the zero-size row and HIGH becomes 0x0801fff8, so EXPECTED goes from 96 to 131064 and R3 calls
  # a correct build a lie by 8 bytes. That is the whole reason the exclusion exists.
  expect_eq "high water ignores it" $(( 0x08000060 )) "$HIGH"
  expect_eq "so the expected length is the real one" 96 "$EXPECTED"
  expect_eq "one section excluded" 1 "$N_ZERO"
  expect_eq "named rather than hidden" '.gnu.sgstubs' "$ZERO_NAMES"
}

case_sram1_bss_file_backed_violation() {
  local rows='  1 .text                                    00000040 08000000 08000000 TEXT
  2 .sram1_bss                               00000400 08000100 08000100 DATA' out
  load_rows "$rows"
  evaluate_sections fixture
  expect_violations 1
  out=$(violations_text)
  expect_has "R2 says which section changed shape" ".sram1_bss: typed 'DATA' rather than BSS" "$out"
  expect_has "and what that means upstream" 'SECTIONS rule stopped matching' "$out"
  expect_eq "the summary still reports what it saw" '.sram1_bss=DATA@0x08000100(1024)' "$SRAM_NOTE"
}

case_sram1_bss_absent_note() {
  load_rows '  1 .text                                    00000040 08000000 08000000 TEXT'
  evaluate_sections fixture
  expect_eq "reported absent rather than implied present" '.sram1_bss=absent' "$SRAM_NOTE"
}

# A name that merely CONTAINS the section is not the section. Without this case, weakening the
# comparison to a substring match keeps every other case green -- `main` prints `absent` either way,
# because no real binary has a second name holding the string -- so the exactness R2 depends on would be
# unasserted.
case_sram1_bss_name_must_match_exactly() {
  load_rows '  1 .text                                    00000040 08000000 08000000 TEXT
  2 .sram1_bss_copy                          00000400 24000ce0 24000ce0 BSS'
  evaluate_sections fixture
  expect_eq "a name that only contains .sram1_bss is not the section" '.sram1_bss=absent' "$SRAM_NOTE"
}

case_section_outside_flash_violation() {
  local rows='  1 .text                                    00000040 08000000 08000000 TEXT
  2 .oops                                    00000020 24001000 24001000 DATA' out
  load_rows "$rows"
  evaluate_sections fixture
  expect_eq "R1 flagged the layout" 1 "$R1_FAILED"
  expect_violations 1
  out=$(violations_text)
  expect_has "names the section responsible" '.oops: LMA 0x24001000..0x24001020' "$out"
  expect_has "against the region parsed from memory-internal.x" 'is outside FLASH 0x08000000..0x08020000' "$out"
  expect_eq "the in-flash part is still measured" 64 "$EXPECTED"
  decide_image fixture
  expect_eq "and no image is claimed, because objcopy must not run" \
    "not-written(would-be-$(( 0x24001020 - 0x08000000 )))" "$IMAGE"
}

case_lowest_lma_must_be_flash_origin() {
  local out
  load_rows '  1 .text                                    00000040 08000100 08000100 TEXT'
  evaluate_sections fixture
  expect_violations 0
  decide_image fixture
  expect_violations 1
  out=$(violations_text)
  expect_has "refuses the length formula, not just the address" \
    'lowest file-backed LMA 0x08000100 is not the FLASH origin 0x08000000' "$out"
  expect_eq "so no length is reported" 'not-measured(bad-low-address)' "$IMAGE"
}

# The bootloaded layout, graded from rows the way the internal one is. `.data` loads from the AXI SRAM
# image just after `.text` and runs from DTCM; the SAI buffers sit in D2. A vector table at 0x24000000
# is what selects the layout, so no knob names it.
ROWS_BOOTLOADED='  1 .vector_table                            00000298 24000000 24000000 DATA
  2 .text                                    00019ac4 24000298 24000298 TEXT
  3 .data                                    00000198 20000000 2401e948 DATA
  4 .bss                                     000031d0 20000198 20000198 BSS
  5 .sram1_bss                               00000400 30000000 30000000 BSS'

case_bootloaded_layout_selected_and_clean() {
  load_rows "$ROWS_BOOTLOADED"
  evaluate_sections fixture
  expect_violations 0
  expect_eq "layout follows the lowest file-backed LMA" bootloaded "$LAYOUT_SEL"
  expect_eq "image length is measured from the AXI SRAM origin" $(( 0x2401e948 + 0x198 - 0x24000000 )) "$EXPECTED"
  expect_eq ".sram1_bss in D2 is reported where it is" '.sram1_bss=BSS@0x30000000(1024)' "$SRAM_NOTE"
  load_rows '  1 .text                                    00000040 08000000 08000000 TEXT'
  evaluate_sections fixture
  expect_eq "an internal-flash image goes back to the internal layout" internal "$LAYOUT_SEL"
}

# R5. Same image with the buffers left in AXI SRAM, which is where the internal layout puts them and
# where the bootloader's copy now lands: DMA would work, the image would be overwritten under it.
case_bootloaded_sram1_bss_outside_d2() {
  local out
  load_rows "${ROWS_BOOTLOADED/30000000 30000000/20003368 20003368}"
  evaluate_sections fixture
  expect_violations 1
  out=$(violations_text)
  expect_has "names the section and the region it left" '.sram1_bss at 0x20003368..0x20003768 is outside RAM_D2 0x30000000..0x30048000' "$out"
}

# R6. The same image with one section pushed to within 1 KB of the end of the region.
case_bootloaded_needs_spare() {
  local out
  load_rows "${ROWS_BOOTLOADED/2401e948 DATA/24077c00 DATA}"
  evaluate_sections fixture
  expect_violations 1
  out=$(violations_text)
  expect_has "names the slack and the requirement" 'bytes of the 480K bootloaded FLASH region are left (need at least 2048)' "$out"
  load_rows "$ROWS_BOOTLOADED"
  evaluate_sections fixture
  expect_violations 0
}

# R4, both halves. The constants are the real parsed ones, moved by assignment inside the case (each case
# runs in its own subshell) to the state where the image would reach the excerpt area.
case_bootloaded_image_must_end_below_excerpt_area() {
  local out
  load_rows "$ROWS_BOOTLOADED"
  evaluate_sections fixture
  expect_violations 0
  expect_eq "the real layout leaves the excerpt area clear" 1 $(( QSPI_APP_OFFSET + BOOT_LEN <= EXCERPT_AREA_START ))
  EXCERPT_AREA_START=$(( QSPI_APP_OFFSET + 0x10000 ))
  evaluate_sections fixture
  expect_violations 2
  out=$(violations_text)
  expect_has "region-level refusal" 'would end at' "$out"
  expect_has "image-level refusal" 'installing excerpts would overwrite the app' "$out"
}

case_no_rows_parsed_is_fatal() {
  local out rc
  CURRENT_ELF=$FX_ROWS
  out=$(parse_section_table 'Idx Name            Size     VMA              Type
nothing here is a section row' 2>&1)
  rc=$?
  expect_refused "objdump text with no rows in it" "$rc" "$out" "no section rows parsed"
}

# --------------------------------------------------------------------------- running them

run_selftest() {
  # Rule C4: refuse before generating anything, so a missing tool is never zero cases reported as a pass.
  command -v rust-objdump >/dev/null 2>&1 \
    || die "no rust-objdump on PATH - the selftest reads its fixtures with cargo-binutils, available in nix develop .#default"
  command -v rust-objcopy >/dev/null 2>&1 \
    || die "no rust-objcopy on PATH - the selftest reads its fixtures with cargo-binutils, available in nix develop .#default"

  # Every case compares against the real FLASH region parsed from firmware/memory-internal.x. There is no knob for
  # faking it: a parameter to inject a region would be exactly the decision this gate exists to refuse.
  parse_flash_region
  fx_make_dir

  fx_arm_image "$FX_OK" || die "could not emit the passing fixture into $FX_DIR"
  fx_arm_image "$FX_BAD" "$FX_DATA_LMA_IN_RAM" || die "could not emit the refusing fixture into $FX_DIR"
  fx_elf64_image "$FX_FOREIGN" || die "could not emit the foreign-width fixture into $FX_DIR"
  printf 'not an ELF, and never was\n' >"$FX_JUNK"

  run_case real-elf-lma-column case_real_elf_lma_column
  run_case real-elf-lma-outside-flash case_real_elf_lma_outside_flash
  run_case guard-not-an-object-file case_guard_not_an_object_file
  run_case guard-wrong-format case_guard_wrong_format
  run_case guard-missing-elf case_guard_missing_elf

  run_case padded-name-trims-to-exact-match case_padded_name_trims_to_exact_match
  run_case defmt-json-name-survives case_defmt_json_name_survives
  run_case composed-type-refused-by-name case_composed_type_refused_by_name
  run_case unknown-type-word-names-section-and-row case_unknown_type_word_names_section_and_row
  run_case empty-type-is-not-loaded case_empty_type_is_not_loaded
  run_case zero-size-alloc-above-highwater-excluded case_zero_size_alloc_above_highwater_excluded
  run_case sram1-bss-file-backed-violation case_sram1_bss_file_backed_violation
  run_case sram1-bss-absent-note case_sram1_bss_absent_note
  run_case sram1-bss-name-must-match-exactly case_sram1_bss_name_must_match_exactly
  run_case section-outside-flash-violation case_section_outside_flash_violation
  run_case lowest-lma-must-be-flash-origin case_lowest_lma_must_be_flash_origin
  run_case no-rows-parsed-is-fatal case_no_rows_parsed_is_fatal
  run_case bootloaded-layout-selected-and-clean case_bootloaded_layout_selected_and_clean
  run_case bootloaded-sram1-bss-outside-d2 case_bootloaded_sram1_bss_outside_d2
  run_case bootloaded-needs-spare case_bootloaded_needs_spare
  run_case bootloaded-image-ends-below-excerpt-area case_bootloaded_image_must_end_below_excerpt_area

  if [ "$ST_FAILED" = 0 ]; then
    printf '%s\n' "$PROG: selftest passed ($ST_TOTAL cases)" >&2
    return 0
  fi
  printf '%s\n' "$PROG: selftest FAILED ($ST_FAILED of $ST_TOTAL cases), see lines above" >&2
  return 1
}

main() {
  local a i
  while [[ $# -gt 0 ]]; do
    case $1 in
      -h | --help) usage; exit 0 ;;
      # Ahead of the positional handling: `--selftest` is a mode, not an ELF to go looking for.
      --selftest)
        [ $# -eq 1 ] || { usage >&2; exit 2; }
        run_selftest
        exit $?
        ;;
      --target-dir)
        [ $# -ge 2 ] || { usage >&2; exit 2; }
        case $2 in /*) TARGET_DIR=$2 ;; *) TARGET_DIR=$ROOT/$2 ;; esac
        shift 2
        ;;
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

  printf 'checking %d ELF(s) against FLASH internal %s..%s (%dK) or bootloaded %s..%s (%dK), QSPI app offset %s, excerpts from %s\n' \
    "${#ELF_PATHS[@]}" "$(hex8 "$INT_ORIGIN")" "$(hex8 $(( INT_ORIGIN + INT_LEN )))" "$(( INT_LEN / 1024 ))" \
    "$(hex8 "$BOOT_ORIGIN")" "$(hex8 $(( BOOT_ORIGIN + BOOT_LEN )))" "$(( BOOT_LEN / 1024 ))" \
    "$(hex8 "$QSPI_APP_OFFSET")" "$(hex8 "$EXCERPT_AREA_START")"

  for (( i = 0; i < ${#ELF_PATHS[@]}; i++ )); do
    require_elf "${ELF_PATHS[$i]}" "${ELF_NAMES[$i]}"
    check_one_elf "${ELF_PATHS[$i]}" "${ELF_NAMES[$i]}"
  done

  report
}

# Parsed-section state lives with evaluate_sections, where resetting it belongs. These three are the
# per-process seeds: IDX is what classify_type reads to name the offending row, CURRENT_ELF what its
# messages name as the file, and R1_FAILED starts each ELF clean.
IDX=0
R1_FAILED=0
CURRENT_ELF=""
main "$@"
exit $?
