---
id: TASK-062.02
title: >-
  elf-check reads the cfg set out of the ELF and refuses a mismatch, and the
  remedy it prints forces a real relink
status: Done
assignee:
  - '@agent'
created_date: '2026-09-13 12:58'
updated_date: '2026-09-13 14:54'
labels:
  - task
  - planned
dependencies:
  - TASK-062.01
parent_task_id: TASK-062
priority: medium
ordinal: 115800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Consumer half of TASK-062, and the half that closes both acceptance criteria the ticket actually hurts on. Once every ELF carries .asp.prov (TASK-062.01), make elf-check reads that blob and refuses an ELF whose cfg set differs from the FEATURES/NO_DEFAULT it was handed - reported by name, with no mtime anywhere in the reasoning - and prints a remedy that genuinely restores a usable state.

Today's two failures, both reproduced again on 2026-09-13 while planning:
1. elf-check cannot see cfg provenance at all. It opens the ELF only to test its existence and compares mtimes against ELF_INPUTS, so a console ELF and an RTT-only ELF are indistinguishable to it even though one of them will silently mislabel every defmt frame the bench then decodes.
2. Its printed remedy "rm -f $(ELF) && make build-elf" does not work. Measured on a scratch crate: after exactly that sequence the file came back with mtime 1789302036 while the wall clock read 1789302070, because cargo re-uplifts the cached deps/main-<hash> and inherits the source artifact's mtime rather than relinking (cargo #15313, cargo #8649). Deleting target/<triple>/release/.fingerprint/<pkg>-*, or cargo clean -p <pkg>, do force a real relink and refresh the mtime; both measured.

One script holds the whole mechanism: scripts/elf-provenance.sh, called by elf-check and later by TASK-062.03's gate. Nothing else in the repo learns to parse the blob.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 scripts/elf-provenance.sh is the only thing in the repo that reads the blob. Interface: "show <elf>" prints the normalized provenance on stdout; "check <elf> <FEATURES> <NO_DEFAULT>" exits 0 on a match, 1 on a mismatch naming both sets, and 2 when the check itself cannot run (no ELF, no .asp.prov section, unparseable blob). Header states that contract, following scripts/check-doc-artifact-names.sh:44-45. Quiet on success.
- [x] #2 Reproduced boardless with the real Makefile: build main console, then main RTT-only, and the SAME elf-check invocation passes the first and fails the second naming features=log_defmt,seed3 default=0 against what was asked. Paste both stderr blocks plus rc. The two digests quoted in TASK-062 AC #2 (9b60b8ff..., 16dc9e5c...) identify the two pre-.01 artifacts only - after TASK-062.01 lands every digest moves, so distinguish the states by the blob, never by digest.
- [x] #3 The expected feature set is derived, not restated: FEATURES plus the closure of the default feature taken from cargo metadata --format-version 1 --no-deps --offline (measured 0.074 s warm, jq is in the dev shell), normalized the same way build.rs normalizes CARGO_FEATURE_*. Nothing hard-codes that default means log-usb. Show the derivation is right for a third cfg set too, e.g. BINARY=rig FEATURES="seed3 stim-ess", where the expected set must include stim_ess.
- [x] #4 The remedy elf-check prints restores a usable state, proven by running it verbatim: afterwards make elf-check exits 0 AND the ELF mtime is not older than any file under ELF_INPUTS. Record the measured wall seconds of whatever remedy is printed. rm -f $(ELF) && make build-elf may not appear as a remedy anywhere any more - it demonstrably leaves the old mtime.
- [x] #5 An ELF with no .asp.prov section fails loudly with a message saying the artifact predates the mechanism or came from an older tree, and telling the operator how to get one. No test -f ... || true, no silent pass, no skip when FEATURES is empty.
- [x] #6 DEFMT_LOG is carried in the blob and reported by show, but is NOT enforced by check, and the Makefile comment says why in one sentence: it selects which frames got compiled in rather than which cfg set this is, and a bench session that exports DEFMT_LOG without rebuilding would otherwise be refused an ELF that is exactly the one on the board.
- [x] #7 Prose stays truthful where it quotes elf-check today: docs/reference/daisy-seed3.md:806 exit-code row and the two-layer rc paragraph at :809-813 (a cfg mismatch is now a third thing that arrives as rc=2 through make, so the paragraph must say match on the message), README.md:268-270, daisy-seed3.md:555-558 where the byte scan strings -a <ELF> | grep -c SEGGER is offered as the way to tell the two images apart (supersede it with rust-objcopy --dump-section, spelled rust-objcopy because rule R2 forbids the cargo objcopy token in docs), and the two "with the elf-check lines left out" preambles at :400 and :837 if the guarded expansion changed shape.
- [x] #8 DFU guard holds: make -n build flash flash-all check byte-identical to HEAD. probe-* recipes untouched. If TASK-056 has landed, keep its content-digest staleness test exactly as it wrote it and add the provenance clause beside it; if it has not, leave the find -newer test alone and do not half-implement a digest. Whichever lands second updates the shared failure-message shape once.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Write `scripts/elf-provenance.sh` first, standalone, following `check-doc-artifact-names.sh`'s
   shape (ROOT anchor at :49-50, `set -uo pipefail` without `-e` so findings accumulate, `die()`
   prefix, exit contract stated in the header, quiet on success). Two modes, `show` and `check`, per
   AC #1. Prove it against the two ELFs TASK-062.01 leaves behind before touching any Makefile.
2. Wire it into `elf-check` through the `$(dir $(firstword $(MAKEFILE_LIST)))` form above, after the
   existing existence test, so an absent ELF still produces today's clearer message. Do not reorder
   or re-implement whatever staleness test is current (`find -newer` today, content digest if
   TASK-056 landed first).
3. Replace the remedy line with the forced-relink form. Measure both candidates warm, land the faster
   one, paste the timing, then prove AC #3 by deleting the ELF, running the printed remedy verbatim,
   and showing `make elf-check` rc=0 plus the mtime comparison.
4. Prose pass last, once the messages are final: the exit-code table row, the two-layer rc paragraph,
   README, and the byte-scan paragraph. Re-run every quoted `make -n` block in
   docs/reference/daisy-seed3.md against reality (TASK-053 AC #6 requires them to match modulo
   whitespace).
5. Finish with `scripts/gates.sh ci` green and the DFU guard diff empty, and leave a comment on
   TASK-056 and on TASK-059.02 saying which order landed, since all three touch these files.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Blob format (written by TASK-062.01, read here)

NUL-terminated key=value lines, first line the tag:

    asp-prov1
    default=1
    features=log_usb,seed3
    defmt_log=

Read it with `rust-objcopy --dump-section .asp.prov=<out> <elf>`. rust-readelf and readelf are not
in the dev shell; LLVM 22 needs no dummy output-file argument. Never `-O binary --only-section`: it
prints nothing for a non-allocated section.

## Deriving what the operator asked for

    expected_default  = 0 if NO_DEFAULT=1 else 1
    expected_features = normalize(FEATURES split on spaces)
                        union (defaults enabled ? closure(default) : {})

normalize: lowercase, '-' to '_', drop the literal token `default`, sort, dedupe.

closure(default) comes from `cargo metadata --format-version 1 --no-deps --offline` run with cwd
`firmware` (it is its own workspace): expand only elements naming one of this package's own
features, i.e. skip anything containing `/` and anything starting with `dep:`, transitively. Today
that yields {log-usb}. Measured cost 0.074 s warm. `cargo metadata` never links, so unlike
`cargo objcopy`/`cargo objdump` it cannot re-point release/main mid-gate - that hazard is why
TASK-059.02's plan bans the cargo forms.

Measured truth to test against, both observed on a patched copy of this tree:

    FEATURES="seed3"                             -> default=1 features=log_usb,seed3
    NO_DEFAULT=1 FEATURES="seed3 log-defmt"      -> default=0 features=log_defmt,seed3

## Where the Makefile reaches the script

Verified expansion (GNU Make 4.4.1, also behaves under `-C`):

    PROV = bash $(dir $(firstword $(MAKEFILE_LIST)))../scripts/elf-provenance.sh

which expands to `bash ./../scripts/elf-provenance.sh ...`. This is the first time the Makefile
reaches up into scripts/; scripts/check-doc-artifact-names.sh already reaches down into the Makefile,
so the coupling is mutual - say so in both headers.

## Remedy candidates, measured

On a scratch crate, same cargo 1.97.1, top-level artifact freshly built then cfg-switched:

  - `rm -f <top-level> && cargo build ...` -> content correct, mtime UNCHANGED (1789302036 vs 1789302070)
  - `rm -rf target/<triple>/release/.fingerprint/<pkg>-* && cargo build ...` -> mtime refreshed (== now)
  - `cargo clean -p <pkg> && cargo build ...` -> relinks, mtime refreshed

Print whichever of the last two is cheaper after measuring both warm in firmware/, and record the
seconds. Prefer `cargo clean -p asperitas-firmware` while it stays under ~60 s: public interface.
Reaching into `.fingerprint/` is allowed but must say so in the comment, since cargo's layout is
private and TASK-062.01 deliberately did not depend on it. The printed command must carry the
FEATURES and NO_DEFAULT the operator actually asked for, so it is copy-pasteable rather than a
template.

Keep elf-check a check and never a build: rebuilding there would overwrite the only host copy of the
symbols describing whatever is running on the bench right now. That is why the remedy is printed
rather than run.

## Message shape

Three lines today, keep three. Line 2 currently reads `$(ELF) is older than <path>`; the provenance
failure gets its own named line, e.g. `<ELF> was linked with default=0 features=log_defmt,seed3;
you asked for default=1 features=log_usb,seed3 (FEATURES="seed3", NO_DEFAULT unset)`. Say what the
ELF is, not what the operator did wrong: probe-log and probe-rtt-list are the callers and the
operator is usually mid-capture.

## Closed by the TASK-062 umbrella run 2026-09-13 (code landed in 7251cd8)

Scripts, Makefile and docs shipped; the checkboxes never got ticked, so the umbrella could not close.
Everything below was re-run against 7251cd8 on this tree, boardless, GNU Make 4.4.1, cargo 1.97.1.

### AC #2: the real-Makefile cfg switch, both directions

    make build-elf FEATURES="seed3"                          -> sha 7407afed..., default=1 features=log_usb,seed3
    make elf-check FEATURES="seed3"                          -> rc=0
    make build-elf FEATURES="seed3 log-defmt" NO_DEFAULT=1   -> sha 1516e909..., default=0 features=log_defmt,seed3
    make elf-check FEATURES="seed3"                          -> rc=2 through make, first line:
      target/thumbv7em-none-eabihf/release/main was linked with default=0 features=log_defmt,seed3;
      you asked for default=1 features=log_usb,seed3 (FEATURES="seed3", NO_DEFAULT=unset)
    make elf-check FEATURES="seed3 log-defmt" NO_DEFAULT=1   -> rc=0
    make build-elf FEATURES="seed3"   (back again, then ask RTT-only)
    make elf-check FEATURES="seed3 log-defmt" NO_DEFAULT=1   -> rc=2, same shape naming both sets

Same invocation, opposite verdicts, distinguished by the blob and reported by name. The two pre-.01
digests quoted in the parent ticket are obsolete exactly as AC #2 predicted.

### AC #4: the printed remedy, followed verbatim

Extracted from the failing `make elf-check` output and eval'd as-is:

    touch src/bin/main.rs && make build-elf BINARY=main FEATURES='seed3 log-defmt' NO_DEFAULT=1
      -> rc=0, wall 2 s
    make elf-check FEATURES="seed3 log-defmt" NO_DEFAULT=1   -> rc=0
    the ELF_INPUTS staleness find                                        -> empty afterwards

The mtime rewind that made the old remedy useless is directly observable here: after switching to the
console build the ELF read mtime 1789309512 with the wall clock at 1789309512; switching to RTT-only
put it back to 1789309462 while the clock still said 1789309512. Content correct, timestamp rewound -
cargo #15313 doing precisely what this ticket's plan said it does.

### AC #3 / #5 / #6: derivation and refusal paths

    make build-elf BINARY=rig FEATURES="seed3 stim-ess"
    elf-provenance.sh show  ... release/rig          -> default=1 features=log_usb,seed3,stim_ess
    elf-provenance.sh check ... "seed3 stim-ess" ""  -> rc=0
    elf-provenance.sh check ... "seed3" ""           -> rc=1, naming stim_ess on the ELF side only

    rust-objcopy --remove-section .asp.prov on a copy, then:
    elf-provenance.sh show  <that copy>              -> rc=2
    elf-provenance.sh check <that copy> ...          -> rc=2, two lines saying it predates the stamp
                                                        and how to get one; no silent pass

Reading the blob leaves the artifact alone: three `show` calls moved neither the mtime (1789309462
before and after) nor the sha256, which is what the trailing /dev/null argument buys.

### One thing 7251cd8's message overclaims

7251cd8's message says the /dev/null guard and the four exit-code paths are "asserted in the script's own
tests". There is no test file - searching scripts/ for elf-provenance finds the script itself and
nothing else. The behaviours are real; they are the measurements above. They just live in nobody's
repo. Filed as a follow-up rather than quietly checked off here.
<!-- SECTION:NOTES:END -->
