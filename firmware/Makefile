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

## Flash firmware.bin to the Seed3 via DFU and start it
## Prerequisite: hold BOOT, tap RESET, release BOOT on the board
##
## `:leave` makes the bootloader jump straight to the application, so no manual
## RESET tap is needed. Verified working on Seed3.
##
## dfu-util exits 74 ("Error during download get_status") even when this fully
## succeeds: the jump tears down the USB connection that the pending GET_STATUS
## was travelling on, so there is nothing left to answer it. The exit code is
## therefore not a usable success signal here.
##
## Nor can we just ignore 74 — dfu-util also returns it for genuine I/O errors
## during download, so `|| true` would silently swallow a failed flash. Instead
## we judge by dfu-util's own success marker, which only appears after the image
## is written and verified. Output is teed rather than captured so the progress
## bars still render live.
flash:
	@log=$$(mktemp); \
	dfu-util -a 0 -s 0x08000000:leave -D firmware.bin 2>&1 | tee "$$log"; \
	if grep -q "File downloaded successfully" "$$log"; then \
		rm -f "$$log"; \
		printf "\nFlashed and started (dfu-util's exit-74 on leave is expected).\n"; \
	else \
		rm -f "$$log"; \
		printf "\nFLASH FAILED - see dfu-util output above.\n" >&2; \
		exit 1; \
	fi

## Build + flash in one step
flash-all: build flash

## Check compilation without producing artifacts
check:
	cargo check --release --features $(FEATURES) --bin $(BINARY)

clean:
	cargo clean
	rm -f firmware.bin
