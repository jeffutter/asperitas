//! Knob names and real-unit values for the rig's control paths.
//!
//! The wire carries a knob id and a normalised level. A person drives a stage by the parameter's
//! name (`root_hz`, `damping`, `cutoff_hz`) and its real unit, exactly as `asperitas-cli process
//! --params` and an automation file do, so this module is the one place the rig turns those into
//! an id and a level. The name tables and the unit translation are `asperitas-dsp`'s
//! ([`ControlSurface`]); nothing here duplicates a range or a taper.
//!
//! Wire codes of the stages are `asperitas_stimulus::replay_fx::FxKind::from_code`'s: 0 bypass,
//! 1 patch, 2 resonator. Every refusal is a [`ControlError::Usage`] that names the key or id, so
//! a bad value never reaches the port.

use asperitas_dsp::asperitas::Controls;
use asperitas_dsp::automation::ControlSurface;
use asperitas_dsp::resonator_controls::ResonatorControls;

use crate::control::ControlError;

/// Stage code of the patch.
pub const FX_PATCH: u8 = 1;
/// Stage code of the resonator.
pub const FX_RESONATOR: u8 = 2;

/// Index of `key` in `C`'s table.
fn index_in<C: ControlSurface>(key: &str) -> Option<usize> {
    C::PARAMS.iter().position(|spec| spec.name == key)
}

/// The knob id `key` names, and the code of the stage whose knob it is.
fn lookup(key: &str) -> Option<(u8, u8)> {
    if let Some(i) = index_in::<Controls>(key) {
        return Some((i as u8, FX_PATCH));
    }
    index_in::<ResonatorControls>(key).map(|i| (i as u8, FX_RESONATOR))
}

fn key_names<C: ControlSurface>() -> String {
    C::PARAMS
        .iter()
        .map(|s| s.name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// How many knob ids stage `fx_f` has; `None` for bypass or a code the rig does not know.
pub fn knob_count(fx_f: u8) -> Option<usize> {
    match fx_f {
        FX_PATCH => Some(Controls::PARAMS.len()),
        FX_RESONATOR => Some(ResonatorControls::PARAMS.len()),
        _ => None,
    }
}

/// Refuse a knob id the selected stage does not have. Bypass and unknown stage codes are left to
/// the device, as before.
pub fn check_knob_for_stage(fx_f: u8, knob: u8) -> Result<(), ControlError> {
    match knob_count(fx_f) {
        Some(n) if usize::from(knob) >= n => Err(ControlError::Usage(format!(
            "knob {knob} does not exist on effect {fx_f} (ids 0..={})",
            n - 1
        ))),
        _ => Ok(()),
    }
}

/// Resolve `--knob`: a decimal id, or a parameter name. With `fx_f` the name must belong to that
/// stage; without one (a bare override, which selects no stage) a name of either stage is taken.
pub fn resolve_knob(arg: &str, fx_f: Option<u8>) -> Result<u8, ControlError> {
    if let Ok(id) = arg.parse::<u8>() {
        return Ok(id);
    }
    let Some((id, stage)) = lookup(arg) else {
        return Err(ControlError::Usage(format!(
            "unknown knob '{arg}': use an id, a patch key ({}) or a resonator key ({})",
            key_names::<Controls>(),
            key_names::<ResonatorControls>()
        )));
    };
    match fx_f {
        Some(f) if f != stage => Err(ControlError::Usage(format!(
            "knob '{arg}' belongs to effect {stage}, but this run selects effect {f}"
        ))),
        _ => Ok(id),
    }
}

/// The knob id `key` names and the normalised level that carries the real-unit `value` on the
/// wire. A value outside the parameter's range, or an unknown key, is refused with the
/// translation's own message, never clamped.
pub fn real_to_level(key: &str, value: f64) -> Result<(u8, f64), ControlError> {
    let (id, stage) = lookup(key).ok_or_else(|| {
        ControlError::Usage(format!(
            "unknown knob '{key}': use a patch key ({}) or a resonator key ({})",
            key_names::<Controls>(),
            key_names::<ResonatorControls>()
        ))
    })?;
    let pairs = [(key, value as f32)];
    let position = match stage {
        FX_PATCH => Controls::from_real_values(&pairs).map(|c| c.positions()[usize::from(id)]),
        _ => ResonatorControls::from_real_values(&pairs).map(|c| c.positions()[usize::from(id)]),
    }
    .map_err(ControlError::Usage)?;
    Ok((id, f64::from(position)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(e: ControlError) -> String {
        match e {
            ControlError::Usage(s) => s,
            other => panic!("not a usage error: {other:?}"),
        }
    }

    #[test]
    fn ids_and_names_resolve_to_the_same_knob() {
        assert_eq!(resolve_knob("3", Some(FX_RESONATOR)).unwrap(), 3);
        assert_eq!(resolve_knob("spread", Some(FX_RESONATOR)).unwrap(), 3);
        assert_eq!(resolve_knob("root_hz", None).unwrap(), 0);
        assert_eq!(resolve_knob("gain_db", Some(FX_PATCH)).unwrap(), 1);
    }

    #[test]
    fn a_name_from_the_other_stage_or_no_stage_is_refused_naming_it() {
        let e = usage(resolve_knob("root_hz", Some(FX_PATCH)).unwrap_err());
        assert!(e.contains("root_hz"), "{e}");
        let e = usage(resolve_knob("bogus", None).unwrap_err());
        assert!(e.contains("bogus") && e.contains("damping"), "{e}");
    }

    #[test]
    fn knob_ids_past_the_stage_are_refused() {
        assert!(check_knob_for_stage(FX_PATCH, 1).is_ok());
        let e = usage(check_knob_for_stage(FX_PATCH, 2).unwrap_err());
        assert!(e.contains("knob 2"), "{e}");
        assert!(check_knob_for_stage(FX_RESONATOR, 12).is_ok());
        assert!(check_knob_for_stage(FX_RESONATOR, 13).is_err());
        assert!(check_knob_for_stage(0, 200).is_ok());
    }

    #[test]
    fn real_values_map_through_the_dsp_tables() {
        let (id, level) = real_to_level("damping", 0.5).unwrap();
        let want = ResonatorControls::from_values(&[("damping", 0.5)])
            .unwrap()
            .positions[2];
        assert_eq!((id, level), (2, f64::from(want)));
        let (id, level) = real_to_level("gain_db", -6.0).unwrap();
        let want = Controls::from_values(&[("gain_db", -6.0)]).unwrap().knobs[1];
        assert_eq!((id, level), (1, f64::from(want)));
    }

    #[test]
    fn out_of_range_and_unknown_real_values_name_the_key() {
        let e = usage(real_to_level("damping", 3.0).unwrap_err());
        assert!(e.contains("damping"), "{e}");
        let e = usage(real_to_level("excitation_preset", 1.5).unwrap_err());
        assert!(e.contains("excitation_preset"), "{e}");
        let e = usage(real_to_level("nope", 1.0).unwrap_err());
        assert!(e.contains("nope"), "{e}");
    }
}
