---
id: TASK-066.01
title: >-
  Add --explain to unblocked-todo.sh so one call names why each task in a status
  is ineligible
status: In Progress
assignee:
  - '@human'
created_date: '2026-09-13 09:03'
updated_date: '2026-09-13 11:05'
labels:
  - planned
dependencies: []
parent_task_id: TASK-066
priority: high
type: chore
ordinal: 110800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`unblocked-todo.sh` already computes all three eligibility rules (dependencies Done, no unfinished
descendant, assignee not `@human`) but reports only a binary verdict: a task is either printed or
not. Its caller in ralph's pi extension therefore cannot say *why* a Dev Ready ticket was refused,
which TASK-066 needs to write a self-describing correction note. Reconstructing the reason in
TypeScript would be a second copy of these rules - the exact drift this script's header exists to
prevent (its own comment records that the archive-shadowing fix `bc5fe61` landed on the repo copy
only, and the assignee filter landed on the deployed copy only).

Add `--explain`: one invocation prints `id|reason` for every task in the target status, so the
caller gets the verdict and the reason for the whole pool in a single ~5 s call. Default output must
not change by a single byte; what the script considers ready is out of scope (TASK-066 AC #5).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 unblocked-todo.sh <status> --explain prints exactly one "id|reason" line per task whose .status matches <status>, including ineligible tasks and tasks the assignee mode would hide, with reason limited to eligible | dependencies-unresolved | container-children-open | assignee-human and precedence dependencies > container > assignee; exit 0.
- [x] #2 Default output is byte-identical to the pre-change output for To Do, Dev Ready, Blocked and Needs Plan at all three assignee modes, proven by diffing recorded captures, and the recorded diffs are shown empty in the notes.
- [x] #3 --explain is parsed before the positional catch-all, so it can never be swallowed into TARGET_STATUS; a still-unknown flag keeps today's behavior (documented, not fixed here).
- [x] #4 The header comment documents the mode and the reason vocabulary, and states that reasons are computed in this file so verdict and explanation cannot disagree.
- [ ] #5 Landed in the home-manager source, applied with ~/bin/rebuild, and verified byte-identical at ~/.pi/agent/extensions/ralph/unblocked-todo.sh; notes carry the home-manager commit sha and confirm the asperitas repo got no code change outside backlog/.
- [x] #6 `--explain` stdout is byte-identical across `--assignee agent|human|all`: reasons name conditions on the agent side (`assignee-human` = some assignee normalises to `human`, so the task is withheld from the agent pool), never the mode verdict, and `--assignee` does not filter explain lines. The header comment states this.
- [x] #7 When `--explain` matches zero tasks, stdout stays empty and exit stays 0, while one line on stderr names the requested status and the statuses present on this board, so a mistyped status is no longer indistinguishable from an empty one. Default (non-explain) mode gains no output on either stream.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Where this lands

One file, one focused session: `~/.config/home-manager/modules/home/languages/ai/pi-extensions/ralph/unblocked-todo.sh` (153 lines), deployed by `modules/home/languages/ai.nix:495-498` (`home.file.".pi/agent/extensions/ralph/unblocked-todo.sh" = { source = ...; executable = true; };`) to `~/.pi/agent/extensions/ralph/unblocked-todo.sh`. No sub-tickets: one file, ~35 added lines, and the verification cannot ship apart from the change.

The asperitas repo gets no code change. Its `backlog/unblocked-todo.sh` is a 34-line wrapper that `exec "$target" "$@"` (l.34) after cd'ing to its own repo root, so argv passes through untouched; only its header comment gains a mention of the flag (usage line l.19).

Anchors verified against the current source (2026-09-13):

- l.5 header usage line.
- l.32-33 `set -euo pipefail`, `cd backlog` - the script runs with cwd at the project root.
- l.35-52 arg parse; the catch-all at l.47-50 makes any unrecognized token the status.
- l.55-61 unknown `--assignee` mode prints to stderr and exits 1 (deliberate: a typo'd mode must not mean "no filter").
- l.76-83 builds `status_of[$id]` from `archive/tasks/*.md completed/*.md tasks/*.md` in that order, so a live task beats an archived stub sharing its ID.
- l.89 `[ "$st" = "$TARGET_STATUS" ] || continue` - the per-status filter explain mode must keep honoring (it is a per-status report, not a full-board dump).
- l.95-102 rule 1 dependencies: every `.dependencies` entry must resolve to exactly `Done`; an unknown ID becomes the literal `MISSING` and therefore blocks.
- l.114-124 rule 2 container: `case "$child_id" in "$id".*)`, so parenthood is the ID string plus a trailing dot, transitively.
- l.132-148 rule 3 assignee: skipped entirely when mode is `all`; strips `@`, deletes whitespace, lowercases, compares to `human`; one `@human` in a list is contagious.
- l.150-152 the only write: `echo "$id - $title"`. Output order is filename glob order and stays whatever it is today.

## Consumers, measured, so the contract change is bounded

- `index.ts:606-622` `listUnblockedByStatus` invokes `[status, "--assignee", assignee]` and parses with `/^(\S+)\s+-\s+(.+?)\s*$/` (l.568). Non-matching lines are **skipped silently**, and an empty result means "backlog drained" (l.2053-2079), not "error". So `id|reason` lines must never reach the default path, and TASK-066.02 needs its own parser. `execCapture` (l.463-483) keeps stdout and stderr in separate fields, so notes on stderr cannot corrupt it.
- `.claude/workflows/ralph-backlog-loop.js` calls `./backlog/unblocked-todo.sh "To Do"` / `"Dev Ready"` with no flags and has an LLM transcribe the output (schema l.88-93, prompt step 4 l.200, verify prompt l.219-231). An agent reading a terminal sees stdout and stderr interleaved, so stderr is not free real estate for this caller - the new note below must fire only in explain mode, which this caller never uses.
- Both consumers read a nonzero exit as "no work". Under `set -euo pipefail`, any subprocess call added to the explain path must keep the existing `2>/dev/null || true` guards; better, reuse fields the loop already parsed instead of re-deriving them.

## Decisions (these were open questions at ticket-writing time; implement as written)

1. **Reasons name conditions, not the mode's verdict.** All four codes are computed identically in all three `--assignee` modes, and `--explain` prints every task whose status matches regardless of mode:
   - `eligible` - none of the three conditions applies, i.e. safe for an agent to pick up.
   - `dependencies-unresolved` - some dependency does not resolve to `Done`.
   - `container-children-open` - some descendant is not `Done`.
   - `assignee-human` - some assignee normalises to `human`, so the task is withheld from the agent pool. A reader running `--assignee human` reads the same fact as "waiting on a person".
   Consequence: `--explain` stdout is byte-identical across the three modes, and `--assignee` is documented as a no-op for explain. This keeps the vocabulary closed at four values (AC #1) instead of inventing a fifth code for "not human-owned", which is what the naive reading of human mode needs. Precedent: kube-scheduler's framework `fwk.Code` collapses many checks into one deterministic answer by fixed precedence, and kubernetes/kubernetes#138991 documents what happens once human prose becomes the de-facto machine interface - the fix there was a small stable low-cardinality reason list, deliberately not carrying detail.
2. **A reason names the condition, never the offending ID**: `TASK-038.06|dependencies-unresolved`, not `...:TASK-038.04`. Low cardinality is what lets a caller switch on it; the ticket's own frontmatter says which dependency. Say this in the header so nobody adds the ID later.
3. **Zero-match status gets one stderr note, in explain mode only.** stdout stays empty and exit stays 0 (stdout's contract is one line per matching task), while stderr gets a single line naming the requested status plus the statuses actually present on the board. That is the whole difference between "typo'd status" and "nothing in that status", which today are indistinguishable in both modes. Default mode gains nothing on either stream. Collect the status vocabulary while building `status_of` - free, no extra subprocess.
4. **No test harness in this ticket.** Verified today: home-manager has no `tests/`, no `.github/`, no `*.bats`, no shellcheck or flake-check wiring, and `unblocked-todo.sh` is the only `.sh` under `pi-extensions/`. bats 1.14 is installed on the machine, but standing up a golden-test convention inside a nix config repo is its own proposal, not something to bolt onto a feature ticket. AC #2's proof stays recorded captures diffed against a frozen fixture.

## Change

1. Add `EXPLAIN=false` beside `TARGET_STATUS`/`ASSIGNEE_MODE` (l.35-36), and a `--explain) EXPLAIN=true; shift ;;` branch inside the case at l.38, ahead of the `*)` catch-all (AC #3). The manual `case` loop is the right tool: `getopts` cannot parse long options and `getopt(1)` mangles empty/whitespace args (BashFAQ/035).
2. Per candidate, carry `reason=eligible` next to `blocked=false` (l.95). Assign `dependencies-unresolved` inside rule 1's branch (l.98-101), `container-children-open` inside rule 2's (l.120-123), `assignee-human` at rule 3 - each guarded by `if [ "$reason" = eligible ]; then ... fi`, because rules 2 and 3 still execute after rule 1 has set `blocked`, and precedence dependencies > container > assignee must survive that.
3. Rule 3 must compute `human_owned` whenever `EXPLAIN` is true, even in `all` mode: widen the l.132 guard to `[ "$ASSIGNEE_MODE" != all ] || [ "$EXPLAIN" = true ]`, copy the yq/jq pipeline verbatim including its `2>/dev/null || true` guards, and leave the mode-dependent `blocked=true` lines (l.143-147) exactly as they are. Without this split, `--assignee all --explain` would print `eligible` for `@human` tickets.
4. Print site (l.150-152) becomes: `if [ "$EXPLAIN" = true ]; then echo "$id|$reason"; elif [ "$blocked" = false ]; then echo "$id - $title"; fi`. Keeping today's `if` as the `elif` arm is what makes byte-identity structural rather than hoped for.
5. Count matches for decision 3: `explained=$((explained + 1))` right after the l.89 guard - not `((explained++))`, which returns nonzero from zero and would abort under `set -e`. After the loop, if explain and the count is zero, `printf` the note to stderr. Exit stays 0.
6. Header comment: extend the l.5 usage to `unblocked-todo.sh [status] [--assignee agent|human|all] [--explain]` and add a paragraph in the voice of the existing TASK-004 and TASK-62 paragraphs covering: the mode, the four codes and their precedence, that reasons are computed in this file so the verdict and the explanation cannot disagree, that they name conditions rather than IDs, that they are mode-invariant, and that explain output is a *different line format* from the listing (`id|reason` vs `ID - Title`) so callers must not feed one to the other's parser. Mirror the flag in the wrapper's usage line at `asperitas/backlog/unblocked-todo.sh:19` - comment-only.

Leave the three rules themselves alone. Changing what counts as ready is TASK-066 AC #5's explicit out-of-scope line.

## Verification

Run everything against a **frozen copy of the board**, never the live one: ralph moves tickets between capture and re-capture, so a live before/after diff can go non-empty for reasons unrelated to this change. One freeze mechanism also supplies the crafted fixture and gives `Dev Ready` real coverage, since it holds zero tasks today and its diffs would otherwise be trivially empty.

V1 Freeze:

```bash
rm -rf /tmp/ubt && mkdir -p /tmp/ubt && cp -R ~/src/asperitas/backlog /tmp/ubt/backlog
cp ~/.config/home-manager/modules/home/languages/ai/pi-extensions/ralph/unblocked-todo.sh /tmp/ubt/pre.sh
chmod +x /tmp/ubt/pre.sh
( cd /tmp/ubt && find backlog -name '*.md' | sort | xargs shasum -a 256 | shasum -a 256 ) | tee /tmp/ubt/manifest.sha
```

Both captures run with cwd `/tmp/ubt` so the script's `cd backlog` finds the frozen board: pre-change via `/tmp/ubt/pre.sh`, post-change via the deployed path. Record the manifest sum twice, once per capture, to prove the inputs did not move.

V2 Matrix. Capture stdout and stderr separately - do **not** use `2>&1`: a stderr note folded into the byte-comparison artifact can either mask a regression or manufacture one.

```bash
run() { # $1 = script, $2 = label
  for st in "To Do" "Dev Ready" "Blocked" "Needs Plan"; do for m in agent human all; do
    slug=$(printf '%s' "$st" | tr ' ' '_')
    "$1" "$st" --assignee "$m" >"/tmp/ubt/$2-$slug-$m.out" 2>"/tmp/ubt/$2-$slug-$m.err"
  done; done
}
run /tmp/ubt/pre.sh before
```

After the edit and rebuild, `run ~/.pi/agent/extensions/ralph/unblocked-todo.sh after`, then `diff -r /tmp/ubt/before /tmp/ubt/after`-equivalent per pair and put the empty results in the notes (AC #2). Assert the twelve `.err` files are empty before and after too. Cost: one call is ~5.3-6.1 s (~440 yq/jq launches over 139 task files), so the 24-call matrix is about two minutes - ping intercom before starting it.

V3 Explain assertions on the same frozen board. Measured ground truth at planning time: `To Do` has 32 candidates, of which 13 are eligible (4 agent / 9 human); `Blocked` 8 candidates, 2 eligible; `Needs Plan` 1; `Dev Ready` 0. Expected `To Do` explain output is exactly 32 lines: 16 `dependencies-unresolved`, 9 `assignee-human`, 4 `eligible`, 3 `container-children-open`. Spot checks:

```
TASK-038.06|dependencies-unresolved    (dep TASK-038.04 is To Do)
TASK-064|container-children-open       (child TASK-064.02 open, while its own dep TASK-061 is Done)
TASK-064.01|eligible                   (and absent from the --assignee human listing)
TASK-056|eligible  TASK-059.02|eligible  TASK-062|eligible   (the four agent-pickable ones)
TASK-018|container-children-open  TASK-019|dependencies-unresolved  TASK-018.04|assignee-human
```

Expected `Blocked`: 8 lines - 6 deps, 1 `assignee-human` (`TASK-037`), 1 `eligible` (`TASK-038.03.02.02`). That last one is a live precedence case worth quoting in the notes: it has both an unresolved dependency and an open child, and must report `dependencies-unresolved`. Expect `Dev Ready` / `Needs Plan` counts to follow wherever TASK-066.01 itself sits when you freeze - it was `Needs Plan` at planning time and moves to `Dev Ready` for execution, giving `TASK-066.01|eligible` in `Dev Ready` and zero lines in `Needs Plan`, which also exercises V5. If any number drifts, recompute it with an independent oracle (a throwaway python pass over the frontmatter) rather than trusting the script's own logic: the point is a second implementation, not a self-consistency check.

V4 Precedence fixture with clean deps, an open child and `@human` - no such combination exists on the live board, so it must be crafted. Recipe validated at planning time (against the pre-change script the parent is correctly held back from `--assignee all`, proving it is the container rule and not the assignee rule):

```bash
mkdir -p /tmp/ubt-prec/backlog/{tasks,completed,archive/tasks}
cat > /tmp/ubt-prec/backlog/tasks/a.md <<'EOF'
---
id: TASK-900
title: Prec fixture parent
status: To Do
assignee:
  - '@human'
dependencies: []
---
EOF
cat > /tmp/ubt-prec/backlog/tasks/b.md <<'EOF'
---
id: TASK-900.01
title: Prec fixture child
status: To Do
assignee:
  - '@agent'
dependencies: []
---
EOF
( cd /tmp/ubt-prec && ~/.pi/agent/extensions/ralph/unblocked-todo.sh "To Do" --explain )
```

Expect `TASK-900|container-children-open` (not `assignee-human`) and `TASK-900.01|eligible`. Add a `TASK-900.02` whose dependency names a nonexistent ID if you want rule 1 over rule 2 covered by construction as well; live `TASK-038.03.02.02` covers it observationally. Never craft fixtures inside the real `backlog/`.

V5 Argument handling and exit codes (AC #3). Measured pre-change, record it as the trap kept: `./backlog/unblocked-todo.sh "To Do" --explain` prints **nothing and exits 0**, while `--explain "To Do"` prints the normal To Do listing, because whichever token arrives last wins `TARGET_STATUS`. Post-change both orders must produce identical 32-line explain output. Also assert:

- `--bogus` still becomes the status: default mode prints nothing, exit 0 (documented, not fixed here).
- the three assignee modes give byte-identical explain output (decision 1).
- zero-match explain: stdout empty, exit 0, one stderr line naming the status and the board's statuses.
- missing `backlog/` still exits 1 (`cd` fails first, measured).
- `/bin/bash` 3.2 still exits 2 on `declare -A` - pre-existing, do not "fix" it here. Requires bash 5 plus mikefarah `yq` (v4.53.3 here) and `jq`.

## Deploy and commit

1. `bash -n` the source, then run the wrapper from a subdirectory once (`cd ~/src/asperitas/backlog && ./unblocked-todo.sh "To Do"`) to prove argv pass-through still works.
2. `~/bin/rebuild` (a `sudo darwin-rebuild switch`, several minutes - ping intercom first), then confirm `shasum -a 256` matches between the source and `~/.pi/agent/extensions/ralph/unblocked-todo.sh`.
3. Commit in the **home-manager** repo on `master` (working tree verified clean today). Subject style follows recent history - `ralph: make the failure-streak cause line count- and article-neutral`, `ralph: name the real cause in the failure-streak stop reason` - so: `ralph: name the unmet condition in unblocked-todo.sh with --explain`. Put the sha in this ticket's notes along with the empty diffs from V2/V5 and the confirmation that `git -C ~/src/asperitas status --porcelain` shows nothing outside `backlog/`.

## Hand-off notes for TASK-066.02 (say in your final summary that these were left alone deliberately)

- `parseUnblockedList` (`index.ts:568`) drops any line that is not `ID - Title`, silently. Feed it an `id|reason` capture and the loop concludes there is no work. A new parser is required, not a tweak.
- `ralph-backlog-loop.js:141-147` and `:225-231` assert that absence from the script's output means "its dependencies are not all Done", and write that claim into the ticket note. It is already wrong for container holds and `@human` holds; `--explain` exists so 066.02 can stop asserting it.
- `index.ts.bak` sits next to the live extension with 23 references to the old no-`--assignee` contract and is deployed by nothing. Deleting it is someone's small chore, not this ticket's.

## Out of scope

Any change to what counts as ready; fixing the unknown-flag-becomes-status trap; rewriting the engine on `backlog task list --json` (it answers the same board in 0.31 s vs 5.3 s and exposes `assignees`/`isReady`, but publishes no individual dependencies so it cannot name which dep is unresolved, its `--ready` ignores containers and assignees - verified it lists `TASK-018` and `TASK-027`, which this script correctly holds back - and the archive/ ID-shadowing ordering exists only here); wiring the flag into the pi extension or the JS workflow, which is TASK-066.02.
<!-- SECTION:PLAN:END -->
## AC #1 - what places `.sram1_bss`, measured on the unmodified tree at `d096b28`

Command that works (`-C link-arg=-Wl,-Map=out.map` fails outright with
`rust-lld: error: unknown argument`):

    cd firmware && cargo rustc --release --features seed3 --bin main -- -C link-arg=-Map=/tmp/before.map

The map names the input and the neighbours (`/tmp/before.map`:1857):

    24000198 24000198      400     4 .sram1_bss
    24000198 24000198      400     4         .../libdaisy_embassy-6c7c5ff991887a26.rlib(...-cgu.0.rcgu.o):(.sram1_bss)
    24000198 24000198      200     1                 daisy_embassy::audio::RX_BUFFER
    24000398 24000398      200     1                 daisy_embassy::audio::TX_BUFFER

The output section immediately before it is `.data` (VMA `0x24000000`, LMA `0x08015808`, so
`AT>FLASH`). That is the whole mechanism: no linker script declares `.sram1_bss`, rust-lld's orphan
rules place it after `.data`, and lld's LMA rule ("if the previous section is also in the default
LMA region ... otherwise the LMA is set to the VMA") hands it LMA == VMA because `.data` is not in
the default load region.

Which `memory.x` cortex-m-rt included: **ours**. `cargo build -vv` shows the `-L` order as
`asperitas-firmware/out`, cortex-m, embassy-stm32, cortex-m-rt, defmt, daisy-embassy, stm32-metapac,
and exactly three of those dirs contain a `memory.x`: ours, embassy-stm32's, daisy-embassy's.
`link.x:23` performs one and only one `INCLUDE memory.x`, so the first wins. Ours has no `SECTIONS`
block; daisy-embassy's has the `(NOLOAD) ... > RAM_D2` rule plus `REGION_ALIAS(RAM, DTCMRAM)`. Their
MEMORY block never entered this link at all: `grep -cE 'RAM_D2|DTCMRAM|ITCMRAM|SDRAM|QSPIFLASH'
/tmp/before.map` returns **0**. Dead code, not an overridden rule.

Two things the map gave that the plan did not predict:

- `__edata` sat at `0x24000598`, i.e. past the orphan, while `.data` itself ends at `0x24000198`.
  cortex-m-rt copies `__sidata..__edata`, so startup was copying 0x598 bytes instead of 0x198 and
  writing flash bytes belonging to `.defmt.*` over the top of both DMA buffers. Harmless only
  because `prepare_interface` (`ca9bcc9 src/audio.rs:66-78`) zeroes them before use. After the
  change `__edata` is back to `0x24000198`.
- `--orphan-handling=error` is confirmed useless as a guard, by my own run rather than planning's:
  on the unmodified tree rust-lld emits a wall of `.debug_info` / `.comment` placements and stops at
  its error limit without ever naming `.sram1_bss`. Do not propose it.

## AC #2 - type and addresses after the change

`rust-objdump -h`, `.sram1_bss` row (VMA | LMA | type):

| image | before | after |
|---|---|---|
| `main` console | `24000198 / 24000198 / DATA` | `240021b8 / 240021b8 / BSS` |
| `rig` console | `24000198 / 24000198 / DATA` | `24001e10 / 24001e10 / BSS` |
| `main` RTT | `240001d0 / 240001d0 / DATA` | `24000ce4 / 24000ce4 / BSS` |
| `rig` RTT | `240001d0 / 240001d0 / DATA` | `24000e34 / 24000e34 / BSS` |

`nm` on the rebuilt RTT `main`: `RX_BUFFER 0x24000ce4`, `TX_BUFFER 0x24000ee4`; console `main` puts
them at `0x240021b8` / `0x240023b8`. Both stay 24 bytes into their 32-byte cache line, as they were
at `0x24000198` (the move is a multiple of 32 either way).

No ALLOC + PROGBITS section loads outside flash in any of the twelve images. Checked every row of
`rust-objdump -h` programmatically, parsing right-to-left because `.defmt.*` names contain spaces,
and excluding only rows whose type column is `BSS` (NOBITS, contributes nothing) or `DEBUG`
(non-ALLOC, LMA 0). Zero hits across six bins x two cfg sets.

## AC #3, #4 - sizes, both cfg sets, plain vs `make build`

Plain `cargo objcopy ... -O binary` (no flags) against what `make build` writes, bytes:

| bin | plain before | plain after | make before | make after |
|---|---|---|---|---|
| main (console) | 469,763,480 | 88,581 | 88,581 | 88,581 |
| rig (console) | 469,763,480 | 106,811 | 106,811 | 106,811 |
| blinky | 65,638 | 65,638 | 65,638 | 65,638 |
| ledtest | 17,774 | 17,774 | 17,774 | 17,774 |
| podtest | 72,689 | 72,689 | 72,689 | 72,689 |
| panictest | 65,958 | 65,958 | 65,958 | 65,958 |
| main (RTT-only) | 469,763,536 | 48,360 | 48,360 | 48,360 |
| rig (RTT-only) | 469,763,536 | 65,696 | 65,696 | 65,696 |
| blinky | 25,176 | 25,176 | 25,176 | 25,176 |
| ledtest | 19,448 | 19,448 | 19,448 | 19,448 |
| podtest | 31,960 | 31,960 | 31,960 | 31,960 |
| panictest | 25,312 | 25,312 | 25,312 | 25,312 |

Every non-zero delta is the four audio-bearing cells, and each is exactly the gap-closing this
ticket is about; the RTT figures are 56 bytes larger than console before the fix because `.data` is
that much bigger there, which is why the span differs too. Plain and `make build` agree byte-for-size
on all twelve measurements after the change. All six binaries still link under both cfg sets.

Invariants held: `__ebss` and `_stack_end` are identical before/after for every bin in both cfg sets
(console main `240025b8`, rig `24002210`, blinky `240016dc`, ledtest `24000354`, podtest `2400174c`,
panictest `24001704`; RTT main `240010e4`/`240014e4`, rig `24001234`/`24001634`, blinky
`24000484`/`24000884`, ledtest `24000394`/`24000794`, podtest `240004f4`/`240008f4`, panictest
`240004ac`/`240008ac`), so zero RAM growth, and the first four little-endian bytes of all twelve
images read `00 00 08 24` = initial SP `0x24080000`.

Sizes are the invariant, bytes are not. Comparing before against after over each image's own post-fix
length: blinky, ledtest, podtest and panictest are **byte-identical** in both cfg sets (they link no
audio); `main` differs in 209 bytes (console) and 67 (RTT), `rig` in 210 and 132, and every one of
those diffs falls inside `.text` - literal pools following the moved statics, at identical section
size. That is precisely why TASK-059.03 exists: no host-side check can hear it.

## AC #5 - prose and numbers corrected in this commit

- `firmware/Makefile`: six `--only-section` lines deleted from `build:`; the comment now explains why
  the plain image is correct, keeps the flash-budget table (re-measured today, same four numbers),
  and drops the `--only-section=.typo` anecdote along with the flags that made it worth knowing.
- `firmware/Cargo.toml`: all four DWARF-cost rows re-cut today, one fresh target dir per row, RTT-only
  `main` (ELF 261,484 / 3,012,380 / 4,738,792 / 9,496,376; shipped cell `main.bin` 48,360, `.text`
  39,136, flash price still 236 bytes). Old numbers kept beside the new ones per TASK-057's dating
  convention. `DEFMT_LOG=info` re-measured too: 48,504, unchanged. Clean builds 20 s at `false` against
  21 s at `2`. The reproduce line now uses `CARGO_PROFILE_RELEASE_DEBUG`, because setting `RUSTFLAGS`
  overrides `firmware/.cargo/config.toml`'s `-C link-arg=-Tlink.x` and links an unrelated 8,896-byte
  image (measured the hard way).
- `docs/reference/daisy-seed3.md`: the objcopy passage no longer calls the flags load-bearing and no
  longer points at TASK-059 "until it lands"; it says a plain `-O binary` is now correct, quotes the
  twelve post-fix sizes, and keeps the 469 MB story as history with the reason it looked intermittent.
  The cache-line table needed no change: `_SEGGER_RTT` `0x24000008` and `defmt_rtt::BUFFER`
  `0x240010e4` reproduce exactly (fresh `nm`, RTT-only `main`), because `.uninit` rose to fill the hole
  the move left; the sentence now records that re-run and warns that `.bss` objects do move (down 1 KiB).

## AC #6 - gates

`scripts/check-doc-artifact-names.sh` had no defmt/`--only-section` assertion to remove - R1/R2 only
ever checked doc tokens and hand-copied recipes. Added R3: the expanded `build:` recipe must contain
no `--only-section` flag at all, read from `make -n -C firmware build` rather than from file text.

Red run recorded: re-adding `--only-section=.text` to the recipe makes the gate exit 1 with
"the build: recipe passes --only-section... if you think one is needed again, an ALLOC section is
loading outside flash, which is the thing to fix", and the Makefile was restored from the backup
before committing. Green run exits 0.

`bash scripts/gates.sh ci` in `nix develop .#default`: **17 gates, 148.0 s, exit 0**. The AC's
"twelve checks" is stale - the tier has grown since the plan was written (`gates.sh --list` prints
counts: commit 9, push 16, ci 17).

<!-- SECTION:NOTES:BEGIN -->
## One human step remains: `~/bin/rebuild` has not run, so AC #5 is not closed

The change is written and committed to home-manager as **`dc4398780c9ed71a842cbc35b13e27dce69497a5`** -
`ralph: name the unmet condition in unblocked-todo.sh with --explain`, one file, +54/-4. Everything
the plan asked for is measured below. What is not done is deployment: applying it needs
`sudo darwin-rebuild switch`, and sudo cannot read a password from an agent session (`sudo: a
terminal is required to read the password; either use ssh's -t option or configure an askpass
helper`). No askpass helper is configured here. So AC #5's second half stays open and this ticket
stays In Progress rather than Done.

That step is now its own `@human` ticket, **TASK-066.01.01**, because "run a command that needs a
password" is exactly the work an agent must not be handed. This parent therefore has an `@human`
descendant and inherits its assignee per the repo convention: it cannot be closed until that child
is, and re-selecting it would only spin.

Evidence that the rebuild, whenever it runs, lands exactly this file and nothing else:

- `nix build .#darwinConfigurations.mbp16.system` succeeds and its `hm_unblockedtodo.sh` is
  byte-identical to the committed source (`shasum -a 256` = `bea239d84ac5...`).
- `diff -rq` between the live home-files tree (`vj85rkhmk...-home-manager-files`) and the newly
  built one (`qmfi28cw...-home-manager-files`) names exactly one differing path:
  `.pi/agent/extensions/ralph/unblocked-todo.sh`.
- Until it runs the deployed copy is the pre-change script, so no consumer sees a half-applied
  state. The wrapper's pass-through was exercised against the deployed copy before the edit
  (`cd backlog && ./unblocked-todo.sh "To Do" --assignee all` prints the normal listing).

## V1 freeze, and why both captures re-verified it

    rm -rf /tmp/ubt && cp -R ~/src/asperitas/backlog /tmp/ubt/backlog

Manifest over `find backlog -name '*.md'`: `3158822370de698af82f4679a2c0a3f5ae386bd0166e72d32bc62339db0ca713`,
recomputed after each capture round and identical both times, so neither diff can be blamed on the
board moving under it. Note the plan's manifest command as written fails on this board - filenames
contain spaces, so `find | xargs shasum` splits them. `-print0 | sort -z | xargs -0` is the fix.

## V2 default output byte-identity (AC #2)

Twelve pairs (4 statuses x 3 assignee modes), stdout and stderr captured separately, never merged.
Pre-change via `/tmp/ubt/pre.sh`, post-change via the edited file.

    $ for st in To_Do Dev_Ready Blocked Needs_Plan; do for m in agent human all; do
        diff "/tmp/ubt/before-$st-$m.out" "/tmp/ubt/after-$st-$m.out" || echo "DIFF $st/$m"
      done; done
    byte-identity failures: 0

All twenty-four `.err` files are zero bytes, before and after, including `Needs Plan` where explain
mode does write a note: default mode gains nothing on either stream. Line counts unchanged per cell
(To Do 4/9/13, Blocked 1/1/2, Dev Ready 1/0/1, Needs Plan 0/0/0).

## V3 explain content, checked against an independent oracle (AC #1, #6)

Ground truth came from a throwaway python pass over the frontmatter (`/tmp/ubt/oracle.py`), not from
the script's own rules. It reproduces the script's output line-for-line for every status:

| status | lines | diff vs oracle |
|---|---|---|
| To Do | 32 | empty |
| Blocked | 8 | empty |
| Dev Ready | 1 | empty |
| Needs Plan | 0 | empty |

`To Do` breaks down as the plan predicted: 16 `dependencies-unresolved`, 9 `assignee-human`,
4 `eligible`, 3 `container-children-open`. Spot checks all land: `TASK-038.06|dependencies-unresolved`,
`TASK-064|container-children-open` (its dep TASK-061 is Done, child TASK-064.02 is not),
`TASK-018|container-children-open`, `TASK-019|dependencies-unresolved`, `TASK-018.04|assignee-human`,
and the four agent-pickable `TASK-056`, `TASK-059.02`, `TASK-062`, `TASK-064.01`.

One correction to the plan: it expected `Blocked`'s single `eligible` line to be `TASK-038.03.02.02`.
That task actually reports `dependencies-unresolved` - deps `.01`/`.03`/`.04` are open - and the
eligible one is **`TASK-038.03.02.04`**, whose only dependency, `TASK-038.03.02.03`, is Done and
which has no children. Counts are exactly as planned (6 deps / 1 human / 1 eligible); only which ID
carries which label differed, and the oracle confirms both labels.

Mode invariance (AC #6): `To Do --explain` hashes `26dbb0fd19e975491c10121479c01c52e123912ecdc6c45fbcc3dee5a5f9d076`
in all three modes, so `--assignee` neither filters nor rewords explain lines. The reason vocabulary
never mentions the mode, which is why no fifth code was needed: `assignee-human` states the condition
(a normalised assignee is `human`), and agent mode and human mode read the same fact differently.

## V4 precedence, constructed rather than hoped for

No live task combines an unresolved dependency, an open descendant and `@human`, so a scratch board
was crafted at `/tmp/ubt-prec` (never inside the real `backlog/`):

    TASK-900|container-children-open     @human parent with an open child -> container wins
    TASK-900.02|dependencies-unresolved  @human, missing dep, open child -> dependencies win both
    TASK-900.01|eligible

Against the pre-change script the same board gives `--assignee all` just `TASK-900.01`, confirming
the parent is held by the container rule rather than the assignee rule - i.e. the fixture tests what
the plan says it tests. Live coverage of dependencies-beats-container comes from `TASK-038.03.02.02`,
which has both and reports `dependencies-unresolved`.

## V5 argument handling and exit codes (AC #3, #7)

Kept as the trap it was, then closed for this flag:

- Pre-change `"To Do" --explain` printed nothing and exited 0; `--explain "To Do"` printed the normal
  listing, because whichever token arrived last won `TARGET_STATUS`. Post-change both orders produce
  the same 32 lines, hash `26dbb0fd...` above.
- `--bogus` still becomes the status: no output, exit 0. Documented, deliberately unfixed here.
- Zero-match explain (`Needs Plan`): stdout empty, exit 0, and one stderr line -
  `unblocked-todo.sh: no task on this board has status "Needs Plan" (statuses present: Blocked,Dev Ready,Done,To Do)`.
  A mistyped status and an empty status are now distinguishable.
- Missing `backlog/` still exits 1 (the `cd` fails first), unchanged.
- `/bin/bash` 3.2 still exits 2 on `declare -A`, unchanged from pre-change. Left alone as agreed.

## Deliberately left for TASK-066.02

- `index.ts:568` `parseUnblockedList` silently drops any line that is not `ID - Title`; an
  `id|reason` capture fed to it reads as "backlog drained". Needs a new parser, not a tweak.
- `ralph-backlog-loop.js:141-147` and `:225-231` still assert "absent from the listing means deps
  not Done", which V3 shows is wrong for container and `@human` holds. Untouched here on purpose.
- `index.ts.bak` next to the live extension is deployed by nothing and still describes the old
  no-`--assignee` contract. Not this ticket's chore.

## Scope confirmation

The asperitas commit for this ticket touches three paths, all under `backlog/`: this ticket,
TASK-066.01.01, and a comment-only addition to `backlog/unblocked-todo.sh`'s usage block (the flag
mirror the plan called for). `git status --porcelain` is empty after it. No code change outside
`backlog/`.
<!-- SECTION:NOTES:END -->
