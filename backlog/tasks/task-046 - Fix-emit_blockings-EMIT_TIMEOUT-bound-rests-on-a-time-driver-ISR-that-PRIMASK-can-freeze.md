---
id: TASK-046
title: >-
  Fix: emit_blocking's EMIT_TIMEOUT bound rests on a time-driver ISR that
  PRIMASK can freeze
status: Dev Ready
assignee:
  - '@agent'
created_date: '2026-09-10 08:08'
updated_date: '2026-09-10 12:35'
labels:
  - planned
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

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The panic-path bound no longer reads the time driver at all: `EMIT_TIMEOUT` as an `embassy_time::Duration` is gone, the deadline comparison `while Instant::now() < deadline` is gone, and the spin is bounded by two documented constants — a processor-cycle bound (480 MHz x 3 s = 1_440_000_000 cycles) and an unconditional poll-iteration ceiling (20_000_000) — whichever expires first ending the loop. Evidence: `grep -n "Instant\|Duration" crates/asperitas-logging/src/usb.rs` shows only `now_ms()` and its import.
- [ ] #2 The cycle bound needs no interrupt: DWT's CYCCNT is brought up inside the spin path (`DCB::enable_trace` for DEMCR.TRCENA, then `DWT::unlock()` for the LAR, then `enable_cycle_counter()`), reached through `#[cfg(target_arch = "arm")]` helpers whose non-arm fallback reports the counter unavailable, with a SAFETY comment naming exactly which `cortex_m::peripheral::Peripherals` fields are touched and why nothing can alias them. No new dependency and no other crate gains one.
- [ ] #3 Elapsed time accumulates from u32 CYCCNT deltas (`wrapping_sub` summed into a u64) so it is monotone by construction and correct across a counter wrap; no subtraction of two `Instant`s appears anywhere in the panic path, and the comment states why (that `Sub` panics on a backwards operand, embassy-rs/embassy#5545, and a second panic inside `#[panic_handler]` recurses).
- [ ] #4 Degradation is immediate exit rather than a long spin: when the counter is absent or its enable does not read back (DWT still locked, TRCENA ignored, host build), the budget reports itself spent before the loop runs once, so `emit_blocking` returns and `handle_panic` reaches the red-LED halt.
- [ ] #5 The iteration ceiling is documented as a bound on termination, not a schedule: it names the crossover (it could preempt the cycle bound only if the poll body averaged fewer than ~72 cycles per iteration) and cites `lib.rs`'s existing "bounded in bytes and iterations, not microseconds" precedent instead of inventing a measurement.
- [ ] #6 Doc comments carry the claims the code cannot make for itself: worst-case wall-clock with the host attached versus absent and with the time-driver ISR running versus unable to run; that the trigger class is PRIMASK *or* FAULTMASK *or* any context at a priority >= TIM5's IRQ priority, not PRIMASK alone; that the frozen-window width (2.000 s) is narrower than the old 3 s bound, which is why the unit was broken and not merely imprecise; the dependence on daisy-embassy's 480 MHz `default_rcc` and how the bound scales if SYSCLK changes; and the retained rationale for not using `embassy_time::Timer` here.
- [ ] #7 Host unit tests exist and actually run in CI under default features (the module is compiled under `cfg(any(feature = "log-usb", test))`), covering: a u32 counter wrap, the exact-limit boundary, a frozen/repeating sample terminating on the iteration ceiling, an absent counter expiring immediately, and monotone non-decreasing elapsed across arbitrary samples. None use `#[should_panic]`.
- [ ] #8 Documentation elsewhere stays true: `panic_handler.rs`'s statement that the emit is time-bounded names the new clock; `emit_blocking`'s doc keeps the transport assumption (CDC needs a live USB interrupt) and the clock assumption (cycles, not ticks) as two distinct sentences; `now_ms()` records in one sentence that its timestamp field goes stale under the same masked condition. No new rustdoc warnings are introduced.
- [ ] #9 `nix develop -c cargo fmt --all --check` passes.
- [ ] #10 `nix develop -c cargo clippy -p asperitas-logging --all-targets -- -D warnings` passes, and `cd firmware && nix develop -c make clippy` passes.
- [ ] #11 `nix develop -c cargo test -p asperitas-logging` passes including the new bound tests, `nix develop -c cargo test -p asperitas-logging --features log-usb` still builds and passes on host, and `nix develop -c cargo test --workspace` passes with and without `--features asperitas-pod/pod-hw`.
- [ ] #12 `cd firmware && nix develop -c cargo build --release --features seed3` succeeds, `git diff` shows no change to `Cargo.lock` or `firmware/Cargo.lock`, and `cargo doc -p asperitas-logging --no-deps` adds no warnings under default features or with `boot-led,log-usb,log-defmt` (pre-empting TASK-043.02's gate).
- [ ] #13 Nothing in this ticket requires the device. Which bound actually fires on real silicon is recorded as a bench observation on TASK-030.04 (`@human`) and is deliberately not an acceptance criterion here, so no criterion may be marked `HUMAN:`.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SETUP (read first): Rust embedded project (`crates/*` host workspace, `firmware/` separate workspace) targeting the Daisy Seed3 (`thumbv7em-none-eabihf`, `panic=abort`). Prefix every command with `nix develop -c`; work from the repository root. Touch no dependency versions and add no dependency — everything needed is already in the tree. This is one atomic change; do not split it into per-file commits.

## 0. What this plan decides, and where it disagrees with the ticket

The ticket leaves the fix open ("count CPU cycles … or keep `Instant` and treat a backwards `now()` as expiry") and asks for a choice made with measurements rather than taste. Measured here; the choice is **DWT CYCCNT**, and the `Instant` patch is rejected on evidence, not taste:

1. **The ticket understates the bug: today's spin does not "sometimes fail to terminate", it *always* fails to terminate whenever the time-driver ISR cannot run.** With `period` frozen, `calc_now(period, counter)` (`embassy-stm32-0.6.0/src/time_driver/gp16.rs:81-83`) keeps tracking the free-running 16-bit `CNT`, so `now()` sweeps a window exactly `2^16` ticks = **2.000 s** wide, while `EMIT_TIMEOUT` is 3 s = 98_304 ticks. The deadline therefore sits outside the reachable range by construction, at every phase and for both parities of `period`. Verified by simulating `calc_now` with `period` pinned and `CNT` advancing one tick at a time over 6 s, for even and odd `period` and six starting `CNT` values: `deadline_reachable_within_6s=False` in all twelve cases (transcript in this ticket's comments / `/tmp/gp16sim.py`). So "keep `Instant`, treat a backwards read as expiry" fixes only non-monotonicity and leaves the plateau case spinning forever — there is no backwards jump when the clock simply cannot reach the deadline. Do not implement it.
2. **Any bound expressed in `embassy_time` ticks is unsound here, not just the 3 s value.** The whole reachable window under a frozen driver is 2 s, so a 1 s bound would expire only about half the time and up to 2 s late otherwise. Changing `EMIT_TIMEOUT`'s number cannot fix this; the *unit* has to go. After this change no part of the panic-path bound reads the time driver at all.
3. **Generalise the trigger beyond `PRIMASK`** in every doc sentence you write: what breaks the bound is *"the time-driver ISR cannot run"*, which covers PRIMASK, FAULTMASK in a fault path, and any execution context at a priority ≥ TIM5's IRQ priority (a panic inside the SAI/audio ISR being the realistic one). TASK-045 removed one caller that reached this masked; it did not remove the class.
4. **Correct two stale claims while here.** (a) The ticket says the driver is "whatever embassy-stm32 defaults to for `stm32h750ib`" — measured instead: `cargo tree --offline -i embassy-time-driver -f "{p} {f}"` in `firmware/` reports `embassy-stm32 v0.6.0 … time-driver-tim5,_time-driver` and `embassy-time v0.5.1 tick-hz-32_768`, both selected by daisy-embassy's own `Cargo.toml` at pin `ca9bcc9`, not by this repo. So the driver module is `gp16.rs` and `TICK_HZ = 32768`; nothing here can flip them by accident, but a daisy-embassy bump can — worth one sentence in the doc comment naming that coupling. (b) `backlog/tasks/task-030.02 …md:372` claims cortex-m 0.7 ships no DWT/CYCCNT driver. It is wrong: `cortex-m-0.7.7/src/peripheral/dwt.rs` has the full API (§1). Hand-writing the LAR sequence is unnecessary.

## 1. Facts established by research — do not re-research

**The clock that breaks**
- Driver: `embassy-stm32-0.6.0/src/time_driver/gp16.rs` (TIM5). `calc_now` at `:81-83`, `next_period` at `:193-199` (the only writer of `period`, called from the UP/CC ISR), `now()` at `:347-353` (reads `period`, fence, reads `CNT`). Prescaler set at `:125`: `psc = timer_freq / TICK_HZ - 1`.
- Units, all derived from `TICK_HZ = 32768`: one tick = 30.518 µs; one period = 2^15 ticks = 1.000 s; full `CNT` span = 2^16 ticks = 2.000 s; `EMIT_TIMEOUT` = 3 s = 98_304 ticks.
- Real tick rate is slightly fast, not slow: TIM5's kernel clock is 240 MHz (APB1 120 MHz with the ×2 timer multiplier, `daisy-embassy/src/lib.rs:113-116`), so `psc = 240_000_000/32768 - 1 = 7323` and the tick runs at 240 MHz/7324 = 32768.98 Hz (+0.0030 %). One `Instant` second is ~0.99997 wall seconds. Irrelevant next to the bug; state it once so the arithmetic table is honest.
- `embassy_time::Instant`'s `Sub` **panics on a backwards subtraction** (upstream embassy-rs/embassy#5545, still open; std made the same operation saturating in 1.60 for exactly this race). Therefore: **no `-` on an `Instant` anywhere in the panic path** — a second panic inside `#[panic_handler]` recurses with no way out, which `usb.rs:378-381` already documents as unacceptable.

**The clock that replaces it**
- `cortex-m = "0.7"` is already a **direct, non-optional** dependency of `asperitas-logging` (`crates/asperitas-logging/Cargo.toml`, `[dependencies]`, with `features = ["critical-section-single-core"]`); resolved to **0.7.7 in both lockfiles**. No new dependency, no lockfile churn.
- API (all in `cortex-m-0.7.7/src/peripheral/dwt.rs`, gated `#[cfg(not(armv6m))]`, i.e. present on host builds too): `has_cycle_counter()` `:102`, `enable_cycle_counter(&mut self)` `:125` (its own doc says set `DCB::enable_trace` first), `cycle_counter_enabled()` `:139`, `cycle_count() -> u32` `:158`, `unlock()` `:175` (writes `0xC5AC_CE55` to `LAR`). Registers are `volatile_register::RW`, so reads are volatile and will not be folded. Companion: `dcb.rs:29 enable_trace(&mut self)` sets `DEMCR.TRCENA`. Instances come from `cortex_m::peripheral::Peripherals` (`mod.rs:113` `DWT`, `:167 take()`, `:179 steal()`); `DWT` itself exposes `pub const PTR` only.
- Both writes are required on this part: without `DEMCR.TRCENA` the CYCCNTENA write may be ignored (implementation-defined, per cortex-m's own doc), and STM32F7/H7-class parts software-lock the DWT after power-on, so without the `LAR` write `CTRL` reads back locked and `CYCCNT` stays 0. Useful corollary used in §2c: `CTRL` and `CYCCNT` sit behind the *same* lock, so a locked DWT also fails the `cycle_counter_enabled()` readback — the readback detects the locked case without sampling the counter twice.
- CYCCNT counts processor cycles. SYSCLK here is 480 MHz (`daisy-embassy/src/lib.rs:88-96` PLL1 P = 480 MHz, `:111` `config.rcc.sys = Sysclk::PLL1_P`), so u32 wraps every 2^32/480e6 = **8.948 s** — longer than any bound below, but §2c handles the wrap anyway rather than relying on that.
- Nobody claims the Cortex-M system peripherals today: no `cortex_m::Peripherals::take()`/`steal()` anywhere in `crates/`, `firmware/src/`, or daisy-embassy. (`embassy-stm32`'s `Peripherals::take()` at its `src/lib.rs:522` returns *its own* generated struct, not `cortex_m::peripheral::Peripherals`.) Nothing else in the repo touches DWT/CYCCNT/`DBGMCU` in code; `DBGMCU_CR` appears only in `docs/reference/daisy-seed3.md:442` and `firmware/Makefile`, where it concerns RTT discovery across sleep — irrelevant here because the executor busy-loops and never sleeps.
- `embassy-stm32` does not use SysTick on this target (the time driver is TIM5), so candidate C was available — see §6 for why it lost.

**The code being changed**
- `crates/asperitas-logging/src/usb.rs` (430 lines, `#![no_std]` crate, module gated on `feature = "log-usb"`): `EMIT_TIMEOUT` doc 47-50 + const 51; imports `use embassy_time::{Duration, Instant};` line 22; `now_ms()` 125-127 (the only other `Instant` user); `emit_panic_record` 343-355 (sole caller of `emit_blocking`, calls it at `:354`); `emit_blocking` doc 357-381 + body 382-430, with the futures built at 398-414, the pinned `select` at 416, and the deadline/poll loop at 416-429. After the timeout the function returns normally with no status flag, the pinned future is dropped, and `handle_panic` halts at the red LED (`panic_handler.rs:95-103`).
- Call chain: each firmware binary's `#[panic_handler]` (`firmware/src/bin/main.rs:19`, `blinky.rs:9`, `podtest.rs:13`, `panictest.rs:89`) → `panic_handler::handle_panic` (`panic_handler.rs:52-103`) → LED first (`:54`, synchronous GPIO, works even masked) → `usb::emit_panic_record` → `emit_blocking`. `handle_panic` takes no lock and does not unmask.
- Why the timeout is load-bearing independent of the clock bug: with interrupts masked the OTG-FS ISR cannot run either, so `cdc.write_packet(..)` never becomes Ready and `device_fut` never completes — the poll loop is permanently `Pending` and only the bound can end it. Host attached versus absent changes only *when* it exits (early via `is_err()`/Ready versus at the bound), never *whether* the bound is needed.
- Crate conventions this must match: gating is done with cargo features, and there is currently **no `cfg(target_arch = ...)` anywhere in this crate** — §2b introduces the first, deliberately, with the reason in its doc comment. Raw peripheral access exists already under `log-usb` (`USB_OTG_FS::steal()`, `PA11/PA12::steal()` at `usb.rs:165-167`, with the safety contract spelled out at `:158-164`); `static mut` is read through `addr_of_mut!` + `read_volatile` (`:100-111`); `frame.rs` is the zero-`unsafe` module and stays that way.
- House precedent for stating a bound honestly: `lib.rs:326-343` ("What the critical section costs, bounded rather than measured") bounds in **bytes and iterations** precisely because "this crate cannot measure any of it — the time driver ticks at 32 768 Hz". That passage names the very driver this ticket distrusts; borrow its habit of naming the units the code can actually count, and cite it.
- Host reachability: `cargo test --workspace` at the root builds `crates/asperitas-logging` with **default features** (`default = []`), and `usb.rs` is `log-usb`-gated, so nothing in `usb.rs` is compiled by the default-feature test run. `--features log-usb` *does* build and link on host (§4 has the measurement). See §4 for where the unit tests therefore have to live.
- Gates that must stay green: `.github/workflows/ci.yml:24,27,30,33,36,41,44` and `lefthook.yml:7,11,17,21,25,30` — `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` (also with `--features asperitas-pod/pod-hw`), `cargo test --workspace` (ditto), `cargo run -p asperitas-logging --example dump_reassemble -- --selftest`, and `cd firmware && cargo build --release --features seed3`. Firmware clippy runs only via `firmware/Makefile:180`. There is no `[lints]` table and no `#![deny(...)]` yet; the rustdoc warnings-as-errors gate arrives with TASK-043.02/TASK-049, so any doc comment added here must produce **zero rustdoc warnings** under default features, under `boot-led,log-usb,log-defmt`, and under `--all-features`. Do not put outer `///` on a `pub mod` declaration line (rust-lang/rust#134904, recorded in TASK-043.01).
- Baseline at HEAD, measured so any failure below is yours: `cargo fmt --all --check` clean, `cargo clippy -p asperitas-logging --all-targets -- -D warnings` clean, `cargo test -p asperitas-logging` green.

## 2. The fix

One idea, implemented once: *the panic-path spin measures itself against a counter that needs no interrupt, and always carries a second bound that needs no hardware at all.*

### 2a. Replace the constant pair (`usb.rs:47-51`)

Delete `const EMIT_TIMEOUT: Duration` and its three-line doc. Both new constants live in `spin_budget.rs` beside the code that enforces them — the module owns "how long a panic-path spin may run", and `emit_blocking` should only ask it for a budget — with `emit_blocking`'s doc comment reaching them by intra-doc link:

```rust
/// How long the panic-path emit loop will try before giving up, in processor cycles.
///
/// <the five-part doc below — see the list; write it as prose, this is the checklist>
const EMIT_TIMEOUT_CYCLES: u64 = 480_000_000 * 3;

/// Second, unconditional bound on the same spin: poll iterations, not cycles.
///
/// <doc — see §2c for the numbers to state>
const EMIT_TIMEOUT_MAX_POLLS: u32 = 20_000_000;
```

`Duration` then has no remaining user in the file (`Instant` still does, `now_ms()` at `:125-127`), so narrow line 22 to `use embassy_time::Instant;` — leaving the unused import in place fails AC #10's `-D warnings`. One doc-link trap with this split: links may point from `usb.rs` into `spin_budget`, never the other way. `spin_budget` compiles under default features and `usb::emit_blocking` does not, so an intra-doc link to it would be a broken link in the default-feature `cargo doc` run (§5 step 5) — name the caller in prose instead.

The `EMIT_TIMEOUT_CYCLES` doc comment must carry these five things, each of which is a claim the code cannot make for itself:

1. **Why cycles and not `Instant`.** The GP16 driver's `period` word is incremented only by the TIM5 ISR, so when that ISR cannot run `now()` is confined to a 2.000 s window and a 3 s deadline is unreachable *every* time — the spin this constant exists to end would then never end. Name the trigger class as "the time-driver ISR cannot run": PRIMASK, FAULTMASK, or a panic in a context at ≥ TIM5's IRQ priority. Say plainly that the units are the problem, so a future reader does not "fix" this by lowering the number.
2. **The arithmetic.** 480 MHz × 3 s = 1_440_000_000 cycles; CYCCNT is u32 and wraps at 2^32/480e6 = 8.948 s, which §2c handles rather than trusting; the cycle rate is the *processor* clock, so the bound is exactly 3 s only while SYSCLK stays at daisy-embassy's 480 MHz (`default_rcc`, `daisy-embassy/src/lib.rs:88-111`) and stretches proportionally if someone lowers the clock (at 240 MHz the same count is 6 s). Point at `default_rcc` by name so the coupling is greppable.
3. **Worst-case wall-clock, per host state.** Host attached: exits the moment the last packet is accepted, as before. Host absent, interrupts live: the CDC write future stays `Pending` and the loop ends at ≤ 3 s plus one poll iteration, then the board halts at the red LED with the record lost. Host absent *and* the driver ISR unable to run: also ≤ 3 s, which is the case this ticket exists for — before the change it was unbounded. Counter unavailable at all: exits immediately (§2c).
4. **That this replaces an `Instant` comparison** and why the old shape survived review: `Instant::now()` looked like "just a counter read" and is one, but the counter is assembled from an ISR-fed word.
5. **The `Timer` note is still true — keep it.** Move the existing inline rationale (`usb.rs:418-422`: `Timer::poll` pushes a waker into the driver queue on every `Pending` poll) into this doc comment so it is not lost with the loop rewrite.

### 2b. The hardware seam: two arch-gated functions

Add these to the new `spin_budget.rs` module (§4 explains why there and not in `usb.rs`). Fully qualify `cortex_m::…` at the use sites rather than adding a `use` — an import that only one cfg branch uses becomes an unused-import warning on the other, and `-D warnings` is everywhere.

```rust
/// Bring up DWT's cycle counter and report whether it is actually counting.
///
/// Called from the panic path, immediately before the spin that measures itself against
/// it, rather than from [`init`]: that ordering contract is the difference between "the
/// counter was running when some other code disabled it" and "we know what state we left
/// it in", and it costs nothing because enabling is idempotent.
///
/// ARM only. On a host build this reports false and the cycle bound stands down entirely
/// (AC #4) — the first `target_arch` cfg in this crate, because `log-usb` deliberately
/// compiles for host too (TASK-043) and a feature cannot express "not this machine".
#[cfg(target_arch = "arm")]
fn cycle_counter_running() -> bool {
    // Safety: DWT and DCB are Cortex-M system peripherals that nothing in this stack
    // claims — no `cortex_m::Peripherals::take()`/`steal()` exists in this repo, in
    // daisy-embassy, or in embassy-stm32 (whose `Peripherals::take()` is its own
    // generated struct). We touch only these two fields. `steal` rather than `take`
    // because a panic path must not depend on whether the singleton was claimed.
    let mut cp = unsafe { cortex_m::peripheral::Peripherals::steal() };
    cp.DCB.enable_trace();                 // DEMCR.TRCENA: CM7 may ignore CYCCNTENA without it
    cortex_m::peripheral::DWT::unlock();   // LAR: H7 locks the DWT after power-on
    if !cortex_m::peripheral::DWT::has_cycle_counter() {
        return false;
    }
    cp.DWT.enable_cycle_counter();
    cortex_m::peripheral::DWT::cycle_counter_enabled()
}

#[cfg(not(target_arch = "arm"))]
fn cycle_counter_running() -> bool {
    false
}

#[cfg(target_arch = "arm")]
fn cycle_count() -> u32 {
    cortex_m::peripheral::DWT::cycle_count()
}

#[cfg(not(target_arch = "arm"))]
fn cycle_count() -> u32 {
    0
}
```

Give the `not(target_arch = "arm"))] fn cycle_counter_running` a one-line comment saying *why* false is the right answer off-device (the registers are at `0xE000_1000`, which exists only on the target; reading it on a host would fault, and there is nothing to deliver over CDC on a host anyway).

Do **not** sample `cycle_count()` twice at startup to prove liveness. `CTRL` and `CYCCNT` are behind the same `LAR` lock, so the `cycle_counter_enabled()` readback already catches the locked/frozen case, and a double read adds a timing assumption to a function whose whole job is to avoid them.

### 2c. `SpinBudget`: monotone by construction, and never dependent on hardware alone

```rust
/// A wall-clock budget for a spin that may run with interrupts masked.
///
/// Two bounds, whichever expires first wins, and neither one assumes an interrupt fires:
/// processor cycles when DWT's counter is going, and poll iterations always. See
/// [`EMIT_TIMEOUT_CYCLES`] for why the crate's own `embassy_time::Instant` cannot be the
/// clock here.
struct SpinBudget {
    /// Cycle samples are u32 and wrap; elapsed total is accumulated in u64 so they may.
    limit_cycles: u64,
    elapsed_cycles: u64,
    prev_sample: u32,
    counting: bool,
    polls: u32,
}

impl SpinBudget {
    fn start() -> Self { /* cycle_counter_running(), one initial sample, zeros elsewhere */ }

    /// Fold one CYCCNT sample into the elapsed total and report whether the budget is spent.
    ///
    /// Monotone by construction: `elapsed_cycles` only ever grows, because a u32 delta
    /// between two adjacent samples is exact modulo 2^32 however many times the counter
    /// has wrapped — adjacent samples are microseconds apart, never 4.29e9 cycles apart.
    /// Deliberately not `now - anchor`: that form silently goes backwards on a wrap, and
    /// `embassy_time::Instant`'s own `Sub` panics on the same class of race
    /// (embassy-rs/embassy#5545), which a panic handler cannot survive.
    fn expired(&mut self, sample: u32) -> bool { /* wrapping_sub accumulate + both comparisons */ }
}
```

Behaviour the implementation must have, and each is an AC:

- `start()` calls `cycle_counter_running()` once, samples `cycle_count()` once into `prev_sample`, and stores `counting`. If `counting` is false, `expired()` returns `true` on the first call — the message is lost either way, so exiting immediately is strictly better than spinning against a clock that will never advance, and it is what makes a host build fall out safely.
- `expired()` accumulates `sample.wrapping_sub(self.prev_sample) as u64` into `elapsed_cycles`, updates `prev_sample`, increments `polls`, and returns `elapsed_cycles >= limit_cycles || polls >= EMIT_TIMEOUT_MAX_POLLS`.
- The iteration cap must be documented as a *ceiling on termination*, not as a schedule: 20_000_000 polls is chosen so it cannot preempt the intended bound in normal operation — spending the cycle budget first requires the poll body to average **more than 72 cycles** (1_440_000_000 / 20_000_000), and one iteration polls `UsbDevice::run()` plus a CDC write future, which is orders of magnitude more than 72 cycles. Say the threshold explicitly instead of measuring, in the spirit of `lib.rs:326-343`: this crate cannot measure it, so state the units and the crossover. Its only job is that no counter behaviour — absent, locked, frozen mid-spin, or counting at an unexpected rate — can turn the spin back into an infinite one.
- Keep `SpinBudget`'s public surface to `start()` and `expired(sample)`. Do not expose a "remaining" accessor nobody calls.

### 2d. The loop (`usb.rs:416-429`) and its doc

Replace the deadline computation and `while Instant::now() < deadline { .. }` with:

```rust
    let mut cx = Context::from_waker(Waker::noop());
    let mut budget = SpinBudget::start();
    while !budget.expired(cycle_count()) {
        if fut.as_mut().poll(&mut cx).is_ready() {
            return;
        }
    }
```

Move the `Timer`-rationale comment block out of the loop body into the `EMIT_TIMEOUT_CYCLES` doc (§2a item 5) and leave one short pointer here. Update the two sentences in `emit_blocking`'s doc that promise "after [`EMIT_TIMEOUT`]" (`:375-376`) to name the new constants, and keep the existing "assumes interrupts are live; does not make them live" paragraph from TASK-045 — it is about the *transport*, whereas the new text is about the *clock*. Those are two different assumptions and the doc should say so in one sentence each, because conflating them is how this bug survived.

## 3. Doc corrections outside the changed lines

1. `panic_handler.rs:61-64` — the sentence "The emit is time-bounded, takes no lock, allocates nothing, and never panics" is now doing more work than it says: name the clock ("bounded by processor cycles, not by the time driver"), since the whole point is that the halt path no longer depends on an ISR it cannot restore. Leave the rest of that comment (and the deliberate no-`bkpt` reasoning at `:88-94`) alone.
2. `usb.rs` module doc diagram (`:12`) needs no change; the route is the same.
3. `now_ms()` (`usb.rs:125-127`) still stamps frames with `Instant`. Add one sentence recording that under the same masked condition the `t_ms` field is stale — cosmetic, because loss detection keys on `seq` (`console.rs:110-112`, `take_seq()` inside the record lock), not on `t_ms`, and the frame CRC does not care. Fixing the timestamp is not this ticket's job: a wrong millisecond on a final record is a nuisance, an unbounded spin is not.
4. Do not fix the unrelated stale claim in `task-030.02 …md:372` (completed-ticket text is historical record); §0 records the correction, and the new code's doc comments carry the truth forward.

## 4. Tests (device-free, and where they must live to run at all)

Measured at HEAD, so nobody has to rediscover it: `nix develop -c cargo test -p asperitas-logging --features log-usb` **does build and pass on host** (`Finished test profile`, `Running unittests src/lib.rs`, and the doc-test list grows `usb.rs - usb::run ... ignored`). That corrects TASK-045's Implementation Note that the `log-usb` code "does not even link on host" — it compiles *and* links for the lib test binary; what it lacks is a CI job that turns the feature on. CI runs `cargo test --workspace` and `--features asperitas-pod/pod-hw` (`ci.yml:33,36`), neither of which enables `log-usb`.

That decides the placement: **a test CI never runs is not a test**, and editing `ci.yml`/`lefthook.yml` to add a third feature matrix entry is scope this ticket does not need. Put `SpinBudget` and the two arch-gated helpers in their own module, `crates/asperitas-logging/src/spin_budget.rs`, declared in `lib.rs` as:

```rust
#[cfg(any(feature = "log-usb", test))]
mod spin_budget;
```

The `any(..., test)` is what keeps the default-feature firmware build warning-free: with only `boot-led` and no tests running, an unconditional module whose sole user is `usb.rs` would trip `dead_code` under `-D warnings`. The module's doc comment carries the one-sentence reason it is not simply `log-usb`-gated — the bound serves only the panic path today, but its arithmetic has to be exercisable on a machine with no Cortex-M in it. Keep it `pub(crate)`; do not widen the crate's public surface for a test. This follows the crate's existing instinct (`frame.rs` and `console.rs` are default-compiled and host-tested precisely so the logic is reachable; `dump.rs` likewise).

Fallback if the cfg gymnastics fights you: leave everything in `usb.rs`, and then AC #7 must instead be satisfied by adding `nix develop -c cargo test -p asperitas-logging --features log-usb` to both `ci.yml` and `lefthook.yml`'s pre-push block, and say so in Implementation Notes.

Either way the tests drive `expired(sample)` with synthetic samples, which is exactly why §2c puts the sample in the argument list instead of reading the register inside:

- `wrap_does_not_send_the_budget_backwards` — anchor near `0xFFFF_FFF0`, sample past zero, assert `elapsed_cycles` grew by the correct small amount and that `expired()` is false well before the limit.
- `counts_exactly_at_the_limit` — feed a sample sequence totalling `limit_cycles - 1` (assert false) then `limit_cycles` (assert true). Boundary matters: `<` versus `>=` is the difference between "≤ 3 s" and "≤ 3 s + one full wrap".
- `frozen_counter_still_terminates` — `counting: true` with the same sample repeated; assert `expired()` becomes true purely at `EMIT_TIMEOUT_MAX_POLLS`.
- `absent_counter_expires_immediately` — `counting: false`; assert the first call returns true.
- `elapsed_never_decreases_across_arbitrary_samples` — a deterministic pseudo-random walk over u32 samples asserting monotone non-decrease of `elapsed_cycles`; proptest is already a dev-dependency if you prefer it, but a fixed seed is enough and cheaper to debug.

No test may require a target build, and no `#[should_panic]` anywhere: this code's entire contract is that it does not panic.

## 5. Verification, in this order

1. `nix develop -c cargo fmt --all --check`
2. `nix develop -c cargo clippy -p asperitas-logging --all-targets -- -D warnings`
3. `nix develop -c cargo test -p asperitas-logging` (this is what runs the new `spin_budget` tests under default features) and `nix develop -c cargo test -p asperitas-logging --features log-usb` (this is what proves `usb.rs` still builds on host)
4. `nix develop -c cargo test --workspace` and `nix develop -c cargo test --workspace --features asperitas-pod/pod-hw`
5. `nix develop -c cargo doc -p asperitas-logging --no-deps` and the same with `--features boot-led,log-usb,log-defmt` — no new rustdoc warnings (pre-empts TASK-043.02's gate)
6. `cd firmware && nix develop -c cargo build --release --features seed3`
7. `cd firmware && nix develop -c make clippy` (the only place firmware clippy runs — `firmware/Makefile:178-180`)
8. `git diff --stat Cargo.lock firmware/Cargo.lock` → must be empty (no new dependency, no re-resolution)
9. Machine check that no `Instant` remains in the panic path: `grep -n "Instant" crates/asperitas-logging/src/usb.rs` → only `now_ms()` and its import.

## 6. Considered and rejected — record the reason, do not re-litigate

- **Keep `Instant`, treat a backwards `now()` as expiry.** Fixes non-monotonicity only. Under a frozen driver there is no backwards jump often enough: the clock plateaus inside a 2 s window below a 3 s deadline, so the spin still never ends (§0.1). Rejected on measurement.
- **Lower `EMIT_TIMEOUT`.** The unit, not the number, is broken (§0.2). Any tick-based bound ≥ 2 s is unreachable whenever the ISR is stalled.
- **Read TIM5's raw `CNT` through `pac::TIM5` and count its wraps locally.** Free-running and dependency-free, and it keeps the seconds-as-units, but it reaches into a peripheral a third-party driver owns, its geometry (`2^16`, `TICK_HZ`, the psc truncation) is duplicated knowledge, and a daisy-embassy bump changing the time-driver feature silently changes what the panic path is counting. DWT is a processor block with a published API and no owner.
- **SysTick as a poll-only free-running counter** (`TICKINT` clear, poll `VAL`): standard practice and unaffected by masking, but it is 24 bits (≈ 35 ms at 480 MHz, so wrap bookkeeping is mandatory rather than defensive), it needs a global side effect on a peripheral the HAL happens not to want today, and probe tooling conventionally owns SysTick. Strictly more machinery than DWT for the same guarantee.
- **Re-enable interrupts before the emit loop** (clear PRIMASK/FAULTMASK in the panic handler). Textbook for fault handlers, and it would make `Instant` correct again — but it lets the SAI/audio ISRs run while the executor is dead and the audio callback's state is inconsistent, and it does nothing for the FAULTMASK or priority-ceiling cases, which are the same failure with a different cause. TASK-045 chose to move the panic out of the lock rather than unmask inside it; this ticket keeps that decision consistent by fixing the clock instead.
- **Enable DWT at boot in `usb::init()`.** Works, but adds a boot-ordering invariant ("nothing may disable trace between init and the panic") in exchange for three register writes moved earlier. Enabling at the top of the spin is idempotent and self-evidently current. If TASK-038.03's rig instrumentation later wants CYCCNT continuously, moving the enable earlier is a reasonable follow-up — it does not change anything here except deleting one call.

## 7. Out of scope, and where the bench question goes

- **Which bound actually fires on real silicon is a bench question, and this design does not need the answer to be correct.** The measurable claim nobody can check from source is "DWT's CYCCNT counts on an STM32H750 after `TRCENA` + `LAR` unlock". If it holds, the cycle bound expires at ~3 s. If it does not, `cycle_counter_enabled()` reads back false and the board halts at the red LED within milliseconds instead. Either way the board stops spinning, which is the property the ticket's ACs are written against, and the ticket explicitly forbids an AC that needs ears and a bench. Recorded as a check to perform on the existing hardware family: comment appended to **TASK-030.04** (`@human`, To Do) asking whoever rigs the console capture to note, when flashing `panictest`, whether the `PANIC:` record still arrives and how long the pre-halt pause is — that observation distinguishes the two branches at no extra setup cost. Deliberately **not** a child of this ticket: a `@human` child would inherit onto this parent and leave it unclosable with no agent work remaining, which is the TASK-004 failure mode recorded in CLAUDE.md.
- **TASK-047** (the `debug_assert!`s still inside `RECORD_BUFS`'s critical section, debug-profile only) reaches this spin with PRIMASK stuck set. Before this change that meant an unbounded hang in a debug build; after it, a bounded one. Worth one sentence in TASK-047's eventual plan pointing here — do not edit that ticket now.
- **Same-shaped hazard on the other transport:** `defmt-rtt` blocks forever when no probe drains its buffer unless `disable-blocking-mode`/`drop-on-contention` is set (knurling-rs/defmt#133, #818). A transport that assumes a reader exists, exactly like the CDC path. Not fixable from here; `docs/reference/daisy-seed3.md` already records it, and it belongs to the `log-defmt`/probe family (TASK-036 lineage) rather than this ticket.
- Do not add an `EMIT_TIMEOUT` accessor, a feature flag for the bound, or a runtime-settable timeout. Two constants and a decision.

## 8. In the Final Summary

State explicitly: (a) the bound is now processor cycles (480 MHz × 3 s) accumulated from DWT CYCCNT deltas plus an unconditional 20 M-poll ceiling, and no part of the panic path reads `embassy_time` any more; (b) why the old bound was not merely imprecise but *deterministically* non-terminating whenever the TIM5 ISR cannot run — the frozen-window width (2.000 s) is smaller than the bound (3 s), with the simulation cited; (c) that the trigger class is broader than PRIMASK (FAULTMASK, priority ≥ TIM5's IRQ) and the docs now say so; (d) how the code degrades when CYCCNT is unavailable (immediate exit) and why the poll cap exists (termination must not depend on any hardware behaving); (e) that `Instant` subtraction is avoided on purpose because its `Sub` panics on backwards and a second panic in the handler recurses; (f) where the tests live and which command runs them; (g) that the bench confirmation of which bound fires is recorded on TASK-030.04, not here, and that nothing in this ticket is markable `HUMAN:`.
<!-- SECTION:PLAN:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-10 12:30
---
Measurement behind plan 0.1 (run at planning time, so nobody has to take the claim on faith). `calc_now` from `embassy-stm32-0.6.0/src/time_driver/gp16.rs:81-83` simulated in Python: `period` pinned (the ISR cannot run), `CNT` advanced one tick per step for 6 s of ticks at 32768 Hz, deadline = `now()` at entry + 98304 ticks (3 s). Twelve cases: `period` even and odd, `CNT` starting at 0x0000, 0x3FFF, 0x7FFF, 0x8000, 0xC000, 0xFFFE.

    P=100 start_cnt=0x0000 now0-base=0x00000 max_now-base=0x0ffff span=2.000s deadline_reachable_within_6s=False
    P=100 start_cnt=0x3FFF now0-base=0x03fff max_now-base=0x0ffff span=2.000s deadline_reachable_within_6s=False
    P=100 start_cnt=0x7FFF now0-base=0x07fff max_now-base=0x0ffff span=2.000s deadline_reachable_within_6s=False
    P=100 start_cnt=0x8000 now0-base=0x08000 max_now-base=0x0ffff span=2.000s deadline_reachable_within_6s=False
    P=100 start_cnt=0xC000 now0-base=0x0c000 max_now-base=0x0ffff span=2.000s deadline_reachable_within_6s=False
    P=100 start_cnt=0xFFFE now0-base=0x0fffe max_now-base=0x0ffff span=2.000s deadline_reachable_within_6s=False
    P=101 start_cnt=0x0000 now0-base=0x08000 max_now-base=0x0ffff span=2.000s deadline_reachable_within_6s=False
    P=101 start_cnt=0x3FFF now0-base=0x0bfff max_now-base=0x0ffff span=2.000s deadline_reachable_within_6s=False
    P=101 start_cnt=0x7FFF now0-base=0x0ffff max_now-base=0x0ffff span=2.000s deadline_reachable_within_6s=False
    P=101 start_cnt=0x8000 now0-base=0x00000 max_now-base=0x0ffff span=2.000s deadline_reachable_within_6s=False
    P=101 start_cnt=0xC000 now0-base=0x04000 max_now-base=0x0ffff span=2.000s deadline_reachable_within_6s=False
    P=101 start_cnt=0xFFFE now0-base=0x07ffe max_now-base=0x0ffff span=2.000s deadline_reachable_within_6s=False

Readout: with the driver ISR stalled, `now()` is confined to a window exactly 2^16 ticks = 2.000 s wide no matter where it starts or which parity `period` has, so a 3 s deadline is unreachable in every case — not roughly half of them. Two consequences the plan encodes: (a) "treat a backwards `now()` as expiry" cannot work, because the failure is a plateau below the deadline, not a backwards jump; (b) the *unit* is unsound, so lowering `EMIT_TIMEOUT` is not a fix. Note also that this contradicts nothing in the ticket's description except its soft "can stop terminating" wording, which understates it.

Two more measurements taken while planning, both recorded in plan section 1: `cargo tree --offline -i embassy-time-driver -f "{p} {f}"` in `firmware/` shows `embassy-stm32 v0.6.0` with `time-driver-tim5` and `embassy-time v0.5.1` with `tick-hz-32_768`, selected by daisy-embassy at pin `ca9bcc9` rather than by this repo; and `nix develop -c cargo test -p asperitas-logging --features log-usb` builds and passes on host at HEAD (doc-test list includes `usb.rs - usb::run ... ignored`), which is why the new tests go in an always-test-compiled module rather than inside `usb.rs`. Baseline at HEAD measured green: `cargo fmt --all --check`, `cargo clippy -p asperitas-logging --all-targets -- -D warnings`, `cargo test -p asperitas-logging`.
---
<!-- COMMENTS:END -->
