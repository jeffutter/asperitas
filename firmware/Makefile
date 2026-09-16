# Asperitas firmware — build and flash targets
#
# Prerequisites (install once):
#   - nix develop .  (provides rustc, cargo-binutils, dfu-util, probe-rs-tools)
#   - No additional setup needed; the flake provides everything.
#
# DFU flashing requires holding BOOT, tapping RESET, releasing BOOT
# to enter the STM32 system bootloader. See docs/reference/daisy-seed3.md.

# The image each target builds is named after the binary it came from: `make build BINARY=rig`
# writes `rig.bin`, never a shared `firmware.bin`. A single name would be a trap on this project,
# where flashing the wrong image is indistinguishable from flashing the right one by ear until you
# hear whether stimulus is playing: `main.bin` and `rig.bin` coexist, and whichever build ran last
# would silently own the file the next `make flash` reads.
BINARY  = main
# Multiple features are passed as one space-separated word, e.g.
# `make flash-all FEATURES="seed3 slow-boot"`. Every use below must keep the
# quotes: unquoted, the shell splits it and cargo rejects the second feature as
# an unexpected positional argument.
FEATURES = seed3
TARGET   = thumbv7em-none-eabihf

# --- Probe flashing (ST-Link) -------------------------------------------------
#
# The probe path drives the ELF, not `$(BINARY).bin`: defmt frames are decoded from
# the ELF's `.defmt` section and its symbol table, so flashing a stripped .bin
# while holding some other ELF on the host guarantees the host's metadata does not
# match what is running. The .bin stays a DFU-only artifact.
#
# probe-rs needs that single `.defmt` section to exist at all, and only a `log-defmt` build has
# one: build.rs passes `-Tdefmt.x`, which consolidates the per-call-site `.defmt_*` sections into
# it. Measured on this workspace with probe-rs-tools 0.32.0 on 2026-09-12, nothing attached:
# `probe-rs attach` and `probe-rs run` refuse a console-only ELF before they look for a probe,
# rc=1, with exactly this chain:
#
#   Caused by:
#       0: Failed to parse defmt data
#       1: defmt version found, but no `.defmt` section - check your linker configuration
#
# while a `log-defmt` ELF parses cleanly and stops at "No connected probes were found".
# `probe-rs download` exits on the missing probe whichever ELF it is handed, so an empty bench says
# nothing about it. So every `probe-*` target below is only usable with `log-defmt` in FEATURES:
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
## llvm-objcopy's `-O binary` output is a memory image spanning the lowest to the highest
## *load* address, so with every file-backed section loaded in flash the plain image is exactly
## what flash must hold -- and it is. `.data` runs from a RAM VMA of 0x24000000 but a flash LMA
## of 0x08015808, and `.sram1_bss`, where daisy-embassy puts its SAI DMA buffers
## (`src/audio.rs:20-22`), is `(NOLOAD)` placement from `firmware/memory.x`. No keep-list is
## needed here, and none belongs here.
##
## What this buys is worth one sentence of vigilance. Before that `(NOLOAD)` rule the section was
## an orphan rust-lld placed with LMA == VMA in AXI SRAM, and a plain `-O binary` of `main` came
## out at 469,763,480 bytes of mostly zeros (TASK-059) while `blinky`, which links no audio, stayed
## at 65,638 -- so the symptom looked intermittent. Nothing fails if that comes back: the image
## silently spans the gap instead. TASK-059.02 files the gate that reads load addresses off the ELF.
##
## Measured 2026-09-13 at `[profile.release] debug = 2`, against the 131,072-byte internal-flash
## budget:
##
##   FEATURES="seed3"                        main.bin 88,581   rig.bin 106,811
##   NO_DEFAULT=1 FEATURES="seed3 log-defmt" main.bin 48,360   rig.bin  65,696
##
## `rig` is the one to watch: at 106,811 it has 24 KB of that sector left, and it is the binary a
## new dependency is most likely to be added for. Treat these as measurements of this profile at
## this commit, not constants: debug info moves .text (see firmware/Cargo.toml).
##
## The output carries `$(BINARY)` in its name so the artifact cannot outlive the command that made
## it. See the note beside `BINARY` above.
build:
	cargo objcopy --release $(CARGO_DEFAULTS) --features "$(FEATURES)" --bin $(BINARY) -- \
		-O binary $(BINARY).bin

## Flash $(BINARY).bin to the Seed3 via DFU and start it
## Prerequisite: hold BOOT, tap RESET, release BOOT on the board
##
## `BINARY` picks the image, so `make flash-all BINARY=rig` builds and flashes rig in one step;
## plain `make flash` re-flashes whatever `$(BINARY).bin` already holds.
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
	dfu-util -a 0 -s 0x08000000:leave -D $(BINARY).bin 2>&1 | tee "$$log"; \
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
# * They take the ELF, not `$(BINARY).bin` (see CHIP/ELF above). Flashing the .bin
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

# --- Is this ELF built from the sources on disk? ------------------------------------
#
# Two halves, and they only work as one thing: `build-elf` records a digest of every input's bytes
# beside the ELF, and `elf-check` recomputes it and compares. No timestamp enters either half, which
# is the entire point. What it replaces was `find $(ELF_INPUTS) -newer $(ELF)`, so it could not tell
# "the sources moved on" from "something rewrote the filesystem metadata": TASK-056 measured
# `make probe-log` going red on a tree `git status` called clean, 2975 s of pure metadata apart. gate-costs:exempt reason="one historical mtime gap, quoted as evidence for dropping -newer"
# The churn behind it was files `git checkout` had just refreshed. Bulk metadata churn of exactly that
# shape is ordinary housekeeping here, and a check that cries wolf on it is a check the loop learns to
# work around.
#
# What it still cannot see, said out loud because the operator's next step is printed advice and not
# a guess:
#
#   * Content equality is not proof the board runs this image. It never was, and hashing does not
#     change that. Only flashing is proof, which is why `elf-check` stays a check and never a build.
#   * The stamp describes the last link that went through `build-elf`. Anything that relinks
#     `release/$(BINARY)` behind its back leaves the stamp talking about an earlier link: the
#     raw-cargo cross-builds in `scripts/gates.sh`, and `make build` itself, because `cargo objcopy`
#     compiles before it objcopies. Bounded consequence: edit a source, run the gates without ever
#     running `make build-elf`, and `elf-check` goes red against an ELF that is in fact current. The
#     printed remedy fixes that, and the window is narrower than the one removed above -- it needs an
#     edit, not a checkout. TASK-069 owns the question of whether the record belongs inside the image.
#   * `firmware/Cargo.lock` is an input and no gate passes `--locked`, while `firmware/Cargo.toml`
#     pins daisy-embassy to a moving `master` branch. A lock refresh therefore turns this red even
#     when nothing linked differently. Deliberate conservatism; the remedy is the printed relink.
#
# One stamp per binary name, shared by both cfg sets. That is fine because the digest covers sources,
# not features -- the `.asp.prov` note TASK-062 stamps inside the image is what separates the console
# and RTT-only builds, and `elf-check` asks it first.

# Inputs whose bytes are what $(ELF) is compiled from. Not `git status`: a dirty tree says nothing
# about this particular artifact. Registry crates come in through Cargo.lock/Cargo.toml, both listed;
# `../crates` is pruned at any `target` directory so build products are not mistaken for sources. The
# PATHS are the contract: adding or renaming a file changes the digest, refreshing every mtime in the
# tree does not.
ELF_INPUTS = src build.rs memory.x Cargo.toml Cargo.lock ../crates

# The one source file whose mtime stands in for "relink this binary", used by the forced relink below
# and printed in its remedy (`$(dir)` on a relative path is empty, so the printed command stays
# relative to firmware/, where every target here runs). Plain `=` so a test can point it at a fixture
# instead of a tracked file.
MAIN_SRC = src/bin/$(BINARY).rs

# The compiler, named once so `make build-elf CARGO=true` can drive the stamping logic with no
# cross-toolchain present -- which is how scripts/check-elf-staleness.sh asserts it against the
# recipe that ships rather than a copy of it. A makefile assignment wins over the environment here,
# so a stray exported CARGO (cargo itself exports one to build scripts) cannot redirect a real build.
CARGO = cargo

# sha256sum where coreutils provides it, `shasum -a 256` where only perl does -- the same dual
# spelling scripts/elf-provenance.sh's selftest uses. Both emit byte-identical "<hex>  <path>" lines,
# including for the names they escape.
SHA256 = $(shell command -v sha256sum >/dev/null 2>&1 && echo sha256sum || echo 'shasum -a 256')

CARGO_BUILD = $(CARGO) build --release $(CARGO_DEFAULTS) --features "$(FEATURES)" --bin $(BINARY)

# Inside target/, so `cargo clean` deletes it along with everything else and a fresh clone has none.
# Either way that is the "no stamp" failure below, never a silent pass. Named after the binary rather
# than the cfg set, because the digest covers sources and not features.
ELF_STAMP = $(dir $(ELF))$(BINARY).elf-inputs.sha256

# Per-file digests of the input set, sorted by path, hashed together. Names ride along in the hashed
# stream, so a rename counts as a change; mtimes never enter it. `sha256sum -c` was considered and
# rejected: mtime-immune and it names the culprit, but it is blind to an input ADDED since the stamp,
# which is half of what this has to catch.
#
# Three details in the body are load-bearing and each was measured rather than assumed:
#
#   * The file list goes through a temp file, not a command substitution. The obvious shape, `find
#     ... | sort -z | xargs shasum`, passes vacuously on an input set that enumerates nothing:
#     xargs with no arguments hashes its stdin, and the digest of empty input is a well-formed hex
#     string that no `[ -z ]` test can reject. Reproduced. `-print0` cannot survive substitution at
#     all (NUL is not expressible in a shell variable), so it has to land in a file before `[ ! -s ]`
#     can judge it.
#   * `LC_ALL=C sort -z`, not bare `sort -z`: BSD and GNU collate the same path list differently, and
#     the digest covers the order.
#   * Both hash steps spell `$(SHA256)`, so the writer and the reader agree by construction.
define ELF_INPUTS_SHA256
	listing=$$(mktemp); \
	trap 'rm -f "$$listing"' EXIT; \
	if ! find $(ELF_INPUTS) \( -name target -prune \) -o \( -type f \
		\( -name '*.rs' -o -name '*.x' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \) \
		-print0 \) > "$$listing"; then \
		echo "could not enumerate the ELF inputs" >&2; exit 1; \
	fi; \
	if [ ! -s "$$listing" ]; then \
		echo "no ELF inputs found under: $(ELF_INPUTS)" >&2; exit 1; \
	fi; \
	LC_ALL=C sort -z < "$$listing" | xargs -0 $(SHA256) | $(SHA256) | cut -d' ' -f1
endef

# Compile, then record what was compiled. Used verbatim by `build-elf` and driven directly by
# scripts/check-elf-staleness.sh.
#
# When the stamp already equals the digest of the inputs, the sources are what built this ELF and
# there is nothing to prove or to write. When it does not, the relink is made certain rather than
# offered: `$(CARGO) build` decides for itself whether to link, and it decides from mtimes, so a
# source whose bytes changed while its mtime went backwards -- a restore from a backup or a sync tool,
# where a checkout does not land -- reports "Finished in 0.29s" and keeps the old bytes. gate-costs:exempt reason="quoting cargo's own output, not a cost this repo publishes"
# Measured here 2026-09-14. Stamping over that would certify bytes nobody compiled, which is a silent
# pass replacing the noisy false alarm this ticket removes, so `$(MAIN_SRC)` is touched first and cargo
# is left no choice. Cost lands only where a recompile was already owed: a warm forced relink of this
# crate measures ~0.69 s {{component:firmware-forced-relink}}.
define ELF_BUILD_AND_STAMP
	want=$$( $(ELF_INPUTS_SHA256) ) || exit 1; \
	if [ -f $(ELF_STAMP) ]; then have=$$(tr -dc '0-9a-f' < $(ELF_STAMP)); else have=; fi; \
	if [ "$$have" = "$$want" ]; then \
		$(CARGO_BUILD) || exit 1; exit 0; \
	fi; \
	if [ ! -f $(MAIN_SRC) ]; then \
		echo "no $(MAIN_SRC), so nothing here can force a relink of $(BINARY)" >&2; exit 1; \
	fi; \
	touch $(MAIN_SRC) || exit 1; \
	$(CARGO_BUILD) || exit 1; \
	printf '%s\n' "$$want" > $(ELF_STAMP).tmp && mv $(ELF_STAMP).tmp $(ELF_STAMP)
endef

## Build the release ELF the probe targets aim at (`build` without the objcopy step)
##
## Last thing it does is record the source-content stamp `elf-check` compares against, so the stamp
## never describes a link that did not happen. See the block above.
build-elf:
	@$(ELF_BUILD_AND_STAMP)

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

# Reading the cfg provenance out of the ELF belongs to scripts/elf-provenance.sh, which is the only
# thing in the repo that parses the `.asp.prov` note firmware/build.rs stamps in. This is the first
# time the Makefile reaches up into scripts/; scripts/check-doc-artifact-names.sh already reaches
# down into this file to enumerate legal image names, so the coupling is mutual and both headers say
# so. `bash` is spelled rather than relying on the executable bit plus shebang, because a checkout
# on another filesystem can lose the bit and turn every probe target into "command not found".
PROV = bash $(dir $(firstword $(MAKEFILE_LIST)))../scripts/elf-provenance.sh

# What the operator asked for, spelled the way they asked for it, so the line elf-check prints is
# copy-pasteable rather than a template. BINARY is included even when it is the default `main` for
# the same reason.
REQUESTED = BINARY=$(BINARY) FEATURES='$(FEATURES)'$(if $(filter 1,$(NO_DEFAULT)), NO_DEFAULT=1,)

# The remedy for any elf-check failure: a relink that really links, then restamps. `rm -f $(ELF) &&
# make build-elf` does not qualify -- measured, cargo re-uplifts the cached deps/main-<hash> hardlink
# and the file comes back byte-identical (cargo #15313, #8649) -- and neither does `cargo clean -p
# asperitas-firmware`, which reported "Removed 0 files", no recompile, and left the ELF untouched.
# Under the content check the reason to prefer it over a plain `make build-elf` is sharper than the
# mtime argument it started with: `build-elf` touches `$(MAIN_SRC)` itself whenever the stamp
# disagrees, so following this line is what makes cargo prove the link instead of declining to make an
# offer. Measured warm on this tree, console cfg set: this remedy ~0.69 s {{component:firmware-forced-relink}}.
# Forcing one by deleting target/$(TARGET)/release/.fingerprint/asperitas-firmware-* instead is slower,
# and it reaches into cargo's private layout.
REMEDY = touch $(MAIN_SRC) && make build-elf $(REQUESTED)

## Fail if $(ELF) was not built from the sources on disk, or if it is a different cfg set than asked
## for.
##
## A check and never a build: whether the ELF matches the sources is knowable here, whether it
## matches the image on the board is not, and rebuilding would overwrite the only host copy of
## the symbols describing whatever is running right now.
##
## The provenance clause exists because `release/main` is one name for two images -- the console and
## RTT-only builds are hardlinks to different artifacts and whichever ran last owns the name -- so an
## ELF can be current with your sources and still be the wrong binary. Provenance is checked first:
## asking whether the content agrees about an image built from a different cfg set answers a question
## nobody asked.
##
## DEFMT_LOG travels in the blob and `show` prints it, but the check below does not compare it: it
## selects which frames got compiled in rather than which cfg set this is, and refusing an ELF that
## is exactly the one on the board because a bench session happens to export DEFMT_LOG would break
## the command used to read the device.
elf-check:
	@test -f $(ELF) || { \
		echo "no $(ELF) - run 'make build-elf' with the FEATURES and NO_DEFAULT you mean to flash" >&2; \
		exit 1; \
	}
	@$(PROV) check $(ELF) "$(FEATURES)" "$(NO_DEFAULT)" || { \
		echo "that ELF was built for a different cfg set than you are asking to decode with, so its symbols and defmt metadata describe a binary that is not on the board." >&2; \
		echo "Force a real relink: $(REMEDY)" >&2; \
		exit 1; \
	}
	@if [ -f $(ELF_STAMP) ]; then have=$$(tr -dc '0-9a-f' < $(ELF_STAMP)); else have=; fi; \
	if [ -z "$$have" ]; then \
		echo "no stamp at $(ELF_STAMP), so nothing here records which sources $(ELF) came from" >&2; \
		echo "either that ELF predates this check or 'cargo clean' took the stamp with the rest of target/. An ELF of unknowable provenance decodes a live stream into output that looks like data - flash the current build instead, and 'make probe-run' does that and streams in one step." >&2; \
		echo "Force a real relink: $(REMEDY)" >&2; \
		exit 1; \
	fi; \
	want=$$( $(ELF_INPUTS_SHA256) ) || exit 1; \
	if [ "$$want" != "$$have" ]; then \
		echo "$(ELF) was not built from the sources on disk" >&2; \
		echo "  stamp   $$have" >&2; \
		echo "  sources $$want" >&2; \
		echo "an ELF that is not your sources describes neither the board nor your edit, and decoding a live stream with it produces output that looks like data. Flash the current build instead - 'make probe-run' flashes and streams in one step." >&2; \
		echo "Force a real relink: $(REMEDY)" >&2; \
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
## This is the interactive per-binary route -- `make clippy BINARY=rig FEATURES="seed3 stim-ess"`
## -- and it stays handier than anything the gates offer, because the recipe below hard-wires
## --bin $(BINARY) and cannot express the whole package.
##
## It is no longer the only place firmware clippy runs. Every tier of the gate set runs
## whole-package `cargo clippy --release ... --bins -- -D warnings` against raw cargo for the console
## and RTT-only cfg sets -- the two "firmware clippy" gates in `scripts/gates.sh`, which both hook
## stages and CI execute. Raw cargo rather than `make clippy` on purpose: the gates need --bins, and
## the recipe above hard-wires one binary. The only place any tier still reaches make is the
## docs-artifact-name lint, and only with `-n` (see its header warning about why that flag is not
## optional). Cost of those two gates, local warm inside `nix develop .#default`, as the commit tier
## that publishes them pays them: 0.23 s {{gate:firmware-clippy}} and 0.21 s {{gate:firmware-clippy-rtt}} when nothing
## changed, several times that straight after a build, tens of seconds in a fresh target dir. Landed
## with TASK-060.03, re-pointed by TASK-061.02 when the hook job names died.
clippy:
	cargo clippy --release $(CARGO_DEFAULTS) --features "$(FEATURES)" --bin $(BINARY) -- -D warnings

clean:
	cargo clean
	rm -f *.bin
