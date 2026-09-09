---
id: TASK-038
title: Generate stimulus and capture audio on-device for self-loopback measurement
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-09 02:12'
updated_date: '2026-09-09 11:47'
labels:
  - planned
dependencies:
  - TASK-038.01
  - TASK-038.02
  - TASK-038.03
  - TASK-038.04
  - TASK-038.05
  - TASK-038.06
documentation:
  - docs/reference/daisy-seed3.md
  - docs/reference/daisy-pod.md
modified_files:
  - crates/asperitas-dsp/src/stimulus.rs
  - crates/asperitas-logging/src/dump.rs
  - crates/asperitas-logging/examples/dump_reassemble.rs
  - crates/asperitas-logging/examples/excerpt_stream.rs
  - firmware/src/bin/rig.rs
  - firmware/Cargo.toml
  - .github/workflows/ci.yml
  - README.md
priority: high
type: feature
ordinal: 44500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Pod self-loopback in TASK-034 closes the electrical path but leaves nobody generating stimulus and nobody recording the return. That is this ticket, and it is the work that replaces buying an external audio interface — cost moved from money to engineering time deliberately.

The shape follows from hardware already present. Stimulus is generated digitally, so its level is a constant rather than a pad switch someone hunts, and clipping is decided in code. Capture lands in the 64 MB SDRAM, which at 48 kHz mono float holds roughly five minutes — enough for the multi-minute dropout runs TASK-019.03 asks for without streaming in real time. Dump happens after the run rather than during it, because full-speed USB CDC has neither the sustained rate nor the isolation from logging traffic that a live stream would need; recording into memory and draining afterwards turns a bandwidth problem into a latency problem.

Real playing material needs no host audio output either: excerpts from audio/instruments/ fit in the 8 MB QSPI flash and replay as exact stimulus. Keep the excerpt small and named — storing a corpus wholesale would spend the whole flash and leave nothing for firmware images.

One constraint worth stating before implementation starts: the dump must not be able to corrupt live audio, and live audio must not be able to corrupt the dump. Sharing DMA buffers between them is how a test passes while measuring its own tearing.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A build mode plays deterministic stimulus out of the Pod jack — sine at a selectable level, swept sine, and an impulse train — generated in the DSP domain so its amplitude is exact and its phase starts at a sample boundary.
- [ ] #2 Input samples are recorded into SDRAM with the footprint stated in bytes per second, and recording cannot overwrite a block that is being dumped.
- [ ] #3 Recorded blocks reach the host through the framed transport from TASK-030 with a checksum that either matches or the host reports loss, and dumping does not starve the audio callback — verified by the drop counter staying at zero during a dump.
- [ ] #4 Maximum capturable duration is reported by the device rather than guessed by the caller, with headroom over live audio buffers stated.
- [ ] #5 A named excerpt from audio/instruments/ can be stored in QSPI and replayed as stimulus bit-exactly, so playing-real-material tests need no host analog path; storage cost per second is documented.
- [ ] #6 Host-side tests parse the dump format from synthetic data with no board attached.
- [ ] #7 Usage is documented alongside the other host tooling, and the SDRAM and QSPI budgets land in docs/reference/daisy-seed3.md.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Six sub-tickets carry this feature. Each has its own `/backlog-planner` session; this plan is the contract between them, so read the numbered decisions before proposing any change of shape.

## Shape

Stimulus synthesis, the dump wire format, the firmware binary, the excerpt store, documentation, and the bench session are separable and each is independently testable, so they are separate tickets. What is deliberately **not** separated out is anything that would have to ship at once: the ring geometry lives with the binary that uses it, the record grammar lives with the codec that defines it, and no "refactor" ticket precedes any of them.

Execution order: **.01** and **.02** in parallel (no shared files), then **.03**, then **.04**, then **.06**, then **.05**. .01 needs nothing from the rest of the repo but the `Processor` trait. .02 needs nothing but `frame.rs`. .03 consumes both. .04 rides inside the binary .03 creates. .06 writes down what .03 and .04 decided. .05 turns the board on and is blocked behind all of it plus TASK-034.

| Ticket | Owner | Ships |
|---|---|---|
| .01 | @agent | sine / ESS sweep / band-limited impulse as `Processor`s, plus the `describe()` grammar |
| .02 | @agent | base64 payload records, block CRCs, ring-capacity policy, host reassembler |
| .03 | @agent | `firmware/src/bin/rig.rs`: interrupt-executor audio, SDRAM capture ring, starvation counters |
| .04 | @agent | QSPI excerpt slots, install protocol, ping-pong replay |
| .06 | @agent | README workflow, SDRAM and QSPI budgets, record grammar reference |
| .05 | @human | the bench session, first real throughput numbers, SDRAM cache evidence |

## Decisions, with the reason

**1. A new `rig.rs`, not a mode inside `podtest.rs`.** The ticket names `podtest.rs` as a modified file, but podtest owns no audio peripherals, awaits a flat two-future select where `main.rs` nests three, and its output vocabulary is pinned by a human-verified criterion in TASK-018.04. Growing it means restructuring a signed-off artifact to add the thing it does not have. `main.rs` stays untouched for the opposite reason: it is the shipping effect binary and must not acquire a harness that can touch live audio buffers. Both files stay out of this epic's diff.

**2. Capture is 16-bit mono, not float.** The parent description says "48 kHz mono float holds roughly five minutes". At 96,000 B/s instead of 192,000 B/s, mono 16-bit doubles capturable duration and halves dump time on a link that is the binding constraint, while the analog loop's noise floor dominates long before 16 bits do. One channel, because the loopback is one signal path; a channel swap is TASK-034 criterion #2's permanently-human job, not a reason to pay double bandwidth here. Consequence for the wording of criterion #2: the footprint to publish is 96,000 bytes per second.

**3. Record-then-dump, never live streaming.** Full-speed CDC bulk cannot sustain the rate a live stream needs at this sample width, so capture lands in SDRAM and drains afterwards. That converts a bandwidth problem into a latency problem, which is the trade the ticket already chose; the arithmetic lives in .02's notes.

**4. Audio moves onto its own `InterruptExecutor` bound to `SAI1`, in `rig.rs` only.** Today audio, USB drain, and LED blink share one cooperative executor through nested `select`, so a dump burst delays the callback directly and AC #3's promise rests on discipline. Upstream `examples/looper.rs` demonstrates the structural fix. Doing it only in the measurement binary keeps the shipping image untouched while making preemption a property of the schedule.

**5. Starvation gets its own counter.** Nothing in this repo measures audio overruns today, so "the drop counter stayed at zero" would be a statement about console records only. `rig.rs` reports delivered-versus-expected block counts and worst-case callback duration from the DWT cycle counter, giving the bench session two independent signals instead of one misleading one.

**6. The frame grammar does not change.** `MAX_BODY = 200` and `MAX_FRAME = 228` are compile-time asserted and shared by encoder, decoder, panic buffer, and record buffer; widening them is a v2 wire change under TASK-030's versioning rule and forces regenerated golden CRCs. Payloads ride inside v1 as standard base64, whose alphabet survives `sanitize_byte` intact, at 150-of-228 useful-byte efficiency. If .05 measures the dump as unbearably slow, raising `MAX_BODY` becomes its own ticket with its eyes open.

**7. Loss is proved by sequence, not by checksum.** A record whose leading `~` disappears produces no integrity failure at all — `frame.rs` documents exactly that blind spot. So blocks close with `AUDEND` carrying a CRC over the raw concatenated bytes plus `n_of_n`, and the assembler refuses a block rather than reporting it as complete.

**8. The dump yields capacity headroom to log traffic.** With a 2,048-byte pipe holding nine maximum frames, a dump writer that fills the ring would make AC #3 fail on the traffic it exists to observe. The writer asks a pure function for permission and never takes the last `MAX_FRAME` bytes, so an ordinary log or status record always fits and the invariant is checked on host.

**9. Stimulus selection is compile-time.** Runtime control over the console link belongs to TASK-032, and queueing this epic behind it would stall the measurement chain for an unrelated reason. Cargo features per stimulus kind, sine at −20 dBFS as the default, every combination built by CI so none rots, and the device emits `RIGCFG` carrying the same `describe()` string the host generates, so there is one owner of "what was played". When TASK-032 lands, its command grammar replaces the features without touching the record formats.

**10. Excerpt replay stages through internal RAM.** The Rust QSPI driver has no memory-mapped read — `Flash.qspi` is private and the HAL's `enable_memory_map` is unreachable — so the callback cannot read flash. A ping-pong pair refilled by `read_async` over MDMA keeps QSPI off the audio deadline. Async flash API only: the blocking status wait is an unbounded busy loop and the async variants panic on timeout, neither of which belongs in a timing-sensitive binary.

**11. Excerpt area starts at QSPI offset 0x100000, fourteen 512 KiB slots.** Above DaisyBootloader's region, so installing the bootloader later does not move excerpts; below the driver's `MAX_ADDRESS` off-by-one, whose top sector is left alone. The app stays in internal flash, which is what makes the whole chip available — and if anyone ever moves the image to QSPI, this budget shrinks in the same commit.

## Verification path

CI proves the parts that need no board: property tests on all three stimulus sources, adversarial streams against the dump codec, ring geometry computed from published constants, slot address arithmetic, install state machine, WAV rejection. That is .01 through .04 and it is all an agent may claim.

Everything else is .05, and it is @human because it needs ears, a scope, a stopwatch, and a cable: sound actually leaving the Pod jack and returning at sane amplitude, a five-minute run whose dump shows both console drops and audio block delivery intact, measured dump throughput replacing the prediction, an excerpt installed and recognised by ear, and the SDRAM cache question settled with a reading rather than a comment. Its criteria ask for recorded numbers, including when the outside witness agrees with the device, since the whole arrangement is the device grading its own homework.

TASK-035 consumes the stimulus/capture pair this epic produces and TASK-019.03 consumes the excerpt replay, so the artifacts .05 archives are their inputs, not this ticket's afterthought.

## Not in any sub-ticket

- Real playback-driven parameter sweeps (`TASK-019.03`) and metric computation (`TASK-035`) stay in those tickets; this epic stops at producing trustworthy sample pairs.
- Raising `MAX_BODY`, adopting COBS framing, or moving the app to QSPI execute-in-place are each larger decisions than this feature and are named here so nobody makes them quietly.
- Deliberate dithering of stimulus waits until something has characterized the noise floor; uncharacterized truncation spurs would be reported as distortion.
<!-- SECTION:PLAN:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-09 11:47
---
Planning on 2026-09-09 verified the ticket's premises against the code and the vendored daisy-embassy checkout (commit ca9bcc9). Four statements here or in neighbouring tickets are wrong or stale, and the sub-tickets are written around the corrected versions:

1. **`podtest.rs` runs no audio.** The listed modified file cannot host a measurement rig: podtest never touches `board.audio_peripherals`, and its output vocabulary is pinned by a human-verified criterion in TASK-018.04. Modified-file list now names `firmware/src/bin/rig.rs` instead, leaving both `podtest.rs` and `main.rs` out of this epic. See TASK-038.03 for the reasoning.
2. **"roughly five minutes at 48 kHz mono float" is not the format we should capture.** Mono 16-bit costs 96,000 B/s instead of 192,000 B/s, which doubles capturable duration and halves dump time on full-speed USB CDC — the actual constraint. A 32 MiB ring therefore holds 349.5 s, clearing TASK-019.03's five-minute run. Criterion #2's footprint is 96,000 bytes per second.
3. **TASK-019.03's note that QSPI "also carries the firmware image via XIP" does not apply to this stack.** `daisy_embassy::flash::Flash` exposes indirect read/write/erase only; `enable_memory_map` exists in the HAL but is unreachable because the `Qspi` field is private, and our app links at 0x08000000 in internal flash. So the excerpt budget is the whole chip minus reserved regions, not "a megabyte or two". Its conclusion (one required excerpt, more as budget allows) stays reasonable; the reason needs correcting, which TASK-038.06 criterion #5 covers.
4. **PR #80 is merged**, matching `docs/reference/rust-daisy-stack.md`, while `CLAUDE.md` still says it is unmerged and asks for a commit-SHA pin. `firmware/Cargo.toml` already tracks `branch = "master"`. Not this ticket's diff, but worth fixing before someone acts on the stale instruction.

Two hazards found that no existing document mentions, both recorded in sub-ticket notes: `sdram.init()` returns **0xC000_0000** while the driver programs its single cacheable MPU region at **0xD000_0000**, an address with nothing behind it (caches are enabled nowhere in the stack today, so accesses are uncached and coherent by accident rather than design — TASK-038.05 settles it with a reading); and the QSPI driver erases the whole sector containing any `write()` address while offering no block or chip erase, so unaligned writes destroy their neighbours.
---
<!-- COMMENTS:END -->
