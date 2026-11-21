//! The `pitch` and `pitch-corpus` commands end to end through the binary.

use std::path::Path;
use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_asperitas-cli"))
        .args(args)
        .output()
        .unwrap()
}

fn write_tone(path: &Path) {
    let frames: Vec<[f32; 2]> = (0..48_000 * 2)
        .map(|i| {
            let v = 0.3 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 48_000.0).sin();
            [v, v]
        })
        .collect();
    asperitas_cli::wav_io::write_wav(path.to_str().unwrap(), 48_000, &frames).unwrap();
}

#[test]
fn pitch_writes_csv_and_png_byte_identically_and_prints_the_row() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("tone.wav");
    write_tone(&wav);
    let run = |name: &str| {
        let prefix = dir.path().join(name);
        let out = cli(&[
            "pitch",
            wav.to_str().unwrap(),
            prefix.to_str().unwrap(),
            "--nominal-hz",
            "220",
        ]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        (
            out.stdout,
            std::fs::read(prefix.with_extension("csv")).unwrap(),
            std::fs::read(prefix.with_extension("png")).unwrap(),
        )
    };
    let (table, csv, png) = run("a");
    assert_eq!((table.clone(), csv.clone(), png.clone()), run("b"));
    assert!(String::from_utf8(table).unwrap().contains("| tone |"));
    assert!(csv.starts_with(b"t_s,estimate_hz,confidence"));
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    // IHDR width and height, big endian.
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 960);
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 560);
}

#[test]
fn pitch_refuses_bad_flags_and_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let prefix = dir.path().join("x");
    let prefix = prefix.to_str().unwrap();
    for args in [
        vec![
            "pitch",
            "synthetic:notes",
            prefix,
            "--clarity-threshold",
            "3",
        ],
        vec!["pitch", "synthetic:notes", prefix, "--tolerance-cents", "0"],
        vec!["pitch", "synthetic:nope", prefix],
        vec!["pitch", "/nonexistent.wav", prefix],
    ] {
        let out = cli(&args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stderr).starts_with("error:"));
    }
}

#[test]
fn pitch_corpus_runs_a_corpus_directory_and_the_built_in_signals() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    std::fs::create_dir(&corpus).unwrap();
    let clip = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../audio/instruments/mandolin_single_note_soft.wav");
    std::fs::copy(&clip, corpus.join("mandolin_single_note_soft.wav")).unwrap();
    let out_dir = dir.path().join("out");
    let out = cli(&[
        "pitch-corpus",
        out_dir.to_str().unwrap(),
        "--corpus-dir",
        corpus.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let table = String::from_utf8(out.stdout).unwrap();
    for name in ["mandolin_single_note_soft", "sweep", "notes", "noise"] {
        assert!(
            table.contains(&format!("| {name} |")),
            "{name} row in\n{table}"
        );
        assert!(out_dir.join(format!("{name}.csv")).exists());
        assert!(out_dir.join(format!("{name}.png")).exists());
    }
    assert_eq!(
        std::fs::read_to_string(out_dir.join("table.md")).unwrap(),
        table
    );
}
