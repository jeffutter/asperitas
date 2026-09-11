---
id: TASK-038.02.05
title: Add mid-frame truncation cases to dump_reassemble's selftest
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-11 01:20'
updated_date: '2026-09-11 01:23'
labels:
  - task
  - planned
dependencies: []
parent_task_id: TASK-038.02
priority: high
ordinal: 81500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-038.02 AC #7 asks for synthetic streams containing truncation, corruption and lost start markers. Corruption and marker-stripping are selftest cases; truncation is not. The only truncation proof in CI is tests/console_dump.rs::truncation_at_the_end_of_a_stream_is_refused, which cuts at record boundaries and drives the assembler directly — so the code that actually decides a truncated capture's verdict, consume()'s decoder.finish()/assembler.finish() handoff in examples/dump_reassemble.rs, is exercised by nothing. That handoff runs every time a person interrupts 'cat /dev/cu.usbmodem… > capture.txt' or unplugs the cable, and TASK-038.05 will have to tell a host-side cut apart from device-side loss. Close the gap with two selftest cases built from the existing synthetic streams.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The selftest gains a case whose synthetic stream stops mid-frame inside a chunk of a block that never closes. It asserts: exactly one abandoned block, naming that block id, with `missing.count()` equal to the chunks that never arrived; every earlier block still completes with samples byte-identical to the clean run minus the abandoned block's; `law_holds()` true; verdict dirty.
- [ ] #2 A second case stops mid-frame after the final `AUDEND`, so every block is closed. It asserts: zero abandoned and zero refused blocks, all blocks completed, samples identical to the clean run, `stats.discarded_bytes > 0` (the assertion that makes a host-side cut visible rather than silent), `law_holds()` true, verdict clean. If observed behaviour disagrees with any of this, that is a finding about `consume()`: choose deliberately, record the choice in the module doc and the commit message, and do not bend the expectation until it passes.
- [ ] #3 Both cases are driven through `consume()` — the shipped read loop — not through the assembler directly, so the path where `read()` returns 0 with bytes still buffered is what is under test. The runner's printed case count goes 9 to 11 and `--selftest` exits 0.
- [ ] #4 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test -p asperitas-logging --all-targets` are clean. Changes stay inside `examples/dump_reassemble.rs`; if a real defect in `consume()` forces a change under `src/`, that fix lands as its own commit and this ticket records why.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Scope

One file: `crates/asperitas-logging/examples/dump_reassemble.rs`. The selftest module already builds synthetic captures in memory and runs them through the shipped `consume()`; this adds two damage shapes to its table. Nothing in `src/` changes unless a real defect turns up (AC #4 says what to do then).

## Why these two shapes and not one

A capture stops early for two different reasons and the tool must answer them differently:

- **Cut inside a block that never closed** — the samples are gone and nothing can prove they ever arrived, so the block must be refused as abandoned with its missing chunk indices named. This is the shape TASK-038.05 will see when a dump genuinely did not finish.
- **Cut after everything closed** — the transfer completed and the terminal lost a partial line. That must read as clean-with-a-number, not as loss, and the number is `stats.discarded_bytes`. If it reads as silence instead, a person cannot tell their own Ctrl-C from a device fault.

The accounting law `pushed == framed_bytes + stats.discarded_bytes + buffered` (`Summary::law_holds`, dump_reassemble.rs:188-191) is what makes both claims arithmetic rather than opinion, which is why both cases assert on it.

## Where the code goes

Selftest module starts at dump_reassemble.rs:505. Model each addition on its nearest neighbour rather than inventing a shape:

| Add | Next to | Notes |
|---|---|---|
| `cases` array, `9` → `11` | `:542` | keep the `(name, stream, check)` tuple form; names are printed by the runner, so make them read as sentences in `dump_reassemble: selftest <name> ok` |
| `cut_mid_block_stream()` | `deleted_record_stream` `:774` | build from `clean_stream()`'s per-block record vectors (`Block { id, chunks, summary, raw }`, `:612`); drop the final `AUDEND` and truncate the last chunk frame at a byte offset *inside* its base64 body — derive the offset from the record's own length, never a literal, so widening `CHUNK_RAW` cannot silently turn the case into a whole-record deletion |
| `cut_after_close_stream()` | `stripped_stream` `:812` | take `clean_stream()` and drop the last N bytes of the final `AUDEND` frame, leaving every block closed |
| two `Check` fns | `expect_one_block_missing` `:896`, `expect_clean` `:879` | same signature, `Result<(), String>` with a message naming the offending numbers |

Useful existing fields: `Summary::{pushed, framed_bytes, buffered, stats, tally, abandoned, refused}` and `Tally::{blocks_completed, blocks_failed, blocks_abandoned}`. `tests/console_dump.rs:1483 deleting_any_record_is_detected` and `:1882 truncation_at_the_end_of_a_stream_is_refused` show how missing-lists are asserted; those stay as they are — this ticket does not touch `tests/`.

## Expected verdicts, decided here

Write the checks against these, then run them. If reality disagrees, that is a finding about `consume()`: choose deliberately, say so in the module doc and the commit message, and change the code or the expectation with a reason — do not bend the expectation until it passes.

1. `cut_mid_block_stream`: exactly one abandoned block naming the last block id, its `missing.count()` equal to the chunks that never arrived, every earlier block completed with `produced` byte-identical to `expected` minus the abandoned block's samples, `law_holds()` true, and the verdict dirty (which is what makes `main` exit 1 at `:105`).
2. `cut_after_close_stream`: zero abandoned and zero refused blocks, all blocks completed, `produced == expected`, `stats.discarded_bytes > 0` (this is the assertion that makes the cut *visible*), `law_holds()` true, verdict clean.

## Verification

```
cargo fmt --all --check
cargo run -q -p asperitas-logging --example dump_reassemble -- --selftest   # must print 11 cases, exit 0
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p asperitas-logging --all-targets                              # untouched suite stays green
```

Do **not** add a shell-level test that pipes a generated capture file through the binary. The synthetic streams live inside the example, so producing one externally means writing throwaway generator code and committing either the generator or a fixture blob — real cost for coverage of three straight-line `if`s in `main` (`:101-108`). The selftest asserts on `verdict_is_dirty()` and `law_holds()`, which are exactly the predicates those `if`s read, so the exit code follows by inspection. Say that in the module doc if it is not already obvious.

## Style contract

Every helper gets a `///` doc saying what the case proves and why that shape is the one worth testing (the module doc at `:495-504` already explains why damage is applied to raw samples before framing — extend it if the new cases differ, which they do: these are transport-level cuts, not sample corruption). Third-person indicative names, `// ── Section ──` banners where a new section appears, no `ProptestConfig`, no new dev-dependencies.
<!-- SECTION:PLAN:END -->
