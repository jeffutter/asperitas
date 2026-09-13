---
id: TASK-062
title: >-
  elf-check cannot see an ELF built for the wrong cfg set, and its own relink
  hint does not refresh the mtime
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-13 00:52'
updated_date: '2026-09-13 14:55'
labels:
  - planned
dependencies:
  - TASK-062.03
references:
  - firmware/Makefile
  - .github/workflows/ci.yml
priority: medium
type: task
ordinal: 98800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two facts measured 2026-09-13 while TASK-060.03 ran CI's firmware section verbatim inside nix develop .#default on aarch64-darwin.

1. 'target/thumbv7em-none-eabihf/release/main' names whichever cfg set cargo linked last. ci.yml builds main twice - console ('--features seed3') then RTT-only ('--no-default-features --features "seed3 log-defmt"') - and the top-level name alternates between the two deps/main-<metadata-hash> artifacts: 10,941,616 bytes sha256 9b60b8ff... for the console build, 9,497,564 bytes sha256 16dc9e5c... for the RTT-only one, both with the same 19:41 mtime because they are hardlinks. Makefile's ELF = target/$(TARGET)/release/$(BINARY) is that same name, so every probe-* target decodes whatever the last build happened to be. Running CI's suite locally therefore swaps the bench decoder silently; CI itself is immune only because each run gets a fresh checkout. elf-check cannot catch this: both files are real builds of the same sources, differing only in cfg provenance, and the staleness test compares mtimes against sources.

2. elf-check's printed remedy - 'rm -f target/.../release/main && make build-elf' - does not work when cargo considers the target fresh. After exactly that sequence the file reappeared with its ORIGINAL mtime (cargo re-hardlinked deps/main-9a4db457eb5c4e1d rather than relinking), so the check stayed red on a tree whose content equals HEAD. The remedy needs to force an actual relink, or the staleness test needs to stop being mtime-based - which is TASK-056.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 #1 The cfg provenance of the ELF at target/thumbv7em-none-eabihf/release/$(BINARY) is knowable to the host without a board - either the artifact name carries it or elf-check reads it from the ELF itself - and a mismatch against FEATURES/NO_DEFAULT is reported by name, not inferred from mtimes.
- [x] #2 #2 Reproduced boardless: build main console, then main RTT-only, then confirm the check distinguishes the two states instead of passing both. Measured digests to aim at: 9b60b8ffde4d2270e9a043feb300f50283762cdeee537ca81537f36b937fe80b (console, 10,941,616 B) and 16dc9e5cdff48c3e433171060bb04c33fa137dd1fec0effd0dac6150206ee431 (RTT-only, 9,497,564 B).
- [x] #3 #3 The remedy elf-check prints actually restores a usable state: after following it verbatim, 'make elf-check' exits 0 and the ELF's mtime is newer than every source it was built from.
- [x] #4 #4 Notes record whether the fix changes what CI's two firmware builds leave behind in target/, since TASK-060.03's clippy gates now run in the same section.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Shape

Three leaf tasks, all `@agent`, chained `.01 -> .02 -> .03`. This umbrella closes when all three are
Done; its four acceptance criteria map onto them as: #1 = .01 plus .02, #2 = .02, #3 = .02, #4 = .03.

## Why provenance rides inside the ELF

Measured while planning (rust/cargo 1.97.1, copy of this tree in `/tmp/pl62fw`):

- Switching cfg sets makes cargo re-uplift the top-level name from the cached `deps/main-<hash>` and
  **inherit the old artifact's mtime**: after switching back to the console build the ELF read mtime
  1789302036 while the wall clock said 1789302070. Content correct, timestamp rewound. Upstream:
  cargo #15313 (external output modification not detected as stale), #8649 (output mtimes after
  cached builds); `-Zchecksum-freshness` is still unstable and `--artifact-dir` nightly-only.
- The remedy elf-check prints today does not recover from that. `rm -f <ELF> && make build-elf`
  reproduced the same unchanged mtime verbatim; deleting
  `target/<triple>/release/.fingerprint/asperitas-firmware-*`, or `cargo clean -p asperitas-firmware`,
  both really relink and refresh.
- A host-side record cannot work here. `scripts/gates.sh:235` and `:241-242` build both cfg sets with
  raw cargo, never through make, so anything a make recipe writes beside the ELF describes a
  different artifact than the name currently holds. Reading `.fingerprint/*/bin-main.json` works
  today and correctly reported `["log-defmt","seed3"]`, but pins us to a cargo-private schema and
  dies on `cargo clean` or a copied artifact. So the blob goes into the image at link time.
- A non-allocated note section survives the real link with no KEEP fragment and costs no flash:
  `rust-objdump -h` shows `.asp.prov` at VMA/LMA 0x0 beside `.defmt` in all six bins, and boardless
  `probe-rs attach --list-rtt` still stops at "No connected probes were found" rather than a parse
  error. One caveat recorded honestly in .01: five of six bins keep byte-identical `.text`, `main`
  grows by 156 B because the asm blob is one more input object (image 88,613 -> 88,773 of the
  131,072-byte budget).

Rejected alternatives, with reasons, are written into .01's description: dual `[[bin]]` targets
(breaks the six-bin inventory, the doc-name gate and quoted error texts), a sidecar (paragraph
above), fingerprint JSON (private schema).

## Sequencing against the neighbours

No hard dependency on TASK-056, deliberately. Both tickets rewrite `elf-check`'s body and its failure
message, but each lands a complete mechanism alone: .02 adds a provenance clause beside whatever
staleness test is current and fixes the remedy independently, which is what makes AC #3 satisfiable
whether or not TASK-056 has run. Whichever lands second integrates once - .02's AC #8 says so, and a
coordination comment is going on TASK-056 now. TASK-059.02 gets the opposite note: it must place its
gate after .03's and reuse the script rather than inventing its own ELF reader, and its plan's
`gates.sh:264` coordinate is stale (the doc-artifact gate is at :186).

## No `@human` split, and why

Nothing here needs the board. Every criterion is answerable with rust-objcopy, cargo, make and
`probe-rs` refusing politely with no probe attached - probe-rs parsing the ELF is exactly the
board-side risk that mattered, and it is observable boardless because a `log-defmt` ELF makes
probe-rs reach "No connected probes were found". Ears would only be needed to claim the bench
decoder behaves better in practice, which no AC here asserts and TASK-059.03 already owns for the
audio path. Keeping the whole chain `@agent` is therefore honest, not over-reach.

## Integration check when all three are Done

`scripts/gates.sh ci` green; `make elf-check` passes for the cfg set you ask about and fails by name
for the other one; the printed remedy restores rc=0 and a fresh mtime; docs match `make -n`; doc-001's
gate matrix matches `scripts/gates.sh --list`. Then this ticket can close, and gates.sh rule 1's
"do not fix it here" note retires.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Discovered by TASK-060.03's executor, which hit both facts while proving the new gates and had to restore the session-start ELF digest by hand. Related but distinct: TASK-056 makes the staleness test content-based so a bulk mtime refresh cannot fail it spuriously - that ticket covers false RED, this one covers the false GREEN from cfg ambiguity plus a broken recovery path. Coordinate so one does not rewrite what the other depends on.

Second reproduction while executing TASK-060.04 minutes later, same shell: pre-push's new firmware-cross-compile-rtt job left target/thumbv7em-none-eabihf/release/main at sha256 16dc9e5cdff48c3e433171060bb04c33fa137dd1fec0effd0dac6150206ee431 (9,497,564 B, RTT-only cfg) with mtime unchanged at Sep 12 19:41; 'rm -f $ELF && make build-elf' put it back to 9b60b8ffde4d2270e9a043feb300f50283762cdeee537ca81537f36b937fe80b (10,941,616 B, console cfg), again WITHOUT a fresh mtime, which is why 'make elf-check' stayed red afterwards. Two things this ticket owns: the name cannot tell the two cfg sets apart, and the remedy elf-check prints does not refresh the timestamp it compares.

## Integration check, run 2026-09-13 with all three leaves landed (tip was 7251cd8)

The plan's four-part check, each part executed rather than asserted:

- `bash scripts/gates.sh ci` -> 18 gates, 140.0 s, rc=0.
- `make elf-check FEATURES="seed3 log-defmt" NO_DEFAULT=1` -> rc=0 against the residue a full push
  run leaves; `make elf-check FEATURES="seed3"` against the same ELF -> rc=2 with the first line
  naming `default=0 features=log_defmt,seed3` against what was asked. One name, two images, now two
  verdicts.
- The remedy that check prints was eval'd verbatim and restored rc=0 plus an ELF newer than every
  input (numbers in TASK-062.02's notes).
- `make -n build flash flash-all check` from this tree is byte-identical to the same command from the
  tree at 6083953, before .01 touched build.rs, apart from make's own directory banner. Nothing in the
  DFU path moved.
- doc-001's matrix block equals `scripts/gates.sh --list` output compared programmatically.

### AC #4, answered as a measurement rather than a paragraph

CI's two firmware builds leave `release/main` holding the **RTT-only** image, and they always did:
both cross-builds are push-tier, so every tier that runs them ends on the second one. The comment that
claimed the hook tiers deliberately ended on the console image was wrong and is corrected in
`scripts/gates.sh`. What changed for TASK-060.03's two cross-clippy gates, which sit directly below
the pair and reuse its artifacts: nothing observable. `cargo clippy` links no binary, so it cannot
re-point the top-level name - measured by running the whole push tier and reading the provenance
afterwards (`default=0 features=log_defmt,seed3`, matching what the gate asserts mid-run). The new
gate sits between the pair and those clippy gates, costs 0.08-0.09 s, and touches nothing under
`firmware/target`: a stat-and-size digest of the whole tree is identical across a run of it.

### Scope note, said plainly

This ticket arrived assigned as the umbrella while .01 and .02 had their code committed (b40e1b9 and
7251cd8) but their statuses still saying To Do with every box unchecked. Closing the umbrella therefore
meant verifying both leaves against that tip and finalizing them, doing .03 itself, and closing the parent
- not starting a fourth thing. One gap surfaced and went to a follow-up ticket instead of being
papered over: 7251cd8's commit message claims elf-provenance's behaviours are covered by "the script's
own tests", and no such file exists in the repo.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-13 04:16
---
Path update from TASK-061.02: the pair of builds that alternate `release/main` between the console and RTT-only artifacts now live in `scripts/gates.sh` (`=== firmware cross-compile ===` then `=== firmware cross-compile (RTT-only, log-defmt) ===`), not in ci.yml. Their adjacency and order are load-bearing enough that the script states them as an ordering rule in its header and lets no later gate build firmware at all. So the alternation you describe keeps happening in the same shape, and any fix here has to work with the end state being "whatever build ran last" rather than assume a fresh checkout.
---

created: 2026-09-13 07:02
---
Coordination note from planning TASK-059 (2026-09-13): TASK-059.02 adds scripts/check-image-load-addresses.sh plus one push-tier gate line in scripts/gates.sh, and its header will state plainly that it validates whichever cfg set built last - your provenance blindness, acknowledged rather than papered over. Its ACs forbid the `test -f ... || true` else-branch pattern this ticket calls out. If you land first, say so in .02's notes so it reuses your mechanism instead of duplicating it.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
All three leaves are Done and the blindness is gone. Every firmware ELF carries the cfg set it was
linked with in a non-allocated `.asp.prov` note written by `firmware/build.rs` (TASK-062.01);
`scripts/elf-provenance.sh` is the repo's only reader and `make elf-check` refuses an ELF whose set
differs from the FEATURES/NO_DEFAULT it was handed, naming both sides, with a remedy that forces a
real relink instead of the one that demonstrably left the old mtime (TASK-062.02); and
`=== firmware ELF cfg provenance ===` makes gates.sh ordering rule 1 machine-checked, so a swapped
cross-build pair or a later firmware build fails the push tier rather than quietly mislabelling every
defmt frame the bench decodes (TASK-062.03).

Measured, all boardless: the same `make elf-check` passes one cfg set and refuses the other in both
directions; its printed remedy restores rc=0 in 2 s with a fresh mtime; the new gate costs 0.08-0.09 s,
builds nothing (stat digest of `firmware/target` identical across a run), goes red through the real
runner when the pair is reversed and green once restored; `gates.sh ci` is 18 gates in 140.0 s;
`make -n build flash flash-all check` is unchanged against the pre-.01 tree; doc-001's matrix equals
`gates.sh --list`.

One honest gap went to a follow-up instead of into a checkbox: 7251cd8's commit message describes tests
for the provenance reader that no file in the repo contains.
<!-- SECTION:FINAL_SUMMARY:END -->
