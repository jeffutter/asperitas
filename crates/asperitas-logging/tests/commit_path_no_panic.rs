//! Machine check: nothing on the record-commit path may panic **while the record lock is held**.
//!
//! # Why this file exists
//!
//! `RECORD_BUFS` (`lib.rs`) is an `embassy_sync` mutex over a `CriticalSectionRawMutex`, so its
//! closure runs with `PRIMASK` set, and the device target is `panic = "abort"`. A panic raised
//! inside that closure never runs the guard's `Drop`: interrupts stay masked for the rest of the
//! program's life, and `usb::emit_blocking` — the panic handler's own way out — spins waiting on
//! the USB interrupt it just froze. The crash prints nothing. TASK-045 moved the stall `panic!` out
//! to `commit_records`, after the lock releases; TASK-047 removed what was left.
//!
//! A rule that lives only in prose is a rule that drifts. TASK-045 recorded its "machine check" as
//! a grep pasted into ticket notes, and that grep went stale the moment TASK-045 funnelled both
//! callers through `commit_records`: the real bodies moved out of the closure that grep looked at,
//! so it passed vacuously ever after. This file is the version that is actually run — by CI's
//! ordinary `cargo test --workspace` and by lefthook's pre-push hook, with no feature flags and no
//! configuration, because it reads this crate's sources as text rather than executing them.
//!
//! # What it looks at
//!
//! The closures *passed to* `commit_records`, the interior of `commit_records`' own lock, and every
//! locally-defined function reachable from those, scanned for tokens that can raise a panic.
//!
//! # Honest limitation, stated rather than implied
//!
//! This is a **lexical** scan. It sees explicit panic macros (`panic!`, `assert!`, `.unwrap()`, …)
//! and it does **not** see implicit ones: slice indexing out of bounds, subtraction and shift
//! overflow, `u32::pow`, division by zero. That asymmetry is precisely why the encoder's field-width
//! checks became compile-time facts (`frame::check_hex_width`, `frame::check_decimal_fits`) instead
//! of being deleted along with the runtime asserts that used to guard them: a width the compiler
//! proves cannot overflow cannot index or shift out of bounds either, and no scanner here would
//! have caught a bad one. Narrowing this check to the tokens it can see is *not* permission to
//! narrow the discipline. If brace-matching ever proves brittle, narrow the **list of scanned
//! functions** and say so in the ticket — never the token list, never the positive controls below.

use std::collections::BTreeSet;

const LIB_RS: &str = include_str!("../src/lib.rs");
const FRAME_RS: &str = include_str!("../src/frame.rs");
const CONSOLE_RS: &str = include_str!("../src/console.rs");
const DUMP_RS: &str = include_str!("../src/dump.rs");

/// One source file, carried as its masked copy.
struct Src {
    name: &'static str,
    /// Comments, string literals and character literals blanked to spaces. Every search in this
    /// file runs against this text, never the raw source.
    masked: String,
}

/// A span of masked source: a `{` through its matching `}`, inclusive.
#[derive(Debug)]
struct Region {
    file: &'static str,
    /// Line of the opening brace in the real file, 1-based.
    start_line: usize,
    text: String,
}

impl Region {
    /// Real-file line number at byte offset `at` within this region.
    fn line_at(&self, at: usize) -> usize {
        self.start_line + self.text[..at].matches('\n').count()
    }
}

// ---------------------------------------------------------------------------
// Masking — strip the prose so the scan sees code and nothing else
// ---------------------------------------------------------------------------

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Blank every comment, string literal and character literal to spaces, keeping newlines and the
/// character count so line numbers survive.
///
/// Without this, the sentence "the panic handler emits the panic text" in a doc comment is a
/// `panic!` token and the build stops for a paragraph. Working on `char`s rather than bytes keeps
/// this repo's doc prose (em-dashes, `≤`, `µs`) from desynchronising the offsets.
fn masked(src: &str) -> String {
    let c: Vec<char> = src.chars().collect();
    let mut out: Vec<char> = c.clone();
    let mut i = 0usize;
    while i < c.len() {
        match c[i] {
            '/' if c.get(i + 1) == Some(&'/') => {
                while i < c.len() && c[i] != '\n' {
                    out[i] = ' ';
                    i += 1;
                }
            }
            '/' if c.get(i + 1) == Some(&'*') => {
                let mut depth = 0usize;
                while i < c.len() {
                    if c[i] == '/' && c.get(i + 1) == Some(&'*') {
                        depth += 1;
                        out[i] = ' ';
                        out[i + 1] = ' ';
                        i += 2;
                    } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
                        depth -= 1;
                        out[i] = ' ';
                        out[i + 1] = ' ';
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        if c[i] != '\n' {
                            out[i] = ' ';
                        }
                        i += 1;
                    }
                }
            }
            '"' => i = blank_string(&c, &mut out, i),
            '\'' => i = blank_char_literal(&c, &mut out, i),
            _ => i += 1,
        }
    }
    out.into_iter().collect()
}

/// Blank a string literal whose `"` sits at `i`; returns the index just past it. Handles escapes and
/// raw-string hashes (`r"…"`, `br"…"`, `r#"…"#`).
fn blank_string(c: &[char], out: &mut [char], i: usize) -> usize {
    let mut hashes = 0usize;
    while c.get(i + 1 + hashes) == Some(&'#') {
        hashes += 1;
    }
    let prev = i.checked_sub(1).and_then(|k| c.get(k));
    let raw = prev == Some(&'r')
        || prev == Some(&'b') && i.checked_sub(2).and_then(|k| c.get(k)) == Some(&'r');
    out[i] = ' ';
    let mut at = i + 1 + hashes;
    while at < c.len() {
        if !raw && c[at] == '\\' {
            out[at] = ' ';
            if at + 1 < c.len() {
                out[at + 1] = ' ';
            }
            at += 2;
            continue;
        }
        if c[at] == '"' {
            let mut k = at + 1;
            let mut seen = 0usize;
            while seen < hashes && c.get(k) == Some(&'#') {
                out[k] = ' ';
                k += 1;
                seen += 1;
            }
            out[at] = ' ';
            if seen == hashes {
                return k;
            }
            continue;
        }
        if c[at] != '\n' {
            out[at] = ' ';
        }
        at += 1;
    }
    at
}

/// Blank a character literal at `i`, or leave the `'` alone if it introduces a lifetime.
///
/// The failure direction that matters is mistaking a lifetime for a literal, because that swallows
/// real code and hides violations. So the rule recognises only `'x'` and `'\…'` and treats every
/// other `'` as a lifetime.
fn blank_char_literal(c: &[char], out: &mut [char], i: usize) -> usize {
    let end: Option<usize> = if c.get(i + 1) == Some(&'\\') {
        let mut k = i + 2;
        while k < c.len() && !(c[k] == '\'' && c[k - 1] != '\\') {
            k += 1;
        }
        if k < c.len() {
            Some(k)
        } else {
            None
        }
    } else if c.get(i + 2) == Some(&'\'') {
        Some(i + 2)
    } else {
        None
    };
    match end {
        Some(end) => {
            for slot in &mut out[i..=end] {
                *slot = ' ';
            }
            end + 1
        }
        None => i + 1,
    }
}

// ---------------------------------------------------------------------------
// Region extraction — brace matching over the masked copy
// ---------------------------------------------------------------------------

/// Byte offsets of the `{` at or after `from` and of its matching `}`.
///
/// Braces are ASCII, so byte indexing is safe even though the surrounding text is not.
fn brace_span(text: &str, from: usize) -> (usize, usize) {
    let open = text[from..]
        .find('{')
        .unwrap_or_else(|| panic!("no `{{` at or after byte {from}"))
        + from;
    let mut depth = 0isize;
    for (off, ch) in text[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return (open, open + off);
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces after byte {open}");
}

impl Src {
    fn new(name: &'static str, raw: &'static str) -> Self {
        Self {
            name,
            masked: masked(raw),
        }
    }

    /// Wrap an already-masked snippet, for the scanner's own positive controls.
    fn from_masked(name: &'static str, masked: String) -> Self {
        Self { name, masked }
    }

    /// The `{…}` block whose opening brace follows `marker`'s first occurrence.
    fn region_after(&self, marker: &str) -> Region {
        let at = self
            .masked
            .find(marker)
            .unwrap_or_else(|| panic!("marker {marker:?} not found in {}", self.name));
        self.region_from(at + marker.len())
    }

    /// The `{…}` block whose opening brace is the first `{` at or after `from`.
    fn region_from(&self, from: usize) -> Region {
        let (open, close) = brace_span(&self.masked, from);
        Region {
            file: self.name,
            start_line: self.line_of(open),
            text: self.masked[open..=close].to_string(),
        }
    }

    /// 1-based line number of byte offset `at`. Sound because masking preserves newlines.
    fn line_of(&self, at: usize) -> usize {
        self.masked[..at].matches('\n').count() + 1
    }
}

/// Every closure passed *to* `commit_records`, as distinct from its definition.
///
/// This indirection is the whole point of the file. TASK-045 funnelled both producers through
/// `commit_records`, which left `RECORD_BUFS.lock` itself holding a three-line forwarding closure
/// that has never contained a violation — a check anchored there passes vacuously forever. The
/// bodies live in the arguments at the call sites.
fn commit_records_closures(lib: &Src) -> Vec<Region> {
    lib.masked
        .match_indices("commit_records(")
        .filter(|(at, _)| !lib.masked[..*at].trim_end().ends_with("fn"))
        .map(|(at, m)| lib.region_from(at + m.len()))
        .collect()
}

/// Functions reached from the record lock that this check reads in full. Adding a callee to the
/// source without adding it here trips [`no_unscanned_callee_reaches_the_record_lock`].
fn scanned_functions(sources: &[Src]) -> Vec<(Region, String)> {
    const FRAME_FNS: [&str; 8] = [
        "fn encode(",
        "fn write_hex(",
        "fn write_decimal(",
        "fn write_whole(",
        "fn level_letter(",
        "fn sanitize_byte(",
        // Reached from `encode`, which is reached from under the record lock. Split into two
        // scanned functions when the incremental form arrived for the `AUDEND` CRC: `crc16_ccitt`
        // now delegates, so both halves are on the path and neither may be assumed panic-free.
        "fn crc16_ccitt(",
        "fn crc16_ccitt_update(",
    ];
    const DUMP_FNS: [&str; 1] = ["fn dump_fits("];
    const CONSOLE_FNS: [&str; 6] = [
        "fn take_seq(",
        "fn record_committed(",
        "fn record_dropped_for_space(",
        "fn body_shortened(",
        // Reached from the three counters above. Both are panic-free — `add` discards the `Err`
        // `fetch_update` reports once saturated — but "panic-free" has to mean *scanned*, not
        // *assumed*.
        "fn bump(",
        "fn add(",
    ];
    // Reached from `try_emit_dump`'s closure, where the tripwire below is what put it on this list.
    // Pure arithmetic over two arguments and a constructor, but "panic-free" has to mean *scanned*.
    const LIB_FNS: [&str; 1] = ["fn refusal_after_admission("];

    let mut out = Vec::new();
    for (file, markers) in [
        ("frame.rs", &FRAME_FNS[..]),
        ("dump.rs", &DUMP_FNS[..]),
        ("console.rs", &CONSOLE_FNS[..]),
        ("lib.rs", &LIB_FNS[..]),
    ] {
        let src = sources
            .iter()
            .find(|s| s.name == file)
            .unwrap_or_else(|| panic!("no such source {file}"));
        for marker in markers {
            let name = marker
                .trim_start_matches("fn ")
                .trim_end_matches('(')
                .to_string();
            out.push((src.region_after(marker), name));
        }
    }
    out
}

/// Everything that must be free of panic-forming tokens.
fn locked_regions(sources: &[Src]) -> Vec<Region> {
    let lib = sources.iter().find(|s| s.name == "lib.rs").expect("lib.rs");
    let mut regions = commit_records_closures(lib);

    // `commit_records`' own lock closure — the forwarding body TASK-045 left behind. Scoped to the
    // lock's braces, deliberately excluding the post-lock panics that belong there.
    let def = lib.region_after("fn commit_records(");
    let lock_at = def
        .text
        .find("RECORD_BUFS.lock(")
        .expect("commit_records no longer takes RECORD_BUFS.lock");
    let (open, close) = brace_span(&def.text, lock_at);
    regions.push(Region {
        file: "lib.rs",
        start_line: def.line_at(open),
        text: def.text[open..=close].to_string(),
    });

    regions.extend(scanned_functions(sources).into_iter().map(|(r, _)| r));
    regions
}

/// Tokens that can raise a panic at runtime. Deliberately wide: the failure mode of this file is a
/// token nobody thought to look for, not a false positive.
const FORBIDDEN: [&str; 12] = [
    "panic!",
    "assert!",
    "assert_eq!",
    "assert_ne!",
    "debug_assert!",
    "debug_assert_eq!",
    "debug_assert_ne!",
    "unreachable!",
    "todo!",
    "unimplemented!",
    ".unwrap(",
    ".expect(",
];

/// Occurrences of any forbidden token in `text`, as `(byte offset, token)`, ignoring matches glued
/// to a preceding word character — which is what stops `debug_assert!` reporting twice, once under
/// `assert!`.
fn forbidden_hits(text: &str) -> Vec<(usize, &'static str)> {
    let chars: Vec<char> = text.chars().collect();
    let mut hits = Vec::new();
    for tok in FORBIDDEN {
        let t: Vec<char> = tok.chars().collect();
        let mut i = 0usize;
        while i + t.len() <= chars.len() {
            if chars[i..i + t.len()] == t[..] {
                if i == 0 || !is_word_char(chars[i - 1]) {
                    hits.push((i, tok));
                }
                i += t.len();
            } else {
                i += 1;
            }
        }
    }
    hits.sort_by_key(|(at, _)| *at);
    hits
}

fn sources() -> Vec<Src> {
    vec![
        Src::new("lib.rs", LIB_RS),
        Src::new("frame.rs", FRAME_RS),
        Src::new("console.rs", CONSOLE_RS),
        Src::new("dump.rs", DUMP_RS),
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Positive controls, plus the shape assumptions every other test here rests on.
///
/// These are the most important assertions in the file. A scanner bug that extracted an empty region
/// would make all the others pass vacuously — which is exactly how TASK-045's grep died.
#[test]
fn scanner_extracts_the_regions_it_claims_to_scan() {
    let s = sources();
    let lib = s.iter().find(|x| x.name == "lib.rs").unwrap();
    let closures = commit_records_closures(lib);
    let functions = scanned_functions(&s);

    // Each named function region must still hold something only it holds.
    let want: &[(&str, &str)] = &[
        ("fn encode(", "write_decimal"),
        ("fn write_hex(", "HEX_DIGITS"),
        ("fn write_decimal(", "v % 10"),
        ("fn write_whole(", "written += n"),
        ("fn level_letter(", "Level::Error"),
        ("fn sanitize_byte(", "0x7F"),
        // The polynomial moved into `crc16_ccitt_update` when the whole-buffer form became a
        // delegation. Each control now names a needle only its own body contains.
        ("fn crc16_ccitt(", "CRC16_INITIAL"),
        ("fn crc16_ccitt_update(", "0x1021"),
        ("fn dump_fits(", "saturating_sub"),
        ("fn take_seq(", "fetch_add"),
        ("fn record_committed(", "bump("),
        ("fn record_dropped_for_space(", "bytes_dropped"),
        ("fn body_shortened(", "truncated"),
        ("fn bump(", "add("),
        ("fn add(", "fetch_update"),
        ("fn refusal_after_admission(", "RefusedForSpace"),
    ];
    for (marker, needle) in want {
        let (region, _) = functions
            .iter()
            .find(|(_, name)| name == marker.trim_start_matches("fn ").trim_end_matches('('))
            .unwrap_or_else(|| panic!("no scanned region for {marker}"));
        assert!(
            region.text.contains(needle),
            "{} came out without {needle:?}, so the region is empty or mis-sliced: the scanner is \
             broken and every other check in this file would pass vacuously",
            marker
        );
    }

    // Both producer closures, identified by what only they contain.
    assert!(
        closures.iter().any(|r| r.text.contains("fill(")),
        "no commit_records closure looks like `emit`'s (it calls its `fill`)"
    );
    assert!(
        closures
            .iter()
            .any(|r| r.text.contains("dump_fits") && r.text.contains("take_seq")),
        "no commit_records closure looks like `try_emit_dump`'s (headroom check then seq)"
    );

    // The funnel: exactly one lock site, and at least the two producers routed through it.
    assert_eq!(
        lib.masked.matches("RECORD_BUFS.lock(").count(),
        1,
        "expected exactly one `RECORD_BUFS.lock(` call site — a second one bypasses \
         `commit_records`, which is the bug this whole family of tickets is about"
    );
    assert!(
        closures.len() >= 2,
        "expected at least the two `commit_records(` call sites (emit, try_emit_dump); found {}",
        closures.len()
    );
    assert_eq!(
        locked_regions(&s).len(),
        closures.len() + 1 + functions.len(),
        "unexpected region count — a region vanished or duplicated"
    );
}

/// Masking must not shift line numbers, and must not leave a brace inside a literal.
#[test]
fn masking_blanks_prose_without_moving_lines() {
    let sample = "//! A doc sentence about the panic! handler.\n\
                  pub fn f<'a>(x: &'a str) -> u32 {\n\
                  \x20   /* a block comment with .unwrap( in it */\n\
                  \x20   let s = \"a string with assert!( in it\";\n\
                  \x20   let b = '{';\n\
                  \x20   let _ = (s, b);\n\
                  \x20   panic!(\"real\")\n\
                  }\n";
    let src = Src::new("sample.rs", sample);
    assert_eq!(
        sample.chars().count(),
        src.masked.chars().count(),
        "masking changed the character count, so reported line numbers would be lies"
    );
    assert_eq!(
        sample.matches('\n').count(),
        src.masked.matches('\n').count(),
        "masking dropped a newline"
    );
    assert_eq!(
        src.line_of(src.masked.find("panic!(").unwrap()),
        7,
        "line numbering moved"
    );
    let hits: Vec<&str> = forbidden_hits(&src.masked)
        .iter()
        .map(|(_, t)| *t)
        .collect();
    assert_eq!(
        hits,
        vec!["panic!"],
        "masking left a prose token visible, or ate the one real panic"
    );

    // And on the real files: no brace ever survives inside a character literal, which is the
    // simplification the brace matcher relies on.
    for src in sources() {
        let braces: Vec<(usize, &str)> = src
            .masked
            .match_indices("'{'")
            .chain(src.masked.match_indices("'}'"))
            .collect();
        assert!(
            braces.is_empty(),
            "{}: {:?} survived in code, which the brace matcher would count as structure",
            src.name,
            braces
                .iter()
                .map(|(at, lit)| (src.line_of(*at), *lit))
                .collect::<Vec<_>>()
        );
    }
}

/// Guard against a scanner that quietly stops scanning: plant violations, prove they are seen.
#[test]
fn token_finder_detects_a_planted_violation() {
    let planted = masked(
        "{ let x = 1; debug_assert!(x > 0, \"prose mentioning panic!\"); assert_eq!(x, 1); \
         let _ = Some(x).unwrap(); unreachable!(); }",
    );
    let got: Vec<&str> = forbidden_hits(&planted).iter().map(|(_, t)| *t).collect();
    assert!(
        got.contains(&"debug_assert!"),
        "missed debug_assert!: {got:?}"
    );
    assert!(got.contains(&"assert_eq!"), "missed assert_eq!: {got:?}");
    assert!(got.contains(&".unwrap("), "missed .unwrap(: {got:?}");
    assert!(
        got.contains(&"unreachable!"),
        "missed unreachable!: {got:?}"
    );
    assert!(
        got.iter().all(|t| *t != "assert!"),
        "`assert!` matched inside `debug_assert!` — the word-boundary rule broke: {got:?}"
    );
    assert!(
        got.iter().all(|t| *t != "panic!"),
        "`panic!` survived inside a string literal that masking should have blanked: {got:?}"
    );
}

/// The rule itself: no panic-forming token anywhere under the record lock.
#[test]
fn nothing_under_the_record_lock_can_panic() {
    let s = sources();
    let mut found = Vec::new();
    for region in locked_regions(&s) {
        for (at, tok) in forbidden_hits(&region.text) {
            found.push(format!("  {}:{}: {tok}", region.file, region.line_at(at)));
        }
    }
    assert!(
        found.is_empty(),
        "\nfound panic-forming tokens inside the record-commit critical section:\n{}\n\
         This target aborts rather than unwinding, and the record lock is a \
         `CriticalSectionRawMutex`: a panic raised here never restores PRIMASK, so the board loses \
         interrupts permanently and the panic handler's serial emit drops the text it exists to \
         deliver. Report the condition as a value instead and raise it in `commit_records`, after \
         `RECORD_BUFS.lock` has returned — see that function's doc comment.",
        found.join("\n")
    );
}

/// The check must not be satisfiable by deleting the crashes it objects to.
#[test]
fn violations_still_crash_outside_the_lock() {
    let s = sources();
    let lib = s.iter().find(|x| x.name == "lib.rs").unwrap();
    let def = lib.region_after("fn commit_records(");
    let lock_at = def
        .text
        .find("RECORD_BUFS.lock(")
        .expect("commit_records no longer takes RECORD_BUFS.lock");
    let (_, close) = brace_span(&def.text, lock_at);
    let after = &def.text[close + 1..];
    assert!(
        after.contains("panic!"),
        "`commit_records` no longer panics after the lock releases. The stall panic is the loud \
         failure TASK-040 and TASK-045 put there deliberately: this check cannot be satisfied by \
         deleting it."
    );
    assert!(
        after.contains("debug_assert!"),
        "`commit_records` no longer raises contract breaks reported out of a closure. Removing the \
         in-lock asserts is only a fix if they still crash somewhere reachable."
    );
}

/// Names introduced by a `fn` anywhere in the scanned sources, split by how a call site could
/// reach them.
#[derive(Default)]
struct Definitions<'a> {
    /// Free functions at module scope: callable *bare* or through a `path::` prefix.
    free: BTreeSet<&'a str>,
    /// Every `fn` in sight, methods and test helpers included: only reachable through a receiver
    /// (`.name(`) or a path.
    any: BTreeSet<&'a str>,
}

/// Qualifiers that may precede `fn` on a module-scope item line.
const FN_QUALIFIERS: [&str; 7] = [
    "pub",
    "pub(crate)",
    "pub(super)",
    "pub(in)",
    "const",
    "unsafe",
    "async",
];

/// Collect [`Definitions`] from the masked sources.
///
/// The distinction that matters is *barely callable*: `commit(…)` inside `commit_records`'s lock is
/// a caller-supplied closure parameter, not a locally-defined function, and treating every
/// identifier-with-parentheses as a potential callee reported it as one (alongside `write_whole`'s
/// `write` sink parameter and a same-named helper in `frame.rs`'s test module). Requiring a
/// module-scope `fn` before a bare call counts as a callee removes exactly that class, and costs
/// only the rare function defined inside another function's body — which the header's honesty note
/// covers.
fn definitions<'a>(sources: impl IntoIterator<Item = &'a Src>) -> Definitions<'a> {
    let mut d = Definitions::default();
    for src in sources {
        for line in src.masked.lines() {
            let indented = line.starts_with(' ') || line.starts_with('\t');
            let trimmed = line.trim_start();
            let head = trimmed.strip_prefix("fn ").or_else(|| {
                let (prefix, rest) = trimmed.split_once(" fn ")?;
                // Every word before `fn` must be a known qualifier: `pub const unsafe fn` qualifies,
                // `where T fn`-shaped noise does not.
                prefix
                    .split_whitespace()
                    .all(|t| FN_QUALIFIERS.contains(&t))
                    .then_some(rest)
            });
            let Some(rest) = head else { continue };
            let at = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            let name = &rest[..at];
            if name.is_empty() {
                continue;
            }
            if !indented {
                d.free.insert(name);
            }
            d.any.insert(name);
        }
    }
    d
}

/// Locally-defined callees reached from `regions` that [`scanned_functions`] does not read.
///
/// Split out from the test below so a planted positive control can prove it detects what it claims.
fn unscanned_callees(
    regions: &[Region],
    d: &Definitions<'_>,
    scanned: &BTreeSet<String>,
) -> Vec<String> {
    // Pure data constructors reached from a closure: their bodies are assignments, so scanning one
    // can only ever find nothing the scan looks for. Name one here only with a reason.
    const ALLOWED: [&str; 1] = ["noted"];

    let mut offenders = Vec::new();
    for region in regions {
        let chars: Vec<char> = region.text.chars().collect();
        let mut i = 0usize;
        while i < chars.len() {
            if !(chars[i].is_ascii_alphabetic() || chars[i] == '_') {
                i += 1;
                continue;
            }
            let mut j = i;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            let mut k = j;
            while k < chars.len() && chars[k].is_whitespace() {
                k += 1;
            }
            let word: String = chars[i..j].iter().collect();
            // How the call reaches its target decides which definition set can satisfy it: a bare
            // or `path::` call resolves to a module-scope free function, a `.method(` call to
            // anything at all. Anything already scanned, or on the allowlist, is covered.
            let receiver = i > 0 && chars[i - 1] == '.';
            let reachable = if receiver {
                d.any.contains(word.as_str())
            } else {
                d.free.contains(word.as_str())
            };
            if k < chars.len()
                && chars[k] == '('
                && reachable
                && !scanned.contains(&word)
                && !ALLOWED.contains(&word.as_str())
            {
                offenders.push(format!("{}:{} -> {word}", region.file, region.line_at(i)));
            }
            i = j;
        }
    }
    offenders
}

/// Tripwire for the blind spot that hid `write_hex`: a callee added inside a locked closure without
/// being scanned.
#[test]
fn no_unscanned_callee_reaches_the_record_lock() {
    let s = sources();
    let d = definitions(&s);
    let scanned: BTreeSet<String> = scanned_functions(&s)
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    let offenders = unscanned_callees(&locked_regions(&s), &d, &scanned);
    assert!(
        offenders.is_empty(),
        "\nnew locally-defined callee reached from the record lock:\n  {}\n\
         Either add it to `scanned_functions` in this file, or prove it panic-free another way. \
         Silence here is not proof: transitive reach is how `write_hex`'s assert stayed hidden \
         behind `encode` until TASK-047 found it.",
        offenders.join("\n  ")
    );

    // Positive control: the tripwire must actually trip. Without this, a `Definitions` that came
    // out empty — a qualifier list too wide, a mask that ate the item lines — would report the real
    // sources clean no matter what they grew.
    let planted_src = Src::from_masked(
        "planted.rs",
        masked("fn smuggled(x: u32) -> u32 { x }\npub fn host(y: u32) -> u32 { smuggled(y) }\n"),
    );
    let planted_defs = definitions([&planted_src]);
    assert!(
        planted_defs.free.contains("smuggled") && planted_defs.free.contains("host"),
        "module-scope free functions were not collected: {:?}",
        planted_defs.free
    );
    let region = Region {
        file: "planted.rs",
        start_line: 2,
        text: masked("{ smuggled(y) }").to_string(),
    };
    let tripped = unscanned_callees(&[region], &planted_defs, &BTreeSet::new());
    assert!(
        tripped.iter().any(|o| o.ends_with("-> smuggled")),
        "the tripwire did not trip on a planted unscanned callee: {tripped:?}"
    );

    // And the negative half of that control: a closure parameter invoked bare is not a locally-defined
    // callee, which is the false positive that would otherwise report `commit_records`'s own `commit`
    // argument every time this file runs.
    let param_src = Src::from_masked(
        "params.rs",
        masked("fn outer(commit: impl FnOnce(u32)) { commit(1) }\n"),
    );
    let param_defs = definitions([&param_src]);
    assert!(
        !param_defs.free.contains("commit"),
        "an `impl`-scoped or indented `fn` was treated as barely callable: {:?}",
        param_defs.free
    );
    let param_region = Region {
        file: "params.rs",
        start_line: 1,
        text: masked("{ commit(1) }").to_string(),
    };
    assert!(
        unscanned_callees(&[param_region], &param_defs, &BTreeSet::new()).is_empty(),
        "a caller-supplied closure parameter was reported as a locally-defined callee"
    );
}
