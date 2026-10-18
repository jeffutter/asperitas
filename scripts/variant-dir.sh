#!/usr/bin/env bash
#
# The cargo target directory one firmware build variant owns, relative to firmware/.
#
#   scripts/variant-dir.sh <LAYOUT> <NO_DEFAULT> <FEATURES> [TAG]
#
#   LAYOUT      internal or bootloaded
#   NO_DEFAULT  1 when the build passes --no-default-features, anything else (or empty) otherwise
#   FEATURES    the cargo feature list, space or comma separated, in any order
#   TAG         optional, for an input that changes the image without being a cargo feature (the
#               ASP_RIG_* environment variables)
#
# Prints e.g. `target/internal-default-seed3` or `target/bootloaded-nodefault-log-defmt+seed3`.
#
# Why every variant gets a directory: cargo uplifts target/<triple>/release/<bin> as a hardlink to
# the last build of that bin, so two builds that differ only in features or link layout fight over
# one path and whichever ran last owns it. The name is derived from the inputs that distinguish the
# images, so a caller cannot point at the wrong variant's ELF by typing the wrong path. The
# directories sit under firmware/target/ so one cache entry (and `cargo clean`) covers all of them.
#
# firmware/Makefile and scripts/gates.sh both call this; nothing else spells the naming.

set -euo pipefail

if [ $# -lt 3 ] || [ $# -gt 4 ]; then
  echo "usage: scripts/variant-dir.sh <LAYOUT> <NO_DEFAULT> <FEATURES> [TAG]" >&2
  exit 2
fi

layout=$1 no_default=$2 features=$3 tag=${4:-}

case $layout in
  internal | bootloaded) ;;
  *) echo "variant-dir: LAYOUT must be internal or bootloaded, got '$layout'" >&2; exit 2 ;;
esac

defaults=default
[ "$no_default" = 1 ] && defaults=nodefault

feats=$(printf '%s' "$features" | tr ' \t,' '\n\n\n' | sed '/^$/d' | sort -u | paste -sd+ -)
[ -n "$feats" ] || feats=none

name=$layout-$defaults-$feats
[ -z "$tag" ] || name+=-$tag

printf 'target/%s\n' "$name"
