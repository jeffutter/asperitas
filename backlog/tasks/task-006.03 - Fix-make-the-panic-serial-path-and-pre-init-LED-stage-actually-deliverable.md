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
