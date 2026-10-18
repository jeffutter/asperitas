//! End-to-end checks of the `asperitas-measure` binary against committed captures.

use std::process::{Command, Output};

use asperitas_host::session::{sidecar_path, Mode, Sidecar};

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

const SINE: &str = "audio/captures/2026-10-13-rig-10s-reset1.console.zst";

/// A copy of `capture` in a fresh directory of its own, with `sidecar` written beside it as
/// `asperitas-rig session` would. Returns the copy's path.
fn with_sidecar(name: &str, capture: &str, sidecar: &Sidecar) -> String {
    let dir = std::env::temp_dir().join(format!(
        "asperitas-measure-cli-{}-{name}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("capture.console.zst");
    std::fs::copy(root(capture), &out).unwrap();
    std::fs::write(sidecar_path(&out), sidecar.render()).unwrap();
    out.to_str().unwrap().to_string()
}

fn sidecar(mode: Mode, knob: u8, from: f64, to: f64) -> Sidecar {
    Sidecar {
        fx_f: 1,
        fx_v: 0,
        knob,
        mode,
        from,
        to,
        duration_s: if mode == Mode::Hold { 0.0 } else { 8.0 },
        start_after_arm_s: 1.0,
        snaps: Vec::new(),
        snap_level: 0.0,
        arm_t_ms: Some(1000),
        first_set_t_ms: Some(2500),
    }
}

#[test]
fn sweep_session_needs_no_flags_and_takes_its_motion_from_the_sidecar() {
    // A steady sine has no response to the knob: a failed verdict with a JSON report.
    for (knob, name) in [(1, "gain"), (0, "lowpass")] {
        let cap = with_sidecar(
            &format!("sweep-{name}"),
            SINE,
            &sidecar(Mode::Sweep, knob, 0.0, 1.0),
        );
        let o = run(&["analyze", &cap]);
        assert_eq!(o.status.code(), Some(1), "{name}");
        let j = json(&o);
        assert_eq!(j["response"]["direction"], "none");
        assert_eq!(j["sweep"]["from"], 0.0);
        assert_eq!(j["sweep"]["to"], 1.0);
        assert_eq!(j["sweep"]["duration_s"], 8.0);
        // Device clock: first SET at 2500 ms, armed at 1000 ms. Not the planned 1.0 s.
        assert_eq!(j["sweep"]["start_s"], 1.5);
    }
}

#[test]
fn sweep_start_falls_back_to_the_planned_delay_without_device_times() {
    let side = Sidecar {
        first_set_t_ms: None,
        ..sidecar(Mode::Sweep, 1, 0.0, 1.0)
    };
    let cap = with_sidecar("sweep-planned", SINE, &side);
    assert_eq!(json(&run(&["analyze", &cap]))["sweep"]["start_s"], 1.0);
}

#[test]
fn sweep_session_on_an_unjudged_knob_is_a_usage_error() {
    let cap = with_sidecar("sweep-knob2", SINE, &sidecar(Mode::Sweep, 2, 0.0, 1.0));
    assert_eq!(run(&["analyze", &cap]).status.code(), Some(2));
}

#[test]
fn soak_session_fails_a_short_capture_and_honours_expect_fail() {
    let mut side = sidecar(Mode::Soak, 0, 0.5, 0.95);
    side.duration_s = 250.0;
    let cap = with_sidecar("soak", SINE, &side);
    // 10 s is not a soak: a failed verdict with a JSON report, not an error.
    let o = run(&["analyze", &cap]);
    assert_eq!(o.status.code(), Some(1));
    let j = json(&o);
    assert_eq!(j["soak"]["from"], 0.5);
    assert_eq!(j["soak"]["to"], 0.95);
    assert_eq!(j["soak"]["duration_s"], 250.0);
    assert!(j["result"]["failures"][0]
        .as_str()
        .unwrap()
        .contains("at least"));
    // Expecting failure does not excuse a capture the step detector never fired on.
    let o = run(&["analyze", &cap, "--expect-fail"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(json(&o)["expect_failure"], true);
}

#[test]
fn hold_session_is_judged_as_a_soak_over_the_whole_capture() {
    let cap = with_sidecar("hold", SINE, &sidecar(Mode::Hold, 1, 0.5, 0.5));
    let o = run(&["analyze", &cap]);
    let j = json(&o);
    assert_eq!(j["soak"]["from"], 0.5);
    assert_eq!(j["soak"]["to"], 0.5);
    assert_eq!(j["soak"]["start_s"], 0.0);
}

#[test]
fn excerpt_replay_takes_effect_and_knob_from_a_hold_sidecar() {
    let cap = with_sidecar("excerpt-hold", REPLAY, &sidecar(Mode::Hold, 1, 1.0, 1.0));
    let o = run(&["analyze", &cap, "--excerpt", &root(CLIP)]);
    let j = json(&o);
    assert_eq!(j["fx"]["code"], 1);
    assert_eq!(j["fx"]["knobs"][0], serde_json::Value::Null);
    assert_eq!(j["fx"]["knobs"][1], 65535);
}

#[test]
fn flags_that_contradict_the_sidecar_are_usage_errors() {
    let sweep = with_sidecar("conflict-sweep", SINE, &sidecar(Mode::Sweep, 1, 0.0, 1.0));
    let hold = with_sidecar("conflict-hold", SINE, &sidecar(Mode::Hold, 1, 0.5, 0.5));
    for args in [
        vec!["analyze", &sweep, "--stimulus", "sine"],
        vec!["analyze", &sweep, "--expect-fail"],
        vec!["analyze", &sweep, "--excerpt", "x.wav"],
        vec!["analyze", &hold, "--stimulus", "sine"],
        vec!["analyze", &hold, "--excerpt", "x.wav", "--expect-fail"],
    ] {
        assert_eq!(run(&args).status.code(), Some(2), "{args:?}");
    }
}

#[test]
fn old_hand_set_flags_are_gone_and_a_bad_sidecar_is_an_error() {
    let cap = root(SINE);
    for args in [
        vec!["analyze", &cap, "--sweep", "0:1", "--sweep-kind", "gain"],
        vec!["analyze", &cap, "--soak", "0:1"],
        vec!["analyze", &cap, "--sweep-start-s", "1"],
        vec!["analyze", &cap, "--fx", "patch"],
        // Without a sidecar there is no soak to expect a failure of.
        vec!["analyze", &cap, "--expect-fail"],
    ] {
        assert_eq!(run(&args).status.code(), Some(2), "{args:?}");
    }
    let cap = with_sidecar("bad", SINE, &sidecar(Mode::Hold, 1, 0.5, 0.5));
    std::fs::write(sidecar_path(std::path::Path::new(&cap)), "nope").unwrap();
    assert_eq!(run(&["analyze", &cap]).status.code(), Some(2));
}
