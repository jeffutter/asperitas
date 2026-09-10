---
id: TASK-046
title: >-
  Fix: emit_blocking's EMIT_TIMEOUT bound rests on a time-driver ISR that
  PRIMASK can freeze
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-10 08:08'
labels: []
dependencies:
  - TASK-045
priority: medium
type: bug
ordinal: 73500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while planning TASK-045, which fixes a different fault in the same function.

`usb::emit_blocking` bounds its panic-path spin with `EMIT_TIMEOUT = 3 s`, compared as `while Instant::now() < deadline` (`crates/asperitas-logging/src/usb.rs:47-51`, `:417-419`). That clock does not keep faith when interrupts are masked. `embassy-stm32 0.6.0`'s GP16 time driver computes `now()` as `(period << 15) + hardware CNT` (`src/time_driver/gp16.rs:81-82`, `:347-353`) and increments `period` **only in the timer ISR** (`next_period`, `:193-199`). With PRIMASK set the period word freezes while the 16-bit counter free-runs, so `Instant::now()` becomes non-monotonic and the deadline comparison can stop terminating — the spin outruns the very bound whose comment promises "rather than spinning here forever". Confirm which time-driver instance the seed3 build actually selects before writing the fix; the repo pins no explicit `time-driver-*` feature, so it is whatever embassy-stm32 defaults to for `stm32h750ib`.

Why it matters on its own: `EMIT_TIMEOUT` is the only thing standing between a board with no host attached and an unbounded halt-loop spin inside the panic handler, and the panic handler is exactly the code that runs when invariants have already broken. TASK-045 removes the one caller that reached this with PRIMASK stuck set, but any panic raised inside a still-masked region reaches it — including the debug-profile asserts tracked as TASK-047.

Candidate directions, to be chosen with measurements rather than taste: count CPU cycles (check whether DWT is usable and unlocked on the Seed3's M7 under a bare panic path, and whether CM7 DWT CYCCNT needs the `DBGMCU` unlock and cycle-count-to-milliseconds conversion at the actual sysclk); or keep `Instant` but make the loop terminate on non-monotonicity (treat a backwards `now()` as expiry), which is a few lines and preserves today's units. State the worst-case spin in milliseconds under the chosen bound, and say what happens when the host is attached versus absent.

Acceptance criteria are deliberately device-free: the change, its honest documentation, and the arithmetic that bounds it must land green under fmt/clippy/test plus the seed3 release build. On-device confirmation that a board with no host attached halts at the red LED in a human-plausible time belongs to the existing hardware verification family (TASK-030.04 et al.), not here — do not write an AC that needs ears and a bench.
<!-- SECTION:DESCRIPTION:END -->
