//! Scripted knob motion for the offline render. The grammar is in
//! `docs/reference/automation.md`.
//!
//! The format is one event per line, split on whitespace. No parsing library is used: nothing in
//! this crate's dependencies expresses a breakpoint list as `time [step|ramp] key=value ...`, and
//! the grammar is four token kinds.

use asperitas_dsp::asperitas::{Asperitas, Controls, ASPERITAS_PARAMS, BLOCK_FRAMES, KNOB_COUNT};
use asperitas_dsp::processor::{Frame, Processor};

/// One knob reaching `position` (normalised knob travel) at `frame`.
#[derive(Clone, Copy, Debug)]
struct Breakpoint {
    frame: u64,
    position: f32,
    /// Arrive by moving linearly from the previous breakpoint; otherwise jump at `frame`.
    ramp: bool,
}

/// A parsed automation file: per knob, breakpoints sorted by frame.
#[derive(Clone, Debug)]
pub struct Automation {
    knobs: [Vec<Breakpoint>; KNOB_COUNT],
}

impl Automation {
    /// Parse `text`, converting each time in seconds to the nearest frame at `sample_rate`.
    /// Every problem is an error naming the line; nothing is clamped or skipped.
    pub fn parse(text: &str, sample_rate: u32) -> Result<Self, String> {
        let mut knobs: [Vec<Breakpoint>; KNOB_COUNT] = std::array::from_fn(|_| Vec::new());
        let mut last_seconds = 0.0f64;
        for (idx, raw) in text.lines().enumerate() {
            let n = idx + 1;
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut tokens = line.split_whitespace().peekable();
            let time = tokens.next().unwrap_or_default();
            let seconds: f64 = time
                .parse()
                .map_err(|_| format!("line {n}: '{time}' is not a time in seconds"))?;
            if !seconds.is_finite() || seconds < 0.0 {
                return Err(format!(
                    "line {n}: time {time} must be a finite, non-negative number of seconds"
                ));
            }
            if seconds < last_seconds {
                return Err(format!(
                    "line {n}: time {time} is earlier than the previous line ({last_seconds}); times must not decrease"
                ));
            }
            last_seconds = seconds;
            let frame = (seconds * f64::from(sample_rate)).round() as u64;

            let ramp = match tokens.peek() {
                Some(&"step") => {
                    tokens.next();
                    false
                }
                Some(&"ramp") => {
                    tokens.next();
                    true
                }
                _ => false,
            };

            let mut values: Vec<(&str, f32)> = Vec::new();
            for token in tokens {
                let (key, value) = token
                    .split_once('=')
                    .filter(|(k, v)| !k.is_empty() && !v.is_empty())
                    .ok_or_else(|| {
                        format!(
                            "line {n}: '{token}' is not key=value (or step/ramp after the time)"
                        )
                    })?;
                let value: f32 = value
                    .parse()
                    .map_err(|_| format!("line {n}: invalid value for {key}: {value}"))?;
                if values.iter().any(|(k, _)| *k == key) {
                    return Err(format!("line {n}: {key} is set twice on one line"));
                }
                values.push((key, value));
            }
            if values.is_empty() {
                return Err(format!("line {n}: no key=value after the time"));
            }
            let controls = Controls::from_values(&values).map_err(|e| format!("line {n}: {e}"))?;
            for (key, _) in &values {
                let knob = ASPERITAS_PARAMS
                    .iter()
                    .position(|spec| spec.name == *key)
                    .expect("from_values accepted the key");
                knobs[knob].push(Breakpoint {
                    frame,
                    position: controls.knobs[knob],
                    ramp,
                });
            }
        }
        Ok(Self { knobs })
    }

    /// The controls in force at exactly `frame`, counted from the start of the clip. A knob holds
    /// its default until first named, holds its last value after its last breakpoint, and
    /// interpolates in knob position (what turning the knob does) toward a `ramp` breakpoint.
    pub fn controls_at(&self, frame: u64) -> Controls {
        let mut controls = Controls::default();
        for (knob, points) in self.knobs.iter().enumerate() {
            let next_idx = points.partition_point(|p| p.frame <= frame);
            let prev = match next_idx.checked_sub(1) {
                Some(i) => points[i],
                None => Breakpoint {
                    frame: 0,
                    position: controls.knobs[knob],
                    ramp: false,
                },
            };
            controls.knobs[knob] = match points.get(next_idx) {
                Some(next) if next.ramp => {
                    // next.frame > frame >= prev.frame, so the span is non-zero.
                    let t = (frame - prev.frame) as f32 / (next.frame - prev.frame) as f32;
                    prev.position + (next.position - prev.position) * t
                }
                _ => prev.position,
            };
        }
        controls
    }

    /// Process `frames` in place in blocks of [`BLOCK_FRAMES`] counted from the start of the clip,
    /// applying the controls at each block's first frame. A breakpoint between two block starts
    /// therefore takes effect at the next block start, as on the device. The effect should have
    /// been built with `controls_at(0)`.
    pub fn render(&self, effect: &mut Asperitas, frames: &mut [Frame]) {
        for (block, chunk) in frames.chunks_mut(BLOCK_FRAMES).enumerate() {
            effect.set_controls(&self.controls_at((block * BLOCK_FRAMES) as u64));
            effect.process_block(chunk);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48_000;

    fn secs(frame: u64) -> String {
        format!("{}", frame as f64 / f64::from(SR))
    }

    fn at(a: &Automation, frame: u64) -> [f32; KNOB_COUNT] {
        a.controls_at(frame).values()
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-3 * b.abs().max(1.0)
    }

    #[test]
    fn step_jumps_at_its_frame_and_holds_after() {
        let a = Automation::parse(&format!("{} gain_db=-12\n", secs(480)), SR).unwrap();
        assert!(near(at(&a, 479)[1], 0.0));
        assert!(near(at(&a, 480)[1], -12.0));
        assert!(near(at(&a, 481)[1], -12.0));
        assert!(near(at(&a, 1_000_000)[1], -12.0));
    }

    #[test]
    fn hold_keeps_a_value_until_the_knob_is_named_again() {
        let text = format!(
            "0 cutoff_hz=1000\n{} gain_db=-6\n{} gain_db=-3\n",
            secs(480),
            secs(960)
        );
        let a = Automation::parse(&text, SR).unwrap();
        for f in [0, 479, 480, 700, 959, 960, 5000] {
            assert!(near(at(&a, f)[0], 1000.0), "cutoff at {f}");
        }
        assert!(near(at(&a, 959)[1], -6.0));
        assert!(near(at(&a, 960)[1], -3.0));
    }

    #[test]
    fn ramp_is_linear_in_knob_position_between_breakpoints() {
        let text = format!("0 step gain_db=-20\n{} ramp gain_db=-10\n", secs(1000));
        let a = Automation::parse(&text, SR).unwrap();
        assert!(near(at(&a, 0)[1], -20.0));
        assert!(near(at(&a, 999)[1], -10.0 - 10.0 * 1.0 / 1000.0 * 1.0));
        assert!(near(at(&a, 1000)[1], -10.0));
        assert!(near(at(&a, 2000)[1], -10.0));
        // Gain is a linear taper, so the midpoint is the arithmetic mean.
        assert!(near(at(&a, 500)[1], -15.0));
        // Cutoff is a log taper: the midpoint of the knob travel is the geometric mean.
        let text = format!("0 cutoff_hz=100\n{} ramp cutoff_hz=10000\n", secs(1000));
        let a = Automation::parse(&text, SR).unwrap();
        assert!(near(at(&a, 500)[0], 1000.0), "{}", at(&a, 500)[0]);
    }

    #[test]
    fn ramp_with_no_earlier_breakpoint_starts_from_the_default() {
        let a = Automation::parse(&format!("{} ramp gain_db=-12\n", secs(1000)), SR).unwrap();
        assert!(near(at(&a, 0)[1], 0.0));
        assert!(near(at(&a, 500)[1], -6.0));
        assert!(near(at(&a, 1000)[1], -12.0));
    }

    fn err(text: &str) -> String {
        Automation::parse(text, SR).unwrap_err()
    }

    #[test]
    fn refusals_name_the_line_and_the_problem() {
        let e = err("0 gain_db=0\n1 bogus=3\n");
        assert!(e.contains("line 2") && e.contains("bogus"), "{e}");
        let e = err("# c\n\n1 gain_db=20\n");
        assert!(e.contains("line 3") && e.contains("out of range"), "{e}");
        let e = err("2 gain_db=0\n1 gain_db=-1\n");
        assert!(e.contains("line 2") && e.contains("decrease"), "{e}");
        let e = err("0 gain_db\n");
        assert!(e.contains("line 1") && e.contains("key=value"), "{e}");
        let e = err("soon gain_db=0\n");
        assert!(e.contains("line 1") && e.contains("soon"), "{e}");
        let e = err("-1 gain_db=0\n");
        assert!(e.contains("line 1"), "{e}");
        let e = err("1\n");
        assert!(e.contains("line 1"), "{e}");
        let e = err("1 gain_db=0 gain_db=-1\n");
        assert!(e.contains("line 1") && e.contains("twice"), "{e}");
        let e = err("1 gain_db=abc\n");
        assert!(e.contains("line 1") && e.contains("gain_db"), "{e}");
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let a = Automation::parse("# hi\n\n0 gain_db=-3 # trailing\n", SR).unwrap();
        assert!(near(at(&a, 0)[1], -3.0));
    }

    fn input() -> Vec<Frame> {
        (0..2000)
            .map(|i| {
                let v = ((i % 200) as f32 - 100.0) / 100.0;
                [v, -v]
            })
            .collect()
    }

    fn render_with(a: &Automation) -> Vec<Frame> {
        let mut frames = input();
        let mut fx = Asperitas::new(SR as f32, &a.controls_at(0));
        a.render(&mut fx, &mut frames);
        frames
    }

    #[test]
    fn a_breakpoint_between_block_starts_lands_on_the_next_block_start() {
        // Frame 40 is inside the block that starts at 32; the change lands at 64.
        let off_grid = Automation::parse(&format!("{} cutoff_hz=500\n", secs(40)), SR).unwrap();
        let on_grid = Automation::parse(&format!("{} cutoff_hz=500\n", secs(64)), SR).unwrap();
        assert_eq!(render_with(&off_grid), render_with(&on_grid));

        // Negative control: applying the same change at exactly frame 40 sounds different.
        let c0 = off_grid.controls_at(0);
        let c1 = off_grid.controls_at(40);
        let mut exact = input();
        let mut fx = Asperitas::new(SR as f32, &c0);
        for (start, end) in [(0usize, 32usize), (32, 40), (40, 64)] {
            fx.set_controls(if start >= 40 { &c1 } else { &c0 });
            fx.process_block(&mut exact[start..end]);
        }
        fx.set_controls(&c1);
        fx.process_block(&mut exact[64..]);
        assert_ne!(exact, render_with(&on_grid));
    }
}
