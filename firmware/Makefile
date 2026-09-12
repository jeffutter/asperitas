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
# the ELF's `.defmt` section and its symbol table, so flashing a stripped .bin
# while holding some other ELF on the host guarantees the host's metadata does not
# match what is running. firmware.bin stays a DFU-only artifact.
#
# probe-rs needs that single `.defmt` section to exist at all, and only a `log-defmt` build has
# one: build.rs passes `-Tdefmt.x`, which consolidates the per-call-site `.defmt_*` sections into
# it. Measured on this workspace, a console-only ELF is rejected during image load — "Failed to
# parse defmt data: no `.defmt` section" — before probe-rs so much as looks for a probe, while a
# `log-defmt` ELF parses cleanly and stops at "No connected probes were found". So every
# `probe-*` target below is only usable with `log-defmt` in FEATURES:
#
# And they need a level asked for at build time. `DEFMT_LOG` unset makes defmt-macros compile
# every non-ERROR call to nothing (see `crates/asperitas-logging/src/defmt_log.rs`), so an
# attach against a plain `make build FEATURES="seed3 log-defmt" NO_DEFAULT=1` image reads as a
# silent RTT channel when what is missing is the frame. Use:
#
#   DEFMT_LOG=info make probe-log FEATURES="seed3 log-defmt" NO_DEFAULT=1
#
# Plain `=` throughout, like the vars above, so `make VAR=x` still overrides from
# the command line.
CHIP        = STM32H750IBKx
ELF         = target/$(TARGET)/release/$(BINARY)

# `FEATURES` adds to the crate's default features; it can never take one away. Dropping the
# framed console — for an RTT-only image, where the diagnostics channel shares nothing with USB —
# therefore needs `--no-default-features` as well:
#
#   make build FEATURES="seed3 log-defmt" NO_DEFAULT=1
#
# Empty by default, so every existing invocation (and CI's `--features seed3`) expands exactly as
# before. An RTT-only image wants it on the probe targets too:
#
#   make probe-run FEATURES="seed3 log-defmt" NO_DEFAULT=1
CARGO_DEFAULTS =
ifeq ($(NO_DEFAULT),1)
CARGO_DEFAULTS = --no-default-features
endif

# Escape hatch for bench experimentation without editing this file, e.g.
#   make probe-flash PROBE_EXTRA="--speed 1000"
# See the note above `probe-flash` for why that may be needed.
PROBE_EXTRA =

# `PROBE_EXTRA` can only add flags, so dropping `--connect-under-reset` — the first thing to try
# when an attach under reset misbehaves, and probe-rs's own FAQ advice — needs a switch of its own:
#   make probe-flash UNDER_RESET=0 FEATURES="seed3 log-defmt" NO_DEFAULT=1
# Same plain-`=` plus parse-time `ifeq` shape as NO_DEFAULT above, which is what lets `make VAR=0`
# win. Default leaves every expansion byte-identical to before the variable existed.
UNDER_RESET = 1
UNDER_RESET_FLAG = --connect-under-reset
ifeq ($(UNDER_RESET),0)
UNDER_RESET_FLAG =
endif

.PHONY: all build flash flash-all clean check build-elf elf-check probe-flash probe-run probe-log probe-rtt-list clippy

all: build

## Build $(BINARY) as a release binary and produce raw .bin for DFU
##
## llvm-objcopy's -O binary output spans from lowest VMA to highest VMA,
## which includes the gap between FLASH (0x08000000) and RAM (0x24000000),
## producing a ~469 MB file. --only-section extracts only FLASH-loadable
## sections, so the result is what flash actually holds: measured 2026-09-12
## at `[profile.release] debug = 2`, 88,741 bytes for FEATURES="seed3" and
## 48,320 for FEATURES="seed3 log-defmt" NO_DEFAULT=1, against the 131,072-byte
## internal-flash budget. Treat those as measurements of this profile at this
## commit, not constants: debug info moves .text (see firmware/Cargo.toml).
build:
	cargo objcopy --release $(CARGO_DEFAULTS) --features "$(FEATURES)" --bin $(BINARY) -- \
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
# * `--non-interactive` goes on every probe command, unconditionally and not behind a knob:
#   with a second probe on the bench probe-rs prompts on stdin to choose one, so an unattended
#   invocation blocks forever on a question nobody is there to answer. Prompting is never useful
#   here; someone who genuinely wants to pick interactively picks the probe up front with
#   `PROBE_EXTRA="--probe VID:PID:SERIAL"`. Confirmed on `download`, `run` and `attach` in
#   probe-rs 0.32.0; `probe-rs list` rejects the flag outright (usage error, rc=2).
# * There is deliberately no `runner` in .cargo/config.toml, even though upstream
#   daisy-embassy ships one: a runner lets `cargo run` in a script, or an editor
#   action, silently reach for a probe.
# * STM32H750xB internal flash is a SINGLE 128 KB sector, so any probe erase wipes
#   the whole application. Harmless while the app is all there is, but it forecloses
#   keeping resident data in internal flash beside the firmware.

## Build the release ELF the probe targets aim at (`build` without the objcopy step)
build-elf:
	cargo build --release $(CARGO_DEFAULTS) --features "$(FEATURES)" --bin $(BINARY)

## Program the ELF over the probe, verify read-back, reset, exit.
##
## No BOOT/RESET handshake: --connect-under-reset drives nRESET low while attaching,
## which is the mechanism that takes the hand out of the loop. Reliability reports
## against the ST-Link V3 MINIE are open (probe-rs #3516), and on any native ST-Link
## probe-rs skips its custom reset sequence there rather than playing one — see
## "Flashing and logging over an ST-Link probe" in docs/reference/daisy-seed3.md.
## The flag stays the default until measured otherwise. The two knobs are `PROBE_EXTRA="--speed 1000"` and
## `UNDER_RESET=0`, which drops the flag from this recipe and from `probe-run`; `make probe-log`
## sidesteps the question by attaching to an already-flashed board with no reset at all.
## TASK-037 records which worked.
probe-flash: build-elf
	probe-rs download $(ELF) --chip $(CHIP) --non-interactive $(UNDER_RESET_FLAG) --verify --reset $(PROBE_EXTRA)

## Flash, then stay attached and stream RTT/defmt. Interactive use — the loop wants
## probe-flash, which exits.
probe-run: build-elf
	probe-rs run $(ELF) --chip $(CHIP) --non-interactive $(UNDER_RESET_FLAG) $(PROBE_EXTRA)

# Inputs whose bytes end up in $(ELF), so a newer one means the ELF on disk is not the one your
# sources describe. Not `git status`: a dirty tree says nothing about this particular artifact.
# Registry crates come in through Cargo.lock/Cargo.toml, both listed; `../crates` is pruned at
# any `target` directory so build products can't trip the check.
ELF_INPUTS = src build.rs memory.x Cargo.toml Cargo.lock ../crates

## Fail if any input to $(ELF) is newer than it.
##
## A check and never a build: whether the ELF matches the sources is knowable here, whether it
## matches the image on the board is not, and rebuilding would overwrite the only host copy of
## the symbols describing whatever is running right now.
elf-check:
	@test -f $(ELF) || { \
		echo "no $(ELF) - run 'make build-elf' with the FEATURES and NO_DEFAULT you mean to flash" >&2; \
		exit 1; \
	}
	@stale=$$(find $(ELF_INPUTS) \( -name target -prune \) -o \
		\( -type f \( -name '*.rs' -o -name '*.x' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \) \
		-newer $(ELF) -print -quit \) ); \
	if [ -n "$$stale" ]; then \
		echo "$(ELF) is older than $$stale" >&2; \
		echo "an ELF behind your sources describes neither the board nor your edit, and decoding a live stream with it produces output that looks like data. Flash the current build instead - 'make probe-run' flashes and streams in one step." >&2; \
		echo "If only timestamps moved, as after a git checkout, force a relink: rm -f $(ELF) && make build-elf." >&2; \
		exit 1; \
	fi

## Attach to an already-flashed board and stream RTT/defmt without reflashing or
## resetting. Needs the ELF from a prior build to locate the RTT control block and
## decode defmt.
##
## `elf-check` enforces that rather than warning about it: the ELF is the decoder, so a stale one
## silently mislabels live output. This target still builds nothing, on purpose (see `elf-check`).
##
## If anything here ever waits in WFI, STM32 parts need DBG_SLEEP/DBG_STANDBY/
## DBG_STOP set in DBGMCU_CR or RTT's control block is never discovered (probe-rs
## #350). Nothing sleeps today — the embassy executor busy-loops — so do not add an
## idle path without reading that.
probe-log: elf-check
	probe-rs attach $(ELF) --chip $(CHIP) --non-interactive $(PROBE_EXTRA)

## Is RTT discoverable at all? Attach, print the RTT channel table, exit.
##
## The cheapest bench question there is: it neither erases the single 128 KB sector nor requires
## the host to keep up with the ring, so it can fail without costing anything. Run it before
## anything that takes the board down. Like `probe-log` it builds nothing, for the same reason.
probe-rtt-list: elf-check
	probe-rs attach $(ELF) --chip $(CHIP) --non-interactive --list-rtt $(PROBE_EXTRA)

## Check compilation without producing artifacts
check:
	cargo check --release $(CARGO_DEFAULTS) --features "$(FEATURES)" --bin $(BINARY)

## Lint the firmware for the embedded target, warnings fatal.
##
## Neither CI (.github/workflows/ci.yml) nor lefthook invokes make inside firmware/,
## so this target is the only place firmware clippy runs.
clippy:
	cargo clippy --release $(CARGO_DEFAULTS) --features "$(FEATURES)" --bin $(BINARY) -- -D warnings

clean:
	cargo clean
	rm -f firmware.bin
