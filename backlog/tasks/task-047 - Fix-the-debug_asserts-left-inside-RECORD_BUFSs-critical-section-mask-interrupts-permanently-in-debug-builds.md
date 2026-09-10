---
id: TASK-047
title: >-
  Fix: the debug_assert!s left inside RECORD_BUFS's critical section mask
  interrupts permanently in debug builds
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-10 08:09'
labels: []
dependencies:
  - TASK-045
priority: low
type: bug
ordinal: 74500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while planning TASK-045, which fixes the release-profile instance of this same fault class.

TASK-045 moves `write_whole`'s stall panic out of the `RECORD_BUFS.lock(|cell| …)` closures, because `RECORD_BUFS` is a `CriticalSectionRawMutex` and this target is `panic="abort"`: a panic raised inside a `critical_section::with` closure never runs the guard's `Drop`, so `PRIMASK` stays set for the remaining life of the program and `usb::emit_blocking` then spins with no USB interrupt and silently drops the panic text it exists to deliver.

That argument applies verbatim to every other panic still inside those closures. After TASK-045 lands, `try_emit_dump` keeps at least two `debug_assert!`s whose condition is about values computed under the lock (`encoded.truncated`, and the "pipe refused a frame the headroom rule already admitted" refusal), and `frame::encode` asserts internally too. Each is debug-profile only, so the shipping release image is unaffected — but a debug build that trips one goes deaf permanently rather than printing where it died, which is precisely the failure mode TASK-045 exists to remove, and it will bite whoever debugs the dump path on the bench.

Work: convert the remaining in-closure panics into reported outcomes checked after the critical section releases, following whatever shape TASK-045 settled on (read its Final Summary and the resulting `commit_records`/outcome code first — do not invent a second pattern). Where an assert genuinely cannot be expressed as a returned value without leaking internals, say so in the doc comment instead of leaving a bare `debug_assert!`, and keep the machine check TASK-045 introduces (`no panic!/assert expression lexically inside a RECORD_BUFS.lock closure`) green by extending it to cover asserts.

Acceptance criteria are deliberately device-free: fmt, `clippy -p asperitas-logging --all-targets -- -D warnings`, `cargo test --workspace`, and the seed3 release build all pass with `firmware/Cargo.lock` unchanged, plus a host test per converted site asserting the reported outcome rather than the crash.
<!-- SECTION:DESCRIPTION:END -->
