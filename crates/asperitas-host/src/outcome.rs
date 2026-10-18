//! The process exit status every Asperitas host command uses.

/// Process exit status. A failed verdict is kept distinct from a usage, device or I/O error so a
/// script can tell "the capture is untrustworthy" from "the capture did not happen".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The run passed.
    Pass,
    /// The run completed but the verdict was fail.
    Fail,
    /// Usage, load, device or I/O error.
    Error,
}

impl Outcome {
    /// Numeric process exit code: 0 pass, 1 failed verdict, 2 error.
    pub fn code(self) -> u8 {
        match self {
            Outcome::Pass => 0,
            Outcome::Fail => 1,
            Outcome::Error => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_0_1_2() {
        assert_eq!(Outcome::Pass.code(), 0);
        assert_eq!(Outcome::Fail.code(), 1);
        assert_eq!(Outcome::Error.code(), 2);
    }
}
