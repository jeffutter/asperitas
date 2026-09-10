---
id: TASK-036.06
title: >-
  Record what the first bench session needs: V3 reset limits, the cache trap,
  and how to unstick RTT
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-10 21:08'
updated_date: '2026-09-10 21:13'
labels:
  - planned
dependencies: []
references:
  - 'https://probe.rs/docs/faq/faq/'
  - 'https://github.com/probe-rs/probe-rs/issues/3516'
  - >-
    https://forum.segger.com/thread/5360-solved-rtt-connection-on-stm32h7xxx-cortex-m7/
documentation:
  - docs/reference/daisy-seed3.md
  - docs/reference/rust-daisy-stack.md
modified_files:
  - firmware/Makefile
  - crates/asperitas-logging/src/defmt_log.rs
parent_task_id: TASK-036
priority: high
type: task
ordinal: 72500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-036.04 documented the two channels honestly, and an audit of every probe claim in docs/ and README found no false ones. What is missing is narrower and all of it is things a person at the bench (TASK-037) or a future audio-DSP change will hit before they surface in code: four measured upstream facts and one Makefile affordance.

1. **Under-reset failing on this probe class is expected, not a broken bench.** probe-rs 0.32 logs `Custom reset sequences are not supported on ST-Link V3. Falling back to standard probe reset.` — the STM32H7-specific attach sequence in `probe-rs/src/vendor/st/sequences/stm32cm7.rs` is skipped on V3-class probes, so `--connect-under-reset` reduces to asserting nRESET. Upstream #3516 (ST-Link V3 MINIE, still open, with a fresh 2026-04 report of intermittent failures on STM32U5 while CubeProgrammer works fine) is therefore a probe-class limitation. daisy-seed3.md:446-453 currently frames #3516 as one reporter's reset circuit. The FAQ's own advice is to try with and without the flag.
2. **A Makefile affordance to drop the flag.** `--connect-under-reset` is hardcoded in `probe-flash` and `probe-run`, and `PROBE_EXTRA` can only add flags, never remove one. So the single experiment TASK-037 most likely needs — flash without under-reset — currently means editing the Makefile at the bench. Fix mirrors the existing `NO_DEFAULT` idiom already in this file.
3. **The Cortex-M7 D-cache is an RTT landmine, currently disarmed by accident.** `_SEGGER_RTT` lives in AXI SRAM at 0x24000000 (the RAM region probe-rs's own target description reports for STM32H750IBKx), and SEGGER's own forum thread 5360 records RTT discovery/delivery failing on H7 with the D-cache enabled — debug-port reads bypass the cache, so CPU-written frames never reach the host — fixed by moving the control block to DTCM or non-cached SRAM. Verified locally: neither embassy-stm32 0.6.0 nor daisy-embassy ca9bcc9 enables I/D-cache or an MPU anywhere in `src/`, and TASK-038.03 independently recorded caches-off as today's state for SDRAM reasons. Nothing connects those two facts yet; whoever turns caching on for DSP headroom would break RTT silently.
4. **The target-side escape hatch has a name.** defmt-rtt 1.3.0 exposes `disable-blocking-mode` (src/channel.rs:33, src/lib.rs:23-24), which forces the non-blocking write path *even after* probe-rs sets BlockIfFull. That is the targeted mitigation for the attached-but-stalled regime if the ~667 us audio deadline ever needs insurance beyond "nothing logs from the audio callback"; the cost is losing losslessness-while-attached, so document it as an option, not a default. TASK-046's notes already assert daisy-seed3.md records this — it does not; this ticket makes that pointer true.
5. **Finish the prior-art record** in `crates/asperitas-logging/src/defmt_log.rs`: `log-to-defmt` 0.1.0 also uses a fixed buffer whose behaviour its own docs call unstable, and `defmt2log` is the opposite direction (defmt to log, host side), so neither is reusable. Cite both beside the existing level-flattening reason.

Docs prose plus ~8 lines in the Makefile. No board, no ears: every claim here is either quoted from upstream or measured on this machine.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 docs/reference/daisy-seed3.md's probe section states that V3-class probes skip probe-rs's custom reset sequences — quoting the fallback message verbatim — so "attaches without --connect-under-reset but fails with it" is written down as an expected outcome of the probe class, with issue #3516 and the FAQ's try-both advice; the current framing of #3516 as one reporter's reset circuit is corrected, not merely appended to.
- [ ] #2 firmware/Makefile gains a variable in the existing NO_DEFAULT style that removes --connect-under-reset from probe-flash and probe-run when set, documented in one comment line with the exact invocation. Prove the default is untouched: 'make -n probe-flash probe-run' output is byte-identical to before the change, and with the variable set the flag is gone from both expansions.
- [ ] #3 The same file records the D-cache trap in at least one sentence that names the mechanism (control block in AXI SRAM at 0x24000000, debug-port reads bypass the cache), the symptom (RTT discovery or delivery silently fails), the fix (move the block to DTCM or non-cached RAM), and the fact that caching is off today so this is latent rather than present.
- [ ] #4 defmt-rtt's disable-blocking-mode cargo feature appears next to the existing host-side --rtt-channel-mode escape hatches with its tradeoff stated, and the three-regime table still reads as one coherent story.
- [ ] #5 defmt_log.rs's module docs cite log-to-defmt's fixed-buffer/unstable behaviour and defmt2log's opposite direction alongside the level-flattening reason already there; cargo fmt --all --check and cargo test --workspace stay green afterwards.
- [ ] #6 Every command the ticket adds to a document appears verbatim in 'make -n' output or in probe-rs --help on the pinned 0.32 build, and no anchor introduced by a heading change is left dangling — re-check inbound links from rust-daisy-stack.md and README.md.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Scope

Five additions, all of which someone needs *before* they discover them at the bench or during an
audio-DSP change. Four are prose in files that already have the right headings; one is eight lines
in `firmware/Makefile`. Nothing here needs a board — every claim below is quoted from upstream or
was measured on this machine, and each carries its source so the next reader can re-check it.

Read first: `docs/reference/daisy-seed3.md:341-471` ("Flashing and logging over an ST-Link probe"
and "What each channel loses"). Do not restructure those sections; extend them. TASK-030.03 owns the
console-grammar region and may land near this one — stay inside the probe region.

## Step 1 — the Makefile affordance (do this first; the docs quote the command)

`--connect-under-reset` is hardcoded at `firmware/Makefile:153` (`probe-flash`) and `:158`
(`probe-run`). `PROBE_EXTRA` (`:56`) only appends, so today the experiment TASK-037 most likely
needs means editing the file at the bench. Copy the `NO_DEFAULT` idiom already at `:59-62` — plain
`=` assignments plus a parse-time `ifeq`, which is what makes `make VAR=x` override work:

    UNDER_RESET = 1
    UNDER_RESET_FLAG = --connect-under-reset
    ifeq ($(UNDER_RESET),0)
    UNDER_RESET_FLAG =
    endif

and put `$(UNDER_RESET_FLAG)` where the literal flag sits in both recipes. Name and shape are yours
to adjust; two things are not: the default expansion must not change by a single byte, and the
override must be shown in a comment above `probe-flash` in the same voice as the existing
`PROBE_EXTRA` example, e.g. `make probe-flash UNDER_RESET=0 FEATURES="seed3 log-defmt" NO_DEFAULT=1`.

Prove both halves before writing any prose:

    cp firmware/Makefile /tmp/Makefile.before
    # ...edit...
    cd firmware && make -n probe-flash probe-run > /tmp/before.txt   # capture BEFORE the edit too
    make -n probe-flash probe-run | diff /tmp/before.txt -           # must print nothing
    make -n probe-flash probe-run UNDER_RESET=0 | grep -c connect-under-reset   # must print 0

Note the trailing space `make` already leaves when `PROBE_EXTRA` is empty — that is pre-existing and
byte-for-byte part of the baseline; don't "fix" it while you're there.

## Step 2 — reframe #3516 as a probe-class limitation (daisy-seed3.md, probe section)

Current text at `:446-453` presents #3516 as one reporter's reset circuit and says the flag "stays
the default until measured otherwise". Correct it rather than appending: on V3-class hardware
probe-rs 0.32 logs, verbatim,

    Custom reset sequences are not supported on ST-Link V3. Falling back to standard probe reset.

so the STM32H7-specific attach sequence in `probe-rs/src/vendor/st/sequences/stm32cm7.rs` never runs
and `--connect-under-reset` reduces to asserting nRESET. Upstream #3516 (ST-Link V3 MINIE, open) has
a fresh 2026-04 report of intermittent failures on STM32U5 where CubeProgrammer works fine on the
same board — i.e. the failure travels with the probe class, not the board. Consequence worth its own
sentence: **"flashes fine without the flag, fails with it" is the expected shape of a bad day here**,
and probe-rs's FAQ says exactly that — try with and without, some chips don't support it. Point at
Step 1's variable as the way to run that test without editing anything.

Add one sentence recording what is *not* known: whether the Seed drives nRESET through a reset
supervisor or buffer rather than RC + button. In #3516 the root cause was precisely a supervisor
(MIC6315) loading the probe's nRESET output, so that one schematic question decides whether this
failure mode applies to this board at all. Nobody has checked it, and it is a five-minute look at
the schematic, not a bench session.

## Step 3 — the D-cache trap (daisy-seed3.md, near the RTT regimes)

One tight paragraph, mechanism → symptom → fix → why it is latent today:

* `_SEGGER_RTT` lands wherever the linker puts `.data`/`.bss`, and this project's RAM is AXI SRAM at
  `0x24000000..0x24080000` — the same region probe-rs's own `STM32H750IBKx` target description
  reports, so this is not hypothetical placement.
* Debug-port reads bypass the Cortex-M7 D-cache, so with caching on, frames the CPU writes never
  reach the host. SEGGER's own thread 5360 documents exactly this on H7 and solved it by moving the
  control block to DTCM or non-cached SRAM.
* Latent, not present: verified locally that neither embassy-stm32 0.6.0 nor daisy-embassy ca9bcc9
  enables I/D-cache or an MPU anywhere in `src/`, and TASK-038.03 independently recorded caches-off
  for SDRAM-coherence reasons. Link that note rather than restating it — the two facts belong side
  by side because whoever enables caching for DSP headroom will break RTT silently and will not
  suspect the cache.

## Step 4 — name the target-side escape hatch (daisy-seed3.md, the three-regime table)

The table at `:413-437` currently offers only the host-side `--rtt-channel-mode no-block-skip|
no-block-trim`. Add defmt-rtt 1.3.0's **`disable-blocking-mode`** cargo feature as the target-side
counterpart: it forces the non-blocking write path even after probe-rs has set BlockIfFull
(`defmt-rtt-1.3.0/src/channel.rs:33`, `src/lib.rs:23-24`). State the tradeoff in the same breath —
you regain the audio deadline, you lose losslessness-while-attached, which is the whole reason this
channel exists. Document it as an option under consideration, not a default, and keep the standing
rule ("nothing logs from the audio callback") as the primary defence.

Side effect to mention in your final summary: TASK-046's notes assert `daisy-seed3.md` already
records `disable-blocking-mode`. It does not today; this step makes that pointer true.

## Step 5 — finish the prior-art record (code doc-comment)

`crates/asperitas-logging/src/defmt_log.rs`'s module docs already reject `log-to-defmt` 0.1.0 for
flattening every level. Add the two missing reasons and the opposite-direction crate, briefly:
`log-to-defmt` also writes through a fixed buffer whose behaviour its own docs call unstable
("will likely introduce such features without declaring breaking changes"), and `defmt2log` goes the
other way (defmt → log, host side), so neither is reusable. This is a comment edit; `cargo fmt
--all --check` and `cargo test --workspace` must stay green.

## Verification

1. The two `make -n` diffs from Step 1 (default unchanged, override removes the flag).
2. Every command printed in the new prose appears verbatim either in `make -n` output or in
   `probe-rs --help` / `probe-rs download --help` on the pinned 0.32 build. Quote-and-grep, don't
   eyeball.
3. Anchors: `rust-daisy-stack.md:109,111` link into daisy-seed3.md's headings, and README's probe
   section links too — if you rename a heading, fix every inbound anchor in the same commit.
4. `cargo fmt --all --check`, `cargo test --workspace`.
5. Read the finished probe section top to bottom once: it should still tell one story in the order
   flash → log → what each channel loses, not five paragraphs bolted on.

## Not in scope

Any claim about what the board actually does — timings, whether under-reset works here, whether the
pads are reachable: that is TASK-037 and it is `@human`. Enabling the D-cache, moving the RTT block
to DTCM, or flipping `disable-blocking-mode` — this ticket records the trap and the lever, it does
not pull either. Bench-convenience flags that change nothing diagnostically (`--list-rtt`,
`--always-print-stacktrace`, format presets) go in TASK-037's notes, not the reference doc.
<!-- SECTION:PLAN:END -->
