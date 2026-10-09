---
id: TASK-071
title: >-
  Carry elf-check's source digest inside the ELF so every build path keeps it
  current
status: Blocked
assignee:
  - '@agent'
created_date: '2026-10-08 01:49'
updated_date: '2026-10-09 11:23'
labels:
  - planned
dependencies:
  - TASK-071.01
  - TASK-071.02
  - TASK-071.03
references:
  - >-
    backlog/tasks/task-069 -
    Decide-whether-elf-checks-freshness-record-belongs-inside-the-image-rather-than-beside-it.md
  - firmware/Makefile
  - firmware/build.rs
  - scripts/check-elf-staleness.sh
  - scripts/elf-provenance.sh
priority: medium
type: chore
ordinal: 125800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`elf-check` guards `probe-log` and `probe-run`. The host ELF is the defmt decoder, so a stale one silently mislabels live output. Today its freshness record is a sidecar stamp (`release/<binary>.elf-inputs.sha256`) that only `make build-elf` writes (TASK-056). Anything else that relinks the same path leaves the stamp describing an earlier link: the raw `cargo build`s in `scripts/gates.sh`, and `make build`, because `cargo objcopy` compiles first. So editing a source and running the gates makes elf-check go red against an ELF that is actually current.

TASK-069 recorded the owner's decision (2026-10-07): move the digest into the `.asp.prov` note that firmware/build.rs already stamps into every ELF (TASK-062.01). The record then travels with the artifact and stays true whichever route built it. This deliberately reverses TASK-056 AC #3, which put the mechanism in the Makefile and nothing in build.rs. Covering `make build` is in scope.

Facts TASK-069 already measured, so nobody re-researches them:
- `scripts/elf-provenance.sh` ignores unknown blob keys by design (selftest case `unknown-key-ignored`).
- `.asp.prov` is non-allocated, so a bigger blob adds nothing to any `.bin` and is invisible to `check-image-load-addresses.sh`.
- firmware/build.rs opts out of cargo's default change detection (it prints rerun-if directives), so it must itself declare every input it digests.

Owner constraint: the input set is defined in one place that build.rs and elf-check both use. Two hand-maintained lists drifting apart was the main cost TASK-069 raised against this route.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 An ELF linked by any route - `make build-elf`, `make build`, or the raw `cargo build` invocations in `scripts/gates.sh` - passes `elf-check` when it was built from the sources on disk, with no `make build-elf` in between.
- [ ] #2 Editing, adding or removing any digested input after the link makes `elf-check` refuse, and the remedy it prints makes it pass again.
- [ ] #3 Cargo relinks when any digested input changes, including a newly added source file, so an edit can never leave a link whose embedded digest is out of date.
- [ ] #4 The set of digested inputs is defined in exactly one place, used by both the producer in build.rs and the checker in elf-check.
- [ ] #5 An ELF whose `.asp.prov` has no source digest (built before this change) is refused with a message that names the missing field and the remedy, not accepted and not misreported as stale.
- [ ] #6 The sidecar `<binary>.elf-inputs.sha256` stamp is gone: nothing writes it, nothing reads it, and no doc or comment still describes it as the mechanism.
- [ ] #7 `scripts/check-elf-staleness.sh`'s selftest covers at least: a raw-cargo relink passes, an edited input fails, an ELF without the digest is refused. It runs in the commit tier as it does today.
- [ ] #8 The comment block above elf-check in firmware/Makefile, the `provenance()` doc in firmware/build.rs, and every doc that describes elf-check's freshness record are updated in the same change. The Cargo.lock caveat (no gate passes `--locked`) is kept or explicitly re-justified.
- [ ] #9 `scripts/gates.sh commit` and `scripts/gates.sh push` pass, and `docs/gate-costs.json` is re-recorded if any gate's cost moves.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Planned against c3cb3d8
Approach: split along the producer/consumer seam. 071.01 makes build.rs embed a source digest in .asp.prov from a single shared input-set definition, with rerun-if-changed covering inputs and containing directories. 071.02 switches elf-check to the embedded digest, adds the missing-field refusal, and deletes the sidecar stamp. 071.03 rewrites the selftest and docs and runs the gates. Order: 01, 02, 03. Risks: digest must be byte-identical between producer (Rust) and checker, so use one implementation; new-file detection needs directory-level rerun-if-changed; no gate passes --locked so Cargo.lock caveat stays. Integration: raw cargo build then make elf-check passes with no build-elf; edit then check refuses. Final: scripts/gates.sh commit and push. No direct work remains in the parent.
<!-- SECTION:PLAN:END -->
