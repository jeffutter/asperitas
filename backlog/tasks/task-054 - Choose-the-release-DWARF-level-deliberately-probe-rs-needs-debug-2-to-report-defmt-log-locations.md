---
id: TASK-054
title: >-
  Choose the release DWARF level deliberately: probe-rs needs debug = 2 to
  report defmt log locations
status: Done
assignee:
  - '@agent'
created_date: '2026-09-12 07:08'
updated_date: '2026-09-12 10:18'
labels: []
dependencies: []
references:
  - 'https://github.com/probe-rs/probe-rs/issues/896'
  - 'https://github.com/probe-rs/probe-rs/issues/2274'
  - 'https://github.com/probe-rs/probe-rs/issues/3309'
documentation:
  - firmware/Cargo.toml
  - docs/reference/daisy-seed3.md
  - docs/reference/rust-daisy-stack.md
priority: high
type: task
ordinal: 85700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The release profile picks `debug = "line-tables-only"` (firmware/Cargo.toml:43) to keep DWARF out of the image while still letting probe-rs symbolicate. Measured here, that setting costs something nobody decided to pay: **probe-rs refuses to produce defmt log locations without full debug info**, which is a hole in the exact channel TASK-036 was built to provide.

Reproducible check, run against four scratch builds of the same RTT-only image (`--no-default-features --features "seed3 log-defmt"`):

    probe-rs attach <ELF> --chip STM32H750IBKx --non-interactive --list-rtt

| `[profile.release] debug` | rustc flag seen | ELF bytes | .debug_* bytes | .debug_info | firmware.bin | warning? |
|---|---|---|---|---|---|---|
| `"line-tables-only"` (today) | `-C debuginfo=line-tables-only` x146 | 3,008,004 | 2,745,149 | 916,073 | 48,084 | yes |
| `false` | none passed | 262,328 | 0 | - | 48,084 | yes |
| `1` | `-C debuginfo=1` x146 | 4,735,936 | 4,472,591 | 1,181,667 | 48,228 | yes |
| `2` | `-C debuginfo=2` x146 | 9,481,416 | 9,217,972 | 3,555,137 | 48,320 | **no** |

Three things follow. The setting *is* honoured (the flag reaches all 146 compilations and the ELF really is smaller than at `1` or `2`) but it does not mean "no `.debug_info`", it means less of it. The flash cost of moving to `2` is **236 bytes on the RTT image**, because `make build`'s `--only-section` list keeps `.debug_*` out of `firmware.bin`; the price is host-side, a 3 MB ELF becoming 9.5 MB. And probe-rs's own message names the value it wants, so this is not speculation about what "enough DWARF" means.

Two sentences in the tree need correcting too. `firmware/Cargo.toml:36-39` argues that "The DWARF itself
costs no flash ... Measured both ways, firmware.bin is 88101 bytes either way". The invariant is real but
narrower than it reads: it was measured for line tables against no debug info, where the two agree byte for
byte, and the frozen figure is stale - the console-form image built the same way is 88613 bytes today.
`debug = 2` is a different pair and it is not free, 236 bytes more. A reader who takes the sentence to mean
"any debug level costs nothing" will make the wrong call here. And `README.md:230` advertises "release line
tables for symbolication", which is exactly the level probe-rs rejects for locations.

What stays unknown until a board exists (TASK-037): whether `2` makes a panic backtrace decode fully where today's value does not. probe-rs stops unwinding at the first PC lacking debug info (#896), reports truncated frames with custom panic handlers (#2274) and has shipped wrong stack traces before (#3309), so a partial trace will be normal either way.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The check is reproduced with its output recorded: `probe-rs attach <ELF> --chip STM32H750IBKx --non-interactive --list-rtt` prints the "Insufficient DWARF info" warning against a fresh build at the current profile value and does not print it against a fresh build at `debug = 2`, both without a board attached.
- [x] #2 firmware/Cargo.toml sets a deliberately chosen `debug` level whose comment carries the measured per-level table (rustc flag, ELF bytes, .debug_* bytes, firmware.bin bytes) and names both the host-side and flash-side costs. The frozen "88101 bytes either way" figure is gone, along with any implication that every debug level is free.
- [x] #3 Both feature configurations are built at the chosen level and their `firmware.bin` sizes reported, with remaining headroom against the 131,072-byte internal-flash budget stated for the console-form image, and no stale size figure left anywhere in the tree.
- [x] #4 docs/reference/daisy-seed3.md, the Toolchain bullet in docs/reference/rust-daisy-stack.md and README.md's probe section describe what is actually shipped, quoting probe-rs's message verbatim and giving the one-line command that checks it. No existing heading is renamed, so the anchors into the probe section still resolve.
- [x] #5 Clean-build wall time is recorded for the old and new level on the same machine, and the comment reflects the result if the difference is material.
- [x] #6 TASK-037's Implementation Notes gain that backtrace quality at the chosen level is still its measurement, citing probe-rs #896, #2274 and #3309, without creating a new hardware ticket.
- [x] #7 Host gates green in nix develop: fmt, the four clippy invocations, both RUSTDOCFLAGS=-D warnings doc runs, cargo test --workspace, and both firmware cross-compiles from ci.yml.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## What is actually being decided

One manifest line (`debug = "line-tables-only"` -> presumably `2`) plus three places where prose claims
something else. The decision rule: locations in the log stream are worth more than 236 bytes of a
128 KB budget and more than a bigger host-side ELF. If the executor disagrees, the ticket is still
satisfied by shipping the *other* choice provided the reasoning and the numbers land in the comment -
what must not survive is a profile chosen by guess and described inaccurately.

## Reproduce before you change anything

```
cd firmware
for L in cur full; do
  D=/tmp/dwarf-$L
  [ $L = cur ] && E="" || E="CARGO_PROFILE_RELEASE_DEBUG=2"
  env $E CARGO_TARGET_DIR=$D cargo build --release --no-default-features --features "seed3 log-defmt"
  probe-rs attach $D/thumbv7em-none-eabihf/release/main --chip STM32H750IBKx --non-interactive --list-rtt
done
```

Expect, verbatim, on the current build only:
`WARN probe_rs::util::rtt::processing: Insufficient DWARF info; compile your program with \`debug = 2\`
to enable location info.` Both runs then fail with `Error: No connected probes were found.` (rc=1);
that is the expected terminal state with no board, and the warning is emitted before probe discovery,
which is what makes this check work without hardware. Save both outputs; they are the evidence for
acceptance criterion #1 and belong in the notes.

The four-way table in the description was produced exactly this way on 2026-09-12 with
`CARGO_TARGET_DIR` scratch dirs, section sizes read straight out of the ELF section header table, and
`firmware.bin` regenerated per level with `make build`'s own `--only-section` list
(`.vector_table .text .rodata .data *.sgstubs *.defmt*`). Re-measuring is cheap; contradicting those
numbers in the notes requires re-running them, not eyeballing.

## Steps

1. Set the chosen value in `firmware/Cargo.toml`. Everything else in `[profile.release]` stays inherited
   as it is today (`opt-level=3`, `codegen-units=16`, `lto=false`, `panic=unwind`, no `strip`) - the
   existing comment already explains why `panic` and `lto` are untouched, and nothing here changes that.
2. Rewrite the comment block at `firmware/Cargo.toml:33-43` around the measured table. It must say:
   which level is chosen and why (probe-rs's location-info requirement, quoted), what it costs in
   flash (bytes, measured) and on host (ELF bytes, measured), that `.debug_*` is non-ALLOC and is kept
   out of `firmware.bin` by `make build`'s section list, and that debuginfo perturbs codegen, which is why
   the frozen "88101 bytes either way" sentence cannot stay: it was measured for line tables against none,
   and it invites the reading that no debug level costs flash.
3. Measure both images at the new level and record real numbers: `make build FEATURES="seed3"` and
   `make build FEATURES="seed3 log-defmt" NO_DEFAULT=1`, reporting `wc -c firmware.bin` each time and
   the remaining headroom against 131,072 bytes for the **console-form** image, which is the large one
   (~88.6 KB today). Do not leave any older size figure in the tree un-refreshed.
4. Correct the three prose claims: `docs/reference/daisy-seed3.md` around :391-395 (what probe-rs
   decodes from the ELF and what it refuses to), the `Toolchain` bullet in
   `docs/reference/rust-daisy-stack.md:101-109`, and `README.md:228-245` ("release line tables for
   symbolication"). Include the verbatim warning and the one-line reproduction command so the claim is
   checkable by anyone who doubts it. Adding `####` children inside the probe section is allowed;
   renaming the `###` heading is not (inbound anchors at rust-daisy-stack.md:108-109, README.md:247).
5. Record the build-time delta honestly: one clean release build per level, wall clock, same machine.
   If it is material, say so in the comment rather than in a footnote.
6. Append to TASK-037's Implementation Notes that backtrace quality at the chosen level is still its
   measurement, citing probe-rs #896, #2274 and #3309 so a partial trace at the bench is not mistaken
   for a broken build. Do not create a new HUMAN ticket; TASK-037 already carries the hardware half.
7. Gates: fmt, four clippy invocations, both RUSTDOCFLAGS doc runs, `cargo test --workspace`, and both
   firmware cross-compiles from ci.yml, in `nix develop .#default`.

## Guard

`firmware.bin` must stay under 131,072 bytes for both feature configurations, and the DFU recipes must
not move: diff `make -n build flash flash-all check` before and after.

## Not in scope

`split-debuginfo` experiments, `-C force-frame-pointers`, `panic-probe`, LTO or `codegen-units` tuning,
and any claim about how good a backtrace looks - that last one needs the board and belongs to TASK-037.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Repro of AC #1, boardless, probe-rs 0.32.0

Each row is a clean `cargo build --release --no-default-features --features "seed3 log-defmt"`
into its own `CARGO_TARGET_DIR` (/tmp/v2-*), then `probe-rs attach <ELF> --chip STM32H750IBKx
--non-interactive --list-rtt`. Section sizes read off the ELF section header table with
`llvm-objdump -h`; `firmware.bin` cut with `make build`'s own `--only-section` list.

| debug | rustc flag (146 compilations) | ELF | .debug_* | .debug_info | firmware.bin | WARN | wall |
|---|---|---|---|---|---|---|---|
| `false` (env `0`) | none passed | 262,328 | 0 | - | 48,084 | yes | 20 s |
| `"line-tables-only"` (was shipped) | `-C debuginfo=line-tables-only` | 3,007,876 | 2,745,028 | 916,073 | 48,084 | yes | 21 s |
| `1` | `-C debuginfo=1` | 4,735,756 | 4,472,411 | 1,181,667 | 48,228 | yes | 20 s |
| `2` (now shipped) | `-C debuginfo=2` | 9,481,280 | 9,217,836 | 3,555,137 | 48,320 | **no** | 20 s |

Verbatim at line tables (`false` and `1` produced the identical pair):

    WARN probe_rs::util::rtt::processing: Insufficient DWARF info; compile your program with `debug = 2` to enable location info.
    Error: No connected probes were found.          # rc=1

At `2`: only `Error: No connected probes were found.`, rc=1. The warning precedes probe discovery,
which is what makes this checkable without hardware; rc is 1 either way because no probe is
attached. Every number lands within ~130 bytes of this ticket's plan table - DWARF embeds build
paths, so the ELF is not byte-stable across scratch dirs.

## Decision

`debug = 2` ships. Log locations are worth 236 bytes of the 131,072-byte internal-flash budget
(48,084 -> 48,320 on the RTT image) and host-side ELF growth of 3.0 MB -> 9.5 MB. Alternative
considered and dropped: `debug = "packed"`, which is a `split-debuginfo` experiment and out of
scope here. It is not even reachable through `CARGO_PROFILE_RELEASE_DEBUG` - cargo rejects the
string in that env var, accepting only bool/0/1/2/none/limited/full/line-tables-only/
line-directives-only - so measuring it would have meant a manifest edit for a value this ticket
already excluded.

## Sizes at the chosen level (AC #3)

`make build FEATURES="seed3"` -> `firmware.bin` **88,741 bytes**, leaving 42,331 bytes (32.3%) of
the 131,072-byte budget free. At line tables the same tree gave 88,677, so the console image pays
64 bytes rather than 236: the flash cost is per-image codegen perturbation, which is exactly why
the old "88101 bytes either way" sentence had to go.
`make build FEATURES="seed3 log-defmt" NO_DEFAULT=1` -> **48,320 bytes**. Guard held:
`make -n build flash flash-all check` is byte-identical before and after, and `probe-rs attach`
against the manifest-built RTT ELF now prints no DWARF warning.

## Build time (AC #5)

Three clean release builds per level, fresh target dir each, same M1 Pro: 19/20/21 s at line
tables against 20/20/21 s at `2`. Immaterial, and stated in the manifest comment rather than
footnoted.

## Files touched

- `firmware/Cargo.toml`: `debug = 2`, with the measured table and both costs in its comment.
- `docs/reference/daisy-seed3.md`: the two gates under *Flashing and logging over an ST-Link
  probe* became three (locations need `debug = 2`), warning quoted verbatim, one-line check
  included, and the exit-code table row rewritten for the shipped profile. No heading renamed, so
  the inbound anchors still resolve.
- `docs/reference/rust-daisy-stack.md`: Toolchain bullet now says what probe-rs takes from the ELF
  and what it refuses below `2`.
- `README.md`: the probe section no longer advertises "release line tables for symbolication".
- `crates/asperitas-logging/src/defmt_log.rs`: the `DEFMT_LOG` size ladder re-measured at `2`.
- `firmware/Makefile`: the "correct ~32 KB binary" claim replaced by the two measured figures.

## One finding nobody asked for

Re-measuring the `DEFMT_LOG` ladder turned up a shape change: at `debug = 2` the `info` build is
47,712 bytes, *below* the error-only baseline of 48,320, where at line tables it was 48,304 above
48,084. Both reproductions are stable across rebuilds (checked twice each). Not investigated - it
is codegen perturbation from debug info, and the doc now says plainly that the ladder belongs to
the profile it was measured at. TASK-055 owns the defmt record generally; noted there.

## Straggler sweep beyond this ticket's title

Completed-ticket notes elsewhere in `backlog/tasks/` still quote 88101 / 88613 / 48084. Left alone
deliberately: those are dated measurements recorded by the tickets that took them, and rewriting
history to match today's profile destroys evidence rather than refreshing it. Every live doc,
source comment and Makefile figure now agrees with the shipped build.

## Gates (AC #7)

In `nix develop .#default`: fmt, clippy workspace, clippy `asperitas-logging` log-usb, clippy
`asperitas-logging` log-defmt, clippy with `asperitas-pod/pod-hw`, both `RUSTDOCFLAGS=-D warnings`
doc runs, `cargo test --workspace` x2, `dump_reassemble --selftest`, and both firmware
cross-compiles from ci.yml. All rc=0.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Chose `debug = 2` for `[profile.release]` and made the record match it. Reproduced the check
boardless across four levels: probe-rs 0.32.0 prints "Insufficient DWARF info; compile your
program with `debug = 2` to enable location info." at `false`, `1` and `line-tables-only` alike,
and goes silent only at `2`. Bought log locations for 236 bytes of flash on the RTT image
(48,084 -> 48,320) and a host ELF that grows 3.0 MB -> 9.5 MB; clean-build wall time is unchanged
at ~20 s (three builds per level). Console-form `firmware.bin` is 88,741 bytes, 42,331 short of
the 131,072-byte budget, and `make -n build flash flash-all check` is byte-identical to before.

Killed the frozen "88101 bytes either way" claim and every other live figure the change staled:
the manifest comment now carries the measured per-level table, `defmt_log.rs`'s `DEFMT_LOG` ladder
is re-measured at the shipped level, and the Makefile's "~32 KB binary" states both real sizes.
README, the rust-daisy-stack Toolchain bullet, and daisy-seed3's probe gates and exit-code table
now describe what ships, quoting probe-rs's warning verbatim with the one boardless command that
checks it; no heading renamed, so inbound anchors still resolve. TASK-037's notes gained that
backtrace quality is judged at `2`, and that #896/#2274/#3309 make a truncated trace normal there.
Full ci.yml gate set green in nix develop. One unowned finding filed forward: at `debug = 2` the
`info` defmt build is smaller than error-only (47,712 vs 48,320), the reverse of line tables,
reproduced twice and left to TASK-055.
<!-- SECTION:FINAL_SUMMARY:END -->
