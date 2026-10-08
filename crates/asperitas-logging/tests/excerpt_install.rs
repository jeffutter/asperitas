//! Host suite for excerpt install: the `install_frames` stream, the `Installer` state machine, the
//! `EXCOK`/`EXCFAIL` bodies, and the pinned golden stream `examples/excerpt_stream.rs` emits.
//!
//! Run with: `cargo test -p asperitas-logging --test excerpt_install`
//! Regenerate goldens with: `UPDATE_GOLDENS=1 cargo test -p asperitas-logging --test excerpt_install`
//!
//! A golden diff means the wire changed. Regenerate only when that change is intended, because
//! a device flashed before the change will refuse the new stream.

use std::path::PathBuf;

use asperitas_logging::excerpt::{
    exc_data_body, exc_end_body, exc_fail_body, exc_ok_body, exc_start_body, install_frames,
    parse_record, parse_wav, slot_base, slot_pcm_base, Action, ExcRecord, Failure, Installer,
    RecordError, SlotHeader, Tag, Verdict, Why, EXC_CHUNK_RAW, MAX_FAIL_BODY_LEN, MAX_OK_BODY_LEN,
    PCM_CAPACITY, SECTOR,
};
use asperitas_logging::frame::{crc16_ccitt, Decoder, MAX_BODY};
use proptest::prelude::*;

const SLOT: u8 = 3;

fn tag(name: &str) -> Tag {
    Tag::new(name.as_bytes()).expect("valid tag")
}

/// Deterministic PCM: a 16-bit ramp, little-endian, `samples` long.
fn ramp(samples: usize) -> Vec<u8> {
    (0..samples)
        .flat_map(|i| ((i as i32 * 37 - 18_000) as i16).to_le_bytes())
        .collect()
}

/// A canonical 44-byte-header PCM mono 48 kHz 16-bit WAV around `pcm`.
fn wav(pcm: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&48_000u32.to_le_bytes());
    out.extend_from_slice(&96_000u32.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

fn start(bytes: u32, crc16: u16) -> ExcRecord {
    ExcRecord::Start {
        slot: SLOT,
        name: tag("clip"),
        bytes,
        crc16,
    }
}

fn data(index: u16, raw: &[u8]) -> ExcRecord {
    let mut body = [0u8; MAX_BODY];
    let len = exc_data_body(index, raw, &mut body).expect("valid chunk");
    parse_record(&body[..len]).expect("round trip")
}

/// What the flash would hold after an install, plus the verdict. Drives the installer the way the
/// device must: every action honoured, the readback taken from what was actually written.
struct Flash {
    /// Sector contents by slot-relative sector index; index 0 is the header sector.
    sectors: std::collections::BTreeMap<u16, Vec<u8>>,
    verify: Option<(u32, u32)>,
    rejects: Vec<Failure>,
    invalidated: u32,
}

impl Flash {
    fn new() -> Self {
        Self {
            sectors: Default::default(),
            verify: None,
            rejects: Vec::new(),
            invalidated: 0,
        }
    }

    fn apply(&mut self, action: Action<'_>) {
        match action {
            Action::InvalidateHeader { slot, address, .. } => {
                assert_eq!(Some(address), slot_base(slot));
                self.sectors.insert(0, vec![0xFF; SECTOR as usize]);
                self.invalidated += 1;
            }
            Action::WriteSector {
                sector,
                address,
                data,
            } => {
                assert!(sector >= 1, "PCM never lands in the header sector");
                assert_eq!(
                    address,
                    slot_base(SLOT).unwrap() + u32::from(sector) * SECTOR
                );
                assert!(
                    self.sectors.insert(sector, data.to_vec()).is_none(),
                    "sector {sector} written twice"
                );
            }
            Action::Verify {
                slot,
                address,
                bytes,
            } => {
                assert_eq!(Some(address), slot_pcm_base(slot));
                self.verify = Some((address, bytes));
            }
            Action::Reject(failure) => self.rejects.push(failure),
            Action::Accepted | Action::Ignored => {}
        }
    }

    /// The PCM area as flash would read it back, `bytes` long.
    fn readback(&self, bytes: u32) -> Vec<u8> {
        let mut out = Vec::new();
        for sector in 1..=bytes.div_ceil(SECTOR) as u16 {
            match self.sectors.get(&sector) {
                Some(data) => out.extend_from_slice(data),
                None => out.extend_from_slice(&[0xFF; SECTOR as usize]),
            }
        }
        out.truncate(bytes as usize);
        out
    }
}

/// Feed `records` and, if the installer asks to verify, read back and take the verdict.
fn run(records: &[ExcRecord]) -> (Flash, Option<Verdict>, Installer) {
    let mut installer = Installer::new();
    let mut flash = Flash::new();
    for record in records {
        let action = installer.feed(*record);
        flash.apply(action);
    }
    let verdict = flash.verify.map(|(_, bytes)| {
        let back = flash.readback(bytes);
        installer.verdict(crc16_ccitt(&back), back.len() as u32)
    });
    (flash, verdict, installer)
}

fn records_for(pcm: &[u8]) -> Vec<ExcRecord> {
    let mut out = vec![start(pcm.len() as u32, crc16_ccitt(pcm))];
    for (i, chunk) in pcm.chunks(EXC_CHUNK_RAW).enumerate() {
        out.push(data(i as u16, chunk));
    }
    out.push(ExcRecord::End);
    out
}

fn assert_ok(verdict: Option<Verdict>, pcm: &[u8]) {
    match verdict {
        Some(Verdict::Ok {
            slot,
            address,
            header,
            encoded,
        }) => {
            assert_eq!(slot, SLOT);
            assert_eq!(Some(address), slot_base(SLOT));
            assert_eq!(header.pcm_bytes(), pcm.len() as u32);
            assert_eq!(header.pcm_crc16(), crc16_ccitt(pcm));
            assert_eq!(SlotHeader::decode(&encoded), Ok(header));
        }
        other => panic!("expected Ok, got {other:?}"),
    }
}

fn single_reject(flash: &Flash) -> Why {
    assert_eq!(
        flash.rejects.len(),
        1,
        "one rejection per install: {:?}",
        flash.rejects
    );
    flash.rejects[0].why
}

// ---------------------------------------------------------------------------
// Happy path, including the sector-boundary lengths
// ---------------------------------------------------------------------------

#[test]
fn installs_round_trip_at_every_boundary_length() {
    for samples in [
        1usize,
        63,
        64,
        65,
        2047,
        2048,
        2049,
        4096,
        10_000,
        PCM_CAPACITY as usize / 2,
    ] {
        let pcm = ramp(samples);
        let (flash, verdict, installer) = run(&records_for(&pcm));
        assert!(flash.rejects.is_empty(), "{samples}: {:?}", flash.rejects);
        assert_eq!(flash.invalidated, 1);
        assert_eq!(flash.readback(pcm.len() as u32), pcm, "{samples} samples");
        assert_eq!(
            flash.sectors.len() - 1,
            pcm.len().div_ceil(SECTOR as usize),
            "{samples}: one write per sector"
        );
        assert_ok(verdict, &pcm);
        assert!(!installer.is_open());
    }
}

#[test]
fn final_partial_sector_is_padded_with_erased_bytes() {
    let pcm = ramp(2049); // 4098 bytes: one full sector and two bytes
    let (flash, _, _) = run(&records_for(&pcm));
    let last = &flash.sectors[&2];
    assert_eq!(&last[..2], &pcm[4096..]);
    assert!(last[2..].iter().all(|b| *b == 0xFF));
}

#[test]
fn write_sector_is_returned_exactly_when_a_sector_fills() {
    let pcm = ramp(4096); // 8192 bytes, 64 chunks
    let mut installer = Installer::new();
    assert!(matches!(
        installer.feed(start(pcm.len() as u32, crc16_ccitt(&pcm))),
        Action::InvalidateHeader {
            slot: SLOT,
            aborted: None,
            ..
        }
    ));
    for (i, chunk) in pcm.chunks(EXC_CHUNK_RAW).enumerate() {
        let action = installer.feed(data(i as u16, chunk));
        if (i + 1) % 32 == 0 {
            match action {
                Action::WriteSector { sector, data, .. } => {
                    assert_eq!(usize::from(sector), (i + 1) / 32);
                    let from = (i + 1 - 32) * EXC_CHUNK_RAW;
                    assert_eq!(&data[..], &pcm[from..from + 4096]);
                }
                other => panic!("chunk {i}: expected WriteSector, got {other:?}"),
            }
        } else {
            assert_eq!(action, Action::Accepted, "chunk {i}");
        }
    }
    assert_eq!(
        installer.feed(ExcRecord::End),
        Action::Verify {
            slot: SLOT,
            address: slot_pcm_base(SLOT).unwrap(),
            bytes: 8192
        }
    );
}

#[test]
fn full_capacity_install_fills_the_slot_exactly() {
    let pcm: Vec<u8> = (0..PCM_CAPACITY).map(|i| (i * 7 + i / 251) as u8).collect();
    let (flash, verdict, _) = run(&records_for(&pcm));
    assert!(flash.rejects.is_empty());
    assert_eq!(flash.sectors.len() - 1, 126);
    assert_eq!(flash.readback(PCM_CAPACITY), pcm);
    assert_ok(verdict, &pcm);
}

// ---------------------------------------------------------------------------
// Verdict
// ---------------------------------------------------------------------------

fn ready_to_verify(pcm: &[u8]) -> Installer {
    let mut installer = Installer::new();
    for record in records_for(pcm) {
        let _ = installer.feed(record);
    }
    installer
}

#[test]
fn verdict_crc_mismatch_fails_without_a_why_tag() {
    let pcm = ramp(100);
    let want = crc16_ccitt(&pcm);
    let mut installer = ready_to_verify(&pcm);
    let verdict = installer.verdict(want ^ 1, 200);
    assert_eq!(
        verdict,
        Verdict::Fail(Failure {
            got: want ^ 1,
            want,
            why: Why::Crc
        })
    );
    assert!(!installer.is_open());
}

#[test]
fn verdict_length_mismatch_fails_even_when_the_crc_matches() {
    let pcm = ramp(100);
    let want = crc16_ccitt(&pcm);
    let mut installer = ready_to_verify(&pcm);
    assert!(matches!(
        installer.verdict(want, 198),
        Verdict::Fail(Failure {
            why: Why::Length,
            ..
        })
    ));
}

#[test]
fn verdict_is_only_ok_once() {
    let pcm = ramp(100);
    let want = crc16_ccitt(&pcm);
    let mut installer = ready_to_verify(&pcm);
    assert!(matches!(installer.verdict(want, 200), Verdict::Ok { .. }));
    assert!(matches!(
        installer.verdict(want, 200),
        Verdict::Fail(Failure {
            why: Why::State,
            ..
        })
    ));
}

#[test]
fn verdict_without_an_install_or_mid_install_fails() {
    let mut idle = Installer::new();
    assert!(matches!(
        idle.verdict(0, 0),
        Verdict::Fail(Failure {
            why: Why::State,
            want: 0,
            ..
        })
    ));

    let pcm = ramp(100);
    let mut mid = Installer::new();
    let _ = mid.feed(start(200, crc16_ccitt(&pcm)));
    let _ = mid.feed(data(0, &pcm[..128]));
    assert!(matches!(
        mid.verdict(crc16_ccitt(&pcm), 200),
        Verdict::Fail(Failure {
            why: Why::State,
            ..
        })
    ));
    // ...and the install is dead: its tail cannot revive it.
    assert_eq!(mid.feed(data(1, &pcm[128..])), Action::Ignored);
    assert_eq!(mid.feed(ExcRecord::End), Action::Ignored);
    assert!(matches!(
        mid.verdict(crc16_ccitt(&pcm), 200),
        Verdict::Fail(_)
    ));
}

// ---------------------------------------------------------------------------
// Strictness: every failure, and none of them reaches Ok
// ---------------------------------------------------------------------------

/// Run `records`, assert exactly one rejection with `expect`, and that no verdict was offered.
fn assert_rejected(records: &[ExcRecord], expect: fn(&Why) -> bool) {
    let (flash, verdict, mut installer) = run(records);
    let why = single_reject(&flash);
    assert!(expect(&why), "unexpected why: {why:?}");
    assert!(
        verdict.is_none(),
        "a rejected install was offered a verdict"
    );
    // Whatever the caller does next, the failed install cannot verify.
    assert!(matches!(installer.verdict(0, 0), Verdict::Fail(_)));
    for crc in [0u16, 0xFFFF] {
        assert!(!matches!(installer.verdict(crc, 200), Verdict::Ok { .. }));
    }
}

#[test]
fn gap_duplicate_and_reorder_are_refused() {
    let pcm = ramp(320); // 640 bytes, 5 chunks
    let all = records_for(&pcm);
    let (s, d, e) = (all[0], &all[1..6], ExcRecord::End);

    // Gap: chunk 1 missing.
    assert_rejected(&[s, d[0], d[2], d[3], d[4], e], |w| {
        *w == Why::Order {
            expected: 1,
            got: 2,
        }
    });
    // Duplicate.
    assert_rejected(&[s, d[0], d[1], d[1], d[2], d[3], d[4], e], |w| {
        *w == Why::Order {
            expected: 2,
            got: 1,
        }
    });
    // Reorder.
    assert_rejected(&[s, d[1], d[0], d[2], d[3], d[4], e], |w| {
        *w == Why::Order {
            expected: 0,
            got: 1,
        }
    });
}

#[test]
fn short_and_odd_chunks_are_refused() {
    let pcm = ramp(320);
    let crc = crc16_ccitt(&pcm);
    // A short chunk that does not end the excerpt.
    assert_rejected(
        &[start(640, crc), data(0, &pcm[..100]), ExcRecord::End],
        |w| matches!(w, Why::Short { index: 0, len: 100 }),
    );
    // An odd-length chunk: can never land exactly on an even declared length.
    assert_rejected(
        &[start(640, crc), data(0, &pcm[..127]), ExcRecord::End],
        |w| matches!(w, Why::Short { index: 0, len: 127 }),
    );
}

#[test]
fn bytes_past_the_declared_length_are_refused() {
    let pcm = ramp(320);
    // Declared 600 bytes, streamed 640: the fifth chunk overruns.
    let mut records = records_for(&pcm);
    records[0] = start(600, crc16_ccitt(&pcm[..600]));
    assert_rejected(&records, |w| matches!(w, Why::Overrun { index: 4 }));
    // Declared 130: the full first chunk is fine, a second full one overruns.
    assert_rejected(
        &[start(130, 0), data(0, &pcm[..128]), data(1, &pcm[128..256])],
        |w| matches!(w, Why::Overrun { index: 1 }),
    );
}

#[test]
fn over_capacity_and_odd_declared_lengths_never_parse() {
    let name = tag("clip");
    let mut body = [0u8; MAX_BODY];
    assert!(exc_start_body(SLOT, &name, PCM_CAPACITY + 2, 0, &mut body).is_err());
    assert!(exc_start_body(SLOT, &name, 641, 0, &mut body).is_err());

    // Hand-forge the bodies the builder refuses, and feed them as the device would.
    for forged in [
        format!(
            "EXCSTART slot=03 name=clip bytes={:06} crc16=0000",
            PCM_CAPACITY + 2
        ),
        "EXCSTART slot=03 name=clip bytes=000641 crc16=0000".to_string(),
    ] {
        let mut installer = Installer::new();
        match installer.feed_body(forged.as_bytes()) {
            Action::Reject(Failure {
                why: Why::Malformed(RecordError::Length(_)),
                ..
            }) => {}
            other => panic!("{forged}: {other:?}"),
        }
        assert!(!installer.is_open());
        // Data for the refused install is ignored, not written.
        assert_eq!(installer.feed(data(0, &[0; 128])), Action::Ignored);
    }
}

#[test]
fn excend_before_every_byte_is_refused() {
    let pcm = ramp(320);
    let crc = crc16_ccitt(&pcm);
    let all = records_for(&pcm);
    assert_rejected(&[all[0], all[1], all[2], ExcRecord::End], |w| {
        *w == Why::Early {
            got: 256,
            declared: 640,
        }
    });
    assert_rejected(&[start(640, crc), ExcRecord::End], |w| {
        *w == Why::Early {
            got: 0,
            declared: 640,
        }
    });
}

#[test]
fn data_or_end_without_excstart_is_refused_once() {
    let pcm = ramp(320);
    let all = records_for(&pcm);
    // Data with no start: one rejection, then the rest of the dead stream is ignored.
    let (flash, verdict, _) = run(&all[1..]);
    assert_eq!(single_reject(&flash), Why::NoStart);
    assert!(verdict.is_none());
    assert!(
        flash.sectors.is_empty(),
        "nothing written for an unopened install"
    );

    let (flash, verdict, _) = run(&[ExcRecord::End]);
    assert_eq!(single_reject(&flash), Why::NoStart);
    assert!(verdict.is_none());
}

#[test]
fn a_stream_whose_crc_disagrees_with_the_declaration_is_refused() {
    let pcm = ramp(320);
    let mut records = records_for(&pcm);
    records[0] = start(640, crc16_ccitt(&pcm) ^ 0x8000);
    assert_rejected(&records, |w| *w == Why::Stream);
}

#[test]
fn a_new_excstart_mid_install_aborts_the_old_one() {
    let old = ramp(320);
    let new = ramp(64);
    let mut installer = Installer::new();
    let _ = installer.feed(start(640, crc16_ccitt(&old)));
    let _ = installer.feed(data(0, &old[..128]));
    let _ = installer.feed(data(1, &old[128..256]));

    let restart = ExcRecord::Start {
        slot: 5,
        name: tag("other"),
        bytes: new.len() as u32,
        crc16: crc16_ccitt(&new),
    };
    assert_eq!(
        installer.feed(restart),
        Action::InvalidateHeader {
            slot: 5,
            address: slot_base(5).unwrap(),
            aborted: Some(SLOT),
        }
    );
    // The old install's next chunk is now out of order for the new one.
    assert!(matches!(
        installer.feed(data(2, &old[256..384])),
        Action::Reject(Failure {
            why: Why::Order {
                expected: 0,
                got: 2
            },
            ..
        })
    ));

    // A restart that replaces the old stream cleanly installs the new excerpt, and only it.
    let mut installer = Installer::new();
    let _ = installer.feed(start(640, crc16_ccitt(&old)));
    let _ = installer.feed(data(0, &old[..128]));
    let _ = installer.feed(restart);
    let action = installer.feed(data(0, &new));
    match action {
        Action::WriteSector {
            sector: 1, data, ..
        } => {
            assert_eq!(&data[..128], &new[..]);
            assert!(data[128..].iter().all(|b| *b == 0xFF), "old bytes leaked");
        }
        other => panic!("{other:?}"),
    }
    let _ = installer.feed(ExcRecord::End);
    assert!(matches!(
        installer.verdict(crc16_ccitt(&new), 128),
        Verdict::Ok { slot: 5, .. }
    ));
}

#[test]
fn excstart_after_a_failure_or_while_verifying_starts_fresh() {
    let pcm = ramp(64);
    let crc = crc16_ccitt(&pcm);
    let mut installer = Installer::new();
    let _ = installer.feed(data(3, &pcm)); // fails: no start
    assert!(matches!(
        installer.feed(start(128, crc)),
        Action::InvalidateHeader { aborted: None, .. }
    ));
    let _ = installer.feed(data(0, &pcm));
    assert!(matches!(
        installer.feed(ExcRecord::End),
        Action::Verify { .. }
    ));
    // Verifying, then a new start: the unverified install is abandoned.
    assert!(matches!(
        installer.feed(start(128, crc)),
        Action::InvalidateHeader {
            aborted: Some(SLOT),
            ..
        }
    ));
    assert!(matches!(
        installer.verdict(crc, 128),
        Verdict::Fail(Failure {
            why: Why::State,
            ..
        })
    ));
}

#[test]
fn records_after_excend_fail_the_unverified_install() {
    let pcm = ramp(64);
    let mut installer = ready_to_verify(&pcm);
    assert!(matches!(
        installer.feed(ExcRecord::End),
        Action::Reject(Failure {
            why: Why::NoStart,
            ..
        })
    ));
    assert!(matches!(
        installer.verdict(crc16_ccitt(&pcm), 128),
        Verdict::Fail(_)
    ));
}

#[test]
fn failure_is_sticky_until_the_next_excstart() {
    let pcm = ramp(320);
    let all = records_for(&pcm);
    let mut installer = Installer::new();
    let _ = installer.feed(all[0]);
    let _ = installer.feed(all[2]); // gap: reject
    for record in &all[3..6] {
        assert_eq!(installer.feed(*record), Action::Ignored);
    }
    assert_eq!(installer.feed(ExcRecord::End), Action::Ignored);
    // Back to idle: a stray EXCEND now is a fresh protocol error.
    assert!(matches!(
        installer.feed(ExcRecord::End),
        Action::Reject(Failure {
            why: Why::NoStart,
            ..
        })
    ));
}

#[test]
fn feed_body_ignores_foreign_records_and_fails_on_malformed_excerpt_ones() {
    let pcm = ramp(64);
    let mut installer = Installer::new();
    let _ = installer.feed(start(128, crc16_ccitt(&pcm)));
    assert_eq!(installer.feed_body(b"STATUS up=1"), Action::Ignored);
    assert!(
        installer.is_open(),
        "a foreign record must not disturb the install"
    );
    match installer.feed_body(b"EXCDATA i=0000 b64=not base64!") {
        Action::Reject(Failure {
            why: Why::Malformed(_),
            want,
            ..
        }) => assert_eq!(want, crc16_ccitt(&pcm)),
        other => panic!("{other:?}"),
    }
    assert!(!installer.is_open());
}

// ---------------------------------------------------------------------------
// Verdict bodies
// ---------------------------------------------------------------------------

#[test]
fn exc_ok_body_renders_the_pinned_format() {
    let header = SlotHeader::new(tag("guitar_a"), 288_000, 0xBEEF).unwrap();
    let mut out = [0u8; MAX_BODY];
    let len = exc_ok_body(&header, &mut out);
    assert_eq!(&out[..len], b"EXCOK name=guitar_a bytes=288000 crc16=beef");
    let longest = SlotHeader::new(tag("abcdefghijklmnop"), 516_096, 0).unwrap();
    assert_eq!(exc_ok_body(&longest, &mut out), MAX_OK_BODY_LEN);
}

#[test]
fn exc_fail_body_renders_got_want_and_why() {
    let mut out = [0u8; MAX_BODY];
    let cases: &[(Why, &[u8])] = &[
        (Why::Crc, b"EXCFAIL got=1234 want=abcd"),
        (Why::Length, b"EXCFAIL got=1234 want=abcd why=length"),
        (Why::Stream, b"EXCFAIL got=1234 want=abcd why=stream"),
        (Why::NoStart, b"EXCFAIL got=1234 want=abcd why=nostart"),
        (
            Why::Order {
                expected: 1,
                got: 2,
            },
            b"EXCFAIL got=1234 want=abcd why=order",
        ),
        (
            Why::Short { index: 0, len: 1 },
            b"EXCFAIL got=1234 want=abcd why=short",
        ),
        (
            Why::Overrun { index: 0 },
            b"EXCFAIL got=1234 want=abcd why=overrun",
        ),
        (
            Why::Early {
                got: 0,
                declared: 2,
            },
            b"EXCFAIL got=1234 want=abcd why=early",
        ),
        (
            Why::Malformed(RecordError::NotExcerpt),
            b"EXCFAIL got=1234 want=abcd why=malformed",
        ),
        (Why::State, b"EXCFAIL got=1234 want=abcd why=state"),
    ];
    let mut longest = 0;
    for (why, expect) in cases {
        let len = exc_fail_body(
            &Failure {
                got: 0x1234,
                want: 0xABCD,
                why: *why,
            },
            &mut out,
        );
        assert_eq!(&out[..len], *expect, "{why:?}");
        longest = longest.max(len);
    }
    assert_eq!(longest, MAX_FAIL_BODY_LEN);
}

// ---------------------------------------------------------------------------
// install_frames and the pinned golden stream
// ---------------------------------------------------------------------------

/// Decode a byte stream into `(seq, body)` pairs, pushing it in the given piece sizes.
fn decode(stream: &[u8], pieces: &[usize]) -> Vec<(u32, Vec<u8>)> {
    let mut decoder = Decoder::new();
    let mut out = Vec::new();
    let mut at = 0;
    let mut piece = pieces.iter().cycle();
    while at < stream.len() {
        let end = (at + *piece.next().unwrap_or(&stream.len()).max(&1)).min(stream.len());
        let mut off = at;
        while off < end {
            off += decoder.push(&stream[off..end]);
            while let Some(record) = decoder.next_record() {
                out.push((record.seq, record.body.to_vec()));
            }
        }
        at = end;
    }
    while let Some(record) = decoder.next_record() {
        out.push((record.seq, record.body.to_vec()));
    }
    decoder.finish();
    let stats = decoder.stats();
    assert_eq!(stats.bad_frames, 0, "install stream must decode cleanly");
    out
}

fn stream(slot: u8, name: &Tag, pcm: &[u8], seq0: u32) -> (Vec<u8>, u32, usize) {
    let mut out = Vec::new();
    let mut calls = 0;
    let next = install_frames(slot, name, pcm, seq0, 0, |frame| {
        out.extend_from_slice(frame);
        calls += 1;
    })
    .expect("valid install");
    (out, next, calls)
}

#[test]
fn install_frames_emits_start_data_end_with_consecutive_seq() {
    let pcm = ramp(1000);
    let (bytes, next, calls) = stream(SLOT, &tag("ramp1000"), &pcm, 0x10);
    let records = decode(&bytes, &[bytes.len()]);
    let chunks = pcm.len().div_ceil(EXC_CHUNK_RAW);
    assert_eq!(calls, chunks + 2, "one sink call per frame");
    assert_eq!(records.len(), chunks + 2);
    assert_eq!(next, 0x10 + chunks as u32 + 2);
    for (k, (seq, _)) in records.iter().enumerate() {
        assert_eq!(*seq, 0x10 + k as u32);
    }
    assert_eq!(
        parse_record(&records[0].1),
        Ok(ExcRecord::Start {
            slot: SLOT,
            name: tag("ramp1000"),
            bytes: 2000,
            crc16: crc16_ccitt(&pcm)
        })
    );
    assert_eq!(parse_record(&records.last().unwrap().1), Ok(ExcRecord::End));
}

#[test]
fn install_frames_refuses_bad_arguments_before_emitting_anything() {
    let mut calls = 0;
    for (slot, pcm) in [
        (SLOT, vec![]),
        (SLOT, vec![0u8; 3]),
        (SLOT, vec![0u8; PCM_CAPACITY as usize + 2]),
        (14, vec![0u8; 2]),
    ] {
        assert!(install_frames(slot, &tag("x"), &pcm, 0, 0, |_| calls += 1).is_err());
    }
    assert_eq!(calls, 0);
}

const GOLDEN_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden");
const REGENERATE: &str = "UPDATE_GOLDENS=1 cargo test -p asperitas-logging --test excerpt_install";

/// Compare `actual` with the golden file `name`, or overwrite it when `UPDATE_GOLDENS` is set.
fn check_golden(name: &str, actual: &[u8]) {
    let path = PathBuf::from(GOLDEN_DIR).join(name);
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(GOLDEN_DIR).expect("create golden dir");
        std::fs::write(&path, actual).expect("write golden");
        return;
    }
    let expected = std::fs::read(&path).unwrap_or_else(|err| {
        panic!(
            "cannot read golden {}: {err}. Run `{REGENERATE}` to generate it.",
            path.display()
        )
    });
    if expected != actual {
        let at = expected
            .iter()
            .zip(actual)
            .position(|(a, b)| a != b)
            .unwrap_or(expected.len().min(actual.len()));
        panic!(
            "{name} drifted from its golden (first difference at byte {at}, {} vs {} bytes). \
             The install wire format changed; if that is intended, run `{REGENERATE}` and \
             re-flash devices that must read the new stream.",
            expected.len(),
            actual.len()
        );
    }
}

/// The exact input `examples/excerpt_stream.rs` is pinned against: a 1000-sample ramp WAV,
/// slot 3, name `ramp1000`, seq0 0, t_ms 0.
#[test]
fn excerpt_stream_matches_pinned_golden_frames() {
    let file = wav(&ramp(1000));
    check_golden("excerpt_ramp.wav", &file);
    // The example's path: parse the WAV, then install_frames to the sink.
    let pcm = parse_wav(&file).expect("synthetic WAV parses");
    let (bytes, _, _) = stream(SLOT, &tag("ramp1000"), pcm, 0);
    check_golden("excerpt_stream.bin", &bytes);
}

/// The golden stream, decoded and run through the installer, yields the source PCM.
#[test]
fn golden_stream_installs_the_source() {
    let pcm = ramp(1000);
    let golden = std::fs::read(PathBuf::from(GOLDEN_DIR).join("excerpt_stream.bin"))
        .unwrap_or_else(|err| panic!("golden missing ({err}); run `{REGENERATE}`"));
    let mut installer = Installer::new();
    let mut flash = Flash::new();
    for (_, body) in decode(&golden, &[61]) {
        flash.apply(installer.feed_body(&body));
    }
    assert!(flash.rejects.is_empty());
    let back = flash.readback(2000);
    assert_eq!(back, pcm);
    assert_ok(Some(installer.verdict(crc16_ccitt(&back), 2000)), &pcm);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Random PCM through install_frames, decoded in random chunkings, installed, read back: the
    /// sectors reassemble to the source and the verdict is Ok.
    #[test]
    fn random_pcm_survives_frames_decoder_and_installer(
        samples in prop::collection::vec(any::<i16>(), 1..12_000),
        pieces in prop::collection::vec(1usize..600, 1..16),
        seq0 in any::<u32>(),
    ) {
        let pcm: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut wire = Vec::new();
        install_frames(SLOT, &tag("prop"), &pcm, seq0, 0, |f| wire.extend_from_slice(f)).unwrap();

        let mut installer = Installer::new();
        let mut flash = Flash::new();
        for (_, body) in decode(&wire, &pieces) {
            flash.apply(installer.feed_body(&body));
        }
        prop_assert!(flash.rejects.is_empty());
        let (_, bytes) = flash.verify.expect("verify requested");
        prop_assert_eq!(bytes as usize, pcm.len());
        let back = flash.readback(bytes);
        prop_assert_eq!(&back, &pcm);
        let verdict = installer.verdict(crc16_ccitt(&back), bytes);
        prop_assert!(matches!(verdict, Verdict::Ok { .. }), "{:?}", verdict);
    }

    /// Dropping any one frame from a valid stream never yields Ok.
    #[test]
    fn dropping_any_frame_never_verifies(
        samples in 1usize..1500,
        drop_seed in any::<usize>(),
    ) {
        let pcm = ramp(samples);
        let mut frames = Vec::new();
        install_frames(SLOT, &tag("prop"), &pcm, 0, 0, |f| frames.push(f.to_vec())).unwrap();
        let drop = drop_seed % frames.len();
        frames.remove(drop);

        let mut installer = Installer::new();
        let mut flash = Flash::new();
        for frame in &frames {
            for (_, body) in decode(frame, &[frame.len()]) {
                flash.apply(installer.feed_body(&body));
            }
        }
        let verdict = flash.verify.map(|(_, bytes)| {
            let back = flash.readback(bytes);
            installer.verdict(crc16_ccitt(&back), bytes)
        });
        prop_assert!(!matches!(verdict, Some(Verdict::Ok { .. })), "dropped frame {} verified", drop);
        // Dropping EXCEND leaves the install open with nothing to say; anything else is refused.
        if drop != frames.len() {
            prop_assert_eq!(flash.rejects.len(), 1);
        }
    }
}

#[test]
fn exc_end_body_is_what_install_frames_ends_with() {
    let mut body = [0u8; MAX_BODY];
    let len = exc_end_body(&mut body);
    assert_eq!(&body[..len], b"EXCEND");
}
