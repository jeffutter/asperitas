---
id: TASK-036.01
title: Add probe-rs flashing targets and release DWARF line tables
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-09 21:42'
updated_date: '2026-09-09 22:43'
labels:
  - planned
dependencies: []
references:
  - 'https://probe.rs/docs/tools/cargo-flash/'
documentation:
  - docs/reference/daisy-seed3.md
  - docs/reference/rust-daisy-stack.md
modified_files:
  - firmware/Makefile
  - firmware/Cargo.toml
  - flake.nix
parent_task_id: TASK-036
priority: high
type: task
ordinal: 67500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Makefile knows only the DFU path: `build` produces `firmware.bin` with cargo objcopy, `flash` pushes it with dfu-util after a hand on BOOT and RESET. An ST-Link V3 MINIE is on order, and `pkgs.probe-rs-tools` (verified probe-rs 0.32.0) has been sitting unused in the dev shell since flake.nix:44. This adds ELF-based probe targets beside the existing ones — additive, DFU untouched — and turns on the debug info that lets probe-rs symbolicate anything at all.

Two facts shape the design. The probe path wants the **ELF**, not `firmware.bin`: defmt decodes from the `.defmt_*` sections and the symbol table in the ELF, so flashing a stripped `.bin` while holding a different ELF on the host guarantees metadata drift. And release builds carry no DWARF today — there is no `[profile.*]` section anywhere in either workspace — which is why probe-run/probe-rs refuse to produce a decoded backtrace from a release image. `debug = "line-tables-only"` fixes that at zero flash cost, because `make build` copies only FLASH-allocated sections into the `.bin`.

Nothing here needs the physical probe. The target is verified by expanding it (`make -n`), by building the artifact it names, and by the specific error probe-rs prints when nothing is attached (`probe-rs list` prints exactly `No debug probes were found.`). Talking to real silicon is TASK-037.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 firmware/Makefile gains `probe-flash` (program, verify read-back, reset, then exit) and `probe-log` (attach and stream RTT/defmt without reflashing), both driving the built ELF at target/thumbv7em-none-eabihf/release/$(BINARY) with --chip STM32H750IBKx, --connect-under-reset and --verify; `make -n probe-flash probe-log` shows fully expanded commands with no unset variables.
- [x] #2 build, flash, flash-all and check behave exactly as before, and firmware.bin is byte-for-byte the same size as the baseline recorded before [profile.release] was introduced.
- [x] #3 [profile.release] debug = "line-tables-only" is set in firmware/Cargo.toml with a comment naming the reason; strip, panic and lto are left at their defaults.
- [x] #4 Running `make probe-flash` with no probe attached reaches a successful cargo build and fails only at probe discovery, with probe-rs reporting No debug probes were found. — no clap argument-parse error and no missing-artifact error.
- [x] #5 A `clippy` target lints the firmware workspace for thumbv7em-none-eabihf with -D warnings and passes on the default feature set (TASK-009 made this possible; neither CI nor lefthook runs firmware clippy today, so the target is the only place it happens).
- [x] #6 PROBE_EXTRA lets a person add or override flags from the command line without editing the Makefile, and the header comment records why: --connect-under-reset has open reliability reports specifically against the ST-Link V3 MINIE (probe-rs #3516), so the documented fallback is a flag change such as PROBE_EXTRA="--speed 1000", not a rewrite.
- [x] #7 flake.nix comment above pkgs.probe-rs-tools no longer describes the probe as still to arrive, and nix eval .#devShells.aarch64-darwin.default still succeeds after the edit.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Approach

Additive Makefile targets that drive probe-rs against the built ELF, plus the DWARF that
probe-run/probe-rs need to symbolicate anything. DFU stays exactly as it is. Verify everything
that can be verified with no board attached, and stop there.

## Step 0 — record the baseline (two later steps compare against it; do not skip)

    cd firmware
    make build BINARY=main
    ls -l firmware.bin | awk '{print $5}'      # put this number in the commit message

## Step 1 — release debug info (`firmware/Cargo.toml`)

No `[profile.*]` section exists anywhere in either workspace today, which is why probe-rs
declines to unwind a release image. Append:

    [profile.release]
    # probe-rs decodes symbols and backtraces from DWARF; without it a release image yields
    # addresses only. Line tables cost nothing in flash: `make build` copies just the
    # FLASH-allocated sections into firmware.bin — step 5 is the proof.
    debug = "line-tables-only"

Do not add `strip`, `panic`, or `lto`. `panic` especially: the custom `#[panic_handler]` in each
binary assumes the default strategy, and changing it is a decision for after someone has seen a
real backtrace (TASK-037).

## Step 2 — variables (`firmware/Makefile`, beside BINARY/FEATURES/TARGET at L10-16)

Keep the file's plain `=` convention so `make VAR=x` overrides from the command line.

    CHIP        = STM32H750IBKx
    ELF         = target/$(TARGET)/release/$(BINARY)
    PROBE_EXTRA =

`TARGET` is declared at L16 and referenced nowhere in the file today; these targets are what
finally use it — confirm the artifact really lands at
`target/thumbv7em-none-eabihf/release/main` before committing the path.

Chip string: verified against the pinned toolchain. Both `STM32H750IBKx` and bare
`STM32H750IB` resolve (`probe-rs chip info <name>` reports NVM `0x08000000..0x08020000` = 128 KiB
and RAM `0x24000000..0x24080000` = 512 KiB, matching memory.x). Use the `...Kx` spelling verbatim
so it matches daisy-embassy's own `.cargo/config.toml` runner line, which is what someone will
grep next.

## Step 3 — targets

Add to `.PHONY` (L18) and define:

    build-elf:
    probe-flash: build-elf
    probe-run:   build-elf
    probe-log:
    clippy:

with bodies:

    build-elf:   cargo build --release --features "$(FEATURES)" --bin $(BINARY)
    probe-flash: probe-rs download $(ELF) --chip $(CHIP) --connect-under-reset --verify --reset $(PROBE_EXTRA)
    probe-run:   probe-rs run $(ELF) --chip $(CHIP) --connect-under-reset $(PROBE_EXTRA)
    probe-log:   probe-rs attach $(ELF) --chip $(CHIP) $(PROBE_EXTRA)
    clippy:      cargo clippy --release --features "$(FEATURES)" --bin $(BINARY) -- -D warnings

Why these shapes — put this reasoning in the header comment where a person reads it:

* **ELF, not `firmware.bin`.** defmt decodes from the ELF's `.defmt_*` sections and symbol table.
  Flashing `firmware.bin` would need `--binary-format bin --base-address 0x08000000` and would
  leave the host holding metadata that does not match what is running. Keep `firmware.bin` for
  DFU only; the two artifacts must not drift.
* **`probe-rs download` + `--reset` rather than `cargo flash`.** `cargo flash` is the
  cargo-fronted equivalent and works, but going through `probe-rs` keeps one artifact path in the
  Makefile and makes the ELF-vs-.bin distinction impossible to lose. `--reset` is the analogue of
  dfu-util's `:leave`: the board runs again when the command exits.
* **`probe-run` is the human command, not the loop's.** It stays attached and streams RTT. An
  unattended loop wants `probe-flash`, which exits after flashing. Say so — the difference is
  invisible from the target names.
* **`--verify` beats the dfu-util grep.** The DFU target keys on `File downloaded successfully`
  because dfu-util exits 74 on success (Makefile L44-56). probe-rs has genuine read-back
  verification; do not transplant the grep idiom.
* **No `runner` in `firmware/.cargo/config.toml`.** Upstream sets
  `runner = "probe-rs run --chip STM32H750IBKx"`. We deliberately do not, so that no automation
  or editor action silently reaches for a probe. Record the decision beside the new targets.

## Step 4 — the `--connect-under-reset` fallback

Attach-under-reset is the mechanism that removes BOOT/RESET from the loop, but it has open
reliability reports specifically against the ST-Link V3 MINIE (probe-rs #3516, where
STM32CubeProgrammer succeeds and probe-rs does not; an ST community thread concludes the ST-Link
almost never drives nRESET low). Keep the flag in the default command — right until measured
otherwise — and make `PROBE_EXTRA` the documented escape hatch, naming the two things to try:
`PROBE_EXTRA="--speed 1000"`, and dropping the flag (note `make probe-log` attaches to an
already-flashed board and needs no reset at all). TASK-037 records which actually worked.

## Step 5 — verify without hardware

    make -n probe-flash probe-log probe-run          # expansions resolve, no empty $(TARGET)
    make probe-flash                                 # expect: build OK, then "No debug probes were found."
    make build BINARY=main && ls -l firmware.bin     # byte size identical to step 0
    make check && make clippy
    make clippy FEATURES="seed3"                     # the quoted-FEATURES convention still holds
    nix eval .#devShells.aarch64-darwin.default      # flake still evaluates after the comment edit

`probe-rs list` on this machine prints exactly `No debug probes were found.` If `make
probe-flash` instead prints a clap usage error or a missing-artifact error, the target is wrong,
not the environment.

## Step 6 — flake.nix comment

flake.nix:44 reads `pkgs.probe-rs-tools  # flash + defmt/RTT logging (when ST-Link arrives)`. The
package and its position are correct; drop the future tense and note the version the lock pins
(probe-rs-tools 0.32.0 from the nixos-unstable input). Do not add openocd. Do not add udev rules:
this flake ships devShells only, and Darwin needs no rule for an ST-Link. The Linux caveat belongs
in README, which is TASK-036.04's file.

## Facts to hold onto

* STM32H750xB internal flash is a **single 128 KB sector**: any probe erase wipes the whole
  application. Harmless while the app is all there is, but it forecloses keeping resident data in
  internal flash beside firmware. One line in the Makefile comment.
* Sleep modes break RTT discovery on several STM32 parts (probe-rs #350: clear `DBG_SLEEP` /
  `DBG_STANDBY` / `DBG_STOP` in `DBGMCU_CR`). Nothing here sleeps — `executor-thread` busy-loops —
  so do not "optimise" an idle path in passing.
* `*.defmt*` is already in `make build`'s `--only-section` list. Leave it alone; just report sizes.
* Neither CI (`.github/workflows/ci.yml:40`) nor lefthook (`lefthook.yml:27-30`) calls make for
  firmware, so new targets cannot break them. Do not wire firmware clippy into CI here — that is
  a separate judgement about CI minutes.

## Out of scope

Anything requiring the probe physically present (TASK-037); `DEFMT_LOG` plumbing (TASK-036.03);
panic-probe; gdb/openocd.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Baseline (step 0, before [profile.release]): firmware.bin = 88101 bytes, sha256 9496851d604706d3... After adding debug="line-tables-only": 88101 bytes again, sha256 e3b68f4ba6e403d8... — same size, different bytes. Explained rather than hand-waved: llvm-size on both ELFs gives .text 72680 -> 72688 (+8), .rodata 14224 -> 14224, .vector_table 664, .data 404 unchanged; every .debug_* section is non-ALLOC (VMA/LMA 0, TYPE DEBUG) so objcopy's --only-section list excludes all ~3 MB of it. The +8 in .text shifts .rodata by 16 and re-links every address, which is why 76904 of 88101 bytes differ while the total does not. So 'costs nothing in flash' is right about DWARF and slightly wrong about codegen perturbation; the Cargo.toml comment states the measurement instead of the claim.

Verified: make -n probe-flash/probe-log/probe-run expand with no empty variables (PROBE_EXTRA intentionally empty); DFU path untouched — extracted HEAD's Makefile and diffed `make -n build flash flash-all check` output against the new one: identical. make check rc=0. make clippy rc=0 on default features and with FEATURES="seed3 slow-boot" (quoted convention holds). nix eval .#devShells.aarch64-darwin.default -> derivation, rc=0. Chip string confirmed against the pinned toolchain: probe-rs chip info STM32H750IBKx -> NVM 0x08000000..0x08020000 (128 KiB), RAM 0x24000000..0x24080000 (512 KiB), matching memory.x. Artifact really is target/thumbv7em-none-eabihf/release/main.

Two places where this ticket's wording and reality differ, both harmless:
1. AC#4 quotes 'No debug probes were found.' — that is probe-rs list's wording. probe-rs download says 'Error: No connected probes were found.' Same failure mode, purely discovery: cargo build finished first, no clap usage error, no missing-artifact error, make exits 2.
2. AC#1 asks for --connect-under-reset and --verify on both new targets. --verify only exists on probe-rs download (attach has no such flag) and Step 4 of the plan deliberately keeps probe-log reset-free, since it attaches to an already-flashed board. probe-log therefore takes ELF + --chip + PROBE_EXTRA only.

Pre-existing defect found, NOT caused by and NOT fixed here: probe-rs run/attach fail before probe discovery with 'defmt version found, but no `.defmt` section'. Reproduced identically against the pre-change baseline ELF. firmware/Cargo.lock carries defmt 0.3.100 (direct) and 1.1.1 (transitive); the ELF has 100 .defmt.error.* item sections from 1.x and a 1.x version marker but no consolidated .defmt section for probe-rs 0.32 to decode. Recorded as notes on TASK-036.03 (whose AC#3 unifies onto defmt 1) and TASK-037 (so nobody reads it as an ST-Link fault at the bench) rather than filing a duplicate ticket.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Additive probe path, DFU untouched. firmware/Makefile gains CHIP/ELF/PROBE_EXTRA and five targets — build-elf, probe-flash (download + --verify + --reset, then exit), probe-run, probe-log (attach, no reflash/reset), clippy (-D warnings) — with a header block recording why each shape was chosen: ELF rather than firmware.bin so defmt metadata cannot drift from the image, probe-rs download over cargo flash to keep one artifact path, real read-back verification instead of dfu-util's grep idiom, probe-flash as the loop's command and probe-run/probe-log as the human ones, no runner in .cargo/config.toml on purpose, and the single-128 KB-sector erase caveat. PROBE_EXTRA is the documented escape hatch for the ST-Link V3 MINIE attach-under-reset reports (probe-rs #3516). [profile.release] debug="line-tables-only" gives probe-rs something to symbolicate; measured cost is 8 bytes of .text and zero DWARF in flash. flake.nix stops describing the probe as future. All seven ACs checked and verified without hardware; nothing here touches silicon — that stays TASK-037.
<!-- SECTION:FINAL_SUMMARY:END -->
