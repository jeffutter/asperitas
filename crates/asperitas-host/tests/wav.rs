//! Host suite for `asperitas_host::excerpt::parse_wav`: which files may become an excerpt.
//!
//! Run with: `cargo test -p asperitas-host --test wav`

use std::path::PathBuf;

use asperitas_host::excerpt::{parse_wav, WavError};
use asperitas_wire::excerpt::check_pcm_len;

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
