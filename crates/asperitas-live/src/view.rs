//! The pure logic of the terminal knob view: key presses to actions and knob readout text.
//!
//! Nothing here touches the terminal, so it is unit-tested. Real units come from the effect's
//! parameter table through [`ViewSurface::real_values`]; this module holds no taper or range.

use asperitas_dsp::asperitas::Controls;
use asperitas_dsp::automation::ControlSurface;
use asperitas_dsp::excitation::EXCITATION_PRESETS;
use asperitas_dsp::resonator_controls::ResonatorControls;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Knob step per key press. 0.05 makes 20 presses from end to end, coarse enough to sweep a
/// knob quickly and fine enough to hear each step.
pub const COARSE_STEP: f32 = 0.05;

/// Knob step per shifted key press, a tenth of [`COARSE_STEP`] for settling on a value.
pub const FINE_STEP: f32 = 0.005;

/// Width of the bar in a readout line, in characters.
const BAR_WIDTH: usize = 10;

/// A control surface the knob view can show: its labels, keys and real-unit readout.
pub trait ViewSurface: ControlSurface {
    /// Knob label and the (raise, lower) keys, in knob order.
    const KEYS: &'static [(&'static str, char, char)];

    /// The real-unit value of every knob, in table order.
    fn real_values(&self) -> Vec<f32>;

    /// Readout text for the real-unit `value` of knob `index`.
    fn format_real(index: usize, value: f32) -> String {
        format_value(value, Self::PARAMS[index].unit)
    }
}

impl ViewSurface for Controls {
    const KEYS: &'static [(&'static str, char, char)] = &[("cutoff", 'q', 'a'), ("gain", 'w', 's')];

    fn real_values(&self) -> Vec<f32> {
        self.values().to_vec()
    }
}

impl ViewSurface for ResonatorControls {
    const KEYS: &'static [(&'static str, char, char)] = &[
        ("root", 'q', 'a'),
        ("structure", 'w', 's'),
        ("damping", 'e', 'd'),
        ("spread", 'r', 'f'),
        ("preset", 't', 'g'),
    ];

    fn real_values(&self) -> Vec<f32> {
        self.values().to_vec()
    }

    fn format_real(index: usize, value: f32) -> String {
        if Self::is_discrete(index) {
            format!("{} of {}", value as usize, EXCITATION_PRESETS.len())
        } else {
            format_value(value, Self::PARAMS[index].unit)
        }
    }
}

/// What a key press asks the view to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action<C> {
    /// Publish these knob positions.
    Set(C),
    /// Leave the view.
    Quit,
    /// Nothing changes.
    None,
}

/// Translate one key event into an [`Action`] given the current knob positions. A continuous
/// knob moves by [`COARSE_STEP`] (or [`FINE_STEP`] when shifted) and clamps to `[0, 1]`; a
/// discrete knob moves one setting per press whatever the step.
pub fn handle_key<C: ViewSurface>(controls: &C, key: KeyEvent) -> Action<C> {
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
    for (knob, &(_, up, down)) in C::KEYS.iter().enumerate() {
        let direction = if c == up {
            1.0
        } else if c == down {
            -1.0
        } else {
            continue;
        };
        let mut next = controls.clone();
        let spec = &C::PARAMS[knob];
        let position = &mut next.positions_mut()[knob];
        if C::is_discrete(knob) {
            let setting = spec.from_normalised(*position).round() + direction;
            *position = spec.to_normalised(setting.clamp(spec.min, spec.max));
        } else {
            *position = (*position + direction * step).clamp(0.0, 1.0);
        }
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
pub fn readout_line<C: ViewSurface>(controls: &C, knob: usize) -> String {
    let (label, up, down) = C::KEYS[knob];
    let position = controls.positions()[knob].clamp(0.0, 1.0);
    let filled = (position * BAR_WIDTH as f32).round() as usize;
    let bar: String = (0..BAR_WIDTH)
        .map(|i| if i < filled { '#' } else { '.' })
        .collect();
    let real = C::format_real(knob, controls.real_values()[knob]);
    format!("{label:<9} [{up}/{down}]  {bar}  {position:.2}  ({real})")
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
pub fn render_view<C: ViewSurface>(controls: &C, source: &str, error: Option<&str>) -> Vec<String> {
    let mut lines = vec![format!(" asperitas-live  src: {source}"), String::new()];
    lines.extend((0..C::KEYS.len()).map(|k| format!(" {}", readout_line(controls, k))));
    lines.push(String::new());
    lines.push(" shift = fine step   esc = quit".to_string());
    lines.push(status_line(error));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use asperitas_dsp::asperitas::ASPERITAS_PARAMS;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn press(c: char) -> KeyEvent {
        key(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn at(a: f32, b: f32) -> Controls {
        Controls { knobs: [a, b] }
    }

    fn set(action: Action<Controls>) -> [f32; 2] {
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
            line.starts_with("cutoff    [q/a]  ######....  0.62  ("),
            "{line}"
        );
        assert!(
            line.contains(&format!("{:.1} kHz", cutoff / 1000.0)),
            "{line}"
        );
        let line = readout_line(&c, 1);
        assert!(
            line.starts_with("gain      [w/s]  #####.....  0.50  ("),
            "{line}"
        );
        assert!(line.ends_with(&format!("({gain:.1} dB)")), "{line}");
    }

    /// The first five positions (the ones the view drives).
    fn rset(action: Action<ResonatorControls>) -> [f32; 5] {
        match action {
            Action::Set(c) => c.positions[..5].try_into().unwrap(),
            other => panic!("expected Set, got {other:?}"),
        }
    }

    /// Controls with the first five positions set and the rest at their defaults.
    fn rat(positions: [f32; 5]) -> ResonatorControls {
        let mut c = ResonatorControls::default();
        c.positions[..5].copy_from_slice(&positions);
        c
    }

    #[test]
    fn resonator_keys_move_the_right_control() {
        let c = rat([0.5; 5]);
        for (i, (up, down)) in [('q', 'a'), ('w', 's'), ('e', 'd'), ('r', 'f')]
            .into_iter()
            .enumerate()
        {
            let mut raised = [0.5; 5];
            raised[i] += COARSE_STEP;
            assert_eq!(rset(handle_key(&c, press(up))), raised, "{up}");
            let mut lowered = [0.5; 5];
            lowered[i] -= COARSE_STEP;
            assert_eq!(rset(handle_key(&c, press(down))), lowered, "{down}");
        }
        let fine = key(KeyCode::Char('E'), KeyModifiers::SHIFT);
        assert_eq!(
            rset(handle_key(&c, fine)),
            [0.5, 0.5, 0.5 + FINE_STEP, 0.5, 0.5]
        );
    }

    #[test]
    fn resonator_values_clamp_at_both_ends() {
        assert_eq!(rset(handle_key(&rat([0.99; 5]), press('q')))[0], 1.0);
        assert_eq!(rset(handle_key(&rat([0.0; 5]), press('a')))[0], 0.0);
    }

    #[test]
    fn preset_moves_one_setting_per_press_fine_or_coarse() {
        let n = EXCITATION_PRESETS.len();
        let position = |p: usize| (p - 1) as f32 / (n - 1) as f32;
        let at_preset = |p: usize| rat([0.5, 0.5, 0.5, 0.5, position(p)]);
        for press_key in [press('t'), key(KeyCode::Char('T'), KeyModifiers::SHIFT)] {
            for p in 1..n {
                let next = rset(handle_key(&at_preset(p), press_key))[4];
                assert!((next - position(p + 1)).abs() < 1e-6, "{p}: {next}");
            }
            assert_eq!(rset(handle_key(&at_preset(n), press_key))[4], 1.0);
        }
        for p in 2..=n {
            let next = rset(handle_key(&at_preset(p), press('g')))[4];
            assert!((next - position(p - 1)).abs() < 1e-6, "{p}: {next}");
        }
        assert_eq!(rset(handle_key(&at_preset(1), press('g')))[4], 0.0);
    }

    #[test]
    fn resonator_readout_shows_normalised_and_real_units() {
        let c = rat([0.5, 0.25, 0.75, 1.0, 0.5]);
        let v = c.values();
        let line = readout_line(&c, 0);
        assert!(
            line.starts_with("root      [q/a]  #####.....  0.50  ("),
            "{line}"
        );
        assert!(line.ends_with(&format!("({:.1} Hz)", v[0])), "{line}");
        let line = readout_line(&c, 3);
        assert!(line.contains("##########  1.00"), "{line}");
        let line = readout_line(&c, 4);
        assert!(line.starts_with("preset    [t/g]"), "{line}");
        assert!(line.ends_with("(2 of 3)"), "{line}");
        let view = render_view(&c, "loop a.wav", None);
        assert_eq!(view.len(), 2 + 5 + 3);
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
