---
id: TASK-054
title: >-
  Choose the release DWARF level deliberately: probe-rs needs debug = 2 to
  report defmt log locations
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-12 07:08'
updated_date: '2026-09-12 07:33'
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
- [ ] #1 The check is reproduced with its output recorded: `probe-rs attach <ELF> --chip STM32H750IBKx --non-interactive --list-rtt` prints the "Insufficient DWARF info" warning against a fresh build at the current profile value and does not print it against a fresh build at `debug = 2`, both without a board attached.
- [ ] #2 firmware/Cargo.toml sets a deliberately chosen `debug` level whose comment carries the measured per-level table (rustc flag, ELF bytes, .debug_* bytes, firmware.bin bytes) and names both the host-side and flash-side costs. The frozen "88101 bytes either way" figure is gone, along with any implication that every debug level is free.
- [ ] #3 Both feature configurations are built at the chosen level and their `firmware.bin` sizes reported, with remaining headroom against the 131,072-byte internal-flash budget stated for the console-form image, and no stale size figure left anywhere in the tree.
- [ ] #4 docs/reference/daisy-seed3.md, the Toolchain bullet in docs/reference/rust-daisy-stack.md and README.md's probe section describe what is actually shipped, quoting probe-rs's message verbatim and giving the one-line command that checks it. No existing heading is renamed, so the anchors into the probe section still resolve.
- [ ] #5 Clean-build wall time is recorded for the old and new level on the same machine, and the comment reflects the result if the difference is material.
- [ ] #6 TASK-037's Implementation Notes gain that backtrace quality at the chosen level is still its measurement, citing probe-rs #896, #2274 and #3309, without creating a new hardware ticket.
- [ ] #7 Host gates green in nix develop: fmt, the four clippy invocations, both RUSTDOCFLAGS=-D warnings doc runs, cargo test --workspace, and both firmware cross-compiles from ci.yml.
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
