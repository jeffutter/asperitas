---
id: TASK-006.03
title: 'Fix: make the panic serial path and pre-init LED stage actually deliverable'
status: In Progress
assignee:
  - '@agent'
created_date: '2026-08-05 15:06'
updated_date: '2026-08-05 15:18'
labels: []
dependencies: []
parent_task_id: TASK-006
type: bug
ordinal: 25000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Three of TASK-006.02's four criteria could not be satisfied by any amount of human effort, because the firmware cannot produce what they ask for. Found while scoping that ticket's verification session.

**Defect 1 — the panic message can never reach the host (blocks AC #3).** panic_handler.rs writes the message into LOG_PIPE. The only code that moves bytes from that pipe to the CDC endpoint is drain_fut, inside the async usb::run() future. On panic the handler spins forever, so the executor is never polled again and usb::run() never runs. The message sits in the pipe permanently. The earlier bkpt()-to-HardFault fix was necessary but not sufficient.

**Defect 2 — the pre-init LED stage never renders at all (blocks AC #2).** led::init creates the three Outputs at the inactive level and stores LED_STATE = PreInit, but never drives the pins. What renders PreInit is blink_task, which main.rs does not start until after set_global_state(Running). The LED is dark for the whole boot, then jumps to steady green. TASK-006.02's note guessed "boot is fast enough that the blink went by unseen"; in fact an infinitely slow boot would also show nothing, so a delay alone would not have fixed it.

**Defect 3 — oversize writes read as a disconnect and drop logs (threatens AC #1).** CdcAcmClass is built with a 64-byte max packet size, but the drain loop reads into a 127-byte buffer and hands the whole slice to write_packet. The OTG driver returns EndpointError::BufferOverflow for anything over the max packet size (embassy-usb-synopsys-otg-0.3.3 src/lib.rs:1313), and the drain loop treats any error as connection loss, so it breaks to wait_connection() and discards the data. Single log lines are ~30 bytes so this usually hides; it bites when two messages queue together.

**Defect 4 — panic messages carry NUL padding.** format_panic_message returns a fixed [u8; 128] and the caller writes all 128 bytes.

Also adds the two fixtures the human session needs: a panictest binary (AC #3 has no build to flash) and a slow-boot feature so the now-visible pre-init stage can be observed long enough to compare against Running (AC #2).

Approach for defect 1 chosen by the owner: blocking flush in the panic handler. The USB interrupt handler stays live during the panic spin, so the driver can still advance; the future only needs something to poll it. Note: do NOT use embassy_time::Timer for the timeout — Timer::poll calls schedule_wake(.., cx.waker()) on every Pending poll, and feeding a no-op waker into the driver timer queue from a panic context risks a panic inside #[panic_handler], which recurses with no way out. Bound the loop with Instant::now() instead, which is only a counter read.

Does not close TASK-006.02 or TASK-006 — both stay @human.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 usb::emit_blocking() delivers a panic message to the CDC endpoint without the async executor, chunked to the max packet size and bounded by a deadline so an unattached board halts rather than hangs
- [x] #2 The panic path never itself panics: no embassy_time::Timer, no unwrap, no indexing that can trip inside #[panic_handler]
- [x] #3 Max packet size is defined once and used at both the CdcAcmClass construction and every write_packet call site
- [x] #4 Panic messages go out with no NUL padding
- [x] #5 led::init drives the LED to PreInit so boot is visible without blink_task; the PreInit doc states what actually renders (steady red)
- [x] #6 firmware/src/bin/panictest.rs exists and panics deliberately after USB is up, flashable via make flash-all BINARY=panictest
- [x] #7 A slow-boot cargo feature holds the pre-init stage long enough to distinguish it from Running, and normal builds are not slowed
- [x] #8 The three stale LED-polarity doc sites (README.md, ledtest.rs, led.rs) match the verified answer in docs/reference/daisy-pod.md
- [x] #9 cargo test --workspace, clippy -D warnings, and fmt are clean; firmware compiles for main, panictest, and the slow-boot feature
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
All four defects fixed; both fixtures added. Compiles and lints clean, but NOTHING here is hardware-verified — that is TASK-006.02's job, and this ticket exists to make that session possible.

usb.rs: added MAX_PACKET_SIZE (64), now the single source of truth for both CdcAcmClass::new and every write_packet path. The drain loop's buffer shrank 127 -> 64, closing defect 3. Added emit_blocking(), which bypasses the log pipe entirely and drives usb_dev.run() + write_packet itself under a hand-rolled poll loop with Waker::noop(), bounded by an Instant::now() deadline (EMIT_TIMEOUT = 3 s).

Deliberately NOT embassy_time::Timer for that deadline: Timer::poll calls schedule_wake(.., cx.waker()) on every Pending poll (embassy-time-0.5.1/src/timer.rs), so a no-op waker in a busy loop would push into the time driver's queue thousands of times per second — and a panic raised inside #[panic_handler] recurses with no way out. Instant::now() is a bare counter read.

panic_handler.rs: format_panic_message now returns ([u8; 128], usize) and the caller emits only the written prefix via msg.get(..len).unwrap_or(&[][..]), which fixes defect 4 without introducing panicking indexing. Terminator is CRLF, matching format_log_record — a raw serial terminal does not translate LF.

led.rs: init() now ends with set_global_state(LedState::PreInit), so every state renders the moment it is set and boot is visible with no task running. This was defect 2, and its real cause was worse than TASK-006.02's recorded guess ('boot too fast to see the blink') — the pins were never driven at all, so an infinitely slow boot would also have shown nothing. A delay alone would not have fixed it.

Also hardened set_global_state to skip the pins when BOOT_LED_REF is still null. A panic before led::init would previously have dereferenced the uninitialized singleton and faulted, discarding the very diagnostic the handler exists to deliver. Serves AC #2's intent.

panictest.rs: counts down 10 s over serial (proving the ordinary pipe path) then panics at a known line. Green during countdown, red on panic, so the LED transition is a change rather than a continuation. The countdown and the PANIC: line travel by different mechanisms, so countdown text with no PANIC: line is a real, diagnosable failure.

slow-boot feature: 3 s Timer before set_global_state(Running) in main.rs. Off by default; normal boots unaffected.

Docs: the three stale polarity sites (README, ledtest.rs, led.rs) now match daisy-pod.md's verified active-low answer. blinky.rs's doc comment also claimed a 1 Hz pre-init blink that never happened — corrected. README gained sections for slow-boot and panictest.

Ran cargo fmt over the firmware crate, which was not fmt-clean before this change (import order in main.rs/blinky.rs, plus a multi-line led::init call). Unrelated to the fix but both files were already being edited, and it makes cargo fmt --check usable as a gate.

Verification: cargo test --workspace 48 passed / 0 failed; cargo clippy --workspace --all-targets -D warnings clean; cargo fmt clean in both workspaces; cargo clippy -D warnings clean for all four firmware binaries under both seed3 and 'seed3 slow-boot'. audio/ untouched, so no golden could have moved.

Known pre-existing and NOT fixed: clippy::deref_addrof fires three times on the *(&raw mut LOG_PIPE) idiom (lib.rs:50, lib.rs:163, usb.rs:175) when asperitas-logging is linted directly with --features log-usb. That idiom is the deliberate way around static_mut_refs; it does not fail any build in the project's actual lint paths, because the firmware workspace treats asperitas-logging as a dependency. Left alone rather than papered over with an allow.
<!-- SECTION:NOTES:END -->
