//! Pipe headroom: a dump commit may never take the last `MAX_FRAME` bytes of the log ring.
//!
//! Checked exhaustively against a real `embassy_sync` ring, at the capacity the firmware builds
//! (`LOG_PIPE_SIZE`), that `dump::dump_fits` agrees with what the ring does at every occupancy,
//! including across the array-end wrap where `try_write` short-writes. This suite lives in
//! `asperitas-logging` because the ring size is that crate's constant; the predicate and the
//! frame codec it drives are in `asperitas-wire`.
//!
//! Run with: `cargo test -p asperitas-logging --test dump_headroom`

use asperitas_logging::LOG_PIPE_SIZE;
use asperitas_wire::dump;
use asperitas_wire::frame::{
    encode, write_whole, WriteOutcome, MAX_BODY, MAX_FRAME, PREFIX_LEN, TRAILER_LEN,
};
use asperitas_wire::Level;

/// The device's ring, referenced rather than restated: `LOG_PIPE_SIZE` is ungated precisely so
/// this suite sweeps the capacity the firmware builds, and cannot quietly disagree with it.
const RING: usize = LOG_PIPE_SIZE;

/// A ring in the shape the device uses, held locally rather than in a `static`.
///
/// `NoopRawMutex` because no `critical-section` implementation is registered for host, which makes
/// `CriticalSectionRawMutex` an undefined symbol at *link* time — and `NoopRawMutex` is `!Sync`
/// (`PhantomData<*mut ()>`), which rules out the static form here. On target the static works,
/// because `CriticalSectionRawMutex` is `Sync`. Same stand-in `src/frame.rs`'s own suite uses.
type Ring = embassy_sync::pipe::Pipe<embassy_sync::blocking_mutex::raw::NoopRawMutex, RING>;

fn ring() -> Ring {
    embassy_sync::pipe::Pipe::new()
}

/// Commit through the real ring exactly as `try_emit_dump` will: `|c| LOG_PIPE.try_write(c).ok()`.
///
/// Deliberately *not* gated on [`dump::dump_fits`] — these tests are about whether the predicate's
/// verdict matches what the ring does, and asking the predicate first would assume the answer.
fn commit_frame(pipe: &Ring, frame: &[u8]) -> bool {
    matches!(
        write_whole(frame, pipe.free_capacity(), |chunk| {
            pipe.try_write(chunk).ok()
        }),
        WriteOutcome::Committed
    )
}

/// Advance both cursors `at` bytes into the backing array, leaving the ring empty by occupancy —
/// the state in which a wrap short-write happens.
fn park_cursors_at(pipe: &Ring, at: usize) {
    let filler = vec![b'f'; at];
    assert_eq!(pipe.try_write(&filler).ok(), Some(at));
    let mut drained = vec![0u8; at];
    assert_eq!(pipe.try_read(&mut drained).ok(), Some(at));
    assert_eq!(
        pipe.free_capacity(),
        RING,
        "ring must report itself empty before occupancy is set"
    );
}

/// Occupy `n` bytes, crossing the array end if it has to.
///
/// Reaching an arbitrary occupancy from parked cursors needs a loop for the same reason
/// `write_whole` exists: `try_write` returns only the contiguous run to the end of the backing
/// array even though `free_capacity()` reports total free.
fn fill_ring(pipe: &Ring, n: usize) {
    const FILLER: [u8; RING] = [b'x'; RING];
    let mut written = 0;
    while written < n {
        let chunk = pipe.try_write(&FILLER[written..n]).ok().unwrap_or(0);
        assert_ne!(
            chunk, 0,
            "ring stopped accepting filler at {written} of {n} bytes"
        );
        written += chunk;
    }
}

/// Everything currently in the ring, however it comes out.
fn drain_all(pipe: &Ring) -> Vec<u8> {
    let mut got = Vec::new();
    let mut buf = [0u8; 256];
    while let Ok(n) = pipe.try_read(&mut buf) {
        if n == 0 {
            break;
        }
        got.extend_from_slice(&buf[..n]);
    }
    got
}

/// A real framed record whose body is exactly `body_len` bytes, built by the encoder rather than
/// assembled by hand, so the bytes under test are the bytes the device ships.
fn frame_of_body(body_len: usize) -> ([u8; MAX_FRAME], usize) {
    let mut out = [0u8; MAX_FRAME];
    let body = vec![b'a'; body_len];
    let enc = encode(Level::Info, 0x0bad_1dea, 4242, &body, &mut out);
    assert_eq!(enc.len, PREFIX_LEN + body_len + TRAILER_LEN);
    assert!(
        !enc.truncated,
        "a body of {body_len} <= MAX_BODY cannot come back truncated"
    );
    (out, enc.len)
}

/// Deterministic RNG so the randomized rounds replay identically on every run.
///
/// Step constants copied from `src/frame.rs`'s `XorShift` (a Numerical Recipes LCG) so results are
/// comparable between the two suites.
struct Lcg(u32);

impl Lcg {
    fn new(seed: u32) -> Self {
        Self(seed)
    }
    fn next(&mut self) -> usize {
        self.0 = self.0.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (self.0 >> 16) as usize
    }
}

/// Every starting occupancy, crossed with the body lengths where a boundary could hide.
///
/// What CI can reach, stated plainly: the predicate and the ring. `try_emit_dump` itself is behind
/// `log-usb`, and on host that feature fails at *link* time for want of a `critical-section`
/// implementation — so nothing here fakes the device entry point. What this proves is the decision
/// the entry point makes, against a real `embassy_sync` ring at every capacity the device ring can
/// report, including across the array-end wrap where `try_write` short-writes.
#[test]
fn predicate_agrees_with_the_ring_at_every_occupancy() {
    // Cursor stops chosen to put a frame across the array end at many occupancies: 0 is the
    // pristine ring, 1 the cheapest possible offset, RING - 1 guarantees the next byte wraps.
    const CURSOR_STOPS: [usize; 3] = [0, 1, RING - 1];
    // Empty, one byte past empty, the full `AUDIO` chunk body, and `MAX_BODY` itself — the last two
    // straddle the point where a full frame plus the reserve exceeds the ring.
    const SWEPT_BODIES: [usize; 4] = [0, 1, dump::FULL_AUDIO_BODY_LEN, MAX_BODY];

    for &cursor_stop in &CURSOR_STOPS {
        for &body_len in &SWEPT_BODIES {
            for occupancy in 0..=RING {
                let pipe = ring();
                if cursor_stop > 0 {
                    park_cursors_at(&pipe, cursor_stop);
                }
                fill_ring(&pipe, occupancy);

                let free = pipe.free_capacity();
                assert_eq!(
                    free,
                    RING - occupancy,
                    "occupancy {occupancy} at cursor stop {cursor_stop} miscounted itself"
                );

                let (buf, len) = frame_of_body(body_len);
                let frame = &buf[..len];
                let admitted = dump::dump_fits(body_len, free);
                let accepted = commit_frame(&pipe, frame);

                if admitted {
                    assert!(
                        accepted,
                        "predicate admitted a {len}-byte frame at {free} free \
                         (body {body_len}, cursor stop {cursor_stop}) and the ring refused it"
                    );
                    assert!(
                        pipe.free_capacity() >= dump::RESERVE,
                        "an admitted commit left {} free, under the {}-byte reserve \
                         (body {body_len}, cursor stop {cursor_stop})",
                        pipe.free_capacity(),
                        dump::RESERVE,
                    );
                } else {
                    assert!(
                        !(len <= free && free - len >= dump::RESERVE),
                        "predicate refused a {len}-byte frame at {free} free that would have left \
                         the reserve intact (body {body_len}, cursor stop {cursor_stop})"
                    );
                }

                // Whatever landed must be whole and in order: the filler that was there first, then
                // the frame, byte for byte. A refusal must leave the filler alone.
                let mut expected = vec![b'x'; occupancy];
                if accepted {
                    expected.extend_from_slice(frame);
                }
                assert_eq!(
                    drain_all(&pipe),
                    expected,
                    "ring contents disagree with the accept/refuse verdict \
                     (body {body_len}, occupancy {occupancy}, cursor stop {cursor_stop}, \
                     admitted {admitted}, accepted {accepted})"
                );
            }
        }
    }
}

/// The post-condition as pure arithmetic, over every capacity the ring can report and every body
/// length the codec can frame. This is AC #5's claim stated as a sweep rather than as a type.
#[test]
fn a_dump_can_never_take_the_last_max_frame() {
    for free in 0..=RING {
        for body_len in 0..=MAX_BODY {
            let frame_len = PREFIX_LEN + body_len + TRAILER_LEN;

            if dump::dump_fits(body_len, free) {
                assert!(
                    free >= frame_len + dump::RESERVE && free - frame_len >= dump::RESERVE,
                    "admitted body {body_len} at {free} free: a {frame_len}-byte frame leaves {}",
                    free - frame_len,
                );
                // Monotone in capacity: a ring can only get emptier while a dumper waits, so a
                // retry never becomes *less* able. That is what lets the rig back off on a
                // timer instead of reasoning about orderings.
                if free < RING {
                    assert!(
                        dump::dump_fits(body_len, free + 1),
                        "refusal is not monotone: body {body_len} fits at {free} but not at {}",
                        free + 1,
                    );
                }
            } else {
                // Every refusal has an arithmetic reason. `saturating_sub` is there to keep the
                // function total below the reserve, not to hide a case worth finding later.
                assert!(
                    frame_len + dump::RESERVE > free,
                    "refused body {body_len} at {free} free although a {frame_len}-byte frame \
                     plus the reserve fit",
                );
            }
        }
    }

    // Inside the reserve there is nothing to give, so even the smallest frame the codec can build
    // waits. The first capacity that admits it is exactly `RESERVE` plus that frame.
    assert!(
        !dump::dump_fits(0, dump::RESERVE),
        "the reserve must survive intact even for a zero-length body"
    );
    assert!(dump::dump_fits(0, dump::RESERVE + PREFIX_LEN + TRAILER_LEN));
}

/// The claim the rig's bench criterion rests on, expressed as something CI can check: with a
/// draining consumer, a maximum-size log record is never refused while at least [`MAX_FRAME`] bytes
/// were free — so a drop counter read during a dump cannot be blamed on the dump.
#[test]
fn log_records_survive_a_saturated_dump() {
    const ROUNDS: usize = 20_000;
    /// Bytes one drain-task wakeup takes out of the ring — `src/usb.rs`'s `DRAIN_BUF_SIZE`, so the
    /// consumer here drains like the one on the device (several 64-byte endpoint packets, not one).
    const DRAIN_BUF_SIZE: usize = 256;

    // The largest record the codec can build: this is what the reservation pays for.
    let (log_buf, log_len) = frame_of_body(MAX_BODY);
    assert_eq!(
        log_len, MAX_FRAME,
        "the log record under test must be a maximum-size one"
    );
    let log_frame = &log_buf[..log_len];

    // One full `AUDIO` chunk, framed by the same builder the device will call.
    let raw: Vec<u8> = (0..dump::CHUNK_RAW).map(|i| (i % 251) as u8).collect();
    let mut body = [0u8; MAX_BODY];
    let mut frame_buf = [0u8; MAX_FRAME];
    let enc = dump::audio_record(Level::Info, 0, 0, 0, 1, 0, &raw, &mut body, &mut frame_buf)
        .expect("a full chunk is a legal record");
    assert_eq!(enc.len, dump::FULL_AUDIO_FRAME_LEN);
    let dump_frame = frame_buf[..enc.len].to_vec();
    let dump_body_len = dump::FULL_AUDIO_BODY_LEN;

    let pipe = ring();
    let mut rng = Lcg::new(0x1234_5678);
    let mut expected: Vec<u8> = Vec::new();
    let mut got: Vec<u8> = Vec::new();
    let mut buf = [0u8; DRAIN_BUF_SIZE];
    let (mut log_commits, mut log_refusals) = (0usize, 0usize);
    let (mut dump_commits, mut dump_refusals) = (0usize, 0usize);

    for round in 0..ROUNDS {
        // Alternate which producer gets the earlier slot, so neither one is structurally favoured
        // by the loop's own order.
        let order = [true, false];
        let turns = if rng.next().is_multiple_of(2) {
            order
        } else {
            [false, true]
        };

        for is_dump_turn in turns {
            let free_before = pipe.free_capacity();
            if is_dump_turn {
                if dump::dump_fits(dump_body_len, free_before) {
                    assert!(
                        commit_frame(&pipe, &dump_frame),
                        "round {round}: the ring refused a dump frame the predicate admitted at {free_before} free"
                    );
                    assert!(
                        pipe.free_capacity() >= dump::RESERVE,
                        "round {round}: a committed dump left {} free, under the reserve",
                        pipe.free_capacity(),
                    );
                    expected.extend_from_slice(&dump_frame);
                    dump_commits += 1;
                } else {
                    dump_refusals += 1;
                }
            } else if commit_frame(&pipe, log_frame) {
                expected.extend_from_slice(log_frame);
                log_commits += 1;
            } else {
                log_refusals += 1;
                assert!(
                    free_before < MAX_FRAME,
                    "round {round}: a maximum-size log record was refused with {free_before} free \
                     — the dump took bytes reserved for it"
                );
            }
        }

        // Consumer: drain a random amount up to one packet, like the USB task does.
        let want = 1 + rng.next() % DRAIN_BUF_SIZE;
        if let Ok(n) = pipe.try_read(&mut buf[..want]) {
            got.extend_from_slice(&buf[..n]);
        }
    }

    while let Ok(n) = pipe.try_read(&mut buf) {
        if n == 0 {
            break;
        }
        got.extend_from_slice(&buf[..n]);
    }

    assert_eq!(
        expected, got,
        "stream is not the exact concatenation of what was committed \
         (log commits={log_commits} refusals={log_refusals}, \
         dump commits={dump_commits} refusals={dump_refusals})"
    );
    assert!(
        log_commits > 0 && dump_commits > 0 && dump_refusals > 0,
        "test exercised fewer than all three interesting paths: \
         log commits={log_commits} dump commits={dump_commits} dump refusals={dump_refusals}"
    );
}

/// The reserve must not be a deadlock dressed as a safety property: the moment the drainer catches
/// up, a dump is admitted again — even with the cursors parked where the next byte wraps.
#[test]
fn empty_ring_always_admits_a_dump() {
    assert!(
        dump::dump_fits(dump::FULL_AUDIO_BODY_LEN, RING),
        "an empty ring must admit a full dump chunk, or a dump would starve forever"
    );
    assert!(
        dump::dump_fits(MAX_BODY, RING),
        "an empty ring must admit even a maximum-body record"
    );

    for &cursor_stop in &[0usize, 1, RING - 1] {
        let pipe = ring();
        if cursor_stop > 0 {
            park_cursors_at(&pipe, cursor_stop);
        }

        let (buf, len) = frame_of_body(dump::FULL_AUDIO_BODY_LEN);
        let frame = &buf[..len];
        assert!(
            commit_frame(&pipe, frame),
            "empty ring at cursor stop {cursor_stop} refused a frame the predicate admitted"
        );
        assert!(
            pipe.free_capacity() >= dump::RESERVE,
            "cursor stop {cursor_stop}: first commit ate into the reserve"
        );

        // Draining frees the ring, and the very next dump goes in: progress is never blocked by the
        // reservation once the consumer has caught up.
        drain_all(&pipe);
        assert!(
            dump::dump_fits(dump::FULL_AUDIO_BODY_LEN, pipe.free_capacity()),
            "cursor stop {cursor_stop}: ring stayed unable to take a dump after full drain"
        );
        assert!(
            commit_frame(&pipe, frame),
            "cursor stop {cursor_stop}: second commit into a drained ring was refused"
        );
    }
}
