#!/usr/bin/env bash
#
# Build every firmware binary that must link for the stock Daisy bootloader (firmware/memory-bootloaded.x)
# and grade each result before the next build overwrites it.
#
# Why this is one script rather than a gate line per build: the bootloaded and internal layouts share
# one ELF path, target/thumbv7em-none-eabihf/release/<bin>, so a build re-points the very file the next
# check would read. Build-then-grade has to be a unit. Per image it asserts three things:
#
#   - the ELF's .asp.prov stamp says layout=bootloaded for the feature set just built
#     (scripts/elf-provenance.sh check), so a build that silently linked the internal layout cannot
#     pass on the strength of the other layout's numbers;
#   - scripts/check-image-load-addresses.sh passes for it, which for a bootloaded image means R1/R3 against
#     the AXI SRAM region, R4 (the image ends below the excerpt slots), R5 (.sram1_bss in D2) and R6
#     (at least 2 KB spare);
#   - the line that script prints is kept, so the sizes are in the gate log.
#
# The matrix is the set TASK-038.04.07.02 names: main, blinky, and rig with each of its three stimulus
# feature sets. rig is the one this layout exists for: with the excerpt install path it no longer fits
# the 128 KB internal flash (TASK-038.04.07). podtest, ledtest and panictest keep the internal layout.
#
# Ordering: this re-points release/<bin> for the bins it builds, so scripts/gates.sh runs it before the
# console/RTT cross-build pair (its ordering rule 1), which then leaves every image in its default state.
#
# Exit codes: 0 every image built and passed, 1 a build or check failed (all of them are attempted and
# reported), 2 the script could not run.

set -uo pipefail

ROOT=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT" || exit 2

# <binary>:<features>
MATRIX=(
  "blinky:seed3"
  "main:seed3"
  "rig:seed3"
  "rig:seed3,stim-ess"
  "rig:seed3,stim-pulse"
)

ELF_DIR=firmware/target/thumbv7em-none-eabihf/release
failed=0

for entry in "${MATRIX[@]}"; do
  bin=${entry%%:*}
  features=${entry#*:}
  printf '%s\n' "--- $bin [$features] layout=bootloaded"

  if ! (cd firmware && ASPERITAS_LAYOUT=bootloaded cargo build --release --features "$features" --bin "$bin"); then
    printf '%s\n' "check-bootloaded-builds: cargo build failed for $bin [$features]" >&2
    failed=1
    continue
  fi

  # elf-provenance.sh takes the feature list space-separated, defaults on (NO_DEFAULT empty).
  if ! bash scripts/elf-provenance.sh check "$ELF_DIR/$bin" "${features//,/ }" "" bootloaded; then
    printf '%s\n' "check-bootloaded-builds: $bin [$features] was not linked for the bootloaded layout" >&2
    failed=1
    continue
  fi

  bash scripts/check-image-load-addresses.sh "$bin" || failed=1
done

exit "$failed"
