---
id: TASK-068
title: 'Assert check-image-load-addresses.sh with fixtures, as a commit-tier gate'
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-14 03:37'
updated_date: '2026-09-14 03:37'
labels:
  - planned
dependencies:
  - TASK-059.02
references:
  - scripts/check-image-load-addresses.sh
  - scripts/elf-provenance.sh
  - scripts/gates.sh
priority: medium
type: chore
ordinal: 118800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-059.02 lands a gate whose whole value rests on a text parser reading `rust-objdump -h --show-lma`
correctly. Nothing asserts that parser. A future edit that breaks it does not turn the gate red, it
turns the gate green for the wrong reason - the exact failure mode TASK-059.02 exists to prevent.

Planning that ticket measured four traps in the parser, each of which silently produces a pass that
checked nothing rather than a failure:

- Bash matches leftmost-longest, so the name capture keeps objdump's column padding: on the row for
  `.sram1_bss` it captures the name plus five trailing spaces. Untrimmed, the `.sram1_bss` comparison
  never fires and assertion #2 reports "absent" for every binary. A prototype hit exactly this bug
  during planning.
- Section names legitimately contain spaces, braces and commas - defmt's interned-string sections are
  named `.defmt.error.` followed by a JSON record containing spaces - so any left-to-right field split
  shreds them.
- The Type column is composed flags (TEXT/DATA/BSS/DEBUG, concatenated), so a row can carry two words,
  and non-allocated rows print nothing at all. Empty must classify as "not loaded", while an
  unrecognised word must be a hard error. Get either wrong and every ELF fails, or a section walks
  through the gate unnoticed.
- `.gnu.sgstubs` is ALLOC+PROGBITS with size 0 at an address *above* the real flash high-water and in
  no PT_LOAD. Counting it breaks rule 3's length equality by 8 bytes on a correct build.

Do for this script what TASK-067 did for `elf-provenance.sh`: fixtures generated at runtime, cases run
through real child processes so the exit codes a gate sees are the ones under test, and a commit-tier
gate beside the elf-provenance selftest so the claim is a check rather than a sentence. Fixtures rather
than the ELFs the cross-build pair leaves behind, for the same reason TASK-067 gave: ordering rule 1
means `release/main` holds the RTT-only image and nothing else, and the commit tier builds no firmware
at all.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 scripts/check-image-load-addresses.sh grows a --selftest that follows elf-provenance.sh's shape: run_case plus spawn_script so each case re-executes the script as a child and the real exit code is graded, one prefixed "selftest <name> ok" line per case on stderr, a passed(N cases) summary, and exit codes 0 pass / 1 a case failed / 2 the suite could not run.
- [ ] #2 Cases cover all four parser traps named in the description, plus both halves of the Type classification: a padded name still equals .sram1_bss; a defmt-style name containing spaces and braces parses with its type intact; a composed two-word type is refused by naming the section rather than skipped; a zero-size ALLOC+PROGBITS section above the high-water mark does not move the expected image length; an empty Type classifies as not-loaded; an unknown Type word is a hard error naming the section and the raw row.
- [ ] #3 At least one case drives the real rust-objdump over a hand-emitted ELF32 little-endian ARM image whose PT_LOAD has p_paddr different from p_vaddr, so the LMA column this script parses positionally is asserted rather than assumed. Fixtures generated at runtime, never committed as blobs - elf-provenance.sh:220-247 gives the reasoning. Also assert the two guards: a file format other than elf32-littlearm is exit 2, and a missing ELF is exit 2 naming the command that would produce it.
- [ ] #4 Registered as a commit-tier gate immediately after the "=== elf-provenance --selftest ===" gate (scripts/gates.sh:207), priced under 1 s warm in the gate banner, with the cost figure, the regenerated doc-001 matrix, the new counts and doc-001's restated tier costs all landing in the same commit.
- [ ] #5 Demonstrated failing, not merely written: mutate one rule in the production half, first dropping the padding trim and then making an unknown Type word a silent skip, show the suite goes red naming the case, restore and confirm green. Record both transcripts in the notes.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
**Step 1 - Copy the harness, not the fixtures.** `elf-provenance.sh` already has the machinery worth
reusing: `run_case` (`:471-483`), `spawn_script` (`:500-503`) for grading real child exit codes,
`fail()` exiting rather than returning (because a case body runs inside a command substitution, so an
assertion that merely returned had its status overwritten by the next one - `0f672ba`), and
`run_selftest` (`:694-743`). Its fixtures emit an ELF64 with zero program headers because provenance
needs only a note section; this script needs LMA, which lives in `PT_LOAD`, so the fixture is a
different animal and only the harness transfers.

**Step 2 - Make the parser testable first, without changing behaviour.** Extract classification and the
three assertions into functions that read objdump text on stdin, so a case can hand them crafted rows
without owning an ELF. Keep the ELF-facing half (`rust-objdump -h --show-lma`, the format guard, the
objcopy length) as the thin wrapper it already is. If this refactor changes any output, it is wrong.

**Step 3 - Row-level cases.** Feed each trap through the extracted function: the padded name (the exact
row `.sram1_bss      00000400 24000ce0 24000ce0 BSS` from today's `main`), a defmt-style name carrying
spaces and braces, a two-word composed type, a zero-size ALLOC+PROGBITS row sitting above the
high-water mark, an empty Type, and an unknown Type word. Assert both verdicts and the message text
where the message is the product (naming the section is the whole point of rule 1).

**Step 4 - One real-ELF case.** Hand-emit a minimal elf32-littlearm image with one `PT_LOAD` whose
`p_paddr` differs from `p_vaddr`, plus a `SHT_NOBITS` section outside any segment, and require the
script to report the reconstructed LMAs objdump prints. This is the case that catches the failure mode
fixtures alone cannot: objdump changing its columns or dropping the LMA column when `--show-lma` stops
being passed.

**Step 5 - Guard cases.** Non-elf32-littlearm input -> exit 2; missing ELF path -> exit 2 with the line
naming the build that produces it; no `rust-objdump` on PATH -> exit 2. Each asserts "a check that
cannot run never reports success", which is the contract both sibling scripts already honour.

**Step 6 - Register and price.** One `gate commit` line after `scripts/gates.sh:207`, banner figure
under 1 s warm (six objdump invocations is the budget ceiling; if a case needs more, cut the case).
Regenerate doc-001's matrix from `--list`, move the counts, correct the restated tier costs, same commit.

**Step 7 - Prove the suite bites.** Two mutations, each restored after: drop the padding trim (expect
the padded-name case red), and turn the unknown-Type hard error into a silent skip (expect the
unknown-Type case red). Both transcripts into the notes - TASK-067 found five genuine disagreements
this way and reported that without the mutation step its own suite would have looked green while lying.
<!-- SECTION:PLAN:END -->
