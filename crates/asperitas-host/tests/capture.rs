//! `asperitas_host::capture`: a console stream becomes PCM only when every block proves complete.
//!
//! Run with: `cargo test -p asperitas-host --test capture`

use asperitas_host::capture::{load_console, load_console_zst, LoadError};
use asperitas_wire::dump::{audend_record, audio_record, CHUNK_RAW};
use asperitas_wire::frame::{crc16_ccitt, MAX_BODY, MAX_FRAME};
use asperitas_wire::Level;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../captures/TASK-038.05/2026-10-07-rig-sine-300s.console.zst"
);

#[test]
fn rig_sine_300s_loads_whole() {
    let proved = load_console_zst(FIXTURE).expect("fixture loads");
    assert_eq!(proved.pcm.len(), 14_401_536 * 2);
    assert_eq!(proved.blocks, 879);
}

#[test]
fn garbage_is_an_error_not_an_empty_capture() {
    let err = load_console(&[0xA5u8; 300][..]).unwrap_err();
    assert!(
        matches!(err, LoadError::Unproved(_) | LoadError::Empty),
        "{err}"
    );
}

#[test]
fn empty_stream_is_an_error() {
    assert!(matches!(load_console(&[][..]), Err(LoadError::Empty)));
}

const CHUNKS: u16 = 6;

/// Wire bytes of a six-chunk block whose PCM is the ramp starting at `base`.
fn block(seq: &mut u32, id: u32, base: i16, drop_chunk: Option<u16>) -> (Vec<u8>, Vec<u8>) {
    let samples: Vec<i16> = (0..(CHUNK_RAW as i16 * CHUNKS as i16 / 2))
        .map(|i| base.wrapping_add(i))
        .collect();
    let raw: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let mut wire = Vec::new();
    for index in 0..CHUNKS {
        let mut body = [0u8; MAX_BODY];
        let mut fb = [0u8; MAX_FRAME];
        let start = index as usize * CHUNK_RAW;
        let e = audio_record(
            Level::Info,
            *seq,
            *seq * 4,
            id,
            CHUNKS,
            index,
            &raw[start..start + CHUNK_RAW],
            &mut body,
            &mut fb,
        )
        .unwrap();
        *seq += 1;
        if Some(index) != drop_chunk {
            wire.extend_from_slice(&fb[..e.len]);
        }
    }
    let mut body = [0u8; MAX_BODY];
    let mut fb = [0u8; MAX_FRAME];
    let e = audend_record(
        Level::Info,
        *seq,
        *seq * 4,
        id,
        CHUNKS,
        raw.len() as u32,
        crc16_ccitt(&raw),
        &mut body,
        &mut fb,
    )
    .unwrap();
    *seq += 1;
    wire.extend_from_slice(&fb[..e.len]);
    (wire, raw)
}

#[test]
fn synthetic_stream_round_trips() {
    let mut seq = 0;
    let (a, pa) = block(&mut seq, 0, 0, None);
    let (b, pb) = block(&mut seq, 1, 1000, None);
    let proved = load_console([a, b].concat().as_slice()).unwrap();
    assert_eq!(proved.blocks, 2);
    assert_eq!(proved.pcm, [pa, pb].concat());
}

#[test]
fn dropped_chunk_is_an_error() {
    let mut seq = 0;
    let (a, _) = block(&mut seq, 0, 0, None);
    let (b, _) = block(&mut seq, 1, 1000, Some(1));
    let (d, _) = block(&mut seq, 2, 2000, None);
    let err = load_console([a, b, d].concat().as_slice()).unwrap_err();
    assert!(matches!(err, LoadError::Unproved(_)), "{err}");
}

#[test]
fn flipped_byte_is_an_error() {
    let mut seq = 0;
    let (mut a, _) = block(&mut seq, 0, 0, None);
    let mid = a.len() / 2;
    a[mid] ^= 0xff;
    let err = load_console(a.as_slice()).unwrap_err();
    assert!(
        matches!(err, LoadError::Unproved(_) | LoadError::Empty),
        "{err}"
    );
}

#[test]
fn attach_prefix_is_reported_not_fatal() {
    let mut seq = 0;
    let (a, pa) = block(&mut seq, 0, 0, None);
    let (b, pb) = block(&mut seq, 1, 1000, None);
    let prefix = &a[40..200];
    let proved = load_console([prefix, &a, &b].concat().as_slice()).unwrap();
    assert_eq!(proved.attach_prefix_bytes, prefix.len() as u64);
    assert_eq!(proved.pcm, [pa, pb].concat());
}

#[test]
fn clean_stream_has_no_attach_prefix() {
    let mut seq = 0;
    let (a, _) = block(&mut seq, 0, 0, None);
    assert_eq!(load_console(a.as_slice()).unwrap().attach_prefix_bytes, 0);
}

#[test]
fn junk_between_frames_is_an_error() {
    let mut seq = 0;
    let (a, _) = block(&mut seq, 0, 0, None);
    let (b, _) = block(&mut seq, 1, 1000, None);
    let err = load_console([&a[..], b"junk", &b[..]].concat().as_slice()).unwrap_err();
    assert!(matches!(err, LoadError::Unproved(_)), "{err}");
}

#[test]
fn prefix_with_an_incomplete_block_is_an_error() {
    let mut seq = 0;
    let (a, _) = block(&mut seq, 0, 0, None);
    let (b, _) = block(&mut seq, 1, 1000, Some(2));
    let err = load_console([&a[40..200], &a[..], &b[..]].concat().as_slice()).unwrap_err();
    assert!(matches!(err, LoadError::Unproved(_)), "{err}");
}

#[test]
fn prefix_only_is_empty() {
    let mut seq = 0;
    let (a, _) = block(&mut seq, 0, 0, None);
    let err = load_console(&a[40..200]).unwrap_err();
    assert!(
        matches!(err, LoadError::Unproved(_) | LoadError::Empty),
        "{err}"
    );
}
