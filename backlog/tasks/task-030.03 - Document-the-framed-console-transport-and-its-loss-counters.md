---
id: TASK-030.03
title: Document the framed console transport and its loss counters
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-09 03:25'
updated_date: '2026-09-10 03:53'
labels:
  - planned
dependencies:
  - TASK-030.02
documentation:
  - docs/reference/daisy-seed3.md
  - README.md
parent_task_id: TASK-030
priority: medium
type: docs
ordinal: 50500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: TASK-030, acceptance criterion #7. Write console protocol v1 down where someone looking for a debug channel will find it: the record grammar and CRC parameters in docs/reference/daisy-seed3.md's 'Debugging without a probe' section (currently lines 146-157), and an honest update to README.md's screen-based instructions (roughly lines 163-200) so they describe what a terminal now shows and how to decode a saved capture. Transcribe field names and widths from the shipped frame.rs and the emission site, not from this ticket. See the plan for the required content list.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 docs/reference/daisy-seed3.md "Debugging without a probe" section states the v1 record grammar field by field with a real example line, and gives the CRC parameters (algorithm, poly, init, reflection, xorout, covered byte range, 123456789 -> 0x29b1 check vector) explicitly enough to reimplement from the document alone.
- [ ] #2 It documents why CRLF is a trustworthy delimiter, the reader resynchronisation rule, and what guarantee that rule buys.
- [ ] #3 It lists the reserved BOOT and STATUS body prefixes with every field, explains each loss counter, and says why sequence numbers alone cannot distinguish a reboot from a loss.
- [ ] #4 It records that the leading byte is reserved device-to-host and a different one for host-initiated commands, pointing at TASK-032, and states the measured ~8.8% baseline this replaces along with the caveat that its raw capture no longer exists.
- [ ] #5 README.md debugging instructions describe what a plain terminal now shows, keep the panictest countdown and PANIC procedure meaningful, and give the console_decode command for checking a saved capture plus a pointer to TASK-031.
- [ ] #6 Every field name, width and counter in both documents was checked against the shipped frame.rs and the emission site rather than transcribed from this ticket, and cargo fmt --all --check passes.
- [ ] #7 It records the USB short-packet rule: that the device terminates every bulk transaction with a short packet or zero-length packet, why an exactly-64-byte final packet would otherwise sit unseen in the host's driver buffer, and that a reader may legitimately observe zero-length reads.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
# Plan (docs-only; no code changes, no sub-tickets)

Transcribe the shipped console transport into the two places someone hunting for a debug channel will
actually read. Everything cited below was read off `frame.rs` / `console.rs` / `usb.rs` / `dump.rs` and the
two doc targets at HEAD `45d9e60` on 2026-09-10. Treat those line references as pre-researched — do not
re-run the exploration — but **re-check each name, width and counter against the file before putting it in
prose** (AC #6 exists because this ticket's own text has already drifted from the code twice).

**No sub-tickets.** Two markdown files describing one protocol, and they have to agree with each other and
with the same source; splitting them buys an ordering problem, not an increment.

**Nothing here needs a person.** Every criterion is satisfied by reading source and running host tests;
the board-dependent claims (legible in a terminal, zero bad frames, audio unharmed) are already owned by
**TASK-030.04** (`@human`) and **TASK-033**. Do not add `HUMAN:` criteria here, and do not touch
TASK-030.04.

## Where the content goes

Locate sections **by heading text**. The ticket's "`Debugging without a probe`, currently lines 146-157" is
stale twice over: TASK-036.04 (`45d9e60`) renamed it and grew the region.

| Target | Location at HEAD | Action |
|---|---|---|
| `docs/reference/daisy-seed3.md` → `### Debugging with nothing attached` | 155–169 | Append the v1 spec (~85–100 lines) after the closing paragraph at 168–169 |
| `docs/reference/daisy-seed3.md` → `### What each channel loses` | 285–299 | Line 291's placeholder "(TASK-030.03 documents that record grammar and field set…)" becomes a real pointer to the new prose |
| `README.md` → `## Debugging` intro | 163–171 | Rewrite 167: what a plain terminal shows now |
| `README.md` → `### Checking the panic path` | 184–198 | Re-frame the expected text; leave the mechanism argument standing |
| `README.md` → new `### Reading a saved capture` | insert before `### Flashing and logging over an ST-Link probe` (200) | `console_decode` usage + forward pointer to TASK-031 |

**Rename no heading.** `docs/reference/rust-daisy-stack.md:109,111` link to
`./daisy-seed3.md#flashing-and-logging-over-an-st-link-probe` and `#flashing-the-seed3`, and lefthook runs
only fmt/clippy/test — there is no markdown linter or link checker anywhere in the repo, so a broken anchor
fails silently. Adding headings is safe; moving or renaming them is not.

House style, taken from the target files: bolded run-in leads (`**DFU mode is not sticky.** …`), 2–5
sentence paragraphs, tables only for genuine comparisons, fenced blocks tagged `text` for wire bytes and
`bash` for commands, dates stated when a fact is date-sensitive, and *italic section titles* used for
cross-references. State the trap and the reason; no tutorial. The existing numbered list of the two
channels and its closing paragraph stay at the top of the section, untouched — the spec goes under them.
This file currently stops at `###`; if the appended block genuinely outgrows ~100 lines, promoting the
bolded leads to `####` is allowed, otherwise don't introduce a new heading depth.

## Content required, mapped to acceptance criteria

### AC #1 — grammar field by field, one real example, CRC parameters

Grammar (verbatim from `frame.rs:20-34`, which stays the normative copy — say so once, explicitly, so
nobody treats the markdown as the spec):

```
record := '~' level SP seq SP t_ms SP body '*' crc CRLF
```

Field by field, with the non-obvious part named for each:

- `~` start marker, device→host only (`frame.rs:34`).
- `level` one of `I W E D T` (`level_letter`, `frame.rs:221-229`; the decoder accepts exactly those five,
  `frame.rs:726`).
- `seq` exactly 8 lowercase hex digits, `u32`, monotonic within one boot, assigned inside the record lock
  so wire order equals seq order (`frame.rs:201`, `console.rs:114`, `lib.rs:334`). **It wraps mod 2³² on
  purpose** while the loss counters saturate — see AC #3.
- `t_ms` exactly 8 decimal digits, milliseconds since boot, transmitted **modulo 100 000 000** so the
  prefix never widens (`T_MS_WRAP`, `frame.rs:115`); wraps ~27.8 h and **neither `t_ms` nor `seq` reveals
  the wrap on its own** — continuity comes from `BOOT` plus `seq`.
- `body` 0..=200 bytes, sanitised (`MAX_BODY` `frame.rs:93`).
- `'*'` then 4 lowercase hex digits then `CR LF` (`TRAILER_LEN` 7, `frame.rs:99,207-212`).
- Widths worth quoting: `PREFIX_LEN` 21, overhead 28 bytes per record, `MAX_FRAME` 228, shortest legal
  record 28 bytes.
- One decoder fact that will bite anyone writing their own reader: **`'*'` is located backwards from the
  CRLF, never by searching forward from `~`**, because sanitisation leaves `*` legal inside a body and a
  forward search mis-decodes exactly those records (`frame.rs:84-87`).

Example lines — use these, and say where they came from. They are the shipped encoder's own output, pinned
byte-for-byte by tests, so nobody has to trust a hand-computed checksum:

- `~I 00000042 00004567 ENC +1*9c17\r\n` — 34 bytes (`frame.rs:877-884`, round-tripped again in
  `tests/console_frame.rs:459,485`).
- `~D 00000000 00000000 *91d4\r\n` — the empty-body case, 28 bytes, i.e. pure overhead (`frame.rs:887-897`).
- Optional third: `~W deadbeef 76980377 knob r2=298 ✓*b321\r\n` — 43 bytes, showing UTF-8 passing through
  untouched and a wrapped `t_ms` (`frame.rs:900-918`). Worth including: the body is literally the truncated
  knob line that nearly got filed as an ADC glitch.

**Never fabricate a capture line.** A bench-captured example would be nicer, but producing one is
TASK-030.04's work; until then the test-pinned vectors are the honest answer and the docs should label
them as generated by the encoder rather than captured from the wire.

CRC parameters, spelled out completely enough to reimplement from the page alone (`frame.rs:118-143`,
table at 124-134): width 16, poly `0x1021`, init `0xFFFF`, `refin=false`, `refout=false`, `xorout=0x0000`,
check `0x29b1` for the 9-byte ASCII string `123456789`, names **CRC-16/CCITT-FALSE**, alias
**CRC-16/IBM-3740**, catalogue <https://reveng.sourceforge.io/crc-catalogue/16.htm>. Then:

- **Covered range:** `level SP seq SP t_ms SP body` — every byte after `~` up to but excluding `*`, spaces
  included (20 / 26 / 220 bytes for the three shapes above; `frame.rs:37-38,80,741-743`).
- **The alias trap, in the docs, kept:** "CRC-16/CCITT" is commonly misidentified — the true CCITT/V.41
  form is reflected (that is KERMIT, check `0x2189`) and XMODEM is the init-`0x0000` variant. A reader who
  grabs a "CCITT" implementation by name gets a stream where every record fails. Copy the parameters, not
  the name (`frame.rs:136-138`).
- No lookup table, deliberately: a table-less bit loop, so both ends of the link can share identical source
  at a cost of ≤ 220×16 iterations per record (`frame.rs:141-143`).
- Integrity is not authenticity: the checksum is affine over GF(2), so two edits whose contributions cancel
  leave it valid while changing the payload (weight-2 forgery classes exist; measured rejection >99 % at
  weight ≥2). Say it in a clause — it sets the ceiling on what a clean CRC proves (`frame.rs:55-58`).

### AC #2 — why CRLF is trustworthy, resync rule, what the rule buys

- Sanitisation at the producer replaces every body byte `< 0x20` (which covers CR and LF) and `0x7F` with
  `'_'`; bytes `≥ 0x80` pass through so UTF-8 survives (`sanitize_byte`, `frame.rs:240-246`, applied at
  195-197). Prefix fields are fixed-width and contain no CR/LF either, so **the first complete CRLF after a
  candidate start must be that record's terminator** (`frame.rs:417-419`). Same sanitisation is also the
  log-injection fix (`frame.rs:15-16`).
- Say plainly that printable punctuation — including `~`, `*`, `|` — deliberately survives, so framing
  strength comes from the rigid grammar plus the CRC and above all the no-CR/LF invariant, **not** from a
  tilde-free payload. A stray `~` or `*` in a body can only produce a CRC mismatch, never a false-valid
  record (`frame.rs:236-239`).
- One sentence contrasting the usual alternatives earns its space: SLIP (RFC 1055) and COBS buy delimiter
  uniqueness by escaping bytes; this transport buys it by sanitising at the producer, which is cheaper and
  keeps every line readable and greppable in a terminal.
- Resynchronisation rule: on any failure — missing `*`, non-hex CRC, CRC mismatch, missing CRLF, over-length
  — advance **strictly past the disqualified start byte**, charge the skipped bytes once, retry at the next
  `~`, never repair a record and never guess a shorter body (`frame.rs:428-439`, implemented 645-690).
  Cite the hazard: ArduPilot's C MAVLink parser desynchronised permanently when a bad-CRC message happened
  to end in a byte equal to the STX magic, because resuming at "the next plausible-looking byte" can land
  *inside* the next real frame (<https://github.com/ArduPilot/pymavlink/issues/881>).
- The guarantee, stated as the code states it: corruption costs **one record, not the capture**; records are
  emitted only from fully validated frames, so **no record is ever invented** — that is the promise, not
  "nothing decodes" (`frame.rs:44,51-53,60-61`).
- Two honest limits belong beside the guarantee, or the doc oversells: (a) a record whose **leading `~` was
  lost produces no integrity failure at all** — the decoder never saw a candidate start — and only a `seq`
  gap or a `STATUS` record reveals it (`frame.rs:47-49`); (b) a byte-level splice between two producers
  legitimately decodes as two good records plus one integrity failure.

### AC #3 — `BOOT` and `STATUS`, every field, every counter, and why seq is not enough

Quote both templates exactly as emitted, plus the pinned examples:

- `BOOT proto=1 fw={} pipe={} maxbody={}` (`console.rs:188`), e.g.
  `BOOT proto=1 fw=0.1.0 pipe=2048 maxbody=200` (`console.rs:349-356`). `fw` is the `asperitas-logging`
  crate version — the firmware binaries have no separate release process (`console.rs:174-177`). `proto`
  comes first so a reader can handshake before trusting anything. It consumes `seq 0`, rides the normal
  commit path so it can be dropped like any other record, and is only emitted once the USB backend is
  installed (`lib.rs:383-394`, triggered at `usb.rs:230`).
- `STATUS proto=1 sent={} dropped_full={} bytes_dropped={} trunc={} ep_err={} seq_next={} pipe_free={}`
  (`console.rs:209-217`), e.g.
  `STATUS proto=1 sent=12 dropped_full=3 bytes_dropped=4096 trunc=1 ep_err=2 seq_next=18 pipe_free=2048`
  (`console.rs:318-325`). Field names and order **are** the contract this ticket documents and TASK-031
  parses (`console.rs:198-202`). Values are absolute since-boot counts, never deltas — the host owns the
  differencing. Pacing: at most once per second (`STATUS_MIN_INTERVAL_MS`, `console.rs:46`) and only when a
  counter changed, emitted only when the ring is empty (`usb.rs:269-278`). Debounce rationale: during a
  full-ring condition the STATUS record competes for the very space that is missing, so an undebounced
  emitter starves the logs it is reporting on (`console.rs:222-227`).

Per-counter meaning, each from its increment site:

| Field | Counts | Site |
|---|---|---|
| `sent` | records committed to the pipe whole — cross-check against the host's decoded count | `console.rs:119` ← `lib.rs:353`, `lib.rs:508` |
| `dropped_full` | records refused for lack of space, **never partially written** | `console.rs:125` ← `lib.rs:349`, `lib.rs:504` |
| `bytes_dropped` | bytes those refused records would have occupied | same sites, adds full frame length |
| `trunc` | bodies shortened by the 200-byte cap — **those records shipped**, which is why they are not in `dropped_full` | `console.rs:134` ← `lib.rs:338` |
| `ep_err` | endpoint writes that failed, i.e. link loss or stalls | `console.rs:139` ← `usb.rs:289,306` |
| `seq_next` | the value the next record will carry | `console.rs:114` |
| `pipe_free` | ring free bytes at the instant of rendering — shows *how close to full* the ring was, which is what makes a drop storm diagnosable afterwards | `console.rs:202-204` |

Then the two design decisions a reimplementer must copy:

- **Counters saturate at `u32::MAX`; `seq` wraps.** A wrapped byte counter turns into an enormous negative
  rate and reads as a decoder bug, and ~24 days of sustained dropping at ring capacity is reachable on a rig
  left running (`console.rs:16-22`, `fetch_update(.., checked_add)` at 163-166). `seq` does the opposite
  because a saturated sequence number would emit the *same* `seq` twice and invent a record, while a wrapped
  one is recoverable by modular subtraction — a jump ≥ 2³¹ means restart or corruption, not loss.
  **"A counter describes an amount, a sequence number describes a position"** is the asymmetry, in the
  source's own words (`console.rs:24-29`).
- **Why seq alone cannot distinguish a reboot from a loss:** CRC and seq detect damage, never absence — a
  hole in the stream leaves no bytes behind, so a gap is equally consistent with "bytes lost" and "board
  rebooted and restarted numbering at 0". `BOOT` is what breaks the tie: a second `BOOT` mid-capture, or a
  seq regression. Deliberately no boot-id in v1 (would need `.noinit` persistence or a peripheral read for
  no gain over the banner's presence; reset *reason* is TASK-032's). Add the honest limit: **a lost `STATUS`
  record is indistinguishable from nothing having changed**, except through the seq gap it leaves
  (`console.rs:250-253`). So silence means "probably nothing changed", not "nothing changed".
- One sentence so readers don't take these two as the whole reserved set: `AUDIO` and `AUDEND` ride the same
  framing carrying base64 audio dumps (`dump.rs:62-63`), documented with the rig workflow by **TASK-038.06**.
  Name them and point away; do not document their grammar here.

### AC #4 — direction markers, and the baseline this replaced

- `~` is device→host only; `>` (0x3E) is **reserved** host→device for TASK-032's commands, with identical
  field and CRC rules so one parser serves both directions (`frame.rs:34`). Corroboration worth a clause:
  the audio dump rejected Ascii85/Z85 partly because those alphabets contain `~` and `<>`, letting a
  corrupted body reassemble into a fake record boundary (`dump.rs:27`). Record only the reservation — the
  command set is TASK-032's to document when it exists.
- The defect this replaces, in two sentences with the caveat: before framing, roughly **8.8 % of log lines
  were truncated over USB CDC — 1226 of 13968**, hand-counted over a 240 s `podtest` capture on 2026-08-08
  (`backlog/tasks/task-018.04 - Verify-every-Pod-control-on-hardware.md:83`, provenance at `:54`). Include
  the consequence that makes the argument: one line read `r2=298` cut mid-number and nearly got reported as
  an ADC glitch. State that **the raw capture no longer exists**, so 8.8 % is a baseline, not a reproducible
  measurement — which is itself part of the case for TASK-031. Root cause, one clause: `Pipe::try_write`
  short-writes at every ring wrap even when the ring is empty, and the old path treated a short write as
  success.

### AC #7 — the USB short-packet rule

Write this for the person writing the *reader*, and say why it is not folklore:

- Bulk transfers must end with a short packet. If the final packet of a transaction is exactly
  `max_packet_size` (64 here, `usb.rs:36`), the host driver holds it — and everything it carried — until a
  shorter packet follows. embassy-usb states it verbatim: "If you write a packet that is exactly
  `max_packet_size` bytes long, it won't be processed by the host operating system until a subsequent
  shorter packet is sent. A zero-length packet (ZLP) can be sent if there is no other data to send"
  (`embassy-usb-0.6.0/src/class/cdc_acm.rs:90-93`), and `CdcAcmClass::write_packet` does nothing about it.
  USB 2.0 §5.8.3 is the normative statement; the ST community thread *"STM32U5 USB (CDC) not transmitting
  data if in exact multiple of 64 bytes"* is the same MCU family showing the same symptom.
- What firmware does about it, both places: the drain loop tracks `last_packet_was_full` and emits a ZLP
  **before parking**, and only when the ring is empty (`usb.rs:261-265,282-293`); the panic path
  `emit_blocking` sends a ZLP when `msg.len() % 64 == 0`, where the miss would be least forgiving because
  the withheld record would be the `PANIC:` line (`usb.rs:393-406`).
- The failure mode it prevents, in the ticket's own terms: at end of session the withheld tail never arrives,
  and our framing would have reported that faithfully as a `seq` gap while the cause sat in the drain loop.
- Consequence for the reader, stated so nobody chases a phantom: **zero-length reads are legitimate.** Do not
  treat a 0-byte read as disconnect or EOF; terminate on record validation and seq continuity instead. Also
  note the endpoint rejects oversize writes rather than splitting them (`BufferOverflow`, `usb.rs:29-35`),
  which is why every write path chunks to 64.

### AC #5 — README

Three edits, no more:

1. **Intro (`163-171`).** Say what `screen /dev/ttyACM0 115200` now shows: every line prefixed
   `~<level> <seq> <t_ms> ` and suffixed `*<crc>`, still readable by eye and still greppable — that legibility
   is why v1 is printable ASCII and not COBS. First line off a fresh boot is `BOOT proto=1 …`; `STATUS …`
   appears at most once a second and only when a counter moved, so a quiet stream usually means nothing
   changed rather than nothing working. Optional verified aside: the baud rate in the command is decorative —
   CDC-ACM has no UART to configure, and neither this firmware nor embassy-usb applies the host's line coding
   to hardware (`grep -n line_coding` finds only storage and a getter, `cdc_acm.rs:317`). Keep it only if you
   re-run that grep. Point at `docs/reference/daisy-seed3.md` → *Debugging with nothing attached* for the
   grammar.
2. **Panic path (`184-198`).** Both the countdown and the panic line now travel framed; the terminal line
   reads `~E <seq> <t_ms> PANIC: <msg> at src/bin/panictest.rs:L:C *<crc>`. Keep the paragraph that explains
   the two mechanisms differ (countdown via the log pipe, panic pushed straight to the endpoint because the
   executor is dead) and keep "countdown text with no `PANIC:` line is a real failure" — framing changes the
   bytes, not that argument. Describe the framed shape in prose and let `firmware/src/bin/panictest.rs:30-49`
   own the table: **do not paste a competing table** — TASK-030.05 is updating that one.
3. **New `### Reading a saved capture`.** Save the raw bytes, not a scrollback, e.g.
   `cat /dev/ttyACM0 > capture.bin`, then:
   ```bash
   cargo run -p asperitas-logging --example console_decode -- capture.bin > clean.txt
   cat capture.bin | cargo run -p asperitas-logging --example console_decode
   ```
   Explain the split, because it is the whole point: stdout carries validated records byte-identical to the
   wire (greppable, and re-feedable into the same program); stderr carries
   `records=… bad_frames=… resyncs=… discarded_bytes=…`, a byte-accounting line, and a `seq continuity` line
   with `gaps=` and `first_gap=<from8>-><to8>` (`examples/console_decode.rs:5-9,167-178,242-271`). Say that
   it **exits 0 whatever the capture contains** — it reports, deciding whether a capture passed is TASK-031's
   job — and that a gap immediately after a `BOOT` record is a restart, not loss. Use `capture.bin`, matching
   the example's own doc comment, not `capture.raw`.

## Explicit non-goals

- `firmware/src/bin/panictest.rs`'s expected-output table → **TASK-030.05**.
- `AUDIO`/`AUDEND`/`RIGCFG`/`CAPSTAT` grammars and the rig workflow → **TASK-038.06**.
- Host→device command semantics beyond the reserved byte → **TASK-032**.
- Pass/fail judgement and scripted captures → **TASK-031**.
- Anything about the probe or RTT → already written by TASK-036.04; extend, don't rewrite.
- Claims about hardware behaviour. Nothing here is measured at the bench; do not imply otherwise.

## Verification (AC #6 and the evidence bar)

1. Re-read each fact into place from source: every `field=` name typed into the docs must appear verbatim in
   `console.rs`'s format strings; every width must match `frame.rs`. Prefer the code's wording when it is
   already good.
2. `cargo test -p asperitas-logging` — this is what proves the example frames quoted in the docs are the
   encoder's asserted bytes rather than something hand-computed. Quote only vectors this run passes.
3. `cargo fmt --all --check` — required by AC #6; a markdown-only diff will not change it, run it anyway.
4. Parent's §7 gates (`cargo clippy --workspace --all-targets -- -D warnings`, the `asperitas-pod/pod-hw`
   clippy pass, `cargo test --workspace`, and the `thumbv7em-none-eabihf --features seed3 --release` firmware
   build) are unaffected by a `.md`-only diff; run them if the diff touches any `.rs`.
5. Check links by hand — no linter will. New internal anchors must match GitHub slug rules
   (`### Debugging with nothing attached` → `#debugging-with-nothing-attached`); confirm the two existing
   anchors in `rust-daisy-stack.md:109,111` still resolve.
6. `git diff --stat` should show exactly `docs/reference/daisy-seed3.md` and `README.md` (plus this ticket
   file). Any change under `crates/` or `firmware/` is out of scope.
7. Compiling is not evidence: the evidence for this ticket is two documents where every number traces to a
   line of shipped code, and a reader who has never seen the repo can write a conforming decoder and tell a
   loss from a reboot from the page alone.

## Commit

Trailer `Task-Id: TASK-030.03`, matching `git log`. In the body, separate what was transcribed from code
(which lines) from what stays unmeasured (anything at the bench, and the 8.8 % hand tally), the way
`45d9e60`'s message does.

## Risks

1. **Docs drifting from code.** Mitigation: cite source paths inline in the prose, name `frame.rs` as the
   normative copy once, and keep the field lists narrow — every extra restated field is a future lie.
2. **Overselling the CRC.** A reader who thinks a valid CRC means a complete capture will draw false
   conclusions from their own data. Mitigation: the "integrity, never absence" sentence and the lost-`~`
   limit go in the same breath as the guarantees, not in a footnote.
3. **Swamping the section.** This file's sections run 15–130 lines; if the append reads like a spec dump
   rather than the file's usual trap-plus-reason prose, cut adjectives, keep one table, and let `frame.rs`
   hold the exhaustive detail.
4. **Stale coordinates in this plan.** Line numbers above are HEAD `45d9e60`; locate by heading text and
   symbol names, and expect drift if TASK-030.05 or TASK-038.x lands first.
<!-- SECTION:PLAN:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-10 03:53
---
Planning pass (2026-09-10): no sub-tickets and no @human split. Two markdown files describing one protocol, mutually dependent and both transcribed from the same source, are one focused increment; splitting them creates an ordering problem and two chances for the field lists to disagree. Every criterion here is satisfiable by reading frame.rs/console.rs/usb.rs and running host tests - the board-dependent claims stay where they already live, TASK-030.04 (@human) and TASK-033.

Found and fixed drift in this ticket's own coordinates: the section is now '### Debugging with nothing attached' at docs/reference/daisy-seed3.md:155-169, not 'Debugging without a probe' at 146-157 - TASK-036.04 (45d9e60) renamed and grew that region and left a forward pointer at :291 saying this ticket owns the grammar. Plan locates everything by heading text and forbids renames: rust-daisy-stack.md:109,111 link to two anchors in that file and the repo has no markdown linter or link checker (lefthook runs fmt/clippy/test only), so a broken anchor is silent.

AC #1's 'real example line' resolved without hardware: the frames '~I 00000042 00004567 ENC +1*9c17', '~D 00000000 00000000 *91d4' and '~W deadbeef 76980377 knob r2=298 (check mark)*b321' are the shipped encoder's output pinned byte-for-byte by tests at frame.rs:877-938, so 'cargo test -p asperitas-logging' proves them. Fabricating a bench-captured line is forbidden in the plan. AC #4's baseline verified at its source: task-018.04 line 83 really does read '1226 of 13968, directly counted', capture provenance (podtest, slow-boot, 240 s, 2026-08-08) at :54.

Two additions beyond the letter of the ACs, both to stop this doc contradicting code that landed after it was written: AUDIO/AUDEND get one named-and-pointed-away sentence (dump.rs:62-63, grammar owned by TASK-038.06) so BOOT/STATUS do not read as the whole reserved-prefix set; and the reader-side facts a conforming decoder needs but the ticket never asked for - the CRC range includes the prefix spaces, '*' is located backwards from CRLF because bodies may legally contain '*', leading-tilde loss produces no integrity failure at all, counters saturate while seq wraps mod 2^32, and zero-length reads are legitimate.

Cross-ticket edit made here: TASK-038.06 now also depends on TASK-030.03 (its existing deps on .03/.04 preserved). It documents AUDIO/AUDEND 'beside what TASK-030.03 documents for BOOT and STATUS' in the same region of the same file, and had no dependency either way.
---
<!-- COMMENTS:END -->
