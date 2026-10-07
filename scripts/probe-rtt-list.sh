#!/usr/bin/env bash
# probe-rtt-list.sh - is RTT discoverable on the attached board? Read the control block, print it, exit.
#
# usage: probe-rtt-list.sh <ELF> <CHIP> <CHIP_DESC> [extra probe-rs read args...]
#
# Why this is not `probe-rs attach --list-rtt`. That flag is documented to list the channels and exit,
# and on probe-rs 0.32.0 against a live target it does neither: it attaches, finds the control block,
# and then streams until killed, printing no table at all (TASK-037, 2026-10-07). Wrapping it in a
# `timeout` would only turn a hang into a permanent failure. The question the target exists to answer
# is "is there a valid control block where the ELF says, and what is in it", and that is two memory
# reads. Reading memory does not halt the core, reset it, or consume anything from the ring, so this is
# as close to a look-only as SWD allows, and it exits on its own.
#
# The control block is SEGGER's layout: a 16-byte ID, max_up and max_down as u32, then one 24-byte
# descriptor per channel (name ptr, buffer ptr, size, write offset, read offset, flags), up channels
# first. Only up channels are printed, because that is what defmt uses.
#
# Exit 0 when the ID reads "SEGGER RTT". Exit 1 when probe-rs cannot read, or the ID is wrong, which
# for a freshly flashed board means defmt-rtt never initialised (nothing has logged yet, or the
# firmware was built without log-defmt).
set -euo pipefail

if [ $# -lt 3 ]; then
  echo "usage: $0 <ELF> <CHIP> <CHIP_DESC> [extra probe-rs read args...]" >&2
  exit 2
fi
elf=$1 chip=$2 desc=$3
shift 3
extra=("$@") # kept in an array: a function's own "$@" is not the script's, and bash 3.2 (macOS) trips set -u on an empty one

addr=$(rust-nm "$elf" | awk '$3 == "_SEGGER_RTT" { print $1 }')
if [ -z "$addr" ]; then
  echo "probe-rtt-list: $elf has no _SEGGER_RTT symbol, so it was built without log-defmt." >&2
  exit 1
fi

# `probe-rs read b8` prints "ADDR: xx xx ..." lines; keep just the bytes, as one hex word per line.
read_bytes() { # <addr> <count>
  probe-rs read b8 "$1" "$2" --chip "$chip" --chip-description-path "$desc" --protocol swd \
    ${extra[@]+"${extra[@]}"} \
    | sed 's/^[0-9a-fA-F]*: //' | tr ' ' '\n' | grep -E '^[0-9a-fA-F]{2}$'
}
u32() { # <hex bytes, little endian: b0 b1 b2 b3>
  printf '%d' "0x$4$3$2$1"
}

block=$(read_bytes "0x$addr" 48) || {
  echo "probe-rtt-list: probe-rs could not read 0x$addr; see its error above." >&2
  exit 1
}
b=($block)

id=$(printf '%s' "${b[*]:0:16}" | tr ' ' '\n' | while read -r h; do [ "$h" = 00 ] && break; printf "\\x$h"; done)
max_up=$(u32 "${b[16]}" "${b[17]}" "${b[18]}" "${b[19]}")
max_down=$(u32 "${b[20]}" "${b[21]}" "${b[22]}" "${b[23]}")

echo "control block at 0x$addr: id \"$id\", $max_up up channel(s), $max_down down channel(s)"
if [ "$id" != "SEGGER RTT" ]; then
  echo "probe-rtt-list: the ID is not \"SEGGER RTT\". defmt-rtt writes it on first use, so nothing has logged." >&2
  exit 1
fi

# One descriptor is enough for defmt (it uses channel 0). The 48 bytes read above cover exactly that one.
name_ptr=$(u32 "${b[24]}" "${b[25]}" "${b[26]}" "${b[27]}")
size=$(u32 "${b[32]}" "${b[33]}" "${b[34]}" "${b[35]}")
wr=$(u32 "${b[36]}" "${b[37]}" "${b[38]}" "${b[39]}")
rd=$(u32 "${b[40]}" "${b[41]}" "${b[42]}" "${b[43]}")
flags=$(u32 "${b[44]}" "${b[45]}" "${b[46]}" "${b[47]}")
name=$(read_bytes "$(printf '0x%x' "$name_ptr")" 16 | while read -r h; do [ "$h" = 00 ] && break; printf "\\x$h"; done)
echo "up 0: \"$name\", ring $size bytes, write offset $wr, read offset $rd, mode flags $flags"
if [ "$wr" = "$rd" ]; then
  echo "(ring is empty: read offset equals write offset)"
fi
