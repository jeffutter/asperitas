#!/usr/bin/env bash
#
# Read the provenance an ELF carries, and print its embedded source digest.
#
# The answer travels inside the image. firmware/build.rs writes a non-allocated `.asp.prov` note
# section holding `key=value` lines led by the format tag `asp-prov1`; build.rs defines the format,
# and this script is the ONLY thing in the repo that reads it. Nothing here restates more of the
# format than the tag and the keys, and nothing else should learn to parse the blob.
#
# Which variant an ELF is (layout, log mode, feature set) is not decided here: every variant builds
# into its own cargo target directory, named by scripts/variant-dir.sh, so the path says it. What the
# blob adds is the source digest, which firmware/Makefile's `elf-check` compares against the sources
# on disk so `probe-log` refuses an ELF that is not built from them.
#
# Modes:
#
#   show       <elf>                          print the normalized provenance on stdout, exit 0
#   digest     <elf>                          print the embedded source digest (exit 2 if the ELF has none)
#   --selftest                                assert this script's own logic, exit 0/1/2
#
# DEFMT_LOG is reported by `show` and nothing compares it: it selects which frames got compiled in,
# not which image this is.
#
# Exit codes: 0 success (for `--selftest`: every case passed), 1 for `--selftest` only: at least one
# case failed, and every failure is reported in the one run rather than stopping at the first, 2 the
# check itself could not run - no ELF at that path, no .asp.prov section, a blob this version cannot
# parse, or, for `--selftest`, no toolchain to run its cases against. A check that cannot run never
# reports success, and never stays silent.
#
# `--selftest` builds nothing and touches nothing outside a `mktemp -d`: the fixtures it grades are
# hand-emitted ELF images, generated at runtime beside the parser they test so a format change moves
# both together. See the section further down for why the fixtures are fake and what that does and
# does not cover.
#
# Coupling is mutual with firmware/Makefile: its `elf-check` target calls this. scripts/check-doc-
# artifact-names.sh already reaches down into the Makefile to enumerate legal image names; this is
# the Makefile reaching up into scripts/, so both headers say so.

set -uo pipefail

PROG=elf-provenance
TAG=asp-prov1

ROOT=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)

die() {
  printf '%s\n' "$PROG: $*" >&2
  exit 2
}

usage() {
  cat <<'EOF'
usage: scripts/elf-provenance.sh show <elf>
       scripts/elf-provenance.sh digest <elf>
       scripts/elf-provenance.sh --selftest
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
  # this is easy to miss: all a reader sees is a timestamp moving. That timestamp is no
  # longer what firmware/Makefile's staleness test reads -- elf-check hashes bytes now, so an inert
  # rewrite of identical bytes defeats nothing -- but the rule it was written for stands on its own:
  # don't mutate the artifact you are auditing. The ELF is the host's only copy of the symbols
  # describing whatever the bench is running, and objcopy rewriting it is not something a read command
  # gets to do as a side effect.
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
  PROV_DEFAULT=""
  PROV_FEATURES=""
  PROV_DEFMT_LOG=""
  PROV_SOURCE_DIGEST=""
  # Absent from an ELF older than the layout field, and every one of those linked into internal flash.
  PROV_LAYOUT="internal"
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
      layout) PROV_LAYOUT=$value ;;
      source_digest) PROV_SOURCE_DIGEST=$value ;; # optional: absent from an ELF older than the source digest
      *) : ;; # unknown key: forward-compatible, ignore it rather than refuse a newer stamp
    esac
  done <<<"$blob"

  [ "$seen_default" = 1 ] || die "stamped ELF has no default= line: the blob is truncated or malformed"
  [ "$seen_features" = 1 ] || die "stamped ELF has no features= line: the blob is truncated or malformed"
}

# --------------------------------------------------------------------------- selftest
#
# Why these fixtures are fake, and what that does and does not cover.
#
# Synthetic images cost no build, so the reader is graded in the commit tier. What fixtures cannot
# prove is that firmware/build.rs emits these bytes; producer drift shows up in the staleness check
# (scripts/check-elf-staleness.sh stubs this reader) and at the bench when `make elf-check` reads a real
# ELF.
#
# Fixtures are generated at runtime rather than committed as blobs: nine stamps' worth of a ~340-byte
# file carries no information this generator does not carry more legibly, they would need a
# regeneration story, and keeping the bytes beside the parser means a format change moves both
# together. Only three of them ever reach disk - the two real images and one file that is not an ELF -
# because the rest exist to be parsed. The case `blob-survives-the-container` is what stops the
# generator marking its own homework: it reads a generated image back through rust-objcopy and requires
# the bytes out to equal the bytes in, so objcopy - not this script - decides whether the container
# counts as an ELF.
#
# Cost rules, because this runs inside the pre-commit hook:
#
#   C1  At most a few child invocations of the whole script - one per exit code a caller can observe
#       plus the read guard case. Everything else calls these functions in-process, where a non-zero
#       code is caught by running the call through a command substitution: a bare fork is far cheaper
#       than an objcopy.
#   C2  A case reads a fixture once and then works on the bytes it got, rather than re-opening a file
#       to test arithmetic. Nothing opens the same image twice inside one process.
#   C3  No `cargo build`, `cargo clippy`, `cargo objcopy`, `cargo objdump` or `make`, and no path under
#       firmware/target/: those either rebuild or touch an artifact other gates read.
#
# What the suite still pays is the tools themselves: a handful of rust-objcopy invocations, every one
# of them the subject of a case rather than scaffolding. Making the suite cheaper means making it
# read fewer real tools, which means testing less.

# The image is assembled as HEX TEXT in one variable and converted to bytes exactly once, at the end.
# Two reasons, both learned the expensive way here.
#
# Cost: every command substitution in bash is a fork. The first version of the generator spelled its
# fields as $(fx_le32 7) and friends, which cost 25 forks per fixture - tens of milliseconds spent
# before a
# single case had run, against a whole-gate budget of a few hundred. Writing into FX_HEX uses printf
# -v, which is a builtin assignment and forks nothing.
#
# Correctness: bash cannot hold a NUL in a variable - $'\0...' truncates the string silently, which in
# an earlier draft emptied .shstrtab and bought back "SHT_STRTAB ... is empty" from objcopy. Hex text
# never contains a NUL, so the assembly can live in a variable and only the one printf that writes the
# file ever produces the byte. That is also why the helpers below emit hex rather than bytes: a helper
# that returned bytes would have to be called through a substitution to be composed, which is exactly
# what costs the fork.
#
# Both shells this repo runs under are verified: bash 3.2.57 (the macOS system shell, which gates.sh
# itself refuses) and 5.3.15 (the devShell's).

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

fx_text() { # ASCII text; every blob here is printable because build.rs stamps lowercase feature names
            # and a DEFMT_LOG value, and `printf "'%c"` would answer with something else for a multibyte
            # character
  local s=$1 i t
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

# `.shstrtab` contents: NUL ".asp.prov" NUL ".shstrtab" NUL, so the two names start at offsets 1
# and 11.
FX_STRTAB_HEX='002e6173702e70726f76002e736873747274616200'
FX_STRTAB_LEN=21
FX_NAME_PROV=1
FX_NAME_STRTAB=11

# Emit a minimal ELF64 whose only content section is a non-allocated SHT_NOTE named `.asp.prov` -
# ET_EXEC, EM_AARCH64, zero program headers, and three section headers (SHT_NULL, the note, the
# string table). `sh_flags = 0` on the note is the load-bearing part: build.rs emits the blob as a
# non-allocated note precisely so the linker never puts it in a segment a flasher would copy, so a
# fixture that marked it allocated would be testing a shape the producer never makes.
#
# `nostamp` drops the section outright - 2 headers, shstrndx still pointing at the last one - which is
# how "an ELF older than the stamp" gets represented without leaving a dangling sh_name.
#
# Widths are the ELF64 ones: a 64-byte file header, 64-byte section headers, everything 8-aligned. The
# blob is followed by one NUL because build.rs writes it through `.asciz`, which terminates what it
# emits; read_blob strips that NUL again with `tr -d '\0'`, so a fixture without one would leave the
# strip untested and the byte-for-byte comparison further down grading a shape no producer makes.
fx_elf() { # <path> <blob> [nostamp]
  local path=$1 blob=$2 nostamp=${3:-}
  local nsec bloblen stroff shoff

  if [ -n "$nostamp" ]; then
    nsec=2; bloblen=0
  else
    nsec=3; bloblen=$(( ${#blob} + 1 ))
  fi
  stroff=$((64 + bloblen))
  shoff=$(((stroff + FX_STRTAB_LEN + 7) / 8 * 8))

  FX_HEX=''
  fx_raw '7f454c46'                                             # e_ident: the ELF magic
  fx_raw '02'                                                   # EI_CLASS = ELFCLASS64
  fx_raw '01'                                                   # EI_DATA = little-endian
  fx_raw '01'                                                   # EI_VERSION = EV_CURRENT
  fx_raw '00'                                                   # EI_OSABI = none
  fx_pad 8                                                      # EI_ABIVERSION and the padding
  fx_raw '0200'                                                 # e_type = ET_EXEC
  fx_raw 'b700'                                                 # e_machine = EM_AARCH64
  fx_raw '01000000'                                             # e_version
  fx_put 0 8                                                    # e_entry: nothing here runs
  fx_put 0 8                                                    # e_phoff: no program headers at all
  fx_put "$shoff" 8                                             # e_shoff
  fx_put 0 4                                                    # e_flags
  fx_raw '4000'                                                 # e_ehsize = 64
  fx_raw '0000'                                                 # e_phentsize
  fx_raw '0000'                                                 # e_phnum
  fx_raw '4000'                                                 # e_shentsize = 64
  fx_put "$nsec" 2                                              # e_shnum
  fx_put $((nsec - 1)) 2                                        # e_shstrndx: the table is the last header

  if [ "$bloblen" -gt 0 ]; then
    fx_text "$blob"
    fx_put 0 1                                                  # the NUL `.asciz` appends
  fi
  fx_raw "$FX_STRTAB_HEX"
  fx_pad $((shoff - stroff - FX_STRTAB_LEN))

  fx_pad 64                                                     # section 0: the SHT_NULL reservation

  if [ -z "$nostamp" ]; then                                    # section 1: .asp.prov
    fx_put "$FX_NAME_PROV" 4                                    # sh_name
    fx_put 7 4                                                  # sh_type = SHT_NOTE
    fx_put 0 8                                                  # sh_flags = 0, see above: not allocated
    fx_put 0 8                                                  # sh_addr: VMA 0, like defmt's own .defmt
    fx_put 64 8                                                 # sh_offset: straight after the header
    fx_put "$bloblen" 8                                         # sh_size
    fx_put 0 4                                                  # sh_link
    fx_put 0 4                                                  # sh_info
    fx_put 4 8                                                  # sh_addralign
    fx_put 0 8                                                  # sh_entsize
  fi

  fx_put "$FX_NAME_STRTAB" 4                                    # last section: .shstrtab
  fx_put 3 4                                                    # sh_type = SHT_STRTAB
  fx_put 0 8                                                    # sh_flags
  fx_put 0 8                                                    # sh_addr
  fx_put "$stroff" 8                                            # sh_offset
  fx_put "$FX_STRTAB_LEN" 8                                     # sh_size
  fx_put 0 4                                                    # sh_link
  fx_put 0 4                                                    # sh_info
  fx_put 1 8                                                    # sh_addralign
  fx_put 0 8                                                    # sh_entsize

  fx_emit "$path" $((shoff + 64 * nsec))
}

# The stamps themselves, spelled the way build.rs writes them: tag line, then `key=value` lines, then
# a trailing newline (the real blob ends in the NUL of an assembler `.asciz`, which read_blob strips).
BLOB_CONSOLE='asp-prov1
default=1
features=log_usb,seed3
defmt_log=
'
# Derived variants, built by editing the literal rather than writing it twice: each is one field away
# from a stamp above, which is what makes them realistic mutations instead of invented ones.
BLOB_FOREIGN_TAG=${BLOB_CONSOLE/asp-prov1/asp-prov2}          # a stamp from a build.rs this script predates
BLOB_NO_FEATURES='asp-prov1
default=1
'
BLOB_NO_DEFAULT='asp-prov1
features=seed3
'
BLOB_WITH_DIGEST=${BLOB_CONSOLE/defmt_log=/source_digest=0123abcd
defmt_log=}
BLOB_FUTURE_KEY=${BLOB_CONSOLE/defmt_log=/boot_stage=seven
}
BLOB_BOOTLOADED=${BLOB_CONSOLE/defmt_log=/defmt_log=
layout=bootloaded}

# One temp dir for the whole run, removed on any exit. Deliberately not under firmware/target/, so a
# selftest can never be mistaken for build residue the provenance gate might read.
fx_make_dir() {
  FX_DIR=$(mktemp -d) || die "mktemp -d failed, so the selftest has nowhere to write its fixtures"
  trap 'if [ -n "${FX_DIR:-}" ]; then rm -rf "$FX_DIR"; fi' EXIT
  FX_SELF=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/$(basename -- "${BASH_SOURCE[0]}")
  FX_CONSOLE=$FX_DIR/console.elf
  FX_DIGESTED=$FX_DIR/digested.elf
  FX_NOSTAMP=$FX_DIR/nostamp.elf
  FX_JUNK=$FX_DIR/not-an-object.bin
}

# --------------------------------------------------------------------------- assertions

ST_TOTAL=0
ST_FAILED=0

# One assertion failing ends its case and nothing more. It has to EXIT rather than RETURN because
# run_case reads each case through a command substitution, and a function that merely returned 1 here
# had its status overwritten by whatever assertion ran last -- measured by breaking normalize_features
# so five cases genuinely disagreed and the suite still reported nineteen passing.
fail() { printf '%s\n' "$*" >&2; exit 1; }

# A case is a function that returns 0, or fails having printed one reason. Nothing short-circuits:
# AC #1 asks for every failure in one run, and the shape is copied from dump_reassemble's `selftest::
# run` (`crates/asperitas-host/src/bin/dump_reassemble.rs`) - one prefixed line per case on
# stderr, stdout staying reserved for `show`'s payload.
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

expect_has() { # <what> <needle> <haystack>   - fixed-string, because half of these contain regex punctuation
  local what=$1 needle=$2 hay=$3
  printf '%s' "$hay" | grep -qF -- "$needle" || fail "$what: [$needle] not present in [$hay]"
}

expect_refused() { # <what> <exit-code-seen> <output> <needle-in-output>
  local what=$1 rc=$2 out=$3 needle=$4
  [ "$rc" = 2 ] || fail "$what: exited $rc, wanted 2 (could-not-run), output: [$out]"
  expect_has "$what output" "$needle" "$out"
}

# Run this script as its callers do, and keep what a shell would see: the exit code that actually
# reaches a Makefile recipe or a gate line, and everything on either stream. Used sparingly - C2.
spawn_script() {
  SPAWN_OUT=$(bash "$FX_SELF" "$@" 2>&1)
  SPAWN_RC=$?
}

# sha256 of a file. Two spellings because the devShell carries coreutils' sha256sum and a bare macOS
# login shell only has shasum; run_selftest refuses up front if neither is there.
sha_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum -- "$1" | cut -d' ' -f1
  else shasum -a 256 -- "$1" | cut -d' ' -f1; fi
}

# A failing assertion exits the case and nothing else: run_case reads each case through a command
# substitution, so `exit 1` dies inside that subshell and run_case turns it into one FAILED line while
# the remaining cases still run.

# --------------------------------------------------------------------------- the cases

# --- the CLI contract, graded through real child processes -------------------------------

case_digest_exits_zero_on_a_stamped_image() {
  spawn_script digest "$FX_DIGESTED"
  [ "$SPAWN_RC" = 0 ] || fail "digest on a stamped image exited $SPAWN_RC: $SPAWN_OUT"
  expect_eq "digest printed" 0123abcd "$SPAWN_OUT"
}

case_cannot_run_exits_two() {
  spawn_script digest "$FX_DIR/no-such-file.elf"
  expect_refused "missing ELF" "$SPAWN_RC" "$SPAWN_OUT" "no ELF at $FX_DIR/no-such-file.elf"
  spawn_script nonsense-mode
  expect_refused "unknown mode" "$SPAWN_RC" "$SPAWN_OUT" "usage: scripts/elf-provenance.sh"
  spawn_script digest "$FX_CONSOLE"
  expect_refused "an image with no source digest" "$SPAWN_RC" "$SPAWN_OUT" "has no source_digest"
}

# --- the reader and the parser ------------------------------------------------------------

case_blob_survives_the_container() {
  local f=$FX_DIR/read-back.bin want=$FX_DIR/expected.bin err=$FX_DIR/read-error.txt diffout
  # Compared as files, not variables: $(read_blob) would strip the trailing newline the stamp ends
  # with and then pass on a blob that had lost it. This is the case that makes every literal above a
  # faithful stand-in for an image - objcopy, not this generator, agrees the container is readable.
  read_blob "$FX_CONSOLE" >"$f" 2>"$err" || fail "rust-objcopy refused the hand-emitted ELF: $(sed -n '1p' "$err")"
  printf '%s' "$BLOB_CONSOLE" >"$want"
  diffout=$(cmp "$f" "$want" 2>&1) || fail "bytes read back differ from the bytes written: $diffout"
}

case_parse_all_three_fields() {
  parse_blob "$BLOB_CONSOLE" || fail "console stamp unparseable"
  # The tag itself is deliberately not among them: there is one format version, so once it has been
  # accepted it says nothing the caller can use. Case `foreign-format-tag-refused` is what proves the
  # acceptance was a check rather than an assumption.
  expect_eq "default" 1 "$PROV_DEFAULT"
  expect_eq "features" log_usb,seed3 "$PROV_FEATURES"
  expect_eq "defmt_log" "" "$PROV_DEFMT_LOG"
}

case_foreign_format_tag_refused() {
  local out rc
  out=$(parse_blob "$BLOB_FOREIGN_TAG" 2>&1)
  rc=$?
  expect_refused "foreign format tag" "$rc" "$out" "starts with 'asp-prov2', not 'asp-prov1'"
}

case_missing_keys_refused() {
  local out rc
  out=$(parse_blob "$BLOB_NO_FEATURES" 2>&1)
  rc=$?
  expect_refused "blob with no features= line" "$rc" "$out" "no features= line"
  out=$(parse_blob "$BLOB_NO_DEFAULT" 2>&1)
  rc=$?
  expect_refused "blob with no default= line" "$rc" "$out" "no default= line"
}

case_unknown_key_ignored() {
  parse_blob "$BLOB_FUTURE_KEY" || fail "a newer stamp was refused rather than ignored"
  expect_eq "fields survive an unknown key" log_usb,seed3 "$PROV_FEATURES"
}

case_layout_field() {
  parse_blob "$BLOB_CONSOLE" || fail "console stamp unparseable"
  expect_eq "a stamp with no layout= line is an internal-flash image" internal "$PROV_LAYOUT"
  parse_blob "$BLOB_BOOTLOADED" || fail "bootloaded stamp unparseable"
  expect_eq "layout" bootloaded "$PROV_LAYOUT"
}

case_source_digest_optional() {
  parse_blob "$BLOB_WITH_DIGEST" || fail "a stamp carrying source_digest was refused"
  expect_eq "digest present" 0123abcd "$PROV_SOURCE_DIGEST"
  expect_eq "other fields unaffected" log_usb,seed3 "$PROV_FEATURES"
  parse_blob "$BLOB_CONSOLE" || fail "a stamp without source_digest was refused"
  expect_eq "digest absent is empty" "" "$PROV_SOURCE_DIGEST"
}

case_no_provenance_section_refused() {
  local out rc
  out=$(read_blob "$FX_NOSTAMP" 2>&1)
  rc=$?
  expect_refused "ELF older than the stamp" "$rc" "$out" "has no .asp.prov section"
  expect_has "and says so twice" "nothing about its cfg set is knowable" "$out"
}

case_not_an_object_file_refused() {
  local out rc
  out=$(read_blob "$FX_JUNK" 2>&1)
  rc=$?
  expect_refused "a file that is not an ELF" "$rc" "$out" "could not read $FX_JUNK"
}

expect_norm() { # <input> <want>
  local in=$1 want=$2 got
  got=$(printf '%s' "$in" | normalize_features)
  expect_eq "normalize_features([$in])" "$want" "$got"
}

case_normalize_features_table() {
  expect_norm "LOG-USB" log_usb                       # case folded, '-' to '_'
  expect_norm "seed3,log-usb log_usb" log_usb,seed3   # mixed separators, deduped, sorted
  expect_norm "default seed3" seed3                   # the implicit token build.rs never stamps
  expect_norm "default" ""                            # and nothing else behind it
  expect_norm "" ""                                   # empty stays empty rather than becoming ""
}

# --- the read guard -----------------------------------------------------------------------

# Reading an ELF must not touch it. This is the assertion that makes the trailing /dev/null in
# read_blob load-bearing rather than decorative: without it rust-objcopy rewrites the image in place,
# which moves the mtime and can shrink the file, all while the bytes it prints stay correct. Both halves
# are asserted because each one fires where the other does not: the mtime moves at any size, the size
# only moves on a multi-megabyte image. Neither half is about firmware/Makefile any more -- since
# elf-check compares content, not timestamps -- so what they guard is the audit's own premise,
# that reading an artifact leaves it alone.
case_read_leaves_the_elf_untouched() {
  local f=$FX_DIR/guarded.elf marker=$FX_DIR/marker sha_before sha_after moved
  cp "$FX_CONSOLE" "$f" || fail "could not stage the guarded-read fixture"
  touch -t 198001010000 "$f" "$marker" || fail "touch -t refused, so mtime cannot be pinned"
  sha_before=$(sha_of "$f")

  spawn_script show "$f"
  [ "$SPAWN_RC" = 0 ] || fail "show on the guarded copy exited $SPAWN_RC: $SPAWN_OUT"

  sha_after=$(sha_of "$f")
  expect_eq "sha256 after reading" "$sha_before" "$sha_after"
  # `find -newer` rather than `stat`: the nix devShell's coreutils stat shadows BSD stat, so
  # `stat -f %m` dies there with "invalid option -- '%'".
  moved=$(find "$f" -newer "$marker")
  [ -z "$moved" ] || fail "reading the ELF moved its mtime: it is now newer than a 1980 marker"
}

# The tripwire under the case above. On the small fixture the unguarded form leaves the bytes alone
# and moves only the mtime, so without this case a reader could believe the sha comparison was doing
# the work. If objcopy ever stops rewriting in place, this goes red and the guard gets re-examined.
case_unguarded_dump_is_the_damage() {
  local f=$FX_DIR/damaged.elf marker=$FX_DIR/damaged-marker moved out
  cp "$FX_CONSOLE" "$f" || fail "could not stage the throwaway copy"
  touch -t 198001010000 "$f" "$marker"
  out=$(rust-objcopy --dump-section ".asp.prov=$FX_DIR/dumped.bin" "$f" 2>&1) ||
    fail "the unguarded dump exited nonzero, so its premise changed: $out"
  moved=$(find "$f" -newer "$marker")
  [ -n "$moved" ] || fail "the unguarded dump left the mtime alone: case_read_leaves_the_elf_untouched asserts nothing"
}

# --------------------------------------------------------------------------- running them

run_selftest() {
  command -v rust-objcopy >/dev/null 2>&1 \
    || die "no rust-objcopy on PATH - the selftest reads its fixtures with cargo-binutils, available in nix develop .#default"
  command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1 \
    || die "no sha256 tool on PATH (tried sha256sum and shasum), so the read-guard case cannot run"
  fx_make_dir

  fx_elf "$FX_CONSOLE" "$BLOB_CONSOLE" || die "could not emit the console fixture into $FX_DIR"
  fx_elf "$FX_NOSTAMP" "" nostamp || die "could not emit the unstamped fixture into $FX_DIR"
  printf 'not an ELF, and never was\n' >"$FX_JUNK"

  fx_elf "$FX_DIGESTED" "$BLOB_WITH_DIGEST" || die "could not emit the digest fixture into $FX_DIR"

  run_case digest-exits-zero-on-a-stamped-image case_digest_exits_zero_on_a_stamped_image
  run_case cannot-run-exits-two case_cannot_run_exits_two

  run_case blob-survives-the-container case_blob_survives_the_container
  run_case parse-all-three-fields case_parse_all_three_fields
  run_case foreign-format-tag-refused case_foreign_format_tag_refused
  run_case missing-keys-refused case_missing_keys_refused
  run_case unknown-key-ignored case_unknown_key_ignored
  run_case source-digest-optional case_source_digest_optional
  run_case layout-field case_layout_field
  run_case no-provenance-section-refused case_no_provenance_section_refused
  run_case not-an-object-file-refused case_not_an_object_file_refused
  run_case normalize-features-table case_normalize_features_table

  run_case read-leaves-the-elf-untouched case_read_leaves_the_elf_untouched
  run_case unguarded-dump-is-the-damage case_unguarded_dump_is_the_damage

  if [ "$ST_FAILED" = 0 ]; then
    printf '%s\n' "$PROG: selftest passed ($ST_TOTAL cases)" >&2
    return 0
  fi
  printf '%s\n' "$PROG: selftest FAILED ($ST_FAILED of $ST_TOTAL cases)" >&2
  return 1
}

# --------------------------------------------------------------------------- modes

mode=${1:-}
case $mode in
  --selftest)
    [ $# -eq 1 ] || { usage >&2; exit 2; }
    run_selftest
    exit $?
    ;;

  show)
    [ $# -eq 2 ] || { usage >&2; exit 2; }
    blob=$(read_blob "$2") || exit 2
    parse_blob "$blob" || exit 2
    printf 'default=%s features=%s defmt_log=%s layout=%s source_digest=%s\n' "$PROV_DEFAULT" "$PROV_FEATURES" "$PROV_DEFMT_LOG" "$PROV_LAYOUT" "$PROV_SOURCE_DIGEST"
    ;;

  digest)
    [ $# -eq 2 ] || { usage >&2; exit 2; }
    blob=$(read_blob "$2") || exit 2
    parse_blob "$blob" || exit 2
    if [ -z "$PROV_SOURCE_DIGEST" ]; then
      printf '%s\n' "$PROG: $2 has no source_digest in its .asp.prov, so it was linked before the digest was embedded - rebuild it: make build-elf FEATURES=\"...\"" >&2
      exit 2
    fi
    printf '%s\n' "$PROV_SOURCE_DIGEST"
    ;;

  *)
    usage >&2
    exit 2
    ;;
esac
