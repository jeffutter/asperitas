---
id: TASK-062.03
title: >-
  Machine-check which cfg set the gate pair leaves in target/ with a push-tier
  provenance gate
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-13 13:02'
updated_date: '2026-09-13 13:03'
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
- [ ] #1 Exactly one new gate line, tier push, declared after "=== firmware cross-compile (RTT-only, log-defmt) ===" and before the two cross-clippy gates, calling scripts/elf-provenance.sh check on firmware/target/thumbv7em-none-eabihf/release/main with the RTT-only expectation. It shells out to bare rust-objcopy only: no cargo objcopy, no cargo objdump, no make build-elf, because any of those rebuilds and would itself re-point the very path the gate reads.
- [ ] #2 Red case demonstrated, not asserted: with a console-cfg ELF sitting at that path (either by running the two cross-builds in reverse order or by copying a console ELF over it), scripts/gates.sh push fails naming both feature sets and exits 1; after rebuilding the pair in the documented order the same gate passes. Paste both runs.
- [ ] #3 The gate builds nothing. Prove it the way TASK-058 did: sha256 over stat -c "%n %Y %s" for everything under firmware/target before and after the gate run, byte-identical, plus the warm wall-clock cost of the new gate (expect well under 0.2 s).
- [ ] #4 backlog/docs/doc-001 gate matrix regenerated from scripts/gates.sh --list rather than hand-edited, including the counts line and the measured commit/push/ci cost line, and the prose restatement of the two ordering rules updated to say rule 1 is now checked.
- [ ] #5 gates.sh header rule 1 reworded to name the gate that enforces it instead of describing the blindness as accepted, and a coordination comment left on TASK-059.02 telling it to place its gate after this one - its plan currently says "immediately after the doc-artifact gate (gates.sh:264)", but the doc-artifact gate is at :186, well before both cross-builds, so that coordinate cannot satisfy its own AC #5.
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
<!-- SECTION:NOTES:END -->
