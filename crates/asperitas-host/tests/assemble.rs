//! Block-assembly suite for `asperitas_host::assemble::BlockAssembler`: given a whole console stream,
//! does the host know when it is *missing* a block?
//!
//! Run with: `cargo test -p asperitas-host --test assemble`
//!
//! The assembler is driven over whole streams: which record goes missing and which byte gets
//! corrupted are generated rather than hand-picked, because "we tested deleting a record" is weak
//! evidence for "no record can go missing unnoticed".

// ---------------------------------------------------------------------------
// Block assembly — refusing a block that cannot be proved complete
// ---------------------------------------------------------------------------
//
// Everything above asks whether a payload decodes to the right bytes. This section asks the harder
// question: given a whole console stream, does the host know when it is *missing* one? A CRC answers
// integrity and never absence — `frame.rs` documents that a record whose leading `~` vanished raises
// no integrity failure at all — so completeness here is proved by sequence: `n` says how many chunks
// were owed, and any index that never arrived refuses the block.
//
// The properties below are adversarial by construction. Each one damages a generated capture in the
// way a real fault does — a lost span of wire, samples that went wrong inside a frame that still
// validates, a retransmission carrying different bytes — and demands that the assembler both refuse
// and name the damage. Two of them run over generated input rather than a single hand-picked case:
// which record goes missing, and which byte gets corrupted, are chosen by `proptest`, because "we
// tested deleting a record" is weak evidence for "no record can go missing unnoticed".

use asperitas_host::assemble::{
    Abandoned, Action, Actions, BlockAssembler, Failure, Finish, Tally,
};
use asperitas_wire::dump::{self, CHUNK_RAW, MAX_CHUNKS_PER_BLOCK};
use asperitas_wire::frame::{
    crc16_ccitt, encode, Decoder, Stats, MAX_BODY, MAX_FRAME, PREFIX_LEN, TRAILER_LEN,
};
use asperitas_wire::Level;
use proptest::prelude::*;

/// Build an `AUDIO` body, failing loudly if the chosen fields were invalid.
fn audio_body_bytes(block_index: u32, chunks: u16, chunk_index: u16, raw: &[u8]) -> Vec<u8> {
    let mut body = [0u8; MAX_BODY];
    let len = dump::audio_body(block_index, chunks, chunk_index, raw, &mut body)
        .expect("fields chosen by these tests are valid");
    body[..len].to_vec()
}

/// Build an `AUDEND` body.
fn audend_body_bytes(block_index: u32, chunks: u16, total_bytes: u32, crc: u16) -> Vec<u8> {
    let mut body = [0u8; MAX_BODY];
    let len = dump::audend_body(block_index, chunks, total_bytes, crc, &mut body)
        .expect("fields chosen by these tests are valid");
    body[..len].to_vec()
}

/// Frame a complete `AUDIO` record and return its bytes.
fn audio_wire(
    level: Level,
    seq: u32,
    now_ms: u32,
    block_index: u32,
    chunks: u16,
    chunk_index: u16,
    raw: &[u8],
) -> Vec<u8> {
    let mut body = [0u8; MAX_BODY];
    let mut frame_buf = [0u8; MAX_FRAME];
    let enc = dump::audio_record(
        level,
        seq,
        now_ms,
        block_index,
        chunks,
        chunk_index,
        raw,
        &mut body,
        &mut frame_buf,
    )
    .expect("fields chosen by these tests are valid");
    frame_buf[..enc.len].to_vec()
}

/// `AUDEND` counterpart of [`audio_wire`].
fn audend_wire(
    level: Level,
    seq: u32,
    now_ms: u32,
    block_index: u32,
    chunks: u16,
    total_bytes: u32,
    crc: u16,
) -> Vec<u8> {
    let mut body = [0u8; MAX_BODY];
    let mut frame_buf = [0u8; MAX_FRAME];
    let enc = dump::audend_record(
        level,
        seq,
        now_ms,
        block_index,
        chunks,
        total_bytes,
        crc,
        &mut body,
        &mut frame_buf,
    )
    .expect("fields chosen by these tests are valid");
    frame_buf[..enc.len].to_vec()
}

/// Chunks per generated block: three, so a block has a middle chunk to lose and a short final chunk
/// to exercise the length arithmetic in the same shape.
const ASSEMBLY_CHUNKS: u16 = 3;

/// Raw bytes in a generated block's final chunk. Not [`CHUNK_RAW`]: the grammar says the last chunk is
/// the short one, and the length checks only bite if that is true.
const ASSEMBLY_TAIL: usize = 40;

/// Raw bytes one generated block carries.
const ASSEMBLY_BLOCK_BYTES: usize = (ASSEMBLY_CHUNKS as usize - 1) * CHUNK_RAW + ASSEMBLY_TAIL;

/// Blocks per generated capture. Three is the smallest number where one block can die while another
/// survives on each side of it, which is the shape most assertions below depend on.
const ASSEMBLY_BLOCKS: u32 = 3;

/// Staging room for the widest block the grammar can name, so no test in this section is refused for
/// capacity unless it asks to be.
const ASSEMBLY_STAGING: usize = MAX_CHUNKS_PER_BLOCK * CHUNK_RAW;

/// Wire overhead of one record, shared with `tests/console_frame.rs`'s accounting law.
const FRAME_OVERHEAD: usize = PREFIX_LEN + TRAILER_LEN;

/// Deterministic samples for chunk `index` of `block`: different for every block and index, spanning
/// the whole byte range across a block so two payloads mixed together could not pass unnoticed.
fn samples(block: u32, index: u16) -> Vec<u8> {
    let len = if index + 1 == ASSEMBLY_CHUNKS {
        ASSEMBLY_TAIL
    } else {
        CHUNK_RAW
    };
    (0..len)
        .map(|byte| (((index as usize * 7 + byte) % 256) as u8) ^ block as u8)
        .collect()
}

/// One generated block, holding both views a test needs: the bodies an assembler consumes, and the
/// wire records a decoder receives.
struct TestBlock {
    /// Block id as it appears on the wire.
    id: u32,
    /// `AUDIO` bodies in send order.
    bodies: Vec<Vec<u8>>,
    /// The `AUDEND` body closing the block.
    summary_body: Vec<u8>,
    /// Complete wire records in send order, summary last.
    records: Vec<Vec<u8>>,
    /// Samples this block's summary checksums: the bytes a completed block must hand back.
    pcm: Vec<u8>,
}

/// Build one block, numbering its records from `seq` as a device would.
fn assembly_block(seq: &mut u32, id: u32) -> TestBlock {
    let mut pcm = Vec::with_capacity(ASSEMBLY_BLOCK_BYTES);
    let mut bodies = Vec::with_capacity(ASSEMBLY_CHUNKS as usize);
    let mut records = Vec::with_capacity(ASSEMBLY_CHUNKS as usize + 1);

    for index in 0..ASSEMBLY_CHUNKS {
        let raw = samples(id, index);
        pcm.extend_from_slice(&raw);
        bodies.push(audio_body_bytes(id, ASSEMBLY_CHUNKS, index, &raw));
        records.push(audio_wire(
            Level::Info,
            *seq,
            *seq * 4,
            id,
            ASSEMBLY_CHUNKS,
            index,
            &raw,
        ));
        *seq += 1;
    }

    let summary_body = audend_body_bytes(id, ASSEMBLY_CHUNKS, pcm.len() as u32, crc16_ccitt(&pcm));
    records.push(audend_wire(
        Level::Info,
        *seq,
        *seq * 4,
        id,
        ASSEMBLY_CHUNKS,
        pcm.len() as u32,
        crc16_ccitt(&pcm),
    ));
    *seq += 1;

    TestBlock {
        id,
        bodies,
        summary_body,
        records,
        pcm,
    }
}

/// A whole capture: blocks in send order, the concatenated stream, and the PCM the stream must yield.
struct Capture {
    /// Indexed by block id, which these helpers always assign as the position in send order.
    blocks: Vec<TestBlock>,
    stream: Vec<u8>,
    pcm: Vec<u8>,
}

fn capture() -> Capture {
    let mut seq = 1u32;
    let blocks: Vec<TestBlock> = (0..ASSEMBLY_BLOCKS)
        .map(|id| assembly_block(&mut seq, id))
        .collect();
    let stream: Vec<u8> = blocks
        .iter()
        .flat_map(|b| b.records.iter())
        .flatten()
        .copied()
        .collect();
    let pcm: Vec<u8> = blocks.iter().flat_map(|b| b.pcm.iter()).copied().collect();
    Capture {
        blocks,
        stream,
        pcm,
    }
}

impl Capture {
    /// Every block's id, in send order.
    fn ids(&self) -> Vec<u32> {
        self.blocks.iter().map(|b| b.id).collect()
    }

    /// Every block's id except `gone`.
    fn ids_except(&self, gone: usize) -> Vec<u32> {
        self.blocks
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != gone)
            .map(|(_, b)| b.id)
            .collect()
    }

    /// Samples of every block except `gone`, in send order: the exact output a refusal must produce,
    /// which is how the tests prove a refused block contributes none of its chunks rather than
    /// contributing the ones that happened to arrive.
    fn pcm_without(&self, gone: usize) -> Vec<u8> {
        self.blocks
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != gone)
            .flat_map(|(_, b)| &b.pcm)
            .copied()
            .collect()
    }

    /// This capture's wire bytes with one record removed. `record` indexes a block's records, where
    /// the last one is its summary.
    fn without_record(&self, block: usize, record: usize) -> Vec<u8> {
        let mut stream = Vec::with_capacity(self.stream.len());
        for (index, b) in self.blocks.iter().enumerate() {
            for (position, wire) in b.records.iter().enumerate() {
                if index == block && position == record {
                    continue;
                }
                stream.extend_from_slice(wire);
            }
        }
        stream
    }

    /// All records flattened, so a test can interleave other traffic between them.
    fn records_flat(&self) -> Vec<&[u8]> {
        self.blocks
            .iter()
            .flat_map(|b| &b.records)
            .map(Vec::as_slice)
            .collect()
    }
}

/// What one pass of decoder-plus-assembler produced, in the terms refusals get judged by.
#[derive(Debug, Default)]
struct Assembly {
    /// `(block, bytes)` for every block that completed, in completion order. Copied out immediately,
    /// because the assembler hands out one block at a time.
    completed: Vec<(u32, Vec<u8>)>,
    refused: Vec<(u32, Failure)>,
    abandoned: Vec<Abandoned>,
    /// Blocks a record arrived for after that block had already reached a verdict.
    late: Vec<u32>,
    malformed: usize,
    ignored: u64,
    tally: Tally,
    stats: Stats,
}

impl Assembly {
    /// Ids of every block that completed, in completion order.
    fn completed_ids(&self) -> Vec<u32> {
        self.completed.iter().map(|(id, _)| *id).collect()
    }

    /// Concatenation of every completed block's bytes, in completion order.
    fn pcm(&self) -> Vec<u8> {
        self.completed
            .iter()
            .flat_map(|(_, pcm)| pcm.iter())
            .copied()
            .collect()
    }

    /// Whether `block` completed.
    fn completed(&self, block: u32) -> bool {
        self.completed.iter().any(|(id, _)| *id == block)
    }

    /// The refusal naming `block`, if it was refused.
    fn failure_of(&self, block: u32) -> Option<Failure> {
        self.refused
            .iter()
            .find(|(id, _)| *id == block)
            .map(|(_, reason)| *reason)
    }
}

/// Charge one record's events to the harness, copying out any samples the assembler just handed over.
///
/// One funnel for both drivers below, so a new [`Action`] variant cannot be quietly dropped by one of
/// them, and so the copy out of the staging buffer happens where the borrow rules make it obvious why
/// it is needed: the next record invalidates whatever the previous one pointed at.
fn absorb(assembler: &BlockAssembler, actions: &Actions, out: &mut Assembly) {
    for action in actions.as_slice() {
        match *action {
            Action::Complete { block, bytes } => {
                assert_eq!(
                    assembler.pcm().len(),
                    bytes,
                    "Complete promises exactly as many bytes as pcm() yields"
                );
                out.completed.push((block, assembler.pcm().to_vec()));
            }
            Action::Failed { block, reason } => out.refused.push((block, reason)),
            Action::Abandoned { block, missing } => {
                out.abandoned.push(Abandoned { block, missing })
            }
            Action::LateRecord { block, .. } => out.late.push(block),
            Action::Malformed { .. } => out.malformed += 1,
            Action::Ignored => out.ignored += 1,
            _ => {}
        }
    }
}

/// Run a stream through the real [`Decoder`] and a [`BlockAssembler`], offering `push_size` bytes at a
/// time, and check the accounting law at every push.
///
/// Chunk sizes are part of what a capture does to a reader: a law that balances only when the whole
/// stream arrives in one call is a law about the test harness.
fn assemble(bytes: &[u8], push_size: usize) -> Assembly {
    let mut staging = vec![0u8; ASSEMBLY_STAGING];
    let mut assembler = BlockAssembler::new(&mut staging);
    let mut decoder = Decoder::new();
    let mut out = Assembly::default();
    let mut pushed = 0u64;
    let mut framed = 0u64;

    let mut off = 0usize;
    while off < bytes.len() {
        let end = (off + push_size).min(bytes.len());
        let before = off;
        off += decoder.push(&bytes[off..end]);
        assert!(
            off > before,
            "decoder took nothing from offset {} of a {}-byte offer",
            before,
            bytes.len()
        );
        pushed += (off - before) as u64;
        drain(&mut decoder, &mut assembler, &mut out, &mut framed);
        check_law(pushed, framed, &decoder, "after push");
    }

    decoder.finish();
    drain(&mut decoder, &mut assembler, &mut out, &mut framed);
    check_law(pushed, framed, &decoder, "after finish");

    let Finish { abandoned, tally } = assembler.finish();
    if let Some(abandoned) = abandoned {
        out.abandoned.push(abandoned);
    }
    out.tally = tally;
    out.stats = decoder.stats();
    out
}

/// Hand every queued record to the assembler, counting what each one cost on the wire.
fn drain(
    decoder: &mut Decoder,
    assembler: &mut BlockAssembler,
    out: &mut Assembly,
    framed: &mut u64,
) {
    while let Some(record) = decoder.next_record() {
        *framed += (FRAME_OVERHEAD + record.body.len()) as u64;
        let actions = assembler.accept(record.body);
        absorb(assembler, &actions, out);
    }
}

/// `bytes_pushed == framed_bytes + discarded_bytes + buffered()` — the same law
/// `tests/console_frame.rs` enforces, restated here because each integration-test binary is its own
/// crate and cannot share a private helper.
fn check_law(pushed: u64, framed: u64, decoder: &Decoder, when: &str) {
    let discarded = decoder.stats().discarded_bytes;
    let buffered = decoder.buffered();
    assert!(
        buffered <= MAX_FRAME,
        "{when}: {} bytes buffered, more than one frame",
        buffered
    );
    let accounted = framed + discarded + buffered as u64;
    assert_eq!(
        pushed, accounted,
        "{when}: pushed {} but framed {} + discarded {} + buffered {} = {}",
        pushed, framed, discarded, buffered, accounted
    );
}

/// Feed bodies straight to the assembler — no framing, no decoder — for the cases where the grammar
/// and the geometry are under test rather than the wire.
fn feed(bodies: &[&[u8]], staging_len: usize) -> Assembly {
    let mut staging = vec![0u8; staging_len];
    let mut assembler = BlockAssembler::new(&mut staging);
    let mut out = Assembly::default();
    for body in bodies {
        let actions = assembler.accept(body);
        absorb(&assembler, &actions, &mut out);
    }
    let Finish { abandoned, tally } = assembler.finish();
    if let Some(abandoned) = abandoned {
        out.abandoned.push(abandoned);
    }
    out.tally = tally;
    out
}

/// A `STATUS` record: the chatter that shares the console with dump traffic and must flow past the
/// assembler without being mistaken for either loss or payload.
fn status_wire(seq: u32, now_ms: u32) -> Vec<u8> {
    let mut buf = [0u8; MAX_FRAME];
    let encoded = encode(Level::Info, seq, now_ms, b"STATUS underruns=0", &mut buf);
    buf[..encoded.len].to_vec()
}

/// A permutation of `0..len`, for the tests that scramble arrival order.
fn permutation(len: usize) -> impl Strategy<Value = Vec<usize>> {
    Just((0..len).collect::<Vec<usize>>()).prop_shuffle()
}

/// The baseline the damaged cases are measured against: a healthy capture completes every block and
/// hands back exactly the samples that went in.
#[test]
fn clean_capture_completes_every_block() {
    let capture = capture();
    let assembly = assemble(&capture.stream, 4096);

    assert_eq!(assembly.completed_ids(), capture.ids());
    assert_eq!(assembly.pcm(), capture.pcm);
    assert!(assembly.refused.is_empty(), "{:?}", assembly.refused);
    assert!(assembly.abandoned.is_empty());
    assert!(assembly.late.is_empty());
    assert_eq!(assembly.malformed, 0);
    assert_eq!(assembly.stats.bad_frames, 0);
    assert_eq!(
        assembly.tally.chunks_stored,
        u64::from(ASSEMBLY_BLOCKS) * ASSEMBLY_CHUNKS as u64
    );
    assert_eq!(assembly.tally.pcm_bytes, capture.pcm.len() as u64);
}

proptest! {
    /// Any record can go missing, so *which* one is generated rather than chosen by whoever wrote the
    /// test. Deleting a chunk must refuse its block and name the absent index; deleting a summary must
    /// report the block abandoned, because nothing was missing — the verdict simply never arrived.
    /// Either way the refused block hands out no samples, and the blocks around it are untouched.
    #[test]
    fn deleting_any_record_is_detected(
        block in 0..ASSEMBLY_BLOCKS as usize,
        record in 0..ASSEMBLY_CHUNKS as usize + 1,
    ) {
        let capture = capture();
        let stream = capture.without_record(block, record);
        let assembly = assemble(&stream, 4096);
        let gone = block as u32;

        prop_assert_eq!(assembly.completed_ids(), capture.ids_except(block));
        prop_assert_eq!(assembly.pcm(), capture.pcm_without(block));

        if record < ASSEMBLY_CHUNKS as usize {
            // A chunk vanished: the summary arrives, counts what came, and refuses.
            let reason = assembly.failure_of(gone);
            match reason {
                Some(Failure::Missing(missing)) => {
                    prop_assert_eq!(missing.count(), 1, "one chunk missing: {:?}", missing);
                    prop_assert!(
                        missing.contains(record as u16),
                        "the missing list must name the deleted chunk: {:?}", missing
                    );
                }
                other => {
                    prop_assert!(false, "expected a missing-chunk refusal, got {:?}", other)
                }
            }
            prop_assert!(assembly.abandoned.is_empty());
        } else {
            // The summary vanished: every chunk is present, so there is nothing to name, and the
            // block dies when the next one's first chunk displaces it — or when the stream ends.
            prop_assert!(assembly.refused.is_empty(), "{:?}", assembly.refused);
            prop_assert_eq!(assembly.abandoned.len(), 1);
            prop_assert_eq!(assembly.abandoned[0].block, gone);
            prop_assert_eq!(
                assembly.abandoned[0].missing.count(),
                0,
                "no chunk was missing; only the verdict never came"
            );
        }
    }
}

proptest! {
    /// Corrupt one character inside a chunk's base64 payload region on the wire, leaving every other
    /// byte — including the record's own checksum — exactly as sent.
    ///
    /// Two receivers stand between that mutation and a false "complete": the frame CRC, which catches
    /// almost all of it, and behind that the strict base64 decode plus the block checksum, which catch
    /// what rides through inside a valid frame. What may not happen is silence. So the property is
    /// stated as a disjunction the generator cannot satisfy by luck: either some frame was rejected, or
    /// the block that owned the mutated byte did not complete.
    ///
    /// A 16-bit block checksum over ~4 kB of samples is thin cover on its own — SIGCOMM 2000's "When
    /// the CRC and TCP checksum disagree" measured real packets that passed end-to-end checks they
    /// should have failed — which is precisely why sequence and strict decoding carry the rest of this
    /// claim rather than the checksum alone.
    #[test]
    fn any_single_byte_body_mutation_is_caught(
        block in 0..ASSEMBLY_BLOCKS as usize,
        chunk in 0..ASSEMBLY_CHUNKS as usize,
        position in 0..CHUNK_RAW * 4 / 3,
        delta in 1u8..=255,
    ) {
        let capture = capture();
        let record = &capture.blocks[block].records[chunk];
        // Payload region: past the fixed-width header, up to the trailer's checksum digits.
        let start = dump::AUDIO_HEADER_LEN;
        let end = record.len() - TRAILER_LEN;
        let at = start + position % (end - start);

        let mut mutated = record.clone();
        mutated[at] ^= delta;
        let mut stream = Vec::with_capacity(capture.stream.len());
        for (index, b) in capture.blocks.iter().enumerate() {
            for (slot, wire) in b.records.iter().enumerate() {
                if index == block && slot == chunk {
                    stream.extend_from_slice(&mutated);
                } else {
                    stream.extend_from_slice(wire);
                }
            }
        }

        let assembly = assemble(&stream, 4096);
        prop_assert!(
            assembly.stats.bad_frames > 0 || !assembly.completed(block as u32),
            "silent success: a mutated payload byte at offset {} completed block {} (bad_frames={})",
            at,
            block,
            assembly.stats.bad_frames
        );
        // Whatever did complete carries exactly the bytes that were sent, never a plausible facsimile.
        for (id, pcm) in &assembly.completed {
            prop_assert_eq!(pcm, &capture.blocks[*id as usize].pcm);
        }
    }
}

/// The other half of the disjunction above, pinned rather than probabilistic: samples that go wrong
/// *before* framing ride through the frame CRC untouched, and only the block checksum stops them. This
/// is what a device fault looks like, as opposed to line noise.
#[test]
fn block_checksum_catches_damage_inside_valid_frames() {
    let mut bodies = Vec::new();
    let honest: Vec<u8> = (0..ASSEMBLY_CHUNKS)
        .flat_map(|index| samples(0, index))
        .collect();

    for index in 0..ASSEMBLY_CHUNKS {
        let mut raw = samples(0, index);
        if index == 0 {
            raw[0] ^= 0xff;
        }
        bodies.push(audio_body_bytes(0, ASSEMBLY_CHUNKS, index, &raw));
    }
    // The summary checksums the samples as they should have been, exactly as a writer would.
    bodies.push(audend_body_bytes(
        0,
        ASSEMBLY_CHUNKS,
        honest.len() as u32,
        crc16_ccitt(&honest),
    ));

    let assembly = feed(
        &bodies.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        ASSEMBLY_STAGING,
    );

    assert_eq!(assembly.completed_ids(), Vec::<u32>::new());
    match assembly.failure_of(0) {
        Some(Failure::Crc { shipped, computed }) => {
            assert_ne!(
                shipped, computed,
                "a refusal that agrees with itself proves nothing"
            );
        }
        other => panic!("expected a checksum refusal, got {other:?}"),
    }
    // Nothing was missing and no length disagreed: the checksum was the only thing left to object.
    assert_eq!(assembly.tally.malformed_records, 0);
}

proptest! {
    /// Chunks land at `chunk_index · CHUNK_RAW`, so permutation is structurally impossible rather than
    /// merely unlikely. Generated here: an independent arrival order per block, since a single lucky
    /// shuffle would prove nothing about the other two.
    #[test]
    fn arrival_order_changes_nothing(
        orders in prop::collection::vec(permutation(ASSEMBLY_CHUNKS as usize), ASSEMBLY_BLOCKS as usize),
    ) {
        let capture = capture();
        let mut stream = Vec::with_capacity(capture.stream.len());
        for (block, order) in capture.blocks.iter().zip(orders) {
            for index in order {
                stream.extend_from_slice(&block.records[index]);
            }
            // The summary still closes its own block: one producer writes sequentially, so interleaving
            // blocks would be loss rather than reordering (see `BlockAssembler`'s documentation).
            stream.extend_from_slice(&block.records[block.records.len() - 1]);
        }

        let assembly = assemble(&stream, 4096);
        prop_assert_eq!(assembly.completed_ids(), capture.ids());
        prop_assert_eq!(assembly.pcm(), capture.pcm.clone());
        prop_assert!(assembly.refused.is_empty(), "{:?}", assembly.refused);
    }
}

/// The blind spot `frame.rs` documents, made concrete: strip the leading `~` from every record and
/// there is nothing to decode, no integrity failure anywhere, and no honest report except "every byte
/// you gave me is gone". Zero completions, zero chunks, and the accounting law still balancing.
#[test]
fn stripped_start_markers_complete_nothing() {
    let capture = capture();
    let stripped: Vec<u8> = capture
        .stream
        .iter()
        .copied()
        .filter(|byte| *byte != b'~')
        .collect();
    let assembly = assemble(&stripped, 4096);

    assert!(assembly.completed.is_empty());
    assert_eq!(assembly.tally.chunks_stored, 0);
    assert_eq!(assembly.stats.records, 0);
    assert_eq!(
        assembly.stats.discarded_bytes,
        stripped.len() as u64,
        "bodies are sanitised, so nothing but a start marker can carry a tilde"
    );
    // And the experiment damaged what it claimed to: exactly one marker per record, which holds only
    // because bodies are sanitised on the way out and so no payload can carry a tilde.
    assert_eq!(
        capture.stream.iter().filter(|byte| **byte == b'~').count(),
        ASSEMBLY_BLOCKS as usize * (ASSEMBLY_CHUNKS as usize + 1),
        "start markers belong to records and nowhere else"
    );
}

/// A retransmission of identical bytes is the retry a lossy link expects: idempotent, and invisible in
/// the counters that gate CI. The same index carrying *different* bytes is two candidate shapes for one
/// block, so it gets a refusal naming the index instead.
#[test]
fn duplicates_are_idempotent_and_conflicts_are_refused() {
    let capture = capture();
    let block = &capture.blocks[0];
    let bodies: Vec<&[u8]> = block
        .bodies
        .iter()
        .chain(std::iter::once(&block.summary_body))
        .map(Vec::as_slice)
        .collect();

    // Identical re-send of the middle chunk.
    let mut retried = bodies.clone();
    retried.insert(2, block.bodies[1].as_slice());
    let assembly = feed(&retried, ASSEMBLY_STAGING);
    assert_eq!(assembly.completed_ids(), vec![0], "a retry changes nothing");
    assert_eq!(assembly.pcm(), block.pcm);
    assert_eq!(assembly.tally.duplicate_chunks, 1);
    assert_eq!(assembly.tally.conflicts, 0);
    assert!(assembly.refused.is_empty());

    // Same index, different samples, under a body that parses perfectly.
    let conflicting = audio_body_bytes(
        0,
        ASSEMBLY_CHUNKS,
        1,
        &samples(0, 1)[..]
            .iter()
            .map(|b| b ^ 0x5a)
            .collect::<Vec<_>>(),
    );
    let mut conflicted = bodies.clone();
    conflicted.insert(2, conflicting.as_slice());
    let assembly = feed(&conflicted, ASSEMBLY_STAGING);
    assert!(
        assembly.completed.is_empty(),
        "two candidate shapes complete as neither"
    );
    match assembly.failure_of(0) {
        Some(Failure::Conflict { index }) => assert_eq!(index, 1, "the refusal names the index"),
        other => panic!("expected a conflict refusal, got {other:?}"),
    }
    assert_eq!(assembly.tally.conflicts, 1);
}

/// Completion is final. A chunk arriving after its block's verdict is reported and dropped, because
/// reopening a published decision would mean a block could change shape depending on when a straggler
/// turned up.
#[test]
fn late_chunk_is_reported() {
    let capture = capture();
    let block = &capture.blocks[0];
    let mut bodies: Vec<&[u8]> = block
        .bodies
        .iter()
        .chain(std::iter::once(&block.summary_body))
        .map(Vec::as_slice)
        .collect();
    bodies.push(block.bodies[1].as_slice());

    let assembly = feed(&bodies, ASSEMBLY_STAGING);
    assert_eq!(assembly.completed_ids(), vec![0], "the verdict stands");
    assert_eq!(assembly.late, vec![0], "and the straggler is named");
    assert_eq!(assembly.tally.late_records, 1);
    assert!(assembly.refused.is_empty());
    assert_eq!(assembly.pcm(), block.pcm);
}

/// An `AUDEND` with no chunks behind it proves only what was owed: the missing list is the whole
/// block, from index zero to index `n − 1`.
#[test]
fn audend_alone_fails_with_full_missing_list() {
    let capture = capture();
    let summary = capture.blocks[2].summary_body.as_slice();
    let assembly = feed(&[summary], ASSEMBLY_STAGING);

    assert!(assembly.completed.is_empty());
    match assembly.failure_of(2) {
        Some(Failure::Missing(missing)) => {
            assert_eq!(missing.count(), ASSEMBLY_CHUNKS as usize);
            assert_eq!(missing.first(), Some(0));
            for index in 0..ASSEMBLY_CHUNKS {
                assert!(missing.contains(index), "index {index} should be missing");
            }
        }
        other => panic!("expected a missing-chunk refusal, got {other:?}"),
    }
}

/// Bodies that open with a dump verb but do not have its shape are refused where they broke, and
/// never stored. These are written as literals rather than built with `audio_body`, which rejects the
/// bad combinations outright — hand-writing them is what tests the receiving side's parser instead of
/// agreeing with our own writer.
#[test]
fn malformed_bodies_name_where_they_broke() {
    // Chunk index at or past `n`: a place in a block that says it has none.
    let beyond = b"AUDIO blk=0001 n=02 c=02 d=QUJD";
    // `n = 00`: no chunk zero to receive this.
    let empty_block = b"AUDIO blk=0001 n=00 c=00 d=QUJD";
    // Uppercase hex is not in the grammar; the encoder cannot produce it.
    let uppercase = b"AUDIO blk=000A n=02 c=00 d=QUJD";
    // A separator moved is a body whose skeleton no longer lines up.
    let shifted = b"AUDIO blk=0001,n=02 c=00 d=QUJD";
    // Payload characters beyond what the grammar budgets for one chunk.
    let oversized = format!(
        "AUDIO blk=0001 n=02 c=00 d={}",
        "A".repeat(dump::FULL_AUDIO_BODY_LEN - dump::AUDIO_HEADER_LEN + 4)
    )
    .into_bytes();

    for body in [
        &beyond[..],
        &empty_block[..],
        &uppercase[..],
        &shifted[..],
        &oversized[..],
    ] {
        let assembly = feed(&[body], ASSEMBLY_STAGING);
        assert_eq!(assembly.malformed, 1, "refused as malformed: {body:?}");
        assert_eq!(assembly.tally.chunks_stored, 0, "nothing stored: {body:?}");
        assert_eq!(
            assembly.tally.non_dump_records, 0,
            "not chatter either: {body:?}"
        );
    }

    // A malformed summary too, so the summary parser is covered by the same scepticism.
    let bad_summary = b"AUDEND blk=0001 n=03 bytes=00009 crc16=ZZZZ";
    let assembly = feed(&[&bad_summary[..]], ASSEMBLY_STAGING);
    assert_eq!(assembly.malformed, 1);
    assert!(
        assembly.refused.is_empty(),
        "a body nobody can read decides nothing"
    );
}

/// A staging buffer too small for a chunk at its geometric offset is reported as needing what the
/// geometry asks for — not silently truncated into a block that looks complete.
#[test]
fn buffer_too_small_for_n_reports_capacity() {
    let capture = capture();
    let block = &capture.blocks[1];
    let bodies: Vec<&[u8]> = block.bodies.iter().map(Vec::as_slice).collect();

    // Room for exactly one chunk: the second one belongs at `CHUNK_RAW` and will not fit.
    let assembly = feed(&bodies, CHUNK_RAW);
    assert!(assembly.completed.is_empty());
    assert_eq!(
        assembly.failure_of(1),
        Some(Failure::Capacity {
            needed: 2 * CHUNK_RAW
        }),
        "the refusal says how much room the block actually needs"
    );
}

/// One producer writes one block at a time, so a chunk naming a different block while one is open
/// means the open block's tail is gone. It is reported with what it still needed, and the newcomer
/// starts fresh — never held open alongside, which would turn a certain refusal into a guess.
#[test]
fn interleaved_blocks_abandon_the_open_one() {
    let capture = capture();
    let first = &capture.blocks[0];
    let second = &capture.blocks[1];

    let bodies: Vec<&[u8]> = first.bodies[..2]
        .iter()
        .chain(second.bodies.iter())
        .chain(std::iter::once(&second.summary_body))
        .map(Vec::as_slice)
        .collect();
    let assembly = feed(&bodies, ASSEMBLY_STAGING);

    assert_eq!(assembly.abandoned.len(), 1);
    assert_eq!(assembly.abandoned[0].block, first.id);
    assert_eq!(
        assembly.abandoned[0].missing.count(),
        1,
        "only the chunk that never got its turn is missing"
    );
    assert!(
        assembly.abandoned[0].missing.contains(2),
        "{:?}",
        assembly.abandoned[0].missing
    );
    assert_eq!(
        assembly.completed_ids(),
        vec![second.id],
        "the newcomer completes on its own"
    );
    assert_eq!(assembly.pcm(), second.pcm);
}

/// A stream ending mid-block leaves a block that may simply not have finished sending — and nothing
/// can prove that from an absent tail, so it is refused as the loss it might be.
#[test]
fn truncation_at_the_end_of_a_stream_is_refused() {
    let capture = capture();
    let owed: usize = capture.blocks[2].records[1..].iter().map(Vec::len).sum();
    let cut = capture.stream.len() - owed;
    let assembly = assemble(&capture.stream[..cut], 4096);

    assert_eq!(assembly.completed_ids(), capture.ids_except(2));
    assert_eq!(assembly.abandoned.len(), 1);
    assert_eq!(assembly.abandoned[0].block, 2);
    assert_eq!(assembly.abandoned[0].missing.count(), 2);
    assert_eq!(assembly.pcm(), capture.pcm_without(2));
}

proptest! {
    /// Dump traffic does not own the console: boot banners, `STATUS` counters, and log lines share the
    /// stream, and the assembler must let them through without charging them as loss or mistaking them
    /// for payload. Generated over how often chatter appears and how few bytes arrive per read, with
    /// the accounting law checked at every push inside [`assemble`] — the same law
    /// `tests/console_frame.rs` enforces, now holding over dump traffic too.
    #[test]
    fn accounting_law_holds_over_dump_traffic(
        push_size in 1usize..=700,
        status_every in 1usize..=4,
    ) {
        let capture = capture();
        let records = capture.records_flat();
        let mut stream = Vec::new();
        let mut seq = 1000u32;
        let mut inserted = 0u64;
        for (index, record) in records.iter().enumerate() {
            stream.extend_from_slice(record);
            if (index + 1) % status_every == 0 {
                stream.extend_from_slice(&status_wire(seq, index as u32));
                seq += 1;
                inserted += 1;
            }
        }

        let assembly = assemble(&stream, push_size);
        prop_assert_eq!(assembly.completed_ids(), capture.ids());
        prop_assert_eq!(assembly.pcm(), capture.pcm.clone());
        prop_assert_eq!(assembly.stats.bad_frames, 0);
        prop_assert_eq!(assembly.tally.non_dump_records, inserted);
        prop_assert_eq!(assembly.tally.records, (records.len() + inserted as usize) as u64);
    }
}
