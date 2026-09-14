---
id: TASK-068
title: 'Assert check-image-load-addresses.sh with fixtures, as a commit-tier gate'
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-14 03:37'
updated_date: '2026-09-14 08:09'
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
- [ ] #6 (corrects only the position clause of #4, which TASK-056 took when it landed first in 7e9eda0) the gate registers as the fifth commit-tier gate, declared immediately after `gate commit "=== elf-staleness --selftest ==="` at scripts/gates.sh:218 and before `cargo fmt`, with doc-001's counts moving 11/20/21 -> 12/21/22.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
**Shape.** One flag on one script, one `gate commit` line, and the doc arithmetic that follows from adding a commit-tier gate. **No sub-tickets**: the suite cannot ship without the extraction that makes it possible, the gate line is meaningless without the suite, and the mutation transcripts belong in this ticket's notes beside the code they mutate - none of the three is an independently shippable increment. TASK-067 (the direct precedent, same job for the sibling script) shipped as one ticket and one commit. Nothing here needs the board, ears, an instrument, or an owner decision, so nothing gets split off `@human`: every criterion is satisfiable with bash, `rust-objdump` and a stopwatch.

Position correction to AC #4: TASK-056 landed first (`7e9eda0`), so the slot is taken. This becomes the **fifth** commit gate, declared immediately after `gate commit "=== elf-staleness --selftest ==="` at scripts/gates.sh:218 and before `cargo fmt` at :220 - exactly what the coordination comment on this ticket predicted. Counts move 11/20/21 -> 12/21/22.

### Step 0 - Capture the baseline before touching anything

    bash scripts/check-image-load-addresses.sh > /tmp/load-addrs-base.txt

Six ELFs, exit 0, today's output ends `main image=47544 ... .sram1_bss=BSS@0x24000ce0(1024)`. If the ELFs are absent, produce them with one `scripts/gates.sh push` run *before* editing; never run a bare `cargo build`/`make` afterwards, because ordering rule 1 (scripts/gates.sh:28-35) means a later build re-points `release/main` at a different cfg set. Steps 1 and 2 must reproduce this file **byte for byte** - that is the equivalence proof, so take it first.

### Step 1 - Split the parser from the ELF, behaviour unchanged

`check_one_elf` (scripts/check-image-load-addresses.sh:239-355) currently owns the objdump call, the row parse, all three rules, the objcopy length and the summary line. Cut it into four pieces along the seams the tests need:

- `read_sections <path>` (:170-205) keeps the shim call, the `file format elf32-littlearm` guard and the `die` on objdump failure, then hands the text to -
- `parse_section_table` - the `while IFS= read -r row` loop (:188-198) plus the "no section rows parsed" refusal, verbatim. Fills `SEC_ROWS/SEC_NAMES/SEC_SIZES/SEC_LMAS/SEC_TYPES`. This is where the padding trim and the positional LMA capture live, so it must be callable with text nobody linked.
- `evaluate_sections <name>` - the R1 loop, the water marks, the zero-size exclusion, R2's scan and R3's `expected` computation (:249-316), appending to `VIOLATIONS` and setting globals `R1_FAILED LOW HIGH EXPECTED SPAN_LOW SPAN_HIGH N_LOADED N_ZERO ZERO_NAMES SRAM_NOTE`. No printing. `IDX` stays a global because `classify_type` reads it in its messages (:227).
- `format_summary` - the `printf '%-9s image=%-30s ...'` composition (:347-353) returned as a string, printed by `check_one_elf`. Cases assert against the string production actually emits, not against a format the test invented.

The objcopy half stays in `check_one_elf`: it is the only part that needs bytes on disk. Row-level cases set `CURRENT_ELF` to their own fixture path so `classify_type`'s messages still name a file, and call `parse_flash_region` once in `run_selftest` so every case compares against the real FLASH region parsed from firmware/memory.x (0x08000000..0x08020000). No injection knob: a parameter to fake the flash region would be a decision this gate exists to refuse making.

Verify: `/tmp/load-addrs-base.txt` diffs clean, and the fixture from Step 3 gives the same line before and after.

### Step 2 - Make the Type column's real grammar reachable (one production line)

Measured, and pinned in source: llvm-objdump builds the column as `Type = Section.isText() ? "TEXT" : ""` then `Type += Type.empty() ? "DATA" : ", DATA"` (same for BSS/DEBUG) - **comma-and-space separated**, and `isDebugSection()` for ELF keys off the *name*, not `SHF_DEBUG`. Confirmed on a hand-emitted probe this session: an allocated PROGBITS section named `.debug_alloc` prints `DATA, DEBUG`, and a NOBITS section flagged ALLOC|EXECINSTR prints `TEXT, BSS`.

`classify_type` iterates `for w in $type`, so a composed row arrives as the words `DATA,` and `DEBUG` and is refused by the *unknown-word* branch. The two semantic branches below it (:226-231, "allocated and debug at once", "both file-backed and NOBITS") are therefore unreachable from real objdump text - dead code that reads as if it were load-bearing. Fix by folding the delimiter in `classify_type`: `local type=${1//,/ }` with a comment citing the `", DATA"` construction above.

This changes no verdict, which is provable rather than hoped: every composed shape ends in `die` either way - `TEXT, BSS` goes from unrecognized-word to both-file-backed-and-NOBITS, `DATA, DEBUG` and `BSS, DEBUG` go from unrecognized-word to allocated-and-debug-at-once, `TEXT, DATA` is impossible (`isData` requires EXECINSTR clear, which is why `.text` prints plain TEXT despite being PROGBITS+ALLOC). Single-word `TEXT`/`DATA`/`BSS`/`DEBUG`/empty are untouched. Only wording changes, and the wording gets better: it names the semantic clash instead of calling ordinary objdump output "unrecognized".

Out of scope, deliberately: whether an *allocated* `.debug*` section should classify as file-backed and fall under R1 instead of refusing. Today's six ELFs print empty Type for every `.debug_*` (they are not allocated), so the question is hypothetical; refusing loudly is the existing policy and changing it is a decision for whoever first meets the build that emits one. Say so in the comment.

### Step 3 - Fixture generator: pure bash hex, runtime only

Copy `elf-provenance.sh`'s emitter machinery (`fx_raw/fx_put/fx_pad/fx_text` :220-247, `fx_emit` :250-259 with its "assembled N bytes, layout says M" self-check) and the single-use `$1` variant that skips building an array. Keep the self-check: it caught two real off-by-ones in one sitting today. Facts measured this session, so the generator can be written rather than debugged:

- A 616-byte hand-emitted ELF32 little-endian ARM image (EM_ARM 40, ET_EXEC, e_flags 0x5000200, e_ehsize 0x34, phentsize 32, shentsize 40) is accepted: `rust-objdump -h --show-lma` prints `file format elf32-littlearm` and the rows. No linker involved. Working scratch generator: `/tmp/fxload/mk3.sh` (throwaway; /tmp may be cleared).
- `.shstrtab` must be built as **hex text**, one `fx_pad 1` for index 0 then `name + NUL` per entry: bash cannot hold a NUL in a variable, and offsets must be counted arithmetically (`STRLONG=1`, `+= len+1`) rather than accumulated. Off-by-one here surfaces as `SHT_STRTAB string table section [index 6] is non-null terminated`.
- **Do not use python3**, even though this shell happens to have one on PATH: `flake.nix`'s devShell does not declare it and neither sibling script imports it.
- The LMA column tracks `p_paddr`, not `p_vaddr`; a section covered by no `PT_LOAD` prints LMA == VMA (objdump falls back to `sh_addr`). `--show-lma` forces the column even when no section differs (`shouldDisplayLMA` returns the flag last), and upstream has broken that flag before (llvm/llvm-project 66228), which is why at least one case must drive the real tool.

Emit two images into `$TMP`, generated once (rule C2):

- **A, the passing image**: `.text` PROGBITS ALLOC|EXEC at VMA=LMA=0x08000000 size 0x40; `.data` PROGBITS ALLOC|WRITE at **VMA 0x24001000 / LMA 0x08000040** size 0x20, with a `PT_LOAD` whose `p_vaddr != p_paddr` carrying that delta; `.sram1_bss` NOBITS ALLOC|WRITE at 0x24001020 size 0x400; `.gnu.sgstubs` PROGBITS ALLOC, size 0, at 0x0801fff8 (above the high-water mark, inside FLASH_END, in no segment); a defmt-style section named `.defmt.error.{"package":"x","tag":"a b"}`, unallocated, size 4; `.shstrtab`. Measured result of running the real script on it: exit 0, `r3ok.elf  image=96  low=0x08000000 high=0x08000060 expected=96 loaded=2 skipped=4 excluded_zero_size=.gnu.sgstubs .sram1_bss=BSS@0x24001020(1024)`, and `rust-objcopy -O binary` really writes 96 bytes.
- **B, the refusing image**: A with `.data`'s `p_paddr` moved to 0x24001040, so its LMA is outside flash while its VMA is not.

Image A is the case that makes the positional LMA parse non-vacuous, measured: a mutant that captures `BASH_REMATCH[3]` (VMA) instead of `[4]` goes red on it with exit 1, naming `.data` and reporting `plain '-O binary' would write 469766176 bytes` - the TASK-059 defect reproduced by a fixture. R3's objcopy equality pins the same column from the other side, since objcopy writes by load address.

### Step 4 - Harness, copied not shared

Copy `fail()` (elf-provenance.sh:456-460, **exits** rather than returns - a case body runs inside `run_case`'s command substitution, so a returning assertion has its status overwritten by whatever ran next; TASK-067 found five cases that disagreed while the suite printed green), `run_case` (:471-483), `expect_output`/`expect_no_output` (:487-496), `spawn_script` (:500-503), `run_selftest`'s loop and summary (:694-743), and `check-elf-staleness.sh`'s local `mk_workdir` (:67-74). Copied rather than shared, per that script's own note at :70-72. Prefix every message with `check-image-load-addresses:`; per-case `selftest <kebab-case-name> ok` on stderr; one summary `selftest passed (N cases)` or `selftest FAILED (M of N cases), see N line(s) above`; exit 0 / 1 / 2 as the header contract already states (:48-51), extended in prose to the new mode. Dispatch `--selftest` from the option loop in `main` (:374-380) ahead of the positional handling, and update `usage()` (:71-79) plus the header's exit-code paragraph in the same edit. No sourcing guard needed.

AC #1 asks for child processes so the graded exit codes are the ones a gate sees. Five cases spawn the whole script; the rest call the extracted functions in the case's own subshell, which is what TASK-067 did and why ("in-process except where the read itself is the subject"). All three exit codes a gate can observe are graded through a real child: 0, 1 and 2.

### Step 5 - Cases

Five children (`spawn_script`):

| case | input | expect |
|---|---|---|
| `real-elf-lma-column` | image A | rc 0, and the summary line carries `image=96`, `high=0x08000060`, `expected=96`, `excluded_zero_size=.gnu.sgstubs`, `.sram1_bss=BSS@0x24001020(1024)` |
| `real-elf-lma-outside-flash` | image B | rc 1, violation names `.data` and says `469` megabytes worth of span, `image=not-written(...)` proves objcopy never ran |
| `guard-not-an-object-file` | `/etc/hosts` | rc 2, message names the `rust-objdump failed` reason (`not recognized as a valid object file`) |
| `guard-wrong-format` | `/bin/ls` (Mach-O) | rc 2, `is not elf32-littlearm` |
| `guard-missing-elf` | a path under `$TMP` that was never written | rc 2, and the message names `make -C firmware build-elf BINARY=` - the affordance TASK-062 was about |

Eleven in-process cases, each feeding literal objdump text to `parse_section_table` then `evaluate_sections`/`classify_type`/`format_summary`:

| case | asserts |
|---|---|
| `padded-name-trims-to-exact-match` | a block whose widest name is the 40-char defmt section, so `.sram1_bss` carries real padding: name equals `.sram1_bss` exactly and the summary prints `.sram1_bss=BSS@0x24000ce0(1024)`, the number R2 reports today for `main` |
| `defmt-json-name-survives` | the JSON-named row parses with its name intact (spaces, braces, quotes, comma) and classifies not-loaded; a variant flagged DATA parses with the type still attached |
| `composed-type-refused-by-name` | literal `DATA, DEBUG` refuses, and the message names the section and the raw row - it is a refusal, never a skip |
| `unknown-type-word-names-section-and-row` | `FOO` refuses, message contains both the section name and the raw row text |
| `empty-type-is-not-loaded` | empty Type contributes nothing to `loaded`, nothing to `EXPECTED`, and shows up in the skipped count |
| `zero-size-alloc-above-highwater-excluded` | the `.gnu.sgstubs` row at 0x0801fff8 leaves `EXPECTED` at the real high-water value and appears in `ZERO_NAMES`; drop the exclusion mentally and R3 is off by 8 bytes |
| `sram1-bss-file-backed-violation` | `.sram1_bss` typed DATA produces the violation that names the SECTIONS rule and daisy-embassy's renamed input sections |
| `sram1-bss-absent-note` | a block without it prints exactly `.sram1_bss=absent` |
| `section-outside-flash-violation` | a file-backed row at 0x2400xxxx produces R1's violation, sets `R1_FAILED`, and yields `image=not-written(would-be-N)` rather than an objcopy call |
| `lowest-lma-must-be-flash-origin` | the first file-backed LMA above 0x08000000 refuses with "not the FLASH origin", because the length formula is silently wrong otherwise |
| `no-rows-parsed-is-fatal` | text with no matching row dies (exit 2) instead of reporting a clean run - the vacuous-pass door |

### Step 6 - Cost rules, written into the script header as C1-C5

- **C1** at most five `spawn_script` children per run - one per exit code plus the three guards AC #3 names; everything else runs in-process (~2 ms/subshell against ~150 ms/child).
- **C2** generate images A and B once, into `$TMP`, before the first case; never regenerate inside a case.
- **C3** never call `cargo objdump`, `cargo objcopy`, `cargo build`, `cargo metadata` or `make`, and never name a path under `firmware/target/` from a case: those either rebuild or re-point the artifact the push-tier gate audits. Use the bare shims.
- **C4** check `rust-objdump` and `rust-objcopy` with `command -v` once at the top of `run_selftest`, before generating anything: missing tools exit 2, never zero cases reported as success.
- **C5** one `mktemp -d` per run, cleaned by the same EXIT trap shape main() installs (:407-408).

Budget: target <= 0.9 s warm, hard ceiling under 1 s. Measured inputs: one whole-script child over one ELF = 0.15 s (two llvm spawns at ~50 ms), so five children ~= 0.75 s plus generation. If the banner figure comes back over 1 s, cut `real-elf-lma-outside-flash` - its assertion is duplicated at row level by `section-outside-flash-violation` - before cutting any guard.

### Step 7 - Register one gate line

Insert after scripts/gates.sh:218, fifth commit gate, with a comment above it covering: why commit tier (builds nothing, reads no artifact, so cheapest-first puts it beside its two siblings, and the autonomous loop is gated at commit); why fixtures rather than the cross-build pair's images (ordering rule 1 leaves `release/main` holding the RTT-only image and nothing else, and the commit tier builds no firmware at all - the same argument the comment at gates.sh:198-206 makes); the measured warm figure; and a pointer to C1-C5 in the script's own header. Touch nothing near :275-300, where the cross-build pair lives. Outside `nix develop` the shims are absent, but gates.sh already exits 3 for the missing thumbv7em std before any gate runs (:166-172), so the selftest's own exit-2 message is a direct-run affordance, not a hook failure.

### Step 8 - Docs and pricing, same commit

Regenerate, do not hand-edit: paste fresh `bash scripts/gates.sh --list ci` output into the fenced block at `backlog/docs/doc-001 - Asperitas-Project-Plan.md:253-279`, then verify mechanically:

    diff <(bash scripts/gates.sh --list ci) <(sed -n '254,278p' "backlog/docs/doc-001 - Asperitas-Project-Plan.md")

must print nothing (today the block is rows 254-276, blank 277, counts 278; one added row shifts the tail of that range by one line). Then, from the same three timed runs (`nix develop .#default --command bash scripts/gates.sh {commit,push,ci}`, take this gate's own `--- N.NNs` line and each tier's total - measure, do not extrapolate or copy the published figures, which already drift: `check-elf-staleness.sh:37` claims 0.4 s while gates.sh:217 says 0.9 s for the same gate, and fixing that one stale line belongs in this commit):

- doc-001:278 counts -> `counts: commit 12, push 21, ci 22`.
- doc-001:281-282 totals and the `136 of those 146 s` sentence.
- doc-001:284-292, the "Two gates sit third and fourth, at 0.9 s each" paragraph: it becomes three gates occupying third, fourth and fifth, and must carry this gate's cost rule (five children, bare shims, never a path under `firmware/target/`). Rewrap the run-on sentence at :288 while in there.
- doc-001:309-319 describes the push-tier `=== image load addresses ===` gate; add the half-sentence distinguishing fixtures from artifacts, or the new commit-tier gate reads as a contradiction of "Its tier is `push` because pre-commit builds no firmware".
- doc-001:322 (`~4 s (commit) or ~75 s (push)`), scripts/gates.sh:20, lefthook.yml:12. A commit-tier addition must touch lefthook.yml; a push-tier one need not (compare `7e9eda0`, which did, with `9ec7db9`, which did not).

Nothing checks the matrix against `--list` mechanically - `check-doc-artifact-names.sh` never opens `backlog/docs/` - so the diff above is the check, and the gap stays open on purpose (TASK-067 left it too).

### Step 9 - Prove the suite bites, then record it

Three mutations, each restored before committing, each transcript quoted into this ticket's Implementation Notes (`--notes`) and summarised in the commit body, the way `0f672ba` did:

1. **Drop the padding trim** (`name=$(rtrim "${BASH_REMATCH[1]}")` -> `name="${BASH_REMATCH[1]}"`). Measured today on the real `main` and `rig`: both print `.sram1_bss=absent` although the section is present, exit still 0 - a green run that checked nothing, which is the exact failure mode this ticket exists to catch. Expect `padded-name-trims-to-exact-match` and `real-elf-lma-column` red (the latter also loses `excluded_zero_size=.gnu.sgstubs` to trailing spaces), rc 1.
2. **Unknown Type word becomes a silent skip** (`if (( unknown ))` -> `if (( 0 ))`). Measured today: on the real `main` the output is byte-identical and rc stays 0, i.e. the push-tier gate cannot see this mutation at all. Expect `unknown-type-word-names-section-and-row` and `composed-type-refused-by-name` red, rc 1.
3. **Bonus, because it is free and it is the scariest**: capture VMA instead of LMA (`BASH_REMATCH[4]` -> `[3]`). Measured today against image A's layout: exit 1, `.data ... is outside FLASH`, `would write 469766176 bytes`. Expect `real-elf-lma-column` red.

Then restore, confirm green, and finish with the checks that prove nothing else moved: the `/tmp/load-addrs-base.txt` diff from Step 0 is still clean, `gates.sh --dry-run commit | tail` shows the new line fifth, and `/bin/bash scripts/check-image-load-addresses.sh --selftest` passes on bash 3.2.57 as well as 5.3.15 - the emitter uses only `printf -v`, arrays and `${s:i:1}`, all present in 3.2, and the header already claims dual-bash fidelity for the row regex (:155-161).

## Non-goals

No bats/ShellSpec/shellcheck/python dependency and no flake input (~16 cases is cheaper served by the ~100 lines already copied twice). No committed binary blobs. No change to firmware/memory.x, the Makefile, ci.yml or elf-provenance.sh. No attempt to make an allocated `.debug*` section classifiable (Step 2). No mechanical doc-matrix-vs-`--list` gate.

## Risks

- **Toolchain strictness about hand-built ELFs.** Accepted and measured on LLVM 21.1.8-rust-1.97.1 here; flake.lock pins the toolchain, so CI gets the same objdump. A future objdump that rejects the layout rejects it through the exit-2 path, loudly - and the real push-tier gate over six linked ELFs remains the backstop either way.
- **objdump column drift.** Upstream has regressed `--show-lma` (66228) and fixed LMA display (#72141). If the column ever disappears, `ROW_RE` matches nothing and the script dies "no section rows parsed" (exit 2), asserted by `no-rows-parsed-is-fatal` - a loud death, not a vacuous pass.
- **Cost creep.** The sub-second budget survives only while C1-C5 hold, which is why they land as comments in the header rather than as advice here.
- **Self-consistency of a hand-emitted fixture.** Image A could in principle satisfy the parser while being something rust-lld would never emit. Mitigated the same way TASK-067 mitigated it: the fixture is read by the real `rust-objdump` and the real `rust-objcopy`, so the tools - not the generator - decide whether it is an ELF, and only the push-tier gate touches linked images.
<!-- SECTION:PLAN:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-14 05:51
---
Coordination note from planning TASK-056 (2026-09-14). TASK-056 plans a new commit-tier gate, scripts/check-elf-staleness.sh --selftest, in the same slot AC #4 of this ticket names: immediately after === elf-provenance --selftest === at scripts/gates.sh:207. Whichever lands second takes the next position; nothing else about either gate changes, and both are sub-1 s warm with runtime-generated fixtures under mktemp -d and no cargo in their path. If TASK-056 lands first, expect its gate line between :207 and your own, and regenerate doc-001's matrix block from scripts/gates.sh --list rather than patching the counts by hand, since its figures already moved once (commit 10 -> 11).
---

created: 2026-09-14 08:07
---
Planning, 2026-09-14. Position resolved: TASK-056 landed first (7e9eda0), so this gate goes fifth, after scripts/gates.sh:218 - see AC #6.

The riskiest assumption in the old plan (that a hand-emitted ELF would make objdump print the rows this parser needs) is now measured rather than assumed. Working scratch generator left at /tmp/fxload/mk3.sh (throwaway, pure bash hex, no linker, no python3; /tmp may be cleared). It emits a 616-byte elf32-littlearm image whose .data has VMA 0x24001000 and LMA 0x08000040 via a PT_LOAD with p_paddr != p_vaddr, plus a zero-size ALLOC .gnu.sgstubs at 0x0801fff8, a NOBITS .sram1_bss and a defmt-style JSON section name. The real script passes it: exit 0, image=96, expected=96, and rust-objcopy really writes 96 bytes. Two emitter traps surfaced by the byte-count self-check, both worth carrying into production: e_ident is 16 bytes (not 19), and .shstrtab needs its index-0 NUL written separately plus a terminator after the final name, or objdump refuses with "SHT_STRTAB string table section [index 6] is non-null terminated".

Non-vacuity of the real-ELF case, measured three ways with mutated copies of the script under /tmp/mut/: capturing VMA instead of LMA makes the fixture go red (exit 1, ".data ... is outside FLASH", "would write 469766176 bytes"); dropping the padding trim makes the real main and rig print .sram1_bss=absent with rc still 0, i.e. the green-run-that-checked-nothing this ticket exists to catch; and turning the unknown-Type refusal into a silent skip leaves the real main's output byte-identical at rc 0, so no real-image gate can ever see that mutation - only a fixture case can.

New fact that changes AC #2's wording, pinned in llvm-objdump.cpp: the Type column composes with ", " (Type += Type.empty() ? "DATA" : ", DATA"), and isDebugSection() for ELF keys off the section *name*, not SHF_DEBUG. So a composed row arrives as the words "DATA," and "DEBUG" and is refused by classify_type's unknown-word branch, never by the two semantic branches at :226-231, which are unreachable from real objdump text while the comma survives. Plan Step 2 folds the delimiter in classify_type (one line) and argues no verdict changes, only wording, by enumerating every reachable composition. Probe evidence: an allocated PROGBITS section named .debug_alloc prints "DATA, DEBUG"; a NOBITS section flagged ALLOC|EXECINSTR prints "TEXT, BSS" - the latter is reachable from legitimate flags, not only from a lying fixture.

No sub-tickets, and no @human split: everything here is host-side bash plus objdump, judged by exit codes and byte counts, so no criterion needs the board, ears or an owner decision. Baseline for the equivalence proof is scripts/check-image-load-addresses.sh over the six ELFs, 0.90 s warm today.
---
<!-- COMMENTS:END -->
