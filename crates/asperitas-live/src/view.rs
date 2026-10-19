//! The pure logic of the terminal knob view: key presses to actions and knob readout text.
//!
//! Nothing here touches the terminal, so it is unit-tested. Real units come from
//! [`ASPERITAS_PARAMS`] through [`Controls::values`]; this module holds no taper or range.

use asperitas_dsp::asperitas::{Controls, ASPERITAS_PARAMS, KNOB_COUNT};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Knob step per key press. 0.05 makes 20 presses from end to end, coarse enough to sweep a
/// knob quickly and fine enough to hear each step.
pub const COARSE_STEP: f32 = 0.05;

/// Knob step per shifted key press, a tenth of [`COARSE_STEP`] for settling on a value.
pub const FINE_STEP: f32 = 0.005;

/// Knob labels and the (raise, lower) keys, in knob order.
const KNOB_KEYS: [(&str, char, char); KNOB_COUNT] = [("cutoff", 'q', 'a'), ("gain", 'w', 's')];

/// Width of the bar in a readout line, in characters.
const BAR_WIDTH: usize = 10;

/// What a key press asks the view to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Publish these knob positions.
    Set(Controls),
    /// Leave the view.
    Quit,
    /// Nothing changes.
    None,
}

/// Translate one key event into an [`Action`] given the current knob positions.
pub fn handle_key(controls: &Controls, key: KeyEvent) -> Action {
    if key.kind == KeyEventKind::Release {
        return Action::None;
    }
    let KeyCode::Char(c) = key.code else {
        return if key.code == KeyCode::Esc {
            Action::Quit
        } else {
            Action::None
        };
    };
    if c == 'c' && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Action::Quit;
    }
    // Terminals report a shifted letter as the uppercase char, with or without the SHIFT flag.
    let fine = key.modifiers.contains(KeyModifiers::SHIFT) || c.is_ascii_uppercase();
    let c = c.to_ascii_lowercase();
    let step = if fine { FINE_STEP } else { COARSE_STEP };
    for (knob, &(_, up, down)) in KNOB_KEYS.iter().enumerate() {
        let delta = if c == up {
            step
        } else if c == down {
            -step
        } else {
            continue;
        };
        let mut next = controls.clone();
        next.knobs[knob] = (controls.knobs[knob] + delta).clamp(0.0, 1.0);
        return Action::Set(next);
    }
    Action::None
}

/// Format a real-unit value: kilo-prefixed when large, with the unit from the parameter table.
fn format_value(value: f32, unit: &str) -> String {
    if value.abs() >= 1000.0 {
        format!("{:.1} k{unit}", value / 1000.0)
    } else {
        format!("{value:.1} {unit}")
    }
}

/// One readout line for `knob`, for example `cutoff [q/a]  ######....  0.62  (2.1 kHz)`.
pub fn readout_line(controls: &Controls, knob: usize) -> String {
    let (label, up, down) = KNOB_KEYS[knob];
    let position = controls.knobs[knob].clamp(0.0, 1.0);
    let filled = (position * BAR_WIDTH as f32).round() as usize;
    let bar: String = (0..BAR_WIDTH)
        .map(|i| if i < filled { '#' } else { '.' })
        .collect();
    let real = format_value(controls.values()[knob], ASPERITAS_PARAMS[knob].unit);
    format!("{label:<6} [{up}/{down}]  {bar}  {position:.2}  ({real})")
}

/// Widest status line, in characters, so it never wraps and breaks the redraw's line count.
const STATUS_WIDTH: usize = 78;

/// The status line: `error` flattened onto one line and cut to [`STATUS_WIDTH`], or empty.
fn status_line(error: Option<&str>) -> String {
    let Some(error) = error else {
        return String::new();
    };
    let flat: String = error
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let line = format!(" {}", flat.trim());
    if line.chars().count() > STATUS_WIDTH {
        let cut: String = line.chars().take(STATUS_WIDTH - 3).collect();
        format!("{cut}...")
    } else {
        line
    }
}

/// The whole view as lines, with no line terminators. The last line is the latest stream error,
/// blank when there is none, so the line count is constant.
pub fn render_view(controls: &Controls, source: &str, error: Option<&str>) -> Vec<String> {
    let mut lines = vec![format!(" asperitas-live  src: {source}"), String::new()];
    lines.extend((0..KNOB_COUNT).map(|k| format!(" {}", readout_line(controls, k))));
    lines.push(String::new());
    lines.push(" shift = fine step   esc = quit".to_string());
    lines.push(status_line(error));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn press(c: char) -> KeyEvent {
        key(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn at(a: f32, b: f32) -> Controls {
        Controls { knobs: [a, b] }
    }

    fn set(action: Action) -> [f32; 2] {
        match action {
            Action::Set(c) => c.knobs,
            other => panic!("expected Set, got {other:?}"),
        }
    }

    #[test]
    fn coarse_keys_move_the_right_knob() {
        let c = at(0.5, 0.5);
        assert_eq!(set(handle_key(&c, press('q'))), [0.5 + COARSE_STEP, 0.5]);
        assert_eq!(set(handle_key(&c, press('a'))), [0.5 - COARSE_STEP, 0.5]);
        assert_eq!(set(handle_key(&c, press('w'))), [0.5, 0.5 + COARSE_STEP]);
        assert_eq!(set(handle_key(&c, press('s'))), [0.5, 0.5 - COARSE_STEP]);
    }

    #[test]
    fn shifted_keys_take_the_fine_step() {
        let c = at(0.5, 0.5);
        let shifted = key(KeyCode::Char('Q'), KeyModifiers::SHIFT);
        assert_eq!(set(handle_key(&c, shifted)), [0.5 + FINE_STEP, 0.5]);
        // Some terminals send the uppercase char alone.
        assert_eq!(set(handle_key(&c, press('S'))), [0.5, 0.5 - FINE_STEP]);
        let flagged = key(KeyCode::Char('w'), KeyModifiers::SHIFT);
        assert_eq!(set(handle_key(&c, flagged)), [0.5, 0.5 + FINE_STEP]);
    }

    #[test]
    fn values_clamp_at_both_ends() {
        assert_eq!(set(handle_key(&at(0.99, 0.0), press('q')))[0], 1.0);
        assert_eq!(set(handle_key(&at(0.0, 0.01), press('s')))[1], 0.0);
        assert_eq!(set(handle_key(&at(0.0, 0.0), press('a'))), [0.0, 0.0]);
    }

    #[test]
    fn twenty_coarse_presses_cross_the_range() {
        let mut c = at(0.0, 0.0);
        for _ in 0..20 {
            c = Controls {
                knobs: set(handle_key(&c, press('q'))),
            };
        }
        assert!((c.knobs[0] - 1.0).abs() < 1e-4, "{:?}", c.knobs);
    }

    #[test]
    fn escape_and_ctrl_c_quit() {
        let c = Controls::default();
        assert_eq!(
            handle_key(&c, key(KeyCode::Esc, KeyModifiers::NONE)),
            Action::Quit
        );
        assert_eq!(
            handle_key(&c, key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Action::Quit
        );
    }

    #[test]
    fn other_keys_and_releases_do_nothing() {
        let c = Controls::default();
        assert_eq!(handle_key(&c, press('x')), Action::None);
        assert_eq!(handle_key(&c, press('c')), Action::None);
        assert_eq!(
            handle_key(&c, key(KeyCode::Enter, KeyModifiers::NONE)),
            Action::None
        );
        let mut release = press('q');
        release.kind = KeyEventKind::Release;
        assert_eq!(handle_key(&c, release), Action::None);
    }

    #[test]
    fn readout_uses_the_parameter_table_for_real_units() {
        let c = at(0.62, 0.5);
        let cutoff = ASPERITAS_PARAMS[0].from_normalised(0.62);
        let gain = ASPERITAS_PARAMS[1].from_normalised(0.5);
        let line = readout_line(&c, 0);
        assert!(
            line.starts_with("cutoff [q/a]  ######....  0.62  ("),
            "{line}"
        );
        assert!(
            line.contains(&format!("{:.1} kHz", cutoff / 1000.0)),
            "{line}"
        );
        let line = readout_line(&c, 1);
        assert!(
            line.starts_with("gain   [w/s]  #####.....  0.50  ("),
            "{line}"
        );
        assert!(line.ends_with(&format!("({gain:.1} dB)")), "{line}");
    }

    #[test]
    fn view_has_header_knobs_and_footer() {
        let lines = render_view(&Controls::default(), "loop a.wav", None);
        assert_eq!(lines[0], " asperitas-live  src: loop a.wav");
        assert!(lines[2].starts_with(" cutoff"));
        assert!(lines[3].starts_with(" gain"));
        assert!(lines[lines.len() - 2].contains("esc = quit"));
    }

    #[test]
    fn error_line_keeps_the_line_count_and_shows_the_error() {
        let c = Controls::default();
        let quiet = render_view(&c, "line-in", None);
        let failed = render_view(&c, "line-in", Some("input stream error: gone"));
        assert_eq!(quiet.len(), failed.len());
        assert!(quiet.last().unwrap().is_empty());
        assert!(failed.last().unwrap().contains("input stream error: gone"));
    }

    #[test]
    fn long_multiline_error_stays_on_one_short_line() {
        let long = format!("first\nsecond\r\n{}", "x".repeat(500));
        let line = status_line(Some(&long));
        assert!(!line.contains(['\n', '\r']));
        assert!(line.chars().count() <= STATUS_WIDTH, "{line}");
        assert!(line.starts_with(" first second"));
    }
}
