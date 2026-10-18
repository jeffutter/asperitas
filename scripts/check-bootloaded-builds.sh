#!/usr/bin/env bash
#
# Build every firmware binary that must link for the stock Daisy bootloader (firmware/memory-bootloaded.x)
# and grade each result.
#
# Each row builds into its own target directory (scripts/variant-dir.sh), so no build can replace the
# ELF another row, or the internal-layout builds, produced. Per image it asserts:
#
#   - scripts/check-image-load-addresses.sh passes for it, which for a bootloaded image means R1/R3 against
#     the AXI SRAM region, R4 (the image ends below the excerpt slots), R5 (.sram1_bss in D2) and R6
#     (at least 2 KB spare). The layout is not checked separately: the directory is derived from it;
#   - the line that script prints is kept, so the sizes are in the gate log;
#   - for the replay generator, its staging buffer sits in the AXI tail.
#
# The matrix is main, blinky, and rig with each of its stimulus feature sets. rig is the one this layout
# exists for: with the excerpt install path it no longer fits the 128 KB internal flash. podtest,
# ledtest and panictest keep the internal layout.
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
  "rig:seed3,stim-excerpt"
)

failed=0

for entry in "${MATRIX[@]}"; do
  bin=${entry%%:*}
  features=${entry#*:}
  printf '%s\n' "--- $bin [$features] layout=bootloaded"

  target_dir=firmware/$(scripts/variant-dir.sh bootloaded 0 "$features")
  elf_dir=$target_dir/thumbv7em-none-eabihf/release

  if ! (cd firmware && ASPERITAS_LAYOUT=bootloaded cargo build --release --target-dir "${target_dir#firmware/}" --features "$features" --bin "$bin"); then
    printf '%s\n' "check-bootloaded-builds: cargo build failed for $bin [$features]" >&2
    failed=1
    continue
  fi

  bash scripts/check-image-load-addresses.sh --target-dir "$target_dir" "$bin" || failed=1

  # The replay staging must sit in the AXI tail above the image (memory-bootloaded.x AXI_TAIL,
  # 0x24078000..0x24080000), never in DTCM where plain .bss would put it.
  case ",$features," in
    *,stim-excerpt,*)
      replay_addr=$(rust-nm "$elf_dir/$bin" 2>/dev/null | awk '$3 ~ /REPLAY/ && $2 ~ /^[dDbB]$/ {print $1; exit}')
      if [ -z "$replay_addr" ]; then
        printf '%s\n' "check-bootloaded-builds: no REPLAY symbol in $bin [$features] (rust-nm missing or symbol stripped)" >&2
        failed=1
      elif [ $((16#$replay_addr)) -lt $((0x24078000)) ] || [ $((16#$replay_addr)) -ge $((0x24080000)) ]; then
        printf '%s\n' "check-bootloaded-builds: REPLAY at 0x$replay_addr is outside the AXI tail 0x24078000..0x24080000" >&2
        failed=1
      else
        printf '%s\n' "REPLAY staging at 0x$replay_addr (AXI tail)"
      fi
      ;;
  esac
done

exit "$failed"
