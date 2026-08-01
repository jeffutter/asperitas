# Asperitas firmware — build and flash targets
#
# Prerequisites (install once):
#   - nix develop .  (provides rustc, cargo-binutils, dfu-util)
#   - No additional setup needed; the flake provides everything.
#
# DFU flashing requires holding BOOT, tapping RESET, releasing BOOT
# to enter the STM32 system bootloader. See docs/reference/daisy-seed3.md.

BINARY  = blinky
FEATURES = seed3
TARGET   = thumbv7em-none-eabihf

.PHONY: all build flash flash-all clean check

all: build

## Build blinky as a release binary and produce raw .bin for DFU
build:
	cargo objcopy --release --features $(FEATURES) --bin $(BINARY) -- -O binary firmware.bin

## Flash firmware.bin to the Seed3 via DFU
## Prerequisite: hold BOOT, tap RESET, release BOOT on the board
flash:
	dfu-util -a 0 -s 0x08000000:leave -D firmware.bin

## Build + flash in one step
flash-all: build flash

## Check compilation without producing artifacts
check:
	cargo check --release --features $(FEATURES) --bin $(BINARY)

clean:
	cargo clean
	rm -f firmware.bin
