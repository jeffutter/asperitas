//! Host suite for `asperitas_logging::excerpt`: slot layout arithmetic, the slot header, the WAV
//! gate and the EXCSTART/EXCDATA/EXCEND grammar.
//!
//! Run with: `cargo test -p asperitas-logging --test excerpt`
//!
//! Layout numbers are pinned to literals written here, not recomputed from the module's own
//! constants, so a change to the module cannot pass by agreeing with itself.

use std::path::PathBuf;

use asperitas_logging::dump::DecodeError;
use asperitas_logging::excerpt::{
    check_pcm_len, exc_data_body, exc_end_body, exc_start_body, parse_record, parse_wav, slot_base,
    slot_pcm_base, ExcRecord, HeaderError, LengthError, RecordError, SlotHeader, Tag, TagError,
    WavError, EXCERPT_AREA_END, EXCERPT_AREA_START, EXC_CHUNK_RAW, HEADER_LEN, MAX_ADDRESS,
    MAX_DATA_BODY_LEN, MAX_DATA_CHUNKS, MAX_START_BODY_LEN, PCM_CAPACITY, SECTOR, SLOT_COUNT,
    SLOT_SPAN, SLOT_STRIDE, TAG_MAX,
};
use asperitas_logging::frame::{crc16_ccitt, MAX_BODY};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

#[test]
fn layout_constants_match_the_parent_ticket() {
    assert_eq!(EXCERPT_AREA_START, 0x10_0000);
    assert_eq!(SLOT_STRIDE, 0x8_0000);
    assert_eq!(SLOT_COUNT, 14);
    assert_eq!(SECTOR, 4096);
    assert_eq!(MAX_ADDRESS, 0x7F_FFFF);
    assert_eq!(PCM_CAPACITY, 516_096);
    assert_eq!(
        EXCERPT_AREA_END, 0x7F_F000,
        "the area ends where the reserved sector begins"
    );
}

#[test]
fn every_slot_is_sector_aligned_inside_the_area_and_disjoint() {
    for slot in 0..SLOT_COUNT {
        let base = slot_base(slot).expect("slot exists");
        let pcm = slot_pcm_base(slot).expect("slot exists");
        assert_eq!(base, 0x10_0000 + u32::from(slot) * 0x8_0000, "slot {slot}");
        assert_eq!(base % 4096, 0, "slot {slot} base");
        assert_eq!(pcm % 4096, 0, "slot {slot} pcm base");
        assert_eq!(pcm, base + 4096, "header owns exactly the first sector");
        assert!(base >= 0x4_0000, "slot {slot} reaches into the bootloader");

        let end = base + SLOT_SPAN;
        assert_eq!(
            pcm + PCM_CAPACITY,
            end,
            "slot {slot} capacity fills its span"
        );
        // The driver bound: address + len <= MAX_ADDRESS for every access, and the top sector
        // stays untouched.
        assert!(end <= 0x7F_FFFF, "slot {slot} exceeds the driver bound");
        assert!(end <= 0x7F_F000, "slot {slot} reaches the reserved sector");
        if let Some(next) = slot_base(slot + 1) {
            assert!(end <= next, "slot {slot} overlaps slot {}", slot + 1);
        }
    }
    assert_eq!(slot_base(13).map(|b| b + SLOT_SPAN), Some(0x7F_F000));
}

#[test]
fn slots_past_the_count_do_not_exist() {
    for slot in SLOT_COUNT..=u8::MAX {
        assert_eq!(slot_base(slot), None, "slot {slot}");
        assert_eq!(slot_pcm_base(slot), None, "slot {slot}");
    }
}

#[test]
fn pcm_length_rule() {
    assert_eq!(check_pcm_len(0), Err(LengthError::Empty));
    assert_eq!(check_pcm_len(3), Err(LengthError::Odd { bytes: 3 }));
    assert_eq!(check_pcm_len(2), Ok(()));
    assert_eq!(check_pcm_len(PCM_CAPACITY), Ok(()));
    assert_eq!(
        check_pcm_len(PCM_CAPACITY + 2),
        Err(LengthError::OverCapacity {
            bytes: PCM_CAPACITY + 2
        })
    );
}

// ---------------------------------------------------------------------------
// Tag
// ---------------------------------------------------------------------------

#[test]
fn tag_charset_excludes_reserved_bytes() {
    assert_eq!(Tag::new(b"mand_chord1").unwrap().as_bytes(), b"mand_chord1");
    assert_eq!(
        Tag::new(&[b'a'; TAG_MAX]).unwrap().as_bytes(),
        &[b'a'; TAG_MAX]
    );
    assert_eq!(Tag::new(b""), Err(TagError::Empty));
    assert_eq!(
        Tag::new(&[b'a'; TAG_MAX + 1]),
        Err(TagError::TooLong { len: TAG_MAX + 1 })
    );
    for byte in [b'<', b'>', b'~', b' ', b'A', b'-', b'=', 0u8, 0xFF] {
        assert_eq!(
            Tag::new(&[b'a', byte]),
            Err(TagError::Char { at: 1, byte }),
            "byte {byte:#04x}"
        );
    }
}

// ---------------------------------------------------------------------------
// Slot header
// ---------------------------------------------------------------------------

fn sample_header() -> SlotHeader {
    SlotHeader::new(Tag::new(b"mand_chord").unwrap(), 288_000, 0xBEEF).unwrap()
}

/// Re-seal a hand-edited header so a test reaches the field checks behind the CRC.
fn reseal(raw: &mut [u8; HEADER_LEN]) {
    let crc = crc16_ccitt(&raw[..HEADER_LEN - 2]);
    raw[HEADER_LEN - 2..].copy_from_slice(&crc.to_le_bytes());
}

#[test]
fn header_round_trips() {
    let header = sample_header();
    let raw = header.encode();
    assert_eq!(&raw[..4], b"ASPX");
    assert_eq!(SlotHeader::decode(&raw), Ok(header));
    // Trailing bytes from the rest of the sector are ignored.
    let mut sector = vec![0xFFu8; SECTOR as usize];
    sector[..HEADER_LEN].copy_from_slice(&raw);
    assert_eq!(SlotHeader::decode(&sector), Ok(header));

    let full = SlotHeader::new(Tag::new(&[b'z'; TAG_MAX]).unwrap(), PCM_CAPACITY, 0).unwrap();
    assert_eq!(SlotHeader::decode(&full.encode()), Ok(full));
}

#[test]
fn blank_and_short_flash_is_not_a_header() {
    assert_eq!(
        SlotHeader::decode(&[0xFF; HEADER_LEN]),
        Err(HeaderError::Magic)
    );
    assert_eq!(
        SlotHeader::decode(&sample_header().encode()[..HEADER_LEN - 1]),
        Err(HeaderError::Short {
            len: HEADER_LEN - 1
        })
    );
}

#[test]
fn every_single_bit_flip_is_rejected() {
    let raw = sample_header().encode();
    for bit in 0..HEADER_LEN * 8 {
        let mut flipped = raw;
        flipped[bit / 8] ^= 1 << (bit % 8);
        assert!(
            SlotHeader::decode(&flipped).is_err(),
            "flip of bit {bit} decoded as valid"
        );
    }
}

#[test]
fn header_crc_mismatch_is_reported_as_such() {
    let mut raw = sample_header().encode();
    raw[22] ^= 0x10; // inside pcm_bytes
    assert!(matches!(
        SlotHeader::decode(&raw),
        Err(HeaderError::Crc { .. })
    ));
}

#[test]
fn header_field_checks_behind_a_valid_crc() {
    let base = sample_header().encode();

    let mut raw = base;
    raw[4] = 2;
    reseal(&mut raw);
    assert_eq!(
        SlotHeader::decode(&raw),
        Err(HeaderError::Version { version: 2 })
    );

    let mut raw = base;
    raw[22..26].copy_from_slice(&(PCM_CAPACITY + 2).to_le_bytes());
    reseal(&mut raw);
    assert_eq!(
        SlotHeader::decode(&raw),
        Err(HeaderError::Length(LengthError::OverCapacity {
            bytes: PCM_CAPACITY + 2
        }))
    );

    let mut raw = base;
    raw[22..26].copy_from_slice(&1001u32.to_le_bytes());
    reseal(&mut raw);
    assert_eq!(
        SlotHeader::decode(&raw),
        Err(HeaderError::Length(LengthError::Odd { bytes: 1001 }))
    );

    let mut raw = base;
    raw[22..26].copy_from_slice(&0u32.to_le_bytes());
    reseal(&mut raw);
    assert_eq!(
        SlotHeader::decode(&raw),
        Err(HeaderError::Length(LengthError::Empty))
    );

    let mut raw = base;
    raw[6 + 12] = b'x'; // after the NUL that ends "mand_chord"
    reseal(&mut raw);
    assert_eq!(SlotHeader::decode(&raw), Err(HeaderError::NamePadding));

    let mut raw = base;
    raw[6] = b'<';
    reseal(&mut raw);
    assert_eq!(
        SlotHeader::decode(&raw),
        Err(HeaderError::Name(TagError::Char { at: 0, byte: b'<' }))
    );

    let mut raw = base;
    raw[6..6 + TAG_MAX].fill(0);
    reseal(&mut raw);
    assert_eq!(
        SlotHeader::decode(&raw),
        Err(HeaderError::Name(TagError::Empty))
    );
}

// ---------------------------------------------------------------------------
// WAV
// ---------------------------------------------------------------------------

/// The `fmt ` fields of a WAV, defaulting to what an excerpt accepts.
#[derive(Clone, Copy)]
struct Fmt {
    tag: u16,
    channels: u16,
    rate: u32,
    byte_rate: u32,
    block_align: u16,
    bits: u16,
}

const GOOD: Fmt = Fmt {
    tag: 1,
    channels: 1,
    rate: 48_000,
    byte_rate: 96_000,
    block_align: 2,
    bits: 16,
};

fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    if body.len() % 2 == 1 {
        out.push(0);
    }
    out
}

fn fmt_chunk(fmt: Fmt) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&fmt.tag.to_le_bytes());
    body.extend_from_slice(&fmt.channels.to_le_bytes());
    body.extend_from_slice(&fmt.rate.to_le_bytes());
    body.extend_from_slice(&fmt.byte_rate.to_le_bytes());
    body.extend_from_slice(&fmt.block_align.to_le_bytes());
    body.extend_from_slice(&fmt.bits.to_le_bytes());
    chunk(b"fmt ", &body)
}

/// A RIFF/WAVE file whose RIFF size matches the chunks given.
fn riff(chunks: &[Vec<u8>]) -> Vec<u8> {
    let inner: Vec<u8> = chunks.concat();
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(inner.len() as u32 + 4).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(&inner);
    out
}

fn pcm(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 7 + 3) as u8).collect()
}

#[test]
fn canonical_wav_yields_its_data_span() {
    let data = pcm(1000);
    let file = riff(&[fmt_chunk(GOOD), chunk(b"data", &data)]);
    assert_eq!(file.len(), 44 + 1000, "canonical header is 44 bytes");
    assert_eq!(parse_wav(&file), Ok(&data[..]));
}

#[test]
fn extra_chunks_are_skipped_including_odd_padded_ones() {
    let data = pcm(64);
    let file = riff(&[
        chunk(b"LIST", b"INFOISFT\x05\x00\x00\x00abcd\x00"),
        fmt_chunk(GOOD),
        chunk(b"fact", b"odd"), // 3 bytes plus a pad byte
        chunk(b"data", &data),
        chunk(b"LIST", b"trailing"),
    ]);
    assert_eq!(parse_wav(&file), Ok(&data[..]));
}

#[test]
fn bytes_outside_the_riff_are_ignored() {
    let data = pcm(8);
    let mut file = riff(&[fmt_chunk(GOOD), chunk(b"data", &data)]);
    file.extend_from_slice(b"junk after riff");
    assert_eq!(parse_wav(&file), Ok(&data[..]));
}

#[test]
fn wav_rejections() {
    let data = pcm(100);
    let ok = |fmt: Fmt| riff(&[fmt_chunk(fmt), chunk(b"data", &data)]);

    let mut not_riff = ok(GOOD);
    not_riff[0] = b'X';
    let mut not_wave = ok(GOOD);
    not_wave[8..12].copy_from_slice(b"AVI ");
    let mut riff_too_long = ok(GOOD);
    riff_too_long[4..8].copy_from_slice(&10_000u32.to_le_bytes());
    let full = ok(GOOD);
    let truncated_data = {
        // RIFF size claims the whole data chunk but the file stops short.
        let mut f = full.clone();
        f.truncate(full.len() - 10);
        let riff_len = (f.len() - 8) as u32;
        f[4..8].copy_from_slice(&riff_len.to_le_bytes());
        f
    };
    let short_fmt = riff(&[chunk(b"fmt ", &[1, 0, 1, 0]), chunk(b"data", &data)]);

    let cases: Vec<(&str, Vec<u8>, WavError)> = vec![
        ("empty input", Vec::new(), WavError::NotRiff),
        ("not RIFF", not_riff, WavError::NotRiff),
        ("not WAVE", not_wave, WavError::NotWave),
        ("RIFF size past end", riff_too_long, WavError::Truncated),
        ("data runs past end", truncated_data, WavError::Truncated),
        ("short fmt", short_fmt, WavError::FmtTooShort { len: 4 }),
        (
            "float",
            ok(Fmt { tag: 3, ..GOOD }),
            WavError::FormatTag { tag: 3 },
        ),
        (
            "extensible",
            ok(Fmt {
                tag: 0xFFFE,
                ..GOOD
            }),
            WavError::FormatTag { tag: 0xFFFE },
        ),
        (
            "stereo",
            ok(Fmt {
                channels: 2,
                byte_rate: 192_000,
                block_align: 4,
                ..GOOD
            }),
            WavError::Channels { channels: 2 },
        ),
        (
            "44.1 kHz",
            ok(Fmt {
                rate: 44_100,
                byte_rate: 88_200,
                ..GOOD
            }),
            WavError::SampleRate { rate: 44_100 },
        ),
        (
            "24-bit",
            ok(Fmt {
                bits: 24,
                byte_rate: 144_000,
                block_align: 3,
                ..GOOD
            }),
            WavError::BitsPerSample { bits: 24 },
        ),
        (
            "8-bit",
            ok(Fmt { bits: 8, ..GOOD }),
            WavError::BitsPerSample { bits: 8 },
        ),
        (
            "bad block align",
            ok(Fmt {
                block_align: 4,
                ..GOOD
            }),
            WavError::Inconsistent,
        ),
        (
            "bad byte rate",
            ok(Fmt {
                byte_rate: 48_000,
                ..GOOD
            }),
            WavError::Inconsistent,
        ),
        (
            "data before fmt",
            riff(&[chunk(b"data", &data), fmt_chunk(GOOD)]),
            WavError::DataBeforeFmt,
        ),
        (
            "no data",
            riff(&[fmt_chunk(GOOD), chunk(b"LIST", b"xx")]),
            WavError::MissingData,
        ),
        (
            "empty data",
            riff(&[fmt_chunk(GOOD), chunk(b"data", &[])]),
            WavError::EmptyData,
        ),
        (
            "odd data",
            riff(&[fmt_chunk(GOOD), chunk(b"data", &pcm(101))]),
            WavError::OddData { len: 101 },
        ),
        (
            "chunk header cut short",
            {
                let mut f = riff(&[fmt_chunk(GOOD)]);
                f.extend_from_slice(b"dat");
                let riff_len = (f.len() - 8) as u32;
                f[4..8].copy_from_slice(&riff_len.to_le_bytes());
                f
            },
            WavError::Truncated,
        ),
    ];

    for (what, file, want) in cases {
        assert_eq!(parse_wav(&file), Err(want), "{what}");
    }
}

#[test]
fn every_corpus_file_parses() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../audio/instruments");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).expect("audio/instruments exists") {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "wav") {
            continue;
        }
        let file = std::fs::read(&path).unwrap();
        let data = parse_wav(&file).unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));
        let declared = u32::from_le_bytes(file[40..44].try_into().unwrap());
        assert_eq!(data.len() as u32, declared, "{}", path.display());
        assert_eq!(
            check_pcm_len(declared),
            Ok(()),
            "{} fits a slot",
            path.display()
        );
        seen += 1;
    }
    assert_eq!(seen, 8, "the corpus has eight clips");
}

// ---------------------------------------------------------------------------
// Grammar
// ---------------------------------------------------------------------------

fn body_of(f: impl FnOnce(&mut [u8; MAX_BODY]) -> usize) -> Vec<u8> {
    let mut out = [0u8; MAX_BODY];
    let len = f(&mut out);
    out[..len].to_vec()
}

#[test]
fn bodies_have_the_documented_shape() {
    let name = Tag::new(b"mand_chord").unwrap();
    let start = body_of(|o| exc_start_body(3, &name, 288_000, 0xbeef, o).unwrap());
    assert_eq!(
        start,
        b"EXCSTART slot=03 name=mand_chord bytes=288000 crc16=beef"
    );
    let data = body_of(|o| exc_data_body(0x2a, b"\x00\x01\xfe", o).unwrap());
    assert_eq!(data, b"EXCDATA i=002a b64=AAH+");
    assert_eq!(body_of(exc_end_body), b"EXCEND");
}

#[test]
fn longest_bodies_fit_a_frame() {
    let name = Tag::new(&[b'q'; TAG_MAX]).unwrap();
    let start = body_of(|o| exc_start_body(13, &name, PCM_CAPACITY, 0xffff, o).unwrap());
    assert_eq!(start.len(), MAX_START_BODY_LEN);
    let raw = [0xA5u8; EXC_CHUNK_RAW];
    let data = body_of(|o| exc_data_body((MAX_DATA_CHUNKS - 1) as u16, &raw, o).unwrap());
    assert_eq!(data.len(), MAX_DATA_BODY_LEN);
    assert!(start.len() <= MAX_BODY && data.len() <= MAX_BODY);
}

#[test]
fn builders_refuse_out_of_range_inputs() {
    let name = Tag::new(b"x").unwrap();
    let mut out = [0x55u8; MAX_BODY];
    assert_eq!(
        exc_start_body(14, &name, 2, 0, &mut out),
        Err(RecordError::SlotOutOfRange { slot: 14 })
    );
    assert_eq!(
        exc_start_body(0, &name, 3, 0, &mut out),
        Err(RecordError::Length(LengthError::Odd { bytes: 3 }))
    );
    assert_eq!(
        exc_start_body(0, &name, PCM_CAPACITY + 2, 0, &mut out),
        Err(RecordError::Length(LengthError::OverCapacity {
            bytes: PCM_CAPACITY + 2
        }))
    );
    assert_eq!(
        exc_data_body(MAX_DATA_CHUNKS as u16, b"ab", &mut out),
        Err(RecordError::IndexOutOfRange {
            index: MAX_DATA_CHUNKS as u16
        })
    );
    assert_eq!(
        exc_data_body(0, b"", &mut out),
        Err(RecordError::EmptyChunk)
    );
    assert_eq!(
        exc_data_body(0, &[0; EXC_CHUNK_RAW + 1], &mut out),
        Err(RecordError::ChunkTooLong {
            len: EXC_CHUNK_RAW + 1
        })
    );
    assert_eq!(
        out, [0x55u8; MAX_BODY],
        "a refusal leaves the buffer untouched"
    );
}

#[test]
fn parser_rejections() {
    let cases: &[(&[u8], RecordError)] = &[
        (b"AUDIO blk=0000 n=01 c=00 d=AAAA", RecordError::NotExcerpt),
        (b"", RecordError::NotExcerpt),
        (b"EXCEND ", RecordError::Malformed { at: 6 }),
        (b"EXCENDX", RecordError::Malformed { at: 6 }),
        // Uppercase hex is not in the grammar.
        (
            b"EXCSTART slot=0A name=a bytes=000002 crc16=0000",
            RecordError::Malformed { at: 14 },
        ),
        (
            b"EXCSTART slot=0e name=a bytes=000002 crc16=0000",
            RecordError::SlotOutOfRange { slot: 14 },
        ),
        (
            b"EXCSTART slot=00 name=a<b bytes=000002 crc16=0000",
            RecordError::Name(TagError::Char { at: 1, byte: b'<' }),
        ),
        (
            b"EXCSTART slot=00 name=a>b bytes=000002 crc16=0000",
            RecordError::Name(TagError::Char { at: 1, byte: b'>' }),
        ),
        (
            b"EXCSTART slot=00 name=a~b bytes=000002 crc16=0000",
            RecordError::Name(TagError::Char { at: 1, byte: b'~' }),
        ),
        (
            b"EXCSTART slot=00 name= bytes=000002 crc16=0000",
            RecordError::Name(TagError::Empty),
        ),
        (
            b"EXCSTART slot=00 name=a  bytes=000002 crc16=0000",
            RecordError::Malformed { at: 23 },
        ),
        (
            b"EXCSTART slot=00 name=a bytes=2 crc16=0000",
            RecordError::Malformed { at: 30 },
        ),
        (
            b"EXCSTART slot=00 name=a bytes=000003 crc16=0000",
            RecordError::Length(LengthError::Odd { bytes: 3 }),
        ),
        (
            b"EXCSTART slot=00 name=a bytes=000000 crc16=0000",
            RecordError::Length(LengthError::Empty),
        ),
        (
            b"EXCSTART slot=00 name=a bytes=999998 crc16=0000",
            RecordError::Length(LengthError::OverCapacity { bytes: 999_998 }),
        ),
        (
            b"EXCSTART slot=00 name=a bytes=000002 crc16=0000x",
            RecordError::Malformed { at: 47 },
        ),
        (
            b"EXCSTART slot=00 name=a bytes=000002 crc16=000",
            RecordError::Malformed { at: 43 },
        ),
        (
            b"EXCDATA i=0fc0 b64=AAAA",
            RecordError::IndexOutOfRange { index: 0x0fc0 },
        ),
        (b"EXCDATA i=0000 b64=", RecordError::EmptyChunk),
        (b"EXCDATA i=000 b64=AAAA", RecordError::Malformed { at: 10 }),
        (b"EXCDATA i=0000 d=AAAA", RecordError::Malformed { at: 14 }),
        (
            b"EXCDATA i=0000 b64=AA<A",
            RecordError::Payload(DecodeError::Char { at: 2, byte: b'<' }),
        ),
        (
            b"EXCDATA i=0000 b64=AAA",
            RecordError::Payload(DecodeError::Length(3)),
        ),
        (
            b"EXCDATA i=0000 b64=AB==",
            RecordError::Payload(DecodeError::TrailingBits { at: 1 }),
        ),
    ];
    for (body, want) in cases {
        assert_eq!(
            parse_record(body),
            Err(*want),
            "{}",
            String::from_utf8_lossy(body)
        );
    }
}

#[test]
fn a_payload_over_the_chunk_size_is_refused() {
    // 129 bytes encode to the same 172 characters as 128, so the character count alone cannot
    // catch it; the decode bound has to.
    let mut b64 = [0u8; 172];
    let n = asperitas_logging::dump::encode(&[7u8; EXC_CHUNK_RAW + 1], &mut b64).unwrap();
    let mut body = b"EXCDATA i=0000 b64=".to_vec();
    body.extend_from_slice(&b64[..n]);
    assert_eq!(
        parse_record(&body),
        Err(RecordError::ChunkTooLong {
            len: EXC_CHUNK_RAW + 1
        })
    );
}

fn tag_strategy() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(
        prop_oneof![b'a'..=b'z', b'0'..=b'9', Just(b'_')],
        1..=TAG_MAX,
    )
}

proptest! {
    #[test]
    fn start_round_trips(
        slot in 0..SLOT_COUNT,
        name in tag_strategy(),
        half in 1..=PCM_CAPACITY / 2,
        crc16 in any::<u16>(),
    ) {
        let name = Tag::new(&name).unwrap();
        let bytes = half * 2;
        let body = body_of(|o| exc_start_body(slot, &name, bytes, crc16, o).unwrap());
        prop_assert_eq!(parse_record(&body), Ok(ExcRecord::Start { slot, name, bytes, crc16 }));
    }

    #[test]
    fn data_round_trips(
        index in 0..MAX_DATA_CHUNKS as u16,
        raw in proptest::collection::vec(any::<u8>(), 1..=EXC_CHUNK_RAW),
    ) {
        let body = body_of(|o| exc_data_body(index, &raw, o).unwrap());
        prop_assert!(body.len() <= MAX_BODY);
        prop_assert!(!body.iter().any(|b| matches!(b, b'<' | b'>' | b'~')));
        match parse_record(&body) {
            Ok(ExcRecord::Data { index: got, chunk }) => {
                prop_assert_eq!(got, index);
                prop_assert_eq!(chunk.as_bytes(), &raw[..]);
            }
            other => prop_assert!(false, "unexpected {:?}", other),
        }
    }

    /// A reserved byte anywhere in a valid body is never accepted.
    #[test]
    fn reserved_bytes_never_parse(
        raw in proptest::collection::vec(any::<u8>(), 1..=EXC_CHUNK_RAW),
        pick in any::<prop::sample::Index>(),
        byte in prop_oneof![Just(b'<'), Just(b'>'), Just(b'~')],
    ) {
        let mut body = body_of(|o| exc_data_body(1, &raw, o).unwrap());
        let at = pick.index(body.len());
        body[at] = byte;
        prop_assert!(parse_record(&body).is_err());
    }
}
