---
id: TASK-057
title: >-
  Fix the flashing recipe in daisy-seed3.md: wrong artifact name, and a manual
  objcopy that would produce a 469 MB image
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-12 19:18'
updated_date: '2026-09-12 22:13'
labels:
  - planned
dependencies: []
priority: medium
type: docs
ordinal: 89800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while correcting the size claims in this same document under TASK-055. That ticket's sweep was
scoped to byte counts; these lines are stale in a way that costs an afternoon.

docs/reference/daisy-seed3.md:95-96 documents the step-by-step route as:

    make build BINARY=blinky   # produces firmware.bin via cargo objcopy
    make flash                 # dfu-util -a 0 -s 0x08000000:leave -D firmware.bin

Neither half is true since the Makefile started naming each image after its binary. `build` writes
`$(BINARY).bin` (firmware/Makefile:102-103), so `make build BINARY=blinky` produces `blinky.bin`; and
`flash` reads `$(BINARY).bin` (:132) with `BINARY` defaulting to `main` (:15), so plain `make flash`
re-flashes `main.bin`, not the blinky image the line above just built. Following the snippet as written
flashes the application while the reader believes they flashed the test binary. The Makefile comment at
:11 calls this exact trap out: "A single name would be a trap on this project."

Worse, the manual route at :104-108:

    cargo objcopy --release --features seed3 --bin blinky -- -O binary firmware.bin
    dfu-util -a 0 -s 0x08000000:leave -D firmware.bin

omits the defaults/target-dir flags and the entire `--only-section` list, which is load-bearing rather
than cosmetic. Per firmware/Makefile:86-89, llvm-objcopy's `-O binary` spans lowest to highest VMA, so
it covers the gap between FLASH (0x08000000) and RAM (0x24000000) and emits a ~469 MB file. A reader
typing this gets a 469 MB image, then a dfu-util failure or worse.

Scope is this document's build/flash quickstart only. No firmware behavior changes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 #1 Every artifact name in reader-facing build/flash prose names an image firmware/Makefile actually produces today - all seven sites in docs/reference/daisy-seed3.md (:95,:96,:106,:107,:162,:404,:408), README.md:92,:93,:242 and firmware/Cargo.toml's [profile.release] comment (:57,:60,:68) - each one quoted line by line against the `build` rule (:101-109) and the `flash` rule (:130-132) as expanded by `make -n`, and the step-by-step quickstart passes BINARY to both halves so it flashes the image it just built rather than the default main.bin.
#2 The hand-written cargo objcopy route is gone from every doc, because the recipe belongs only in firmware/Makefile:101-109, and the prose that replaces it states why the six --only-section flags are load-bearing with the numbers attributed to the binary they were measured on: raw -O binary spans lowest to highest load address, .sram1_bss is loaded where it runs in AXI SRAM, and that is what turns main and rig into a ~469 MB image while blinky (no audio module) comes out byte-identical to make build. No size claim may sit beside a command that would not produce it, and firmware/Makefile:86-89 gets the same precision fix (load address, not VMA; name the section, not "the gap").
#3 Every command a reader can run in docs/reference/daisy-seed3.md is checked against the Makefile dry-run expansion or the pinned probe-rs-tools 0.32.0 CLI, and any date this ticket refreshes names what it was verified against; nothing needing the board, the probe or ears is restamped - those keep their bench dates. Includes the four concrete non-name drift findings from the sweep: lsusb is not provided by nix develop ., the prerequisites bullet omits probe-rs-tools, the cargo nm invocation at :576 relinks release/main as a side effect it does not warn about, and the exit-code table at :751-758 has no row for the missing-ELF case.
#4 Host gates green in nix develop .#default, verbatim from ci.yml: cargo fmt --all --check, the four clippy invocations, both cargo doc variants with RUSTDOCFLAGS=-D warnings, both workspace test runs, dump_reassemble --selftest, and both firmware cross-compiles - even though no firmware logic is touched.

- [x] #2 #2 The hand-written cargo objcopy route is gone from every doc, because the recipe belongs only in firmware/Makefile:101-109, and the prose that replaces it states why the six --only-section flags are load-bearing with the numbers attributed to the binary they were measured on: raw -O binary spans lowest to highest load address, .sram1_bss is loaded where it runs in AXI SRAM, and that is what turns main and rig into a ~469 MB image while blinky (no audio module) comes out byte-identical to make build. No size claim may sit beside a command that would not produce it, and firmware/Makefile:86-89 gets the same precision fix (load address, not VMA; name the section, not "the gap").
- [x] #3 #3 Every command a reader can run in docs/reference/daisy-seed3.md is checked against the Makefile dry-run expansion or the pinned probe-rs-tools 0.32.0 CLI, and any date this ticket refreshes names what it was verified against; nothing needing the board, the probe or ears is restamped - those keep their bench dates. Includes the four concrete non-name drift findings from the sweep: lsusb is not provided by nix develop ., the prerequisites bullet omits probe-rs-tools, the cargo nm invocation at :576 relinks release/main as a side effect it does not warn about, and the exit-code table at :751-758 has no row for the missing-ELF case.
- [x] #4 #4 Host gates green in nix develop .#default, verbatim from ci.yml: cargo fmt --all --check, the four clippy invocations, both cargo doc variants with RUSTDOCFLAGS=-D warnings, both workspace test runs, dump_reassemble --selftest, and both firmware cross-compiles - even though no firmware logic is touched.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [x] #1 Every command in the touched section copy-pasteable against the current Makefile
- [x] #2 No line in README.md, docs/**, firmware/Makefile or firmware/Cargo.toml instructs a reader to build, flash, read or size an image called firmware.bin; the only surviving mentions are prose that exists to explain the rename (firmware/Makefile:11, .gitignore:18-21)
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Shape of this ticket

Docs and comments only. Four files: `docs/reference/daisy-seed3.md` (the substance), `README.md`,
`firmware/Cargo.toml` (a comment block), `firmware/Makefile` (two comment lines). No firmware logic, no linker
change, no board, no ears, no owner decision - every criterion is satisfiable by reading the Makefile, running
`make -n`, running host-only probe-rs commands with nothing attached, or running the CI gates. Assignee stays
`@agent` and nothing splits to `@human`. The one thing a person should eventually do - actually flash blinky from
the corrected recipe and look at the LED - is handed to TASK-037's bench session as a comment, not turned into an
AC here (step 9).

**No sub-tickets.** Every remaining site of this bug is a one-token prose edit of the same class, three of them in
the same file being rewritten anyway; splitting buys no independently shippable increment and guarantees conflicts
in `daisy-seed3.md`. Two genuinely separate pieces of work did come out of planning and are filed as their own
tickets, deliberately *not* children of this one so they cannot hold it open: TASK-058 (a host gate that fails when
docs name an image the Makefile does not produce, depends on this ticket landing first) and TASK-059 (fix the link
layout so the `--only-section` crutch stops being needed). TASK-060 came out of checking what AC#4's gates actually
cover.

## Scope widened from the ticket text - read this before working

The description says "scope is this document's build/flash quickstart only". Seven sites in `daisy-seed3.md` carry
the dead name and only four of them are in the quickstart; `README.md:92,93,242` and `firmware/Cargo.toml:57,60,68` repeat
the identical error outside it. Fixing only the quickstart leaves a document that contradicts itself two sections
later (`:404` would still be titled "DFU takes `firmware.bin`"), leaves the same trap live in the file a newcomer
opens first, and leaves TASK-058's gate red on its first run. All thirteen sites are the same one-word edit, so AC#1
now covers all of them. Recorded as an AC rewrite, not a silent widening.

What did **not** move into scope: the link-layout fix (TASK-059), the gate (TASK-058), firmware's invisibility to
fmt/clippy (TASK-060), and every historical mention under `backlog/` - a dozen closed tickets name `firmware.bin`
as evidence and must keep doing so.

## Ground truth, re-measured 2026-09-12 at `a1376f3`

Trust this table over the line numbers in the description (they drifted by ~4 lines when TASK-055 landed):

| Fact | Where | Value |
|---|---|---|
| Default binary | `firmware/Makefile:15` | `BINARY = main` |
| Image written by `build` | `:101-109` | `-O binary $(BINARY).bin` + six `--only-section` flags |
| Image read by `flash` | `:130-132` | `dfu-util -a 0 -s 0x08000000:leave -D $(BINARY).bin` |
| One-shot | `:143` | `flash-all: build flash` |
| Why the names are per-binary | `:11-14` | "A single name would be a trap on this project." |
| When the rename happened | git | `41f9cae` (TASK-038.03.02.03), 2026-09-12 |
| Raw objcopy, `main` | measured, output to `/tmp` | **469,763,480 B** = `0x24000598 - 0x08000000` |
| Raw objcopy, `rig` | measured | **469,763,480 B** |
| Raw objcopy, `blinky` | measured | **65,638 B**, byte-identical to `make build BINARY=blinky` |
| With the six flags, `main` | measured | 88,581 B (matches `Makefile:96`) |
| Offending section | `cargo objdump -h` on `main` | `.sram1_bss` size `0x400`, VMA == LMA == `0x24000198`, type **DATA** (allocated, file-backed) |
| Innocent neighbour | same | `.data` VMA `0x24000000` but LMA `0x08015808`, i.e. in flash |
| `.bss` | same | `0x24000598`, NOBITS, contributes nothing to the span |
| Where `.sram1_bss` comes from | `daisy-embassy@ca9bcc9 src/audio.rs:20-22` | two `GroundedArrayCell::uninit()` statics, audio module only |
| Silent typo hazard | measured | `--only-section=.no_such_section` exits **0** and writes a **0-byte file**, no warning |

Two consequences worth internalising before writing prose. First, the blowup is **per binary**: `blinky` links no
audio module, so the doc's own example command produces a perfectly good 65,638-byte image. Writing "~469 MB" next
to a `--bin blinky` line would be a fresh false claim of exactly the kind this ticket exists to kill - attribute
every number to the binary it was measured on. Second, the mechanism is *load* address, not VMA, which makes the
existing `Makefile:86-89` comment ("spans from lowest VMA to highest VMA ... the gap between FLASH and RAM") wrong
in both halves: `.data` has a RAM VMA and is harmless, and the actual contributor has a name.

Re-run the measurements at your commit before quoting them (about a minute, and it must not touch `target/`):

    cd firmware
    for b in main rig blinky; do
      cargo objcopy --release --features seed3 --bin $b -- -O binary /tmp/raw-$b.bin
      stat -c%s /tmp/raw-$b.bin
    done
    cargo objdump --release --features seed3 --bin main -- -h | grep -E '\.data|\.bss|sram1_bss'

Use absolute paths for the output: `cargo objcopy` resolves the `-O binary` argument relative to `firmware/`, and a
relative path there silently becomes another stray artifact in the tree. Delete the files afterwards.

## Steps

### 1. Rewrite the step-by-step quickstart (AC#1) - `daisy-seed3.md:90-99`

Both halves must carry `BINARY`. Target shape:

    # One-shot build + flash
    make flash-all BINARY=blinky

    # Or step by step - BINARY has to appear on both lines
    make build BINARY=blinky    # writes blinky.bin
    make flash BINARY=blinky    # dfu-util -a 0 -s 0x08000000:leave -D blinky.bin

Then say why the repetition is not ceremony: `BINARY` defaults to `main` (`Makefile:15`), `build` writes
`$(BINARY).bin`, `flash` reads `$(BINARY).bin`, so `make build BINARY=blinky` followed by plain `make flash` builds
one image and flashes the application while printing a completely successful DFU transcript. Point at
`Makefile:11-14` for the reasoning rather than restating it. Keep the existing "`BINARY` defaults to `main`, so plain
`make flash-all` flashes the application" sentence at `:99`; it is correct and now reinforced.

Evidence for the notes, verbatim: `make -n -C firmware build flash BINARY=blinky` (expect `-O binary blinky.bin` and
`-D blinky.bin`) and `make -n -C firmware flash` (expect `-D main.bin`).

### 2. Delete the manual route, keep the lesson (AC#2) - `:102-108`

Remove the `Or manually:` heading and its fenced block entirely. Do **not** transcribe the flag list into prose: a
mistaken or renamed section name makes llvm-objcopy exit 0 with a 0-byte file, so a hand-kept copy of that list is a
truncated image waiting to happen, and TASK-058 will enforce "the recipe lives only in `firmware/Makefile:101-109`".

Replace with one paragraph carrying the mechanism plus the attributed numbers above, and closing with the reason
nobody should re-simplify it: the flags are load-bearing, `-O binary` means "memory image", one section is loaded
where it runs, hence 469,763,480 bytes of zeros for anything that links the SAI buffers, while blinky looks fine.
Name the section and cite `daisy-embassy src/audio.rs:20-22`. Mention that TASK-059 is the real fix. Say explicitly
that `.data` is not the culprit, because the old Makefile comment implies it is.

### 3. Precision-fix the Makefile's own comment (AC#2) - `firmware/Makefile:86-89`

Two sentences, same information, corrected: spans lowest to highest **load** address; the section responsible is
`.sram1_bss` (LMA == VMA in AXI SRAM, file-backed despite being uninit data), not "the gap between FLASH and RAM";
and it applies to binaries that link the audio module. Leave the measured size table at `:92-97` untouched - those
figures are today's, from TASK-054/055, and re-dating them without re-measuring is the failure mode in step 8.

### 4. The other three sites in the same document (AC#1)

- `:162` - the bench trick reads the initial SP from the raw image. Name a file that exists: `main.bin` (or
  `$(BINARY).bin` if the sentence wants to stay generic). It is the one stale mention that a reader can actually act
  on with no probe attached.
- `:404` - section heading "Why the probe path takes the ELF and DFU takes `firmware.bin`" -> name the DFU image
  correctly.
- `:408` - "`firmware.bin` stays a DFU-only artifact" -> same. `Makefile:32` already words this correctly ("The .bin
  stays a DFU-only artifact"); match it.

### 5. README.md (AC#1) - `:92,:93,:242`

`:92` `make build   # produces firmware.bin` -> `main.bin`. `:93` `make flash   # flashes firmware.bin via DFU` ->
`main.bin`, and consider mirroring the doc fix by showing `BINARY=` on the flash half if the surrounding block
invites it. `:242` "All three drive the release **ELF**, never `firmware.bin`" - the claim is right, the noun is
dead; use "the DFU `.bin`".

### 6. firmware/Cargo.toml comment (AC#1) - `:57,:60,:68` - includes one measurement

Three mentions inside the `[profile.release]` DWARF-cost comment, including the table column header at `:60`. Rename
them to `main.bin`, since the table was cut with default `BINARY`. That rename makes a pre-existing disagreement
visible: this table says the RTT-only `debug = 2` image is **48,320** bytes while `Makefile:96-97` says
`NO_DEFAULT=1 FEATURES="seed3 log-defmt"` gives `main.bin` **48,360**. Reproduce it deliberately, the way the table
was made (fresh target dir per row, e.g. `CARGO_TARGET_DIR=$(mktemp -d) make build NO_DEFAULT=1 FEATURES="seed3
log-defmt"`, then size the resulting image), record what you get, and put the date on the row. If your number matches
neither, say so in both places with dates rather than overwriting either - these are measurements of a profile at a
commit, and the Makefile says as much. Note plainly in the comment that a plain `make build` here relinks
`target/.../release/main` and replaces the console image currently sitting there.

### 7. Sweep findings beyond the names (AC#3)

Concrete edits, each verified against a source today:

1. `:84` - `lsusb` is not in the dev shell: `flake.nix:38-63` provides dfu-util, probe-rs-tools, cargo-binutils,
   pkg-config, lefthook, yq-go, jq (+ALSA on Linux), no usbutils. Offer `dfu-util --list` as the in-shell form and
   note `lsusb` needs usbutils outside it.
2. `:150` - "Prerequisites: `nix develop .` provides rustc, cargo-binutils, and dfu-util" omits `probe-rs-tools`,
   which every `probe-*` command in this document needs. `Makefile:4` lists all four; match it.
3. `:576` - the `cargo nm --release --no-default-features --features "seed3 log-defmt" --bin main` invocation lacks
   the `CARGO_TARGET_DIR=$(mktemp -d)` guard that the sibling command at `:511` carries, so it silently relinks
   `release/main` - the exact trap `:515-517` warns about for a different mistake. Add the guard.
4. `:751-758` - the exit-code table has no row for a missing ELF, whose distinct stderr is `no $(ELF) - run 'make
   build-elf' with the FEATURES and NO_DEFAULT you mean to flash` at make rc 2 (`Makefile:208-215`), and it implies
   elf-check's stderr is one line when two more advisory lines follow. Both matter to the unattended driver this
   section exists for. Add the row; do not restructure the table.
5. `:787` - that expansion block's header does not say the three `elf-check` lines were elided, while the equivalent
   header at `:377-378` does. One clause.
6. `:414-419` vs `firmware/build.rs:14-27` - the quoted rejection string is paraphrased ("Failed to parse defmt data:
   no `.defmt` section") where build.rs records "Failed to parse defmt data / defmt version found, but no `.defmt`
   section - check your linker configuration". Only change this if you can reproduce probe-rs's literal wording; the
   reproduction needs a `log-defmt`-off build plus a probe-rs call, and if you cannot run it, leave the passage and
   its date alone.
7. `:452` region - the defmt-rtt crate-docs quote spans `lib.rs:15-22`, cited as `15-21`. Trivial.

Leave alone, deliberately: `:359` ("as of 2026-09-10 no probe has been attached" - TASK-037 owns that), the DFU
bench claims at `:125-143`, and every mention under `backlog/`.

### 8. Dating protocol (AC#3) - the part most likely to be faked

Refresh a date **only** for a claim you re-verified at your commit. Host-safe with nothing attached, and therefore
fair game: `probe-rs chip info STM32H750IBKx` (`:399`, `:606`), `probe-rs list`, `probe-rs list --non-interactive`
(`:737-758`), and `probe-rs download|attach|run --help` (`:491`, `:802-815`, `:836-848`). Stamp each with the tool
version (probe-rs-tools 0.32.0) and the date.

Never stamp: `:leave` behaviour, DFU-not-sticky, dfu-util's exit 74, anything about an LED, the four RTT regimes,
under-reset reliability, `--dry-run` erase-before-stop. They keep their existing bench dates, and AC#3 is satisfied
by leaving them honest.

Do not re-measure things measured today by TASK-054/055 just to move a date forward: `:145-149` sizes (corroborated
right now by `firmware/blinky.bin` 21,202, `main.bin` 88,581, `rig.bin` 106,811 on disk), `:507-520` frame-symbol
counts, `:543-590` RTT symbol addresses. A doc where everything says today's date but three things were re-checked is
worse than the current one. The single exception is the Cargo.toml row in step 6, which this ticket invalidates by
renaming it.

Paste into the ticket notes: every `make -n` comparison used as evidence, the objcopy measurements with their sizes,
and the list of claims you declined to re-date.

### 9. Hand the bench check to whoever has hands

Add a `--comment` to TASK-037 (@human, Blocked, the next probe session) asking that whoever attaches the ST-Link also
run `make flash-all BINARY=blinky` from the corrected quickstart and confirm steady green, so a human eye closes the
loop on the recipe this ticket rewrites. It is a courtesy note, not an AC: making it one here would stall this
ticket on hardware it does not need.

### 10. Host gates (AC#4)

Verbatim from `ci.yml` inside `nix develop .#default --command bash -c` (single job, `set -euo pipefail`):

    cargo fmt --all --check                                                    (:24)
    cargo clippy --workspace --all-targets -- -D warnings                      (:27)
    cargo clippy -p asperitas-logging --features log-usb --lib -- -D warnings  (:32)
    cargo clippy -p asperitas-logging --features log-defmt --lib -- -D warnings (:38)
    cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings (:41)
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps                 (:49)
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features  (:52)
    cargo test --workspace                                                     (:55)
    cargo test --workspace --features asperitas-pod/pod-hw                     (:58)
    cargo run -p asperitas-logging --example dump_reassemble -- --selftest     (:63)
    cd firmware && cargo build --release --features seed3                      (:66)
    cargo build --release --no-default-features --features "seed3 log-defmt"   (:72)

Subtleties: `cd firmware` at `:66` persists into `:72` in the same shell, and firmware is a separate workspace
(excluded at root `Cargo.toml:5`), so the last line fails from the repo root for reasons unrelated to this change.
`.cargo/config.toml` supplies the target and link arg, so no `--target` flag. There is **no firmware clippy in CI** -
`make clippy` (`Makefile:252-255`) is the only place it runs, and adding it is TASK-060's business, not this ticket's.
Note also that `cargo fmt --all --check` passes at the root while `cd firmware && cargo fmt --check` is currently red
in `rig.rs`; that is TASK-060, and do not "fix" it here.

## Verification

1. `grep -rn 'firmware\.bin' README.md docs/ firmware/Makefile firmware/Cargo.toml` -> only `firmware/Makefile:11`
   survives, and only because it exists to say the name is gone ("writes `rig.bin`, never a shared
   `firmware.bin`"). `.gitignore:18-21` likewise. Anything else is a half-rename.
2. `grep -rn 'cargo objcopy' README.md docs/` -> empty.
3. `make -n -C firmware build flash BINARY=blinky | grep blinky.bin` -> both halves present; `make -n -C firmware
   flash | grep main.bin` -> confirms the default the prose warns about.
4. Every number in the rewritten passages traces to a measurement pasted in the ticket notes, with the binary and
   feature set named beside it.
5. Step 10's gates green; paste tails.

## Risks

- **Restamping dates.** Called out above; it is the main way this ticket can make things worse.
- **Renaming the Cargo.toml column** exposes a size disagreement. Handle it in the open (step 6), do not quietly
  pick a favourite number.
- **Scope creep toward the linker.** Tempting, since the mechanism is now well understood. Resist: TASK-059 owns it,
  and moving a section's LMA without a bench is exactly the class of change this project's conventions exist to stop.
- **Half-renames.** If you stop after `daisy-seed3.md`, TASK-058 arrives into a red tree. The thirteen sites are one
  commit; do them together.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Executed 2026-09-12 at `a1376f3`. Docs and comments only: docs/reference/daisy-seed3.md, README.md, firmware/Cargo.toml, firmware/Makefile. No firmware logic, no linker change, no board.

## Names, every live site (AC#1)

Quickstart now reads `make build BINARY=blinky` / `make flash BINARY=blinky`, with the sentence explaining that `build` writes `$(BINARY).bin` and `flash` reads it, so a bare `make flash` after a `BINARY=blinky` build programs `main.bin` while printing a successful DFU transcript. The SP-reading bench tip names the image you flashed (`main.bin` or whichever `$(BINARY).bin`); the ELF-vs-DFU heading says `$(BINARY).bin` and its body now reads "The `.bin` stays a DFU-only artifact", matching the Makefile's own wording. README `:92/:93` -> `main.bin` plus a line saying an override has to appear on both halves; README `:242` -> "never the DFU `.bin`"; README `:36`'s tool list says `cargo-binutils` rather than the literal command, which also keeps AC#2's grep empty. Cargo.toml's table column and its two prose mentions -> `main.bin`.

Evidence pasted verbatim:
- `make -n -C firmware build flash BINARY=blinky` -> `-O binary blinky.bin` and `-D blinky.bin`
- `make -n -C firmware flash` -> `-D main.bin`
- `make -n -C firmware flash-all` -> both halves `main`

Surviving `firmware.bin` in the tree: `firmware/Makefile:11` and `.gitignore:19,21`, both of which exist to explain the rename. DoD#2 met.

## The deleted route and the numbers beside it (AC#2)

Re-measured at a1376f3 with output to /tmp (absolute paths, deleted afterwards):

    raw -O binary      main 469,763,480 | rig 469,763,480 | blinky 65,638
    make build         main 88,581      | rig 106,811     | blinky 65,638
    cmp raw-blinky make-built-blinky -> identical
    0x24000598 - 0x08000000 = 469,763,480 exactly (end of .sram1_bss)
    sections (main): .data VMA 0x24000000 LMA 0x08015808 DATA
                     .sram1_bss 0x400 @ VMA == LMA 0x24000198 DATA
                     .bss 0x24000598 NOBITS (contributes nothing)
    --only-section=.no_such_section -> rc 0, 0-byte file

Prose attributes each figure to the binary it was measured on, names `.sram1_bss` and daisy-embassy `ca9bcc9 src/audio.rs:20-22`, says `.data` is innocent, and points at TASK-059 as the real fix. No flag list transcribed outside the Makefile. The Makefile's own comment got the same precision fix (load address, not VMA; the section by name; per-binary caveat). Its measured size table is untouched and re-corroborated by a rebuild: main.bin 88,581, rig.bin 106,811.

## Cargo.toml's row (plan step 6)

`CARGO_TARGET_DIR=$(mktemp -d) make build NO_DEFAULT=1 FEATURES="seed3 log-defmt"`, fresh dir per variant, DEFMT_LOG unset -> `main.bin` 48,360, `.text` 0x98e0 = 39,136. With DEFMT_LOG=info -> 48,504. So the table's shipped row (48,320, `.text` 39,096) does not reproduce today, while firmware/Makefile's 48,360 does. Both kept, the disagreement dated inside the comment, with a note that the other four cells were not re-cut. Afterwards `make build` and `make build BINARY=rig` restored the default images and the console ELF (0 SEGGER strings in release/main, matching the doc's contrast claim).

## Sweep findings beyond the names (AC#3)

- lsusb: doc offers `dfu-util --list` as the in-shell form (dfu-util 0.11 prints its banner only, rc 0, nothing attached) and notes usbutils is absent from `flake.nix:38-63`; README's DFU section got the same fix.
- Prerequisites bullet now includes probe-rs-tools and cites `flake.nix:38-63` plus `Makefile:4`.
- The D-cache `cargo nm` invocation got the `CARGO_TARGET_DIR=$(mktemp -d)` guard its sibling carries, with a sentence saying why. Re-ran it guarded: reproduces `_SEGGER_RTT` 0x24000008 and `defmt_rtt::BUFFER` 0x240010e4.
- Exit-code table: added the missing-ELF row (measured with `make probe-log BINARY=nope-not-built`: the `no <ELF> - run 'make build-elf' ...` line plus make's own error, rc 2) and corrected the stale-ELF row to three stderr lines. Wrote `Makefile:<line>` rather than a real number on purpose: this ticket itself moved elf-check from 209 to 224.
- The `probe-log` PROBE_EXTRA expansion header now says the elf-check lines are elided.
- defmt rejection string reproduced (probe-rs-tools 0.32.0, nothing attached): `attach` and `run` both print "Failed to parse defmt data / defmt version found, but no `.defmt` section - check your linker configuration", rc 1, before probe discovery. New finding while doing it: `probe-rs download` does not reach that parse on an empty bench (console ELF -> plain `No connected probes were found.`), so the doc and the Makefile comment now scope the claim to attach/run and the doc quotes the chain verbatim.
- Declined, plan item 7: the quote does not span `lib.rs:15-22`. defmt-rtt-1.3.0/src/lib.rs:15-21 is exactly the quoted block and line 22 is a blank `//!`. Citation was already correct, so left alone.

## Dating discipline

Restamped only what got re-run here: `probe-rs chip info STM32H750IBKx` (NVM 0x08000000..0x08020000, AXI 0x24000000..0x24080000, unchanged), `probe-rs list` (rc 0, `No debug probes were found.`), `probe-rs list --non-interactive` (rc 2, unexpected argument), the defmt-rejection passage, the `dfu-util --list` behaviour, the missing-ELF row, the guarded nm output, and the Cargo.toml row that renaming invalidated. Declined to restamp: `:leave`, DFU not being sticky, dfu-util's 74, anything about an LED, the four RTT regimes, under-reset reliability, `--dry-run` erase behaviour, the "as of 2026-09-10 no probe has been attached" line (TASK-037 owns it), and today's size/frame-symbol/symbol-address figures from TASK-054/055.

## Gates (AC#4)

All twelve ci.yml invocations in one `nix develop .#default --command bash -c` with `set -euo pipefail`, 2026-09-12 22:02-22:04 UTC: fmt, four clippy runs, both `cargo doc` runs with RUSTDOCFLAGS=-D warnings, both workspace test runs, `dump_reassemble --selftest` (11 cases passed), and both firmware cross-compiles. Ended `=== ALL GATES GREEN ===`. Two minutes total because the target dirs were warm from the measurements above. For TASK-060's record: `cd firmware && cargo fmt --check` is still red in rig.rs and no CI job sees it; untouched here as instructed.

## Bench handoff

TASK-037 already carries the planning pass's comment asking whoever attaches the ST-Link to run `make flash-all BINARY=blinky` from the corrected quickstart and confirm steady green. Not duplicated.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-12 21:34
---
Planning 2026-09-12: AC#1 rewritten to cover all twelve live sites of the stale name (nine in this doc incl. :162/:404/:408, plus README.md:92,93,242 and firmware/Cargo.toml:57,60,68), because fixing only the quickstart leaves the document contradicting itself and leaves TASK-058's gate red on arrival. AC#2 sharpened after measuring the mechanism directly: raw -O binary is 469,763,480 B for main/rig but 65,638 B for blinky, byte-identical to make build, so the ~469 MB claim must be attributed per binary or it becomes a new false claim. Manual objcopy route deleted rather than transcribed: a mistyped --only-section exits 0 with a 0-byte file. No sub-tickets; filed TASK-058 (docs gate, depends on this), TASK-059 (flash LMA for .sram1_bss), TASK-060 (firmware invisible to fmt/clippy, found while checking AC#4's coverage). Stays @agent: nothing here needs the board, ears, an instrument, or an owner decision.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Rewrote the flashing recipe in docs/reference/daisy-seed3.md and swept the same stale artifact name out of README.md, firmware/Cargo.toml and firmware/Makefile. The step-by-step quickstart now carries BINARY on both halves, because `build` writes `$(BINARY).bin` and `flash` reads `$(BINARY).bin` with the default `main`: verified line by line against `make -n`. The hand-typed objcopy route is gone from every doc rather than transcribed, and the prose that replaces it says why the six `--only-section` flags are load-bearing: `.sram1_bss`, daisy-embassy's SAI DMA buffers, is loaded where it runs at 0x24000198, so a raw `-O binary` spans to 0x24000598 and emits 469,763,480 bytes for `main` and `rig`, while `blinky` links no audio module and comes out at 65,638, byte-identical to `make build`. Each number sits beside the binary it was measured on, along with the silent-failure hazard (a mistyped section name exits 0 with a 0-byte file) and TASK-059 as the real fix. Measured fixes in passing: `dfu-util --list` substituted for `lsusb` inside the dev shell (no usbutils in the flake), probe-rs-tools added to the prerequisites, the D-cache `cargo nm` invocation given the temp-target-dir guard, a missing-ELF row added to the exit-code table, the `probe-log` expansion header told to admit what it elides, and probe-rs's defmt rejection quoted verbatim with its scope narrowed to attach/run because `download` never reaches the parse on an empty bench. Cargo.toml's DWARF-cost table keeps both figures with the disagreement dated: re-running its shipped row gives 48,360, matching firmware/Makefile, against the 48,320 cut earlier the same day. All twelve ci.yml host gates green in `nix develop .#default`. No bench-dependent claim was restamped; TASK-037's existing comment asks the next probe session to flash blinky from the new recipe and confirm steady green.
<!-- SECTION:FINAL_SUMMARY:END -->
