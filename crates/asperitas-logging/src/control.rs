//! Host control protocol: the verbs a host sends the device, the replies it gets back, and the
//! dispatcher that tells a control record from an excerpt-install record.
//!
//! Control rides the same ordinary v1 `~` frames as excerpt install ([`crate::inbound`]), so
//! nothing about framing, CRC or backpressure is restated here. This module owns only the body
//! grammar and the routing decision. It touches no hardware and holds no knob state: the knob
//! table and range checks belong to the layer that applies a value (TASK-032.02), and the
//! firmware wiring is TASK-032.03.
//!
//! # Why this module is not behind `log-usb`
//!
//! Same reason as [`crate::frame`] and [`crate::excerpt`]: pure byte arithmetic, so the default
//! feature `cargo test --workspace` proves the grammar and the router on the host.
//!
//! # Grammar (normative)
//!
//! Host to device, every body starts with `CTL ` (note the space; `EXC` is the excerpt prefix):
//!
//! ```text
//! CTL VER                      report firmware version
//! CTL RSN                      report the last reset reason
//! CTL RST                      restart the device
//! CTL SET k=<2 hex> v=<4 hex>  override knob k with value v (0x0000..=0xFFFF spans 0.0..=1.0)
//! CTL REL k=<2 hex>            release the override on knob k
//! ```
//!
//! Device to host:
//!
//! ```text
//! CTLR VER <text>              1..=32 printable ASCII characters, no space
//! CTLR RSN <token>             1..=16 characters from [a-z0-9_-]
//! CTLR OK <VER|RSN|RST|SET|REL>   the verb was accepted
//! CTLR ERR <code>              unknown-verb | malformed | bad-knob | bad-value
//! ```
//!
//! Hex is fixed width and lowercase, like the rest of the wire formats. Parsing is strict:
//! no trailing bytes, no extra spaces, no uppercase, no other widths. The knob id is an opaque
//! `u8` here; whether it names a real knob is `bad-knob`, decided where the table lives.
//!
//! # Rejection never changes state
//!
//! [`route`] hands the control consumer only a fully parsed [`Command`]. Anything else bumps a
//! [`ControlStats`] counter, is dropped, and yields a [`Reply::Error`] the caller may send back.

use core::sync::atomic::{AtomicU32, Ordering};

use crate::frame::MAX_BODY;
use crate::inbound::InboundRecord;

/// Prefix of every host-to-device control body.
pub const CMD_PREFIX: &[u8] = b"CTL ";
/// Prefix of every device-to-host reply body.
pub const REPLY_PREFIX: &[u8] = b"CTLR ";
/// Prefix shared by every excerpt-install body (`EXCSTART`, `EXCDATA`, `EXCEND`).
pub const EXCERPT_PREFIX: &[u8] = b"EXC";

/// Longest version text in a reply.
pub const VERSION_MAX: usize = 32;
/// Longest reset-reason token in a reply.
pub const REASON_MAX: usize = 16;

/// A body buffer, sized to the frame body budget.
pub type BodyBuf = [u8; MAX_BODY];

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Why a body did not parse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlError {
    /// The body does not carry the prefix this parser handles.
    NotControl,
    /// Right prefix, but the verb is not one this build knows.
    UnknownVerb,
    /// A known verb with a bad body; `at` is the offset where parsing gave up.
    Malformed { at: usize },
}

impl ControlError {
    /// The wire error code a device should answer with.
    pub fn code(self) -> ErrorCode {
        match self {
            ControlError::UnknownVerb | ControlError::NotControl => ErrorCode::UnknownVerb,
            ControlError::Malformed { .. } => ErrorCode::Malformed,
        }
    }
}

/// Error codes carried by `CTLR ERR`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    UnknownVerb,
    Malformed,
    /// Knob id not in the device's table. Produced by the layer that owns the table.
    BadKnob,
    /// Value not acceptable for the knob. Produced by the layer that applies it.
    BadValue,
}

impl ErrorCode {
    const ALL: [ErrorCode; 4] = [
        ErrorCode::UnknownVerb,
        ErrorCode::Malformed,
        ErrorCode::BadKnob,
        ErrorCode::BadValue,
    ];

    fn token(self) -> &'static [u8] {
        match self {
            ErrorCode::UnknownVerb => b"unknown-verb",
            ErrorCode::Malformed => b"malformed",
            ErrorCode::BadKnob => b"bad-knob",
            ErrorCode::BadValue => b"bad-value",
        }
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// A host-to-device control command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    GetVersion,
    GetResetReason,
    Restart,
    OverrideSet { knob: u8, value: u16 },
    OverrideRelease { knob: u8 },
}

/// The verb of a [`Command`], also what `CTLR OK` echoes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Version,
    ResetReason,
    Restart,
    Set,
    Release,
}

impl Verb {
    const ALL: [Verb; 5] = [
        Verb::Version,
        Verb::ResetReason,
        Verb::Restart,
        Verb::Set,
        Verb::Release,
    ];

    fn token(self) -> &'static [u8] {
        match self {
            Verb::Version => b"VER",
            Verb::ResetReason => b"RSN",
            Verb::Restart => b"RST",
            Verb::Set => b"SET",
            Verb::Release => b"REL",
        }
    }
}

impl Command {
    /// The verb this command carries.
    pub fn verb(&self) -> Verb {
        match self {
            Command::GetVersion => Verb::Version,
            Command::GetResetReason => Verb::ResetReason,
            Command::Restart => Verb::Restart,
            Command::OverrideSet { .. } => Verb::Set,
            Command::OverrideRelease { .. } => Verb::Release,
        }
    }

    /// Write the body into `dst`, returning its length.
    pub fn encode(&self, dst: &mut BodyBuf) -> usize {
        let mut w = Writer::new(dst);
        w.put(CMD_PREFIX);
        w.put(self.verb().token());
        match *self {
            Command::OverrideSet { knob, value } => {
                w.put(b" k=");
                w.hex(u32::from(knob), 2);
                w.put(b" v=");
                w.hex(u32::from(value), 4);
            }
            Command::OverrideRelease { knob } => {
                w.put(b" k=");
                w.hex(u32::from(knob), 2);
            }
            _ => {}
        }
        w.len
    }
}

/// Parse a host-to-device control body strictly.
pub fn parse_command(body: &[u8]) -> Result<Command, ControlError> {
    let rest = body
        .strip_prefix(CMD_PREFIX)
        .ok_or(ControlError::NotControl)?;
    let at = CMD_PREFIX.len();
    let verb_len = rest.iter().position(|&b| b == b' ').unwrap_or(rest.len());
    let (verb, args) = rest.split_at(verb_len);
    let verb = Verb::ALL
        .into_iter()
        .find(|v| v.token() == verb)
        .ok_or(ControlError::UnknownVerb)?;
    let mut r = Reader::new(args, at + verb_len);
    let cmd = match verb {
        Verb::Version => Command::GetVersion,
        Verb::ResetReason => Command::GetResetReason,
        Verb::Restart => Command::Restart,
        Verb::Set => {
            let knob = r.field(b" k=", 2)? as u8;
            let value = r.field(b" v=", 4)? as u16;
            Command::OverrideSet { knob, value }
        }
        Verb::Release => Command::OverrideRelease {
            knob: r.field(b" k=", 2)? as u8,
        },
    };
    r.finish()?;
    Ok(cmd)
}

// ---------------------------------------------------------------------------
// Replies
// ---------------------------------------------------------------------------

/// A fixed-capacity ASCII string for reply payloads, validated on construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Text<const N: usize> {
    len: u8,
    bytes: [u8; N],
}

/// Why a [`Text`] was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadText;

impl<const N: usize> Text<N> {
    fn new(s: &[u8], ok: impl Fn(u8) -> bool) -> Result<Self, BadText> {
        if s.is_empty() || s.len() > N || !s.iter().all(|&b| ok(b)) {
            return Err(BadText);
        }
        let mut bytes = [0u8; N];
        bytes[..s.len()].copy_from_slice(s);
        Ok(Self {
            len: s.len() as u8,
            bytes,
        })
    }

    /// The text bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}

/// Firmware version text: printable ASCII without space.
pub type VersionText = Text<VERSION_MAX>;
/// Reset-reason token: `[a-z0-9_-]`.
pub type ReasonText = Text<REASON_MAX>;

fn is_version_byte(b: u8) -> bool {
    (0x21..=0x7e).contains(&b)
}

fn is_reason_byte(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-'
}

impl VersionText {
    /// Validate version text.
    pub fn version(s: &[u8]) -> Result<Self, BadText> {
        Self::new(s, is_version_byte)
    }
}

impl ReasonText {
    /// Validate a reset-reason token.
    pub fn reason(s: &[u8]) -> Result<Self, BadText> {
        Self::new(s, is_reason_byte)
    }
}

/// A device-to-host reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    Version(VersionText),
    ResetReason(ReasonText),
    Ok(Verb),
    Error(ErrorCode),
}

impl Reply {
    /// Write the body into `dst`, returning its length.
    pub fn encode(&self, dst: &mut BodyBuf) -> usize {
        let mut w = Writer::new(dst);
        w.put(REPLY_PREFIX);
        match self {
            Reply::Version(t) => {
                w.put(b"VER ");
                w.put(t.as_bytes());
            }
            Reply::ResetReason(t) => {
                w.put(b"RSN ");
                w.put(t.as_bytes());
            }
            Reply::Ok(v) => {
                w.put(b"OK ");
                w.put(v.token());
            }
            Reply::Error(c) => {
                w.put(b"ERR ");
                w.put(c.token());
            }
        }
        w.len
    }
}

/// Parse a device-to-host reply body strictly.
pub fn parse_reply(body: &[u8]) -> Result<Reply, ControlError> {
    let rest = body
        .strip_prefix(REPLY_PREFIX)
        .ok_or(ControlError::NotControl)?;
    let bad = |at: usize| ControlError::Malformed { at };
    let sp = rest
        .iter()
        .position(|&b| b == b' ')
        .ok_or(bad(REPLY_PREFIX.len() + rest.len()))?;
    let (kind, arg) = (&rest[..sp], &rest[sp + 1..]);
    let at = REPLY_PREFIX.len() + sp + 1;
    match kind {
        b"VER" => VersionText::version(arg)
            .map(Reply::Version)
            .map_err(|_| bad(at)),
        b"RSN" => ReasonText::reason(arg)
            .map(Reply::ResetReason)
            .map_err(|_| bad(at)),
        b"OK" => Verb::ALL
            .into_iter()
            .find(|v| v.token() == arg)
            .map(Reply::Ok)
            .ok_or(bad(at)),
        b"ERR" => ErrorCode::ALL
            .into_iter()
            .find(|c| c.token() == arg)
            .map(Reply::Error)
            .ok_or(bad(at)),
        _ => Err(ControlError::UnknownVerb),
    }
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

/// Where a record belongs, decided by prefix alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Routed<'a> {
    /// An excerpt-install body, untouched; `excerpt::parse_record` stays its only interpreter.
    Excerpt(&'a [u8]),
    /// A control body, parsed.
    Control(Result<Command, ControlError>),
    /// Neither prefix.
    Unknown,
}

/// Classify one inbound record.
pub fn dispatch(rec: &InboundRecord) -> Routed<'_> {
    let body = rec.body();
    if body.starts_with(EXCERPT_PREFIX) {
        Routed::Excerpt(body)
    } else if body.starts_with(CMD_PREFIX) {
        Routed::Control(parse_command(body))
    } else {
        Routed::Unknown
    }
}

/// Counters for records the router refused. Cumulative since boot, saturating.
#[derive(Debug, Default)]
pub struct ControlStats {
    unknown_verbs: AtomicU32,
    malformed: AtomicU32,
}

/// One read of [`ControlStats`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ControlCounters {
    /// Unknown control verbs and records with neither known prefix.
    pub unknown_verbs: u32,
    /// Known verbs with a malformed body.
    pub malformed: u32,
}

/// The device's control counters.
pub static CONTROL: ControlStats = ControlStats::new();

impl ControlStats {
    /// Zeroed, as at boot.
    pub const fn new() -> Self {
        Self {
            unknown_verbs: AtomicU32::new(0),
            malformed: AtomicU32::new(0),
        }
    }

    /// Both counters in one read.
    pub fn snapshot(&self) -> ControlCounters {
        ControlCounters {
            unknown_verbs: self.unknown_verbs.load(Ordering::Relaxed),
            malformed: self.malformed.load(Ordering::Relaxed),
        }
    }

    fn bump(counter: &AtomicU32) {
        let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
            Some(v.saturating_add(1))
        });
    }
}

/// Route one record to its consumer.
///
/// Excerpt bodies go to `on_excerpt` byte-for-byte unchanged. A fully parsed command goes to
/// `on_control`. Anything else is counted in `stats`, ignored, and returned as the
/// [`Reply::Error`] to send back; `None` means the record was routed.
pub fn route(
    rec: &InboundRecord,
    stats: &ControlStats,
    on_excerpt: impl FnOnce(&[u8]),
    on_control: impl FnOnce(Command),
) -> Option<Reply> {
    match dispatch(rec) {
        Routed::Excerpt(body) => {
            on_excerpt(body);
            None
        }
        Routed::Control(Ok(cmd)) => {
            on_control(cmd);
            None
        }
        Routed::Control(Err(e)) => {
            match e {
                ControlError::Malformed { .. } => ControlStats::bump(&stats.malformed),
                _ => ControlStats::bump(&stats.unknown_verbs),
            }
            Some(Reply::Error(e.code()))
        }
        Routed::Unknown => {
            ControlStats::bump(&stats.unknown_verbs);
            Some(Reply::Error(ErrorCode::UnknownVerb))
        }
    }
}

// ---------------------------------------------------------------------------
// Reset reason (STM32H7 RCC_RSR)
// ---------------------------------------------------------------------------

/// Map an STM32H7 `RCC_RSR` value to the token `CTLR RSN` reports.
///
/// Several flags are set together on one reset, so the choice is by priority, most specific
/// cause first: a power-on reset also sets BORRSTF and PINRSTF, and a software reset also sets
/// PINRSTF (the reset is driven out through NRST). Reading PINRSTF first would label every
/// restart "pin". Bits per RM0433 RCC_RSR: 17 CPURSTF, 21 BORRSTF, 22 PINRSTF, 23 PORRSTF,
/// 24 SFTRSTF, 26 IWDG1RSTF, 28 WWDG1RSTF, 30 LPWRRSTF. No cause flag set (cleared by a
/// bootloader before our code ran) is `unknown`.
pub fn reset_reason_from_rsr(rsr: u32) -> ReasonText {
    const PORRSTF: u32 = 1 << 23;
    const BORRSTF: u32 = 1 << 21;
    const IWDG1RSTF: u32 = 1 << 26;
    const WWDG1RSTF: u32 = 1 << 28;
    const LPWRRSTF: u32 = 1 << 30;
    const SFTRSTF: u32 = 1 << 24;
    const PINRSTF: u32 = 1 << 22;
    let token: &[u8] = if rsr & PORRSTF != 0 {
        b"power-on"
    } else if rsr & BORRSTF != 0 {
        b"brown-out"
    } else if rsr & IWDG1RSTF != 0 {
        b"iwdg"
    } else if rsr & WWDG1RSTF != 0 {
        b"wwdg"
    } else if rsr & LPWRRSTF != 0 {
        b"low-power"
    } else if rsr & SFTRSTF != 0 {
        b"software"
    } else if rsr & PINRSTF != 0 {
        b"pin"
    } else {
        b"unknown"
    };
    ReasonText::reason(token).unwrap_or_else(|_| ReasonText::reason(b"unknown").unwrap())
}

// ---------------------------------------------------------------------------
// Byte helpers
// ---------------------------------------------------------------------------

struct Writer<'a> {
    dst: &'a mut BodyBuf,
    len: usize,
}

impl<'a> Writer<'a> {
    fn new(dst: &'a mut BodyBuf) -> Self {
        Self { dst, len: 0 }
    }

    // Every body this module builds is far below MAX_BODY (longest: "CTLR VER " + 32).
    fn put(&mut self, s: &[u8]) {
        self.dst[self.len..self.len + s.len()].copy_from_slice(s);
        self.len += s.len();
    }

    fn hex(&mut self, v: u32, width: usize) {
        for i in (0..width).rev() {
            let nib = ((v >> (4 * i)) & 0xf) as u8;
            self.dst[self.len] = b"0123456789abcdef"[nib as usize];
            self.len += 1;
        }
    }
}

struct Reader<'a> {
    rest: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(rest: &'a [u8], at: usize) -> Self {
        Self { rest, at }
    }

    /// Consume `label` then exactly `width` lowercase hex digits.
    fn field(&mut self, label: &[u8], width: usize) -> Result<u32, ControlError> {
        let bad = ControlError::Malformed { at: self.at };
        let after = self.rest.strip_prefix(label).ok_or(bad)?;
        let digits = after.get(..width).ok_or(bad)?;
        let mut v = 0u32;
        for &d in digits {
            let n = match d {
                b'0'..=b'9' => d - b'0',
                b'a'..=b'f' => d - b'a' + 10,
                _ => return Err(bad),
            };
            v = (v << 4) | u32::from(n);
        }
        self.rest = &after[width..];
        self.at += label.len() + width;
        Ok(v)
    }

    fn finish(self) -> Result<(), ControlError> {
        if self.rest.is_empty() {
            Ok(())
        } else {
            Err(ControlError::Malformed { at: self.at })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::excerpt;
    use crate::frame::{encode, Decoder, MAX_FRAME};
    use crate::inbound::{InboundFeed, InboundStats};
    use log::Level;
    use proptest::prelude::*;

    #[test]
    fn reset_reason_prefers_specific_cause() {
        let r = |v| reset_reason_from_rsr(v).as_bytes().to_vec();
        assert_eq!(r((1 << 23) | (1 << 22) | (1 << 21)), b"power-on");
        assert_eq!(r((1 << 24) | (1 << 22)), b"software");
        assert_eq!(r(1 << 22), b"pin");
        assert_eq!(r((1 << 26) | (1 << 22)), b"iwdg");
        assert_eq!(r(1 << 28), b"wwdg");
        assert_eq!(r(1 << 30), b"low-power");
        assert_eq!(r(1 << 21), b"brown-out");
        assert_eq!(r(0), b"unknown");
        assert_eq!(r(1 << 17), b"unknown");
    }

    fn command() -> impl Strategy<Value = Command> {
        prop_oneof![
            Just(Command::GetVersion),
            Just(Command::GetResetReason),
            Just(Command::Restart),
            (any::<u8>(), any::<u16>())
                .prop_map(|(knob, value)| Command::OverrideSet { knob, value }),
            any::<u8>().prop_map(|knob| Command::OverrideRelease { knob }),
        ]
    }

    fn reply() -> impl Strategy<Value = Reply> {
        prop_oneof![
            proptest::collection::vec(0x21u8..=0x7e, 1..=VERSION_MAX)
                .prop_map(|v| Reply::Version(VersionText::version(&v).unwrap())),
            proptest::collection::vec(
                prop_oneof![b'a'..=b'z', b'0'..=b'9', Just(b'_'), Just(b'-')],
                1..=REASON_MAX
            )
            .prop_map(|v| Reply::ResetReason(ReasonText::reason(&v).unwrap())),
            proptest::sample::select(Verb::ALL.to_vec()).prop_map(Reply::Ok),
            proptest::sample::select(ErrorCode::ALL.to_vec()).prop_map(Reply::Error),
        ]
    }

    fn inbound(body: &[u8]) -> InboundRecord {
        let mut buf = [0u8; MAX_FRAME];
        let n = encode(Level::Info, 3, 9, body, &mut buf).len;
        let mut dec = Decoder::new();
        dec.push(&buf[..n]);
        let out = dec.next_record().map(|r| InboundRecord::from_record(&r));
        out.expect("record decodes")
    }

    proptest! {
        #[test]
        fn command_round_trips(c in command()) {
            let mut b = [0u8; MAX_BODY];
            let n = c.encode(&mut b);
            prop_assert_eq!(parse_command(&b[..n]), Ok(c));
        }

        #[test]
        fn reply_round_trips(r in reply()) {
            let mut b = [0u8; MAX_BODY];
            let n = r.encode(&mut b);
            prop_assert_eq!(parse_reply(&b[..n]), Ok(r));
        }

        #[test]
        fn command_survives_the_frame_path(c in command()) {
            let mut b = [0u8; MAX_BODY];
            let n = c.encode(&mut b);
            let mut f = [0u8; MAX_FRAME];
            let len = encode(Level::Info, 1, 2, &b[..n], &mut f).len;
            let stats = InboundStats::new();
            let mut feed = InboundFeed::new(&stats);
            let mut got = None;
            feed.feed(&f[..len], |r| { got = Some(*r); true });
            let rec = got.unwrap();
            prop_assert_eq!(dispatch(&rec), Routed::Control(Ok(c)));
        }

        #[test]
        fn arbitrary_bytes_never_panic(body in proptest::collection::vec(any::<u8>(), 0..=MAX_BODY)) {
            let _ = parse_command(&body);
            let _ = parse_reply(&body);
        }

        #[test]
        fn mutated_valid_bodies_never_panic(c in command(), i in 0usize..32, x in any::<u8>()) {
            let mut b = [0u8; MAX_BODY];
            let n = c.encode(&mut b);
            b[i % n] = x;
            let _ = parse_command(&b[..n]);
            let _ = parse_command(&b[..i % n]);
        }
    }

    #[test]
    fn adversarial_commands_are_rejected() {
        let cases: &[&[u8]] = &[
            b"",
            b"CTL",
            b"CTL ",
            b"CTL VER ",
            b"CTL VERX",
            b"CTL RST extra",
            b"CTL SET",
            b"CTL SET k=0",
            b"CTL SET k=01",
            b"CTL SET k=01 v=",
            b"CTL SET k=01 v=fff",
            b"CTL SET k=01 v=fffff",
            b"CTL SET k=01 v=FFFF",
            b"CTL SET k=1g v=ffff",
            b"CTL SET k=001 v=ffff",
            b"CTL SET k=01 v=ffff ",
            b"CTL SET k=01  v=ffff",
            b"CTL SET v=ffff k=01",
            b"CTL SET k=01 v=-123",
            b"CTL SET k=01 v=1.5",
            b"CTL REL",
            b"CTL REL k=zz",
            b"CTL REL k=01 v=0000",
            b"ctl VER",
            "CTL SET k=0\u{e9} v=ffff".as_bytes(),
            b"CTL \xff\xfe",
            b"CTLR OK VER",
        ];
        for c in cases {
            assert!(parse_command(c).is_err(), "accepted {:?}", c);
        }
        let big = [b'A'; MAX_BODY];
        assert!(parse_command(&big).is_err());
        let mut long = *b"CTL SET k=01 v=ffff";
        assert!(parse_command(&long).is_ok());
        long[0] = b'X';
        assert_eq!(parse_command(&long), Err(ControlError::NotControl));
    }

    #[test]
    fn adversarial_replies_are_rejected() {
        let cases: &[&[u8]] = &[
            b"",
            b"CTLR",
            b"CTLR VER",
            b"CTLR VER ",
            b"CTLR VER a b",
            b"CTLR RSN UPPER",
            b"CTLR RSN ",
            b"CTLR OK",
            b"CTLR OK XXX",
            b"CTLR OK VER ",
            b"CTLR ERR nope",
            b"CTLR ERR",
            b"CTLR WHAT x",
            "CTLR VER v\u{e9}".as_bytes(),
        ];
        for c in cases {
            assert!(parse_reply(c).is_err(), "accepted {:?}", c);
        }
        assert!(VersionText::version(&[b'a'; VERSION_MAX + 1]).is_err());
        assert!(ReasonText::reason(&[b'a'; REASON_MAX + 1]).is_err());
    }

    #[test]
    fn excerpt_records_reach_the_excerpt_consumer_unchanged() {
        let bodies: [&[u8]; 3] = [
            b"EXCSTART slot=00 name=kick bytes=000100 crc16=beef",
            b"EXCDATA i=0000 b64=AAAA",
            b"EXCEND",
        ];
        // Anything excerpt::parse_record owns routes to the excerpt side.
        for body in bodies {
            assert!(!matches!(
                excerpt::parse_record(body),
                Err(excerpt::RecordError::NotExcerpt)
            ));
            let rec = inbound(body);
            let stats = ControlStats::new();
            let mut seen: Option<([u8; MAX_BODY], usize)> = None;
            let reply = route(
                &rec,
                &stats,
                |b| {
                    let mut a = [0u8; MAX_BODY];
                    a[..b.len()].copy_from_slice(b);
                    seen = Some((a, b.len()));
                },
                |_| panic!("excerpt must not reach control"),
            );
            assert_eq!(reply, None);
            let (a, n) = seen.unwrap();
            assert_eq!(&a[..n], body);
            assert_eq!(stats.snapshot(), ControlCounters::default());
        }
    }

    #[test]
    fn control_reaches_control_and_rejects_are_counted_without_effect() {
        let stats = ControlStats::new();
        let mut applied = 0;

        let ok = inbound(b"CTL SET k=02 v=8000");
        let r = route(
            &ok,
            &stats,
            |_| panic!("not excerpt"),
            |c| {
                assert_eq!(
                    c,
                    Command::OverrideSet {
                        knob: 2,
                        value: 0x8000
                    }
                );
                applied += 1;
            },
        );
        assert_eq!((r, applied), (None, 1));

        let cases: [(&[u8], ErrorCode); 4] = [
            (b"CTL FLY", ErrorCode::UnknownVerb),
            (b"CTL SET k=zz v=0000", ErrorCode::Malformed),
            (b"hello", ErrorCode::UnknownVerb),
            (b"CTL RST junk", ErrorCode::Malformed),
        ];
        for (body, code) in cases {
            let rec = inbound(body);
            let r = route(
                &rec,
                &stats,
                |_| panic!("not excerpt"),
                |_| panic!("state change on a rejected record"),
            );
            assert_eq!(r, Some(Reply::Error(code)));
        }
        assert_eq!(applied, 1);
        assert_eq!(
            stats.snapshot(),
            ControlCounters {
                unknown_verbs: 2,
                malformed: 2
            }
        );
    }
}
