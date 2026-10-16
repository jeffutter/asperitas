//! The host side of excerpt install: `install_frames` driven through the real frame decoder into
//! the device `Installer`, and the pinned golden stream `examples/excerpt_stream.rs` emits.
//!
//! Run with: `cargo test -p asperitas-host --test excerpt_install`
//! Regenerate goldens with: `UPDATE_GOLDENS=1 cargo test -p asperitas-host --test excerpt_install`
//!
//! A golden diff means the wire changed. Regenerate only when that change is intended, because
//! a device flashed before the change will refuse the new stream.

use std::path::PathBuf;

use asperitas_host::excerpt::{install_frames, parse_wav};
use asperitas_wire::excerpt::{
    exc_end_body, parse_record, slot_base, slot_pcm_base, Action, ExcRecord, Failure, Installer,
    SlotHeader, Tag, Verdict, EXC_CHUNK_RAW, PCM_CAPACITY, SECTOR,
};
use asperitas_wire::frame::{crc16_ccitt, Decoder, MAX_BODY};
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
const REGENERATE: &str = "UPDATE_GOLDENS=1 cargo test -p asperitas-wire --test excerpt_install";

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
