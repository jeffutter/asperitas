---
id: TASK-038.02.03
title: >-
  Add the host block assembler, the dump_reassemble example, and the adversarial
  integrity suite
status: Done
assignee:
  - '@ralph'
created_date: '2026-09-09 15:51'
updated_date: '2026-09-09 19:01'
labels:
  - planned
dependencies:
  - TASK-038.02.02
modified_files:
  - crates/asperitas-logging/src/dump.rs
  - crates/asperitas-logging/examples/dump_reassemble.rs
  - crates/asperitas-logging/tests/console_dump.rs
  - .github/workflows/ci.yml
parent_task_id: TASK-038.02
priority: high
type: task
ordinal: 64500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The format is only self-verifying if something refuses a block it cannot prove complete. This ticket ships the host-side block assembler and the adversarial suite that proves it, plus the example program that turns a captured console byte stream into raw PCM.

Loss must be proved by sequence, not checksum: `frame.rs:47-49` documents that a record whose leading `~` was lost produces no integrity failure at all, so `n_of_n` plus a CRC over the raw concatenated bytes is what makes absence provable. The three receiver classes XMODEM names — complement mismatch, duplicate, out-of-sequence — all need defined behaviour here; the parent ticket left the last two undefined. A CRC-16 cannot detect chunk permutation either, so ordering needs its own property test rather than inheriting confidence from the CRC.

Depends on TASK-038.02.02 for the grammar. No pipe, no firmware interaction.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `dump::BlockAssembler` consumes `frame::Record.body` slices with no allocation beyond one caller-provided staging buffer, parses `AUDIO`/`AUDEND` bodies itself, ignores any other verb, places each chunk at `chunk_index * CHUNK_RAW`, and returns an outcome value that names block index, missing indices, and conflicts.
- [x] #2 A block completes only when all `n` chunks are present and `crc16_ccitt` over the raw concatenated bytes equals `AUDEND`; every other path is a refusal that names the missing chunk indices and yields no partially assembled data.
- [x] #3 A repeated chunk with identical bytes is idempotent; a repeated `chunk_index` carrying different bytes marks the block conflicted, names the index, and refuses the block. A chunk arriving for a different block index while one is open abandons the open block and reports it with its missing list. Chunks already placed mean arrival order cannot change the result, which the tests prove rather than assume.
- [x] #4 Adversarial properties hold over generated streams in `tests/console_dump.rs`: deleting any whole record is detected, mutating any body byte fails the block CRC or the frame CRC, scrambling arrival order reassembles byte-identically, stripping leading `~` markers completes zero blocks, an `AUDEND` alone or a chunk arriving after its block closed are both reported, and the accounting law from `console_frame.rs` still balances over dump traffic.
- [x] #5 `examples/dump_reassemble.rs` reads stdin or a file in 4 KiB chunks, writes raw PCM to `--out`, prints a manifest plus the accounting law and measured versus theoretical useful-bytes-per-wire-byte to stderr prefixed `dump_reassemble: `, and exits 0 clean / 1 on any incomplete, conflicted, or CRC-mismatched block / 2 on unusable input — the deliberate divergence from `console_decode`'s always-exit-0 contract is stated in its doc header.
- [x] #6 The same example gains `--selftest`, which generates synthetic streams (clean, one record deleted, one body byte corrupted, chunks reordered, start markers stripped, `AUDEND` alone), asserts each expected outcome, and exits non-zero on any surprise; `.github/workflows/ci.yml` runs it right after the existing `cargo test --workspace` steps, so CI proves the tool end to end with no board attached.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Depends on TASK-038.02.02 (grammar). This is where the format earns the words "self-verifying": before this ticket nothing refuses a block it cannot prove complete.

## Assembler API (`dump.rs`, pure `no_std`, one caller-provided staging buffer)
```rust
pub struct BlockAssembler<'a> { /* buf: &'a mut [u8], present bitmask, active block, expected chunks, conflict */ }
pub enum Action {
    Started { block: u32 },
    ChunkStored { block: u32, index: u16 },
    Duplicate { block: u32, index: u16 },                 // identical bytes: idempotent
    Conflict { block: u32, index: u16 },                  // same index, different bytes: block refused
    Complete { block: u32, bytes: usize },
    Failed { block: u32, reason: Failure },               // missing list, CRC mismatch, or conflict
    LateChunk { block: u32, index: u16 },                 // arrived after the block closed
    Abandoned { block: u32, missing_chunks: usize },      // a different blk index appeared mid-block
    Malformed { at: usize },                              // body claims to be AUDIO but does not parse
    Ignored,                                              // BOOT/STATUS/PANIC/anything else
}
pub enum Failure { Missing(MissingChunks), Crc { shipped: u16, computed: u16 }, Conflict { index: u16 }, Capacity { needed: usize } }
impl<'a> BlockAssembler<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self;
    pub fn accept(&mut self, body: &[u8]) -> Action;      // owns AUDIO/AUDEND parsing
    pub fn finish(self) -> Finish;                        // reports the still-open block as Abandoned
    pub fn pcm(&self) -> &[u8];                           // meaningful only right after Complete
}
```
Design points to hold:
- **The assembler parses bodies itself.** It sits beside the encoder and shares the byte templates, so grammar knowledge has one owner; `frame::Decoder` deliberately knows nothing about verbs (frame.rs:694-748 validates framing and CRC only) and hands bodies through untouched. Widen `frame.rs`'s private `parse_hex`/`parse_decimal` (:808, :821) to `pub(crate)` rather than writing a third parser; keep the lowercase-only rule.
- Chunks land at `index * CHUNK_RAW`, so permutation is structurally impossible and arrival order cannot matter — say that in the doc comment and then prove it in a test rather than trusting it.
- Duplicates: compare the incoming bytes against the slot. Equal ⇒ `Duplicate`, no state change. Different ⇒ mark conflicted, refuse the block at `AUDEND`, name the index. Without this, a device that re-sends a chunk after a partial write silently overwrites good data.
- Single active block. A chunk naming a different `blk` while one is open means the previous block's tail is gone: report `Abandoned` with its missing count and start the new one. Justify in-doc: one producer writes sequentially, so interleaving across blocks is loss, not concurrency.
- `Ignored` keeps STATUS/BOOT flowing past the assembler without noise; count them in the example's manifest rather than returning them per record.

## Example: `examples/dump_reassemble.rs`
Mirror `examples/console_decode.rs` exactly where the shape is generic: `const CHUNK_BYTES: usize = 4096`, input from `env::args().nth(1)` or stdin, the push/drain handshake with the "decoder took no further bytes" bail-out, `BufWriter<StdoutLock>` tolerance of `BrokenPipe`, all stderr lines prefixed `dump_reassemble: `.
Divergences, each stated in the doc header with its reason:
- stdout carries nothing by default; raw PCM goes to `--out <file>` because a WAV-less byte soup on a terminal is useless. Manifest on stderr: blocks complete/failed/abandoned, chunks stored/duplicate/conflict/late, records decoded, `bad_frames`, `resyncs`, `discarded_bytes`, the accounting law line copied from `console_decode`'s report, one row per completed block (`blk`, bytes, PCM offset), and measured useful-bytes-per-wire-byte against the theoretical value derived from TASK-038.02.02's constants.
- Exit codes: `0` everything provably complete, `1` any failed, conflicted, or abandoned block (or a non-zero `bad_frames`), `2` input it could not read. `console_decode` deliberately always exits 0 except on unreadable input; this tool gates CI, so it must fail loudly. Say so, or a reviewer will "fix" the inconsistency.
- `--selftest`: generate synthetic streams in memory (three clean blocks; one whole record deleted; one body byte corrupted; chunks reordered within a block; every leading `~` stripped; `AUDEND` alone; a duplicated chunk; a duplicated chunk with flipped bytes), run each through the same decoder→assembler path, print one `dump_reassemble: selftest <name> ok` line per case with its expected outcome, and exit 1 if any case disagrees. No board, no files, no tempdirs.

## Tests (`tests/console_dump.rs`)
Drive `frame::Decoder` + `BlockAssembler` directly over generated streams; the example's `--selftest` covers the binary, these cover the library:
1. `deleting_any_record_is_detected` — build N blocks, delete one record at a proptest-chosen offset, assert no `Complete` returns wrong bytes and the failure names the missing index.
2. `any_single_byte_body_mutation_is_caught` — flip one byte inside the base64 payload region: either the frame CRC rejects it (`bad_frames > 0`) or the block CRC/strict-base64 rejects it; never a silent success. Note in-doc that a 16-bit block CRC over ~8 kB is thin cover on its own (SIGCOMM 2000's "When the CRC and TCP checksum disagree" measured real packets passing end-to-end checks they should have failed), which is why sequence and strict decoding carry the rest.
3. `arrival_order_changes_nothing` — shuffle chunks across blocks with a proptest permutation, assert identical PCM and the same completions.
4. `stripped_start_markers_complete_nothing` — remove every `~`: zero `Complete`, everything charged to `discarded_bytes`, accounting law balances. This is the blind spot at `frame.rs:47-49` made concrete.
5. `duplicates_are_idempotent_and_conflicts_are_refused`, `late_chunk_is_reported`, `audend_alone_fails_with_full_missing_list`, `chunk_index_beyond_n_is_malformed`, `buffer_too_small_for_n_reports_capacity`.
6. `accounting_law_holds_over_dump_traffic` — reuse `Outcome`'s law from `console_frame.rs:156-172` (copy the shape; it lives in a different integration-test binary) over mixed AUDIO/AUDEND/STATUS streams pushed in random chunk sizes.

## CI wiring
Append to `.github/workflows/ci.yml` immediately after `cargo test --workspace --features asperitas-pod/pod-hw`, inside the existing `nix develop` heredoc:
```
cargo run -p asperitas-logging --example dump_reassemble -- --selftest
```
`--all-targets` clippy already compiles the example; this makes CI actually execute it and therefore check exit codes. Keep the diff to that one line — do not restructure the workflow.

## Verification
`cargo fmt --all --check` · `cargo clippy --workspace --all-targets -- -D warnings` · `cargo test -p asperitas-logging` · `cargo run -p asperitas-logging --example dump_reassemble -- --selftest; echo $?` (expect 0, and expect 1 after deliberately breaking one assertion) · `cd firmware && cargo build --release --features seed3`.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Deviations from the plan, each one forced by arithmetic or by a defect the tests found:

1. **No `Action::Started`.** The plan's enum had it. A block already announces itself with its first stored chunk, so the variant carried no information the caller did not reconstruct from context. Shipped set: `ChunkStored`, `Duplicate`, `Conflict`, `Complete`, `Failed`, `LateRecord`, `Abandoned`, `Malformed`, `Ignored`.
2. **`Failure` has six variants, not three.** The plan named missing-list, CRC mismatch, and conflict. Three more have no honest home elsewhere: `Capacity { needed }` because the staging buffer is caller-provided and may be narrower than the geometry demands (silently refusing to place a chunk would produce a block that looks complete); `Length { declared, placed, expected }` because placement at `i · CHUNK_RAW` means a short non-final chunk leaves a hole, and comparing what was placed against what the geometry requires is the only proof that every non-final chunk was full; `ChunkCount { expected, found }` because two chunks of one block disagreeing about `n` leaves the block with no geometry to assemble against. Only corruption produces it — one writer emits one `n` per block.
3. **The missing list is an inline bitmask** (`[u64; MASK_WORDS]`, four words for the 256 indices two hex digits can name) rather than a `Vec` or a fixed `[u16; 255]`: it names any subset of indices, counts them, and answers `contains(index)` in constant time without allocating, which is what `no_std` and AC #1's "no allocation beyond the staging buffer" require together.
4. **Arrival order is permuted within blocks, not across them** (plan's Tests item 3 said across). AC #3 *requires* that a chunk naming a different block while one is open abandons the open block, so a cross-block shuffle is loss by construction, not reordering — a test doing it would assert the opposite of the semantics. `arrival_order_changes_nothing` therefore generates an independent permutation per block, and `interleaved_blocks_abandon_the_open_one` covers the cross-block case as the abandonment it is.
5. **Two corruption models, deliberately kept apart.** The library property (`any_single_byte_body_mutation_is_caught`) mutates a byte already on the wire and asserts a disjunction — frame CRC rejects it, or the block does not complete — because line noise lands wherever it lands and pinning which receiver catches it would over-specify. The example's `samples-corrupted` case corrupts raw samples *before* framing, which is what a device fault looks like: every frame validates, `bad_frames == 0`, and only the block checksum objects. Testing only the first would leave the second unexercised, and it is the one the firmware can actually produce.
6. **A capture with no `--out` discards samples instead of holding them.** Accumulating in memory whenever no file was named would make the counting tool allocate hundreds of megabytes on a long capture. `WriteOrDiscard::to_file` drops, `to_memory` keeps, and only `--selftest` asks for the latter.
7. **One `drain()` serves both the read loop and the drain after `Decoder::finish`.** They were separate matches, and the second handled only `Action::Failed` — so a record still queued when a read gave up could complete a block whose samples were never written and whose completion was never counted. Same bug class this ticket exists to close, found by reading the diff rather than by a test.

`frame::parse_hex` and `frame::parse_decimal` became `pub(crate)`: the assembler parses the same grammar on the receiving side, and a second spelling of "valid lowercase hex" is how an encoder and an assembler drift apart. Writing half stayed owned by `frame` (TASK-038.02.02 widened those).

Evidence worth keeping: `dump_reassemble` was run end to end on a capture generated outside Rust (CPython built frames with its own base64 and CCITT-FALSE loop) — 1,908 wire bytes, 894 PCM bytes, exit 0, `cmp` clean against the source samples; deleting one record gave exit 1 with `block 0001 refused: Missing(1 missing [0])` and 596 PCM bytes; stdin input behaved identically; a nonexistent path gave exit 2.

Verification: cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test --workspace (console_dump 34, console_frame 33, dump unit 31), cargo run -p asperitas-logging --example dump_reassemble -- --selftest (9 cases, exit 0), cd firmware && cargo build --release --features seed3 all pass. No new dependencies; both Cargo.lock files unchanged.
<!-- SECTION:NOTES:END -->


## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Host-side block assembly shipped in crates/asperitas-logging/src/dump.rs: BlockAssembler<'a> consumes frame::Record.body slices against one caller-provided staging buffer, parses AUDIO and AUDEND bodies itself, ignores every other verb, places each chunk at chunk_index * CHUNK_RAW, and returns Actions — at most two events per record, so a record that kills one block and decides another reports both. A block completes only when all n chunks arrived, the placed bytes match the geometry, and crc16_ccitt over the assembled samples equals AUDEND's; every other path yields Failure::{Missing, Crc, Conflict, Capacity, Length, ChunkCount}, and a refused block hands out no bytes. Identical re-sends are idempotent; the same index with different bytes refuses and names the index; a chunk for another block abandons the open one with its missing list; a chunk arriving after a verdict is reported late, not applied.

examples/dump_reassemble.rs reads a file or stdin in 4 KiB pieces, writes raw PCM to --out, and prints the manifest, the accounting law, and measured versus theoretical useful-bytes-per-wire-byte to stderr under dump_reassemble: . Its exit code is a verdict (0 proved, 1 something failed to prove itself, 2 input unreadable), the deliberate break from console_decode's always-0 contract, stated with its reason in the doc header. --selftest drives nine synthetic captures through the shipped consume() path — clean, record deleted, samples corrupted before framing, chunks reordered, start markers stripped, AUDEND alone, duplicate chunk, duplicate with flipped bytes, late chunk after close — and CI now runs it inside the existing nix develop heredoc right after the workspace tests, so refusal behaviour is checked with no board attached.

Fourteen new tests in tests/console_dump.rs (20 to 34) attack the assembler rather than agree with it: which record goes missing and which payload byte gets corrupted are proptest-chosen, permutation is generated independently per block, and mixed AUDIO/AUDEND/STATUS traffic over generated push sizes holds console_frame.rs's law (pushed == framed + discarded + buffered) at every push. Two failures the suite earned: the pre-existing draft of this ticket's code did not compile at all, and the example's post-finish drain interpreted a queued record differently from the read loop's, dropping a completed block's samples in silence.
<!-- SECTION:FINAL_SUMMARY:END -->
