#!/usr/bin/env bash
#
# Digest (or enumerate) the source files a firmware ELF is built from. The input set lives in
# firmware/elf-inputs.manifest and nowhere else; this is the one implementation of what to do with it,
# called by firmware/build.rs (to embed the digest in .asp.prov), by firmware/Makefile (the sidecar
# stamp, until it is removed) and by the checker, so producer and checker cannot disagree.
#
#   digest [path...]   print the hex digest on stdout. Optional paths replace the manifest's `path`
#                      lines (the staleness selftest points the digest at a fixture tree).
#   list               print every input file and every directory that contains inputs, one per line,
#                      relative to firmware/, for cargo:rerun-if-changed. Directories are listed so a
#                      newly added file, which no existing path names, still triggers a relink.
#   paths              print the manifest's `path` lines, one per line.
#
# Semantics are those of the Makefile's former ELF_INPUTS_SHA256: per-file sha256 in a stream sorted by
# path under LC_ALL=C, then the sha256 of that stream. Names ride in the hashed stream, so a rename is
# a change; mtimes never enter. The file list goes through a temp file so an empty set is an error
# rather than the digest of empty input.
set -euo pipefail

PROG=elf-inputs-digest.sh
die() { printf '%s\n' "$PROG: $*" >&2; exit 1; }

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
fw="$here/../firmware"
manifest="$fw/elf-inputs.manifest"
[ -f "$manifest" ] || die "no manifest at $manifest"
cd "$fw"

if command -v sha256sum >/dev/null 2>&1; then sha=(sha256sum); else sha=(shasum -a 256); fi

paths=() names=() prunes=()
while read -r kind arg rest || [ -n "${kind:-}" ]; do
  case ${kind:-} in
    '' | '#'*) continue ;;
    path) paths+=("$arg") ;;
    name) names+=("$arg") ;;
    prune) prunes+=("$arg") ;;
    *) die "unknown manifest directive '$kind' in $manifest" ;;
  esac
  [ -z "${rest:-}" ] || die "trailing text after '$kind $arg' in $manifest"
done <"$manifest"
[ ${#paths[@]} -gt 0 ] && [ ${#names[@]} -gt 0 ] || die "manifest has no path or no name lines"

mode=${1:-}
[ $# -gt 0 ] && shift
case $mode in
  paths) printf '%s\n' "${paths[@]}"; exit 0 ;;
  digest) [ $# -eq 0 ] || paths=("$@") ;;
  list) [ $# -eq 0 ] || die "list takes no arguments" ;;
  *) die "usage: $PROG digest [path...] | list | paths" ;;
esac

prune_expr=()
for p in "${prunes[@]}"; do
  [ ${#prune_expr[@]} -eq 0 ] || prune_expr+=(-o)
  prune_expr+=(-name "$p")
done
name_expr=()
for n in "${names[@]}"; do
  [ ${#name_expr[@]} -eq 0 ] || name_expr+=(-o)
  name_expr+=(-name "$n")
done

listing=$(mktemp)
trap 'rm -f "$listing"' EXIT

if [ "$mode" = digest ]; then
  find "${paths[@]}" \( "${prune_expr[@]}" \) -prune -o \( -type f \( "${name_expr[@]}" \) -print0 \) >"$listing" \
    || die "could not enumerate the ELF inputs"
  [ -s "$listing" ] || die "no ELF inputs found under: ${paths[*]}"
  LC_ALL=C sort -z <"$listing" | xargs -0 "${sha[@]}" | "${sha[@]}" | cut -d' ' -f1
else
  find "${paths[@]}" \( "${prune_expr[@]}" \) -prune -o \( -type f \( "${name_expr[@]}" \) -print0 \) >"$listing" \
    || die "could not enumerate the ELF inputs"
  [ -s "$listing" ] || die "no ELF inputs found under: ${paths[*]}"
  {
    tr '\0' '\n' <"$listing"
    # every directory that is searched (not pruned), so an added file changes a watched mtime
    find "${paths[@]}" \( "${prune_expr[@]}" \) -prune -o -type d -print
  } | LC_ALL=C sort -u
fi
