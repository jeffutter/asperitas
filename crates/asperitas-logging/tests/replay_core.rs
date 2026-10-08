//! Host suite for the replay staging core in `asperitas_logging::replay`.
//!
//! Run with: `cargo test -p asperitas-logging`
//!
//! The oracle throughout is `frame::crc16_ccitt` over the source bytes and the source samples
//! themselves: the core's CRC is never compared against its own arithmetic. Every fill writes a
//! poison pattern into the claimed half before the real data, so a callback that ever read a half
//! still being filled would hand out poison and fail the sample comparison, not just the CRC.

use asperitas_logging::frame::crc16_ccitt;
use asperitas_logging::replay::{
    transition_ok, Block, HalfState, PassReport, ReplayCore, StartError, BLOCK_BYTES,
    BLOCK_SAMPLES, HALF_BYTES,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A deterministic source of `bytes` bytes whose samples are all distinct from [`POISON`] and
/// from silence, so a sample read from the wrong place cannot pass for the right one.
fn source(bytes: usize) -> Vec<u8> {
    (0..bytes / 2)
        .flat_map(|i| {
            let sample = (i as u16).wrapping_mul(7919) | 0x0001;
            let sample = if sample == POISON { 0x0101 } else { sample };
            sample.to_le_bytes()
        })
        .collect()
}

/// The pattern written into a claimed half before its real data. Odd, so `source` avoids it.
const POISON: u16 = 0xA5A5;

/// What the test harness saw the callback hand out over one pass.
#[derive(Default)]
struct Played {
    /// Source samples, in order, with padding and inserted silence excluded.
    samples: Vec<i16>,
    underruns: u32,
}

/// Run the callback once and record what it produced, checking the per-block contract: underrun
/// and idle blocks are all silence, and a played block is a run of source samples followed by
/// zero padding only when the pass ends inside it.
fn callback(core: &ReplayCore, src: &[u8], played: &mut Played) -> Block {
    let mut out = [0x5555_i16; BLOCK_SAMPLES];
    let block = core.take_block(&mut out);
    match block {
        Block::Underrun => {
            assert!(
                out.iter().all(|&s| s == 0),
                "an underrun handed out non-silence"
            );
            played.underruns += 1;
        }
        Block::Idle => assert!(out.iter().all(|&s| s == 0), "idle handed out non-silence"),
        Block::Played => {
            let remaining = src.len() / 2 - played.samples.len();
            let real = remaining.min(BLOCK_SAMPLES);
            played.samples.extend_from_slice(&out[..real]);
            assert!(
                out[real..].iter().all(|&s| s == 0),
                "the tail of the final block was not zero-padded"
            );
        }
    }
    block
}

/// Claim the next half if one is free, poison it, then fill it from `src` and publish.
fn refill(core: &ReplayCore, src: &[u8]) -> bool {
    let Some(mut claim) = core.claim_fill() else {
        return false;
    };
    let range = claim.source();
    let buf = claim.buffer();
    for pair in buf.chunks_exact_mut(2) {
        pair.copy_from_slice(&POISON.to_le_bytes());
    }
    buf.copy_from_slice(&src[range.start as usize..range.end as usize]);
    claim.finish();
    true
}

/// Source bytes as the `i16` samples the callback should hand out.
fn samples_of(src: &[u8]) -> Vec<i16> {
    src.chunks_exact(2)
        .map(|p| i16::from_le_bytes([p[0], p[1]]))
        .collect()
}

/// Play a whole pass with the refill always on time: both halves primed, refill after every block.
fn clean_pass(src: &[u8]) -> (PassReport, Played) {
    let core = Box::new(ReplayCore::new());
    core.start(src.len() as u32).unwrap();
    while refill(&core, src) {}
    let mut played = Played::default();
    let mut blocks = 0;
    while core.finish().is_none() {
        callback(&core, src, &mut played);
        refill(&core, src);
        blocks += 1;
        assert!(
            blocks <= src.len() / BLOCK_BYTES + 2,
            "the pass never completed"
        );
    }
    (core.finish().unwrap(), played)
}

// ---------------------------------------------------------------------------
// Half lifecycle
// ---------------------------------------------------------------------------

#[test]
fn transition_table_allows_exactly_the_four_hand_off_edges() {
    let legal = [
        (HalfState::Empty, HalfState::Filling),
        (HalfState::Filling, HalfState::Ready),
        (HalfState::Ready, HalfState::Playing),
        (HalfState::Playing, HalfState::Empty),
    ];
    for from in HalfState::ALL {
        for to in HalfState::ALL {
            assert_eq!(
                transition_ok(from, to),
                legal.contains(&(from, to)),
                "{from:?} -> {to:?}"
            );
        }
    }
}

#[test]
fn state_bytes_round_trip_and_unknown_bytes_name_no_state() {
    for state in HalfState::ALL {
        assert_eq!(HalfState::from_u8(state.as_u8()), Some(state));
    }
    for byte in 4..=u8::MAX {
        assert_eq!(HalfState::from_u8(byte), None);
    }
}

// ---------------------------------------------------------------------------
// Clean passes: CRC equals the source's
// ---------------------------------------------------------------------------

#[test]
fn clean_pass_crc_equals_the_source_crc_at_every_boundary_length() {
    let lengths = [
        2,               // one sample
        BLOCK_BYTES - 2, // one short block
        BLOCK_BYTES,     // exactly one block
        BLOCK_BYTES + 2, // one block and a sample
        HALF_BYTES,      // exactly one half
        HALF_BYTES + 2,  // half plus one sample
        2 * HALF_BYTES,  // both halves exactly
        3 * HALF_BYTES + 6,
        480_000, // a corpus-sized clip: 5 s of mono 48 kHz
    ];
    for len in lengths {
        let src = source(len);
        let (report, played) = clean_pass(&src);
        assert_eq!(report.crc16, crc16_ccitt(&src), "length {len}");
        assert_eq!(report.samples as usize, len / 2, "length {len}");
        assert!(report.is_clean(), "length {len}");
        assert_eq!(played.samples, samples_of(&src), "length {len}");
    }
}

#[test]
fn zero_length_pass_completes_at_once_with_the_empty_crc() {
    let core = ReplayCore::new();
    core.start(0).unwrap();
    assert!(core.claim_fill().is_none());
    let report = core.finish().unwrap();
    assert_eq!(report.crc16, crc16_ccitt(&[]));
    assert_eq!(report.samples, 0);
    assert!(report.is_clean());
    let mut out = [1; BLOCK_SAMPLES];
    assert_eq!(core.take_block(&mut out), Block::Idle);
    assert_eq!(out, [0; BLOCK_SAMPLES]);
}

#[test]
fn completed_pass_hands_out_silence_without_counting_underruns() {
    let src = source(BLOCK_BYTES + 2);
    let core = Box::new(ReplayCore::new());
    core.start(src.len() as u32).unwrap();
    refill(&core, &src);
    let mut played = Played::default();
    assert_eq!(callback(&core, &src, &mut played), Block::Played);
    assert_eq!(callback(&core, &src, &mut played), Block::Played);
    for _ in 0..4 {
        assert_eq!(callback(&core, &src, &mut played), Block::Idle);
    }
    assert_eq!(core.finish().unwrap().underruns, 0);
}

#[test]
fn final_half_claim_covers_only_the_remaining_bytes() {
    let core = Box::new(ReplayCore::new());
    core.start((HALF_BYTES + 6) as u32).unwrap();
    let first = core.claim_fill().unwrap();
    assert_eq!(first.half(), 0);
    assert_eq!(first.source(), 0..HALF_BYTES as u32);
    first.finish();
    let mut second = core.claim_fill().unwrap();
    assert_eq!(second.half(), 1);
    assert_eq!(second.source(), HALF_BYTES as u32..HALF_BYTES as u32 + 6);
    assert_eq!(second.buffer().len(), 6);
    second.finish();
    assert!(
        core.claim_fill().is_none(),
        "claimed past the end of the source"
    );
}

// ---------------------------------------------------------------------------
// Underruns
// ---------------------------------------------------------------------------

#[test]
fn late_refill_underruns_without_advancing_and_still_reaches_the_source_crc() {
    let src = source(2 * HALF_BYTES + BLOCK_BYTES);
    let core = Box::new(ReplayCore::new());
    core.start(src.len() as u32).unwrap();
    let mut played = Played::default();

    // Before any fill: underrun.
    assert_eq!(callback(&core, &src, &mut played), Block::Underrun);

    // Half 0 claimed and poisoned but not finished: the callback must not read it.
    let mut claim = core.claim_fill().unwrap();
    claim
        .buffer()
        .chunks_exact_mut(2)
        .for_each(|p| p.copy_from_slice(&POISON.to_le_bytes()));
    assert_eq!(callback(&core, &src, &mut played), Block::Underrun);
    let range = claim.source();
    claim
        .buffer()
        .copy_from_slice(&src[range.start as usize..range.end as usize]);
    claim.finish();

    // Play all of half 0 with half 1 never filled, then underrun on half 1.
    for _ in 0..HALF_BYTES / BLOCK_BYTES {
        assert_eq!(callback(&core, &src, &mut played), Block::Played);
    }
    assert_eq!(callback(&core, &src, &mut played), Block::Underrun);
    assert_eq!(callback(&core, &src, &mut played), Block::Underrun);
    assert_eq!(core.underruns(), 4);
    assert_eq!(
        played.samples.len(),
        HALF_BYTES / 2,
        "an underrun advanced the stream"
    );

    // The data finally arrives: the rest of the pass plays in order.
    while core.finish().is_none() {
        refill(&core, &src);
        callback(&core, &src, &mut played);
    }
    let report = core.finish().unwrap();
    assert_eq!(report.crc16, crc16_ccitt(&src));
    assert_eq!(report.underruns, 4);
    assert!(!report.is_clean());
    assert_eq!(played.samples, samples_of(&src));
}

// ---------------------------------------------------------------------------
// Start / restart
// ---------------------------------------------------------------------------

#[test]
fn start_refuses_a_running_pass_and_an_odd_length() {
    let core = Box::new(ReplayCore::new());
    assert_eq!(core.start(3), Err(StartError::OddLength { bytes: 3 }));
    core.start(4).unwrap();
    assert_eq!(core.start(4), Err(StartError::Running));
}

#[test]
fn a_second_pass_after_the_first_reports_its_own_crc() {
    let core = Box::new(ReplayCore::new());
    for len in [HALF_BYTES + 10, 3 * BLOCK_BYTES] {
        let src = source(len).into_iter().rev().collect::<Vec<_>>();
        core.start(src.len() as u32).unwrap();
        let mut played = Played::default();
        // One underrun first, so the counter's reset is observable too.
        callback(&core, &src, &mut played);
        while core.finish().is_none() {
            refill(&core, &src);
            callback(&core, &src, &mut played);
        }
        let report = core.finish().unwrap();
        assert_eq!(report.crc16, crc16_ccitt(&src));
        assert_eq!(report.underruns, 1);
        assert_eq!(played.samples, samples_of(&src));
    }
}

// ---------------------------------------------------------------------------
// Arbitrary interleavings
// ---------------------------------------------------------------------------

/// One step of the refill side or the callback.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// Claim a half (if none is outstanding) and poison it.
    Claim,
    /// Fill and publish the outstanding claim, if any.
    Publish,
    /// Run the callback once.
    Callback,
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![Just(Step::Claim), Just(Step::Publish), Just(Step::Callback)]
}

proptest! {
    #[test]
    fn arbitrary_interleavings_play_the_source_in_order(
        samples in 0usize..(3 * HALF_BYTES / 2),
        steps in prop::collection::vec(step(), 0..2_000),
    ) {
        let src = source(samples * 2);
        let core = Box::new(ReplayCore::new());
        core.start(src.len() as u32).unwrap();
        let mut played = Played::default();
        let mut claim = None;
        let mut underrun_blocks = 0;
        let expected = samples_of(&src);
        let mut checked = 0;
        let finish_claim = |claim: Option<asperitas_logging::replay::FillClaim<'_>>| {
            if let Some(mut c) = claim {
                let range = c.source();
                c.buffer().copy_from_slice(&src[range.start as usize..range.end as usize]);
                c.finish();
            }
        };
        for step in steps {
            match step {
                Step::Claim => {
                    if claim.is_none() {
                        claim = core.claim_fill().map(|mut c| {
                            c.buffer()
                                .chunks_exact_mut(2)
                                .for_each(|p| p.copy_from_slice(&POISON.to_le_bytes()));
                            c
                        });
                    }
                }
                Step::Publish => finish_claim(claim.take()),
                Step::Callback => {
                    if callback(&core, &src, &mut played) == Block::Underrun {
                        underrun_blocks += 1;
                    }
                }
            }
            // The stream never contains poison and is always a prefix of the source. Checked
            // incrementally: only the samples this step added.
            prop_assert_eq!(&played.samples[checked..], &expected[checked..played.samples.len()]);
            checked = played.samples.len();
        }
        finish_claim(claim.take());
        let mut guard = 0;
        while core.finish().is_none() {
            refill(&core, &src);
            if callback(&core, &src, &mut played) == Block::Underrun {
                underrun_blocks += 1;
            }
            guard += 1;
            prop_assert!(guard < 10_000, "the pass never completed");
        }
        let report = core.finish().unwrap();
        prop_assert_eq!(report.underruns, underrun_blocks);
        prop_assert_eq!(report.underruns, played.underruns);
        prop_assert_eq!(&played.samples, &expected);
        prop_assert_eq!(report.samples as usize, samples);
        // Stronger than the clean-pass claim the report sells: stalling on underrun keeps the CRC
        // reachable on every pass, clean or not.
        prop_assert_eq!(report.crc16, crc16_ccitt(&src));
    }
}

// ---------------------------------------------------------------------------
// Real threads
// ---------------------------------------------------------------------------

/// The two sides on two OS threads, refill deliberately jittery. The interleavings above are
/// sequential; this exercises the atomics' orderings under a real scheduler.
#[test]
fn concurrent_refill_and_callback_play_the_source_in_order() {
    let src = source(20 * HALF_BYTES + 6);
    let core: &'static ReplayCore = Box::leak(Box::new(ReplayCore::new()));
    core.start(src.len() as u32).unwrap();
    let refill_src = src.clone();
    let refiller = std::thread::spawn(move || {
        let mut n = 0u32;
        while core.finish().is_none() {
            if refill(core, &refill_src) {
                n += 1;
                if n.is_multiple_of(3) {
                    std::thread::yield_now();
                }
            } else {
                std::hint::spin_loop();
            }
        }
    });
    let mut played = Played::default();
    while core.finish().is_none() {
        callback(core, &src, &mut played);
    }
    refiller.join().unwrap();
    let report = core.finish().unwrap();
    assert_eq!(report.crc16, crc16_ccitt(&src));
    assert_eq!(report.underruns, played.underruns);
    assert_eq!(played.samples, samples_of(&src));
}
