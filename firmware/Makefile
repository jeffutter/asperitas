# Asperitas firmware — build and flash targets
#
# Prerequisites (install once):
#   - nix develop .  (provides rustc, cargo-binutils, dfu-util, probe-rs-tools)
#   - No additional setup needed; the flake provides everything.
#
# DFU flashing requires holding BOOT, tapping RESET, releasing BOOT
# to enter the STM32 system bootloader. See docs/reference/daisy-seed3.md.

BINARY  = main
# Multiple features are passed as one space-separated word, e.g.
# `make flash-all FEATURES="seed3 slow-boot"`. Every use below must keep the
# quotes: unquoted, the shell splits it and cargo rejects the second feature as
# an unexpected positional argument.
FEATURES = seed3
TARGET   = thumbv7em-none-eabihf

# --- Probe flashing (ST-Link) -------------------------------------------------
#
# The probe path drives the ELF, never firmware.bin: defmt frames are decoded from
# the ELF's `.defmt_*` sections and its symbol table, so flashing a stripped .bin
# while holding some other ELF on the host guarantees the host's metadata does not
# match what is running. firmware.bin stays a DFU-only artifact.
#
# Plain `=` throughout, like the vars above, so `make VAR=x` still overrides from
# the command line.
CHIP        = STM32H750IBKx
ELF         = target/$(TARGET)/release/$(BINARY)

# Escape hatch for bench experimentation without editing this file, e.g.
#   make probe-flash PROBE_EXTRA="--speed 1000"
# See the note above `probe-flash` for why that may be needed.
PROBE_EXTRA =

.PHONY: all build flash flash-all clean check build-elf probe-flash probe-run probe-log clippy

all: build

## Build $(BINARY) as a release binary and produce raw .bin for DFU
##
## llvm-objcopy's -O binary output spans from lowest VMA to highest VMA,
## which includes the gap between FLASH (0x08000000) and RAM (0x24000000),
## producing a ~469 MB file. --only-section extracts only FLASH-loadable
## sections, giving a correct ~32 KB binary for DFU flashing.
build:
	cargo objcopy --release --features "$(FEATURES)" --bin $(BINARY) -- \
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

# --- Probe targets ------------------------------------------------------------
#
# Why these look the way they do:
#
# * They take the ELF, not firmware.bin (see CHIP/ELF above). Flashing the .bin
#   would additionally need `--binary-format bin --base-address 0x08000000`, which
#   is one more way to lose the distinction between the two artifacts.
# * `probe-rs download --reset` rather than `cargo flash`: equivalent, but it keeps
#   a single artifact path in this file. `--reset` is the analogue of dfu-util's
#   `:leave` — the board runs again when the command exits.
# * `--verify` is genuine read-back verification. Do NOT transplant the "File
#   downloaded successfully" grep from `flash:` above; that idiom exists only
#   because dfu-util exits 74 even on success.
# * `probe-flash` exits when it is done, which is what an unattended loop needs.
#   `probe-run` and `probe-log` stay attached and stream RTT — they are the human
#   commands. The target names do not make that difference visible, so say it here.
# * There is deliberately no `runner` in .cargo/config.toml, even though upstream
#   daisy-embassy ships one: a runner lets `cargo run` in a script, or an editor
#   action, silently reach for a probe.
# * STM32H750xB internal flash is a SINGLE 128 KB sector, so any probe erase wipes
#   the whole application. Harmless while the app is all there is, but it forecloses
#   keeping resident data in internal flash beside the firmware.

## Build the release ELF the probe targets aim at (`build` without the objcopy step)
build-elf:
	cargo build --release --features "$(FEATURES)" --bin $(BINARY)

## Program the ELF over the probe, verify read-back, reset, exit.
##
## No BOOT/RESET handshake: --connect-under-reset asserts nRESET and nTRST while
## attaching, which is the mechanism that takes the hand out of the loop. It has
## open reliability reports against the ST-Link V3 MINIE specifically (probe-rs
## #3516, where STM32CubeProgrammer succeeds and probe-rs does not; an ST community
## thread concludes the ST-Link almost never drives nRESET low). The flag stays the
## default until measured otherwise, and PROBE_EXTRA is the documented fallback:
## try `PROBE_EXTRA="--speed 1000"`, or drop the flag — `make probe-log` attaches to
## an already-flashed board and needs no reset at all. TASK-037 records which worked.
probe-flash: build-elf
	probe-rs download $(ELF) --chip $(CHIP) --connect-under-reset --verify --reset $(PROBE_EXTRA)

## Flash, then stay attached and stream RTT/defmt. Interactive use — the loop wants
## probe-flash, which exits.
probe-run: build-elf
	probe-rs run $(ELF) --chip $(CHIP) --connect-under-reset $(PROBE_EXTRA)

## Attach to an already-flashed board and stream RTT/defmt without reflashing or
## resetting. Needs the ELF from a prior build to locate the RTT control block and
## decode defmt, so build first if the firmware changed.
##
## If anything here ever waits in WFI, STM32 parts need DBG_SLEEP/DBG_STANDBY/
## DBG_STOP set in DBGMCU_CR or RTT's control block is never discovered (probe-rs
## #350). Nothing sleeps today — the embassy executor busy-loops — so do not add an
## idle path without reading that.
probe-log:
	probe-rs attach $(ELF) --chip $(CHIP) $(PROBE_EXTRA)

## Check compilation without producing artifacts
check:
	cargo check --release --features "$(FEATURES)" --bin $(BINARY)

## Lint the firmware for the embedded target, warnings fatal.
##
## Neither CI (.github/workflows/ci.yml) nor lefthook invokes make inside firmware/,
## so this target is the only place firmware clippy runs.
clippy:
	cargo clippy --release --features "$(FEATURES)" --bin $(BINARY) -- -D warnings

clean:
	cargo clean
	rm -f firmware.bin
