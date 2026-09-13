---
id: TASK-062.03
title: >-
  Machine-check which cfg set the gate pair leaves in target/ with a push-tier
  provenance gate
status: Done
assignee:
  - '@agent'
created_date: '2026-09-13 13:02'
updated_date: '2026-09-13 14:54'
labels:
  - task
  - planned
dependencies:
  - TASK-062.02
parent_task_id: TASK-062
priority: medium
ordinal: 116800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Turn TASK-062.02's mechanism into an invariant the gate set enforces, and answer TASK-062 AC #4 with a red/green run instead of a paragraph.

scripts/gates.sh:26-33 states ordering rule 1 as prose: the console cross-build comes immediately before the RTT-only one, nothing builds firmware after them, because whichever ran last is what firmware/target/thumbv7em-none-eabihf/release/main names and every make probe-* decodes whatever that path holds. Prose is not enforcement - a future gate line can violate it silently and the only symptom is a bench decoding defmt frames with the wrong symbols. One push-tier gate line after the pair asserts that release/main carries the RTT-only provenance, so the adjacency and the order become machine-checked and the residue question stops being folklore.

Depends on TASK-062.02 for scripts/elf-provenance.sh; this ticket adds no parsing of its own.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Exactly one new gate line, tier push, declared after "=== firmware cross-compile (RTT-only, log-defmt) ===" and before the two cross-clippy gates, calling scripts/elf-provenance.sh check on firmware/target/thumbv7em-none-eabihf/release/main with the RTT-only expectation. It shells out to bare rust-objcopy only: no cargo objcopy, no cargo objdump, no make build-elf, because any of those rebuilds and would itself re-point the very path the gate reads.
- [x] #2 Red case demonstrated, not asserted: with a console-cfg ELF sitting at that path (either by running the two cross-builds in reverse order or by copying a console ELF over it), scripts/gates.sh push fails naming both feature sets and exits 1; after rebuilding the pair in the documented order the same gate passes. Paste both runs.
- [x] #3 The gate builds nothing. Prove it the way TASK-058 did: sha256 over stat -c "%n %Y %s" for everything under firmware/target before and after the gate run, byte-identical, plus the warm wall-clock cost of the new gate (expect well under 0.2 s).
- [x] #4 backlog/docs/doc-001 gate matrix regenerated from scripts/gates.sh --list rather than hand-edited, including the counts line and the measured commit/push/ci cost line, and the prose restatement of the two ordering rules updated to say rule 1 is now checked.
- [x] #5 gates.sh header rule 1 reworded to name the gate that enforces it instead of describing the blindness as accepted, and a coordination comment left on TASK-059.02 telling it to place its gate after this one - its plan currently says "immediately after the doc-artifact gate (gates.sh:264)", but the doc-artifact gate is at :186, well before both cross-builds, so that coordinate cannot satisfy its own AC #5.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Land only after TASK-062.02; reuse its `check` mode with the RTT-only expectation rather than
   parsing anything here.
2. Add the one gate line in the position named in the notes, with a short comment that names rule 1
   and forbids the cargo objcopy/objdump forms by reference to what they would do to the path being
   checked.
3. Prove red, then green, per AC #2, and take the no-write evidence per AC #3 in the same run so the
   numbers are comparable.
4. Regenerate doc-001's matrix from `scripts/gates.sh --list`, update both cost lines from a fresh
   paired warm measurement against HEAD (the way TASK-061.02 recorded its before/after), and reword
   rule 1 plus the stale "console image in the hook tiers" sentence.
5. Leave the coordination comment on TASK-059.02, then run `scripts/gates.sh ci` once end to end.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Placement

`scripts/gates.sh` runs gates in declaration order (`gate <min-tier> <banner> [-C dir] <cmd...>`,
helper at :100-152). Insert after the RTT-only cross-build at :241-242 and before the two
cross-clippy gates at :245-252, so the artifact it reads is exactly what rule 1 promises and clippy
still reuses the build artifacts above it. `gate push` puts it in pre-push and CI; the commit tier
never builds firmware, so it correctly does not run there.

The gate line takes no `-C`: `elf-provenance.sh` anchors its own ROOT the way
`check-doc-artifact-names.sh:49-50` does and passes an absolute ELF path to rust-objcopy.

## What "the residue" actually is today, measured by reading the tiers

Both cross-builds are `push`-tier, so every tier that runs them ends on the RTT-only image - including
pre-push, which contradicts the comment at :231-234 claiming "deliberately the console image in the
hook tiers". AC #4 of TASK-062 asks for a note about what CI leaves behind; this ticket replaces that
note with a check, and the executor should correct that stale sentence while touching the file.

## Cost and safety

rust-objcopy --dump-section on a ~9.5 MB ELF plus one string compare. Nothing else. If the ELF is
absent the script exits 2 and the gate fails loudly - a missing artifact means the pair did not run,
which is itself a rule violation, and silently skipping would recreate the blind spot this ticket
exists to close.

## Landed 2026-09-13

One gate line, `push` tier, placed after the RTT-only cross-build and before both cross-clippy gates:

    gate push "=== firmware ELF cfg provenance ===" \
      bash scripts/elf-provenance.sh check \
        firmware/target/thumbv7em-none-eabihf/release/main "$RTT_ONLY_FEATURES" 1

`RTT_ONLY_FEATURES="seed3 log-defmt"` is now one definition shared by the build line above and this
check. Two literals would have been free to drift, and a drifted expectation makes the gate refuse the
image its own neighbour just built - which would have been this ticket shooting itself in the foot.

### AC #2: red, through the real runner, then green again

Red was produced by violating rule 1 for exactly one run: a copy of `scripts/gates.sh` with the two
cross-build stanzas physically swapped (RTT first, console last), copied over the real file, tier run,
file restored from a backup taken beforehand.

    === firmware cross-compile (RTT-only, log-defmt) ===   Finished in 0.07s   --- 0.13s
    === firmware cross-compile ===                         Finished in 0.07s   --- 0.13s
    === firmware ELF cfg provenance ===
    firmware/target/thumbv7em-none-eabihf/release/main was linked with default=1 features=log_usb,seed3;
    you asked for default=0 features=log_defmt,seed3 (FEATURES="seed3 log-defmt", NO_DEFAULT=1)

    *** gate failed: === firmware ELF cfg provenance ===
    *** tier: push (13 of 14 gates completed before it)
    push rc=1

Residue afterwards read `default=1 features=log_usb,seed3`, so the failure describes the tree rather
than the checker. Restoring the file and re-running: `tier push: 17 gates, 73.0s`, rc=0, new gate
0.09 s. A second clean run measured 74.0 s.

The other half of AC #2's options - copying a console ELF over `release/main` and running the tier -
does not go red, and the reason is worth recording because it says something about cargo: the RTT-only
build gate runs before this one, finishes in 0.07 s having relinked nothing, and re-uplifts the cached
RTT hardlink anyway. The copied bytes are overwritten by the mechanism, so the state cannot survive
to the check. Ordering violations are the reachable failure.

### AC #3: the gate builds nothing

    $ find firmware/target -exec stat -c '%n %Y %s' {} + | LC_ALL=C sort | sha256sum
      b68c4a4d80647afadd6196cda91a4533fd3b33089e1602a61aa2ec6f19f09b96
    $ bash scripts/elf-provenance.sh check firmware/target/.../release/main "seed3 log-defmt" 1   # rc=0
    $ ... again
      b68c4a4d80647afadd6196cda91a4533fd3b33089e1602a61aa2ec6f19f09b96      <- identical, sizes and mtimes

Ten warm invocations cost 0.764 s total, so ~0.08 s per gate, matching the 0.08 s / 0.09 s the tier
runs printed. No cargo process at all: asking with NO_DEFAULT=1 skips the metadata derivation, so the
only child process is rust-objcopy.

### AC #4 / #5: docs and prose

doc-001's matrix block is now byte-identical to `scripts/gates.sh --list` output (compared
programmatically, not by eye): counts commit 9, push 17, ci 18. Costs updated to the measured
commit 2 s, push 73 s (twice: 73.0 s, 74.0 s), ci 140 s, with the two cargo-test gates named as 133
of the 140. The ordering-rule paragraph gained a paragraph saying rule 1 is checked and how it was
measured. gates.sh rule 1 names the enforcing gate instead of describing the blindness as accepted,
and the pair's own comment no longer claims the hook tiers end on the console image - they do not,
both builds are push-tier, so every tier that runs them ends on the RTT-only one.

TASK-059.02 already carried the coordination note this AC asked for; it cited gates.sh:241-242, which
this ticket moved. Appended the landed coordinates so it does not chase stale line numbers twice.
<!-- SECTION:NOTES:END -->
