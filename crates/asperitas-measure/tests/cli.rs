//! End-to-end checks of the `asperitas-measure` binary against committed captures.

use std::process::{Command, Output};

fn root(p: &str) -> String {
    format!("{}/../../{p}", env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_asperitas-measure"))
        .args(args)
        .output()
        .unwrap()
}

fn json(o: &Output) -> serde_json::Value {
    serde_json::from_slice(&o.stdout).expect("stdout is one JSON document")
}

const REPLAY: &str = "audio/captures/2026-10-15-rig-excerpt-replay-mand-soft.console.zst";
const CLIP: &str = "audio/instruments/mandolin_single_note_soft.wav";

#[test]
fn healthy_sine_capture_passes() {
    let cap = root("audio/captures/2026-10-13-rig-10s-reset1.console.zst");
    let o = run(&["analyze", &cap]);
    let j = json(&o);
    eprintln!("{j}\n{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(j["pass"], true);
    assert!(j["metrics"]["level"]["deviation_db"].is_number());
}

#[test]
fn excerpt_replay_passes_against_its_reference() {
    let o = run(&["analyze", &root(REPLAY), "--excerpt", &root(CLIP)]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(json(&o)["comparison"]["residual_rms_db"].is_number());
}

#[test]
fn wrong_stimulus_is_a_failed_verdict() {
    // The replay capture is music, not the rig sine.
    let o = run(&["analyze", &root(REPLAY)]);
    assert_eq!(o.status.code(), Some(1));
    let j = json(&o);
    assert_eq!(j["pass"], false);
    assert!(!j["failures"].as_array().unwrap().is_empty());
}

#[test]
fn wrong_excerpt_is_a_failed_verdict() {
    let other = root("audio/instruments/mandolin_chord.wav");
    let o = run(&["analyze", &root(REPLAY), "--excerpt", &other]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(json(&o)["pass"], false);
}

#[test]
fn missing_file_and_bad_usage_are_errors() {
    assert_eq!(
        run(&["analyze", "/nonexistent.console.zst"]).status.code(),
        Some(2)
    );
    assert_eq!(run(&[]).status.code(), Some(2));
    let cap = root(REPLAY);
    assert_eq!(
        run(&["analyze", &cap, "--stimulus", "square"])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        run(&["analyze", &cap, "--stimulus", "sine", "--excerpt", "x.wav"])
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn sweep_mode_rejects_bad_usage_and_reports_short_capture() {
    let cap = root("audio/captures/2026-10-13-rig-10s-reset1.console.zst");
    // Missing kind, bad range and mixing with --stimulus are usage errors (exit 2).
    for args in [
        vec!["analyze", &cap, "--sweep", "0:1"],
        vec!["analyze", &cap, "--sweep", "0:2", "--sweep-kind", "gain"],
        vec![
            "analyze",
            &cap,
            "--sweep",
            "0:1",
            "--sweep-kind",
            "gain",
            "--stimulus",
            "sine",
        ],
        vec!["analyze", &cap, "--sweep-kind", "gain"],
    ] {
        assert_eq!(run(&args).status.code(), Some(2), "{args:?}");
    }
    // A steady sine has no response to the knob: a failed verdict (exit 1) with a JSON report.
    let o = run(&["analyze", &cap, "--sweep", "0:1", "--sweep-kind", "gain"]);
    assert_eq!(o.status.code(), Some(1));
    let j = json(&o);
    assert_eq!(j["pass"], false);
    assert_eq!(j["response"]["direction"], "none");
}
