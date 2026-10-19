//! The one way a record body is written and the one way it is read.
//!
//! [`BodyWriter`] is a bounded cursor over a caller-supplied buffer: it never allocates, never
//! indexes out of range and never panics, because log and console bodies are built on paths
//! that may not fail. Running out of room is a quiet stop. [`BodyReader`] is its receiving
//! counterpart over a body's fixed-width skeleton, and shares the frame parser's rule for what
//! counts as a valid number.

use core::fmt;

use crate::frame::{parse_decimal, parse_hex, write_decimal, write_hex};

/// Widest numeric field either side handles: a `u32` in decimal.
const MAX_FIELD_DIGITS: usize = 10;

/// A bounded cursor over a body buffer that stops at the end instead of overflowing it.
///
/// Every writer method keeps whatever fits and drops the rest, so a body that ran past its window
/// is a shortened record, which the encoder then reports on the wire as `trunc`. As a
/// [`fmt::Write`] a piece that does not fit at all returns [`fmt::Error`], which aborts the
/// enclosing `write!` and leaves whatever already landed.
pub struct BodyWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> BodyWriter<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Bytes written so far.
    pub fn filled(&self) -> usize {
        self.pos
    }

    /// Append as much of `bytes` as fits.
    pub fn put(&mut self, bytes: &[u8]) {
        let Some(room) = self.buf.get_mut(self.pos..) else {
            return;
        };
        let len = bytes.len().min(room.len());
        if let (Some(dst), Some(src)) = (room.get_mut(..len), bytes.get(..len)) {
            dst.copy_from_slice(src);
            self.pos += len;
        }
    }

    /// Append `value` as exactly `digits` lowercase hex digits.
    ///
    /// The width contract is the caller's: `digits` is at most 10 and `value` fits in it. It is
    /// proven where each width is declared (`check_hex_width` in `frame.rs`, or the field kind's
    /// `WIDTH` in `console.rs`) and asserted here in debug builds. Release builds never panic and
    /// fall back to dropping the high digits, so a violated contract is a malformed body that the
    /// receiver rejects, not a crash on a path that may not fail.
    pub fn hex(&mut self, value: u32, digits: usize) {
        debug_assert!(digits <= MAX_FIELD_DIGITS, "hex width {digits} too wide");
        debug_assert!(
            digits >= 8 || value >> (4 * digits) == 0,
            "{value:#x} does not fit {digits} hex digits"
        );
        let mut scratch = [0u8; MAX_FIELD_DIGITS];
        let digits = digits.min(MAX_FIELD_DIGITS);
        if let Some(field) = scratch.get_mut(..digits) {
            write_hex(value, field);
            self.put(field);
        }
    }

    /// Append `value` as exactly `digits` zero-padded decimal digits.
    ///
    /// Same contract as [`Self::hex`]: `digits` is at most 10 and `value` fits, asserted in debug
    /// builds, with the high digits dropped as the release-build fallback.
    pub fn decimal(&mut self, value: u32, digits: usize) {
        debug_assert!(
            digits <= MAX_FIELD_DIGITS,
            "decimal width {digits} too wide"
        );
        debug_assert!(
            digits >= MAX_FIELD_DIGITS || value < 10u32.pow(digits as u32),
            "{value} does not fit {digits} decimal digits"
        );
        let mut scratch = [0u8; MAX_FIELD_DIGITS];
        let digits = digits.min(MAX_FIELD_DIGITS);
        if let Some(field) = scratch.get_mut(..digits) {
            write_decimal(value, field);
            self.put(field);
        }
    }

    /// The unwritten tail, for an encoder that fills it directly. Follow with [`Self::advance`].
    pub fn rest_mut(&mut self) -> &mut [u8] {
        self.buf.get_mut(self.pos..).unwrap_or_default()
    }

    /// Mark `n` bytes of [`Self::rest_mut`] as written.
    pub fn advance(&mut self, n: usize) {
        self.pos = self.pos.saturating_add(n).min(self.buf.len());
    }
}

impl fmt::Write for BodyWriter<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if s.is_empty() {
            return Ok(());
        }
        let before = self.pos;
        self.put(s.as_bytes());
        if self.pos == before {
            return Err(fmt::Error);
        }
        Ok(())
    }
}

/// Positional reader over a body's fixed-width skeleton.
///
/// On failure `at` stays where it was, which is what lets a caller point at the byte that broke
/// the shape instead of merely reporting that something was wrong.
pub struct BodyReader<'b> {
    body: &'b [u8],
    pub at: usize,
}

impl<'b> BodyReader<'b> {
    pub fn new(body: &'b [u8]) -> Self {
        Self { body, at: 0 }
    }

    /// Consume `template`, or fail without moving.
    pub fn literal(&mut self, template: &[u8]) -> Option<()> {
        let end = self.at.checked_add(template.len())?;
        if self.body.get(self.at..end) == Some(template) {
            self.at = end;
            Some(())
        } else {
            None
        }
    }

    /// Consume exactly `digits` lowercase hex digits. Uppercase is not in the grammar, so the frame
    /// parser's rule is reused rather than restated: two spellings of “valid” is one too many.
    pub fn hex(&mut self, digits: usize) -> Option<u32> {
        let end = self.at.checked_add(digits)?;
        let value = parse_hex(self.body.get(self.at..end)?)?;
        self.at = end;
        Some(value)
    }

    pub fn decimal(&mut self, digits: usize) -> Option<u32> {
        let end = self.at.checked_add(digits)?;
        let value = parse_decimal(self.body.get(self.at..end)?)?;
        self.at = end;
        Some(value)
    }

    /// Consume `label` then exactly `digits` lowercase hex digits; on failure nothing moves.
    pub fn labelled_hex(&mut self, label: &[u8], digits: usize) -> Option<u32> {
        let start = self.at;
        let value = self.literal(label).and_then(|()| self.hex(digits));
        if value.is_none() {
            self.at = start;
        }
        value
    }

    /// True once every byte has been consumed.
    pub fn is_empty(&self) -> bool {
        self.at >= self.body.len()
    }

    /// Everything from here to the end of the body.
    pub fn rest(&self) -> &'b [u8] {
        self.body.get(self.at..).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write;

    #[test]
    fn writer_keeps_what_fits_and_never_overruns() {
        let mut buf = [0u8; 6];
        let mut w = BodyWriter::new(&mut buf);
        w.put(b"ab");
        w.hex(0xbeef, 4);
        w.put(b"zzz");
        w.hex(1, 2);
        assert_eq!(w.filled(), 6);
        assert_eq!(&buf, b"abbeef");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "does not fit")]
    fn hex_asserts_a_value_wider_than_its_field() {
        BodyWriter::new(&mut [0u8; 8]).hex(0x1_0000, 4);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "does not fit")]
    fn decimal_asserts_a_value_wider_than_its_field() {
        BodyWriter::new(&mut [0u8; 8]).decimal(1_000_000, 6);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "too wide")]
    fn hex_asserts_a_width_past_the_widest_field() {
        BodyWriter::new(&mut [0u8; 16]).hex(0, 11);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "too wide")]
    fn decimal_asserts_a_width_past_the_widest_field() {
        BodyWriter::new(&mut [0u8; 16]).decimal(0, 11);
    }

    #[test]
    fn writer_fields_are_fixed_width() {
        let mut buf = [0u8; 12];
        let mut w = BodyWriter::new(&mut buf);
        w.hex(0xab, 4);
        w.decimal(42, 6);
        let filled = w.filled();
        assert_eq!(&buf[..filled], b"00ab000042");
    }

    #[test]
    fn fmt_write_errors_only_when_nothing_fits() {
        let mut buf = [0u8; 3];
        let mut w = BodyWriter::new(&mut buf);
        assert!(w.write_str("ab").is_ok());
        assert!(w.write_str("cd").is_ok());
        assert!(w.write_str("e").is_err());
        assert_eq!(&buf, b"abc");
    }

    #[test]
    fn rest_and_advance_let_an_encoder_fill_the_tail() {
        let mut buf = [0u8; 8];
        let mut w = BodyWriter::new(&mut buf);
        w.put(b"x=");
        w.rest_mut()[..3].copy_from_slice(b"abc");
        w.advance(3);
        w.advance(100);
        let filled = w.filled();
        assert_eq!(filled, 8);
        assert_eq!(&buf[..5], b"x=abc");
    }

    #[test]
    fn labelled_hex_moves_only_on_success() {
        let mut r = BodyReader::new(b" k=0a v=00ff");
        assert_eq!(r.labelled_hex(b" k=", 2), Some(0x0a));
        assert_eq!(r.at, 5);
        assert_eq!(r.labelled_hex(b" v=", 5), None);
        assert_eq!(r.at, 5);
        assert_eq!(r.labelled_hex(b" v=", 4), Some(0xff));
        assert!(r.is_empty());
    }
}
