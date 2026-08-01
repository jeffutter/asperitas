# Asperitas firmware — build and flash targets
#
# Prerequisites (install once):
#   - nix develop .  (provides rustc, cargo-binutils, dfu-util)
#   - No additional setup needed; the flake provides everything.
#
# DFU flashing requires holding BOOT, tapping RESET, releasing BOOT
# to enter the STM32 system bootloader. See docs/reference/daisy-seed3.md.

BINARY  = main
FEATURES = seed3
TARGET   = thumbv7em-none-eabihf

.PHONY: all build flash flash-all clean check

all: build

## Build $(BINARY) as a release binary and produce raw .bin for DFU
##
## llvm-objcopy's -O binary output spans from lowest VMA to highest VMA,
## which includes the gap between FLASH (0x08000000) and RAM (0x24000000),
## producing a ~469 MB file. --only-section extracts only FLASH-loadable
## sections, giving a correct ~32 KB binary for DFU flashing.
build:
	cargo objcopy --release --features $(FEATURES) --bin $(BINARY) -- \
		-O binary firmware.bin \
		--only-section=.vector_table \
		--only-section=.text \
		--only-section=.rodata \
		--only-section=.data \
		--only-section='*.sgstubs' \
		--only-section='*.defmt*'

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
