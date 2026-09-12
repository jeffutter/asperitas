//! A wall-clock budget for a spin that may run with interrupts masked.
//!
//! Built for the panic-path emit loop in `usb::emit_blocking`: after a panic the board must
//! still stop spinning within a predictable time even when nothing can deliver an interrupt.
//! Two bounds, whichever expires first wins, and neither one assumes an interrupt fires.
//!
//! Compiled under `any(feature = "log-usb", test)` rather than `log-usb` alone because the
//! bound serves only the panic path today, but its arithmetic has to be exercisable on a
//! machine with no Cortex-M in it: CI runs `cargo test --workspace` with default features,
//! which enable no backend at all, and a test CI never runs is not a test. An unconditional
//! module whose sole user is feature-gated would trip `dead_code` under `-D warnings` in the
//! default-feature firmware build, hence the `test` escape hatch instead of a bare gate.

/// How long the panic-path emit loop will try before giving up, in processor cycles.
///
/// This replaces an `embassy_time::Instant` deadline (`while Instant::now() < deadline`
/// against a 3 s `Duration`). That shape looked like "just a counter read" and it is one —
/// but the counter is assembled from an ISR-fed word. The GP16 time driver computes `now()`
/// as `(period << 15) + CNT` and increments `period` **only in the timer ISR** (TIM5 here,
/// selected by daisy-embassy's `embassy-stm32` features, ticked at 32 768 Hz). When that ISR
/// cannot run, the `period` word freezes while the 16-bit `CNT` free-runs, so `now()` is
/// confined to a window exactly 2^16 ticks = 2.000 s wide — narrower than the 3 s it was
/// compared against. The old deadline was therefore unreachable *every* time the ISR was
/// stalled, not roughly half of them: the **unit was broken, not merely imprecise**, and
/// lowering the number would not have fixed it. Do not "fix" this constant by tuning it.
///
/// The trigger class is "the time-driver ISR cannot run", which is broader than `PRIMASK`:
/// it covers `FAULTMASK` in a fault path and any execution context at a priority >= TIM5's
/// IRQ priority — a panic raised inside the SAI/audio ISR being the realistic one.
///
/// The arithmetic: 480 MHz x 3 s = 1_440_000_000 cycles. CYCCNT is a u32 and wraps every
/// 2^32 / 480e6 = 8.948 s; [`SpinBudget`] accumulates u32 deltas so the wrap is handled
/// rather than trusted away. The cycle rate is the *processor* clock, so the bound is
/// exactly 3 s only while SYSCLK stays at daisy-embassy's 480 MHz `default_rcc`; if someone
/// lowers the clock — say to 240 MHz — the same count stretches proportionally, to 6 s.
///
/// Worst-case wall-clock, per host state:
///
/// - Host attached: exits the moment the last packet is accepted, as before.
/// - Host absent, interrupts live: the CDC write future stays `Pending` and the loop ends
///   at <= 3 s plus one poll iteration; the board then halts at the red LED with the record
///   lost.
/// - Host absent *and* the time-driver ISR unable to run: also <= 3 s, which is the case
///   this constant exists for — before it, that case never ended.
/// - Counter unavailable at all ([`cycle_counter_running`] reporting false): the budget is
///   spent before the loop runs once, so the board halts within milliseconds.
///
/// Deliberately not an `embassy_time::Timer` either, as the replaced code already said:
/// `Timer::poll` calls `schedule_wake(.., cx.waker())` on every `Pending` poll, so using one
/// here would push a no-op waker into the time driver's queue on every iteration of a loop
/// that may exist precisely because that driver is stalled. A counter read cannot fail, and
/// that is the whole point.
const EMIT_TIMEOUT_CYCLES: u64 = 480_000_000 * 3;

/// Second, unconditional bound on the same spin: poll iterations, not cycles.
///
/// A ceiling on termination, not a schedule. Its only job is that no counter behaviour —
/// absent, locked, frozen mid-spin, or counting at an unexpected rate — can turn the spin
/// back into an infinite one. It is chosen so it cannot preempt the intended bound in normal
/// operation: spending the cycle budget first would require the poll body to average fewer
/// than 1_440_000_000 / 20_000_000 = 72 cycles per iteration, and one iteration polls
/// `UsbDevice::run()` plus a CDC-write future, which is orders of magnitude more than 72.
///
/// Stated as arithmetic rather than measurement on purpose: this crate cannot measure its
/// own timing. That is the same reason `lib.rs` bounds the commit critical section "in bytes
/// and iterations instead of microseconds" — name the units the code can actually count.
const EMIT_TIMEOUT_MAX_POLLS: u32 = 20_000_000;

/// Bring up DWT's cycle counter and report whether it is actually counting.
///
/// Called from the panic path, immediately before the spin that measures itself against it,
/// rather than from `usb::init`: that ordering contract is the difference between "the
/// counter was running when some other code disabled it" and "we know what state we left it
/// in", and it costs nothing because enabling is idempotent.
///
/// ARM only. On a host build this reports false and the cycle bound stands down entirely —
/// the first `target_arch` cfg in this crate, because `log-usb` deliberately compiles for
/// host too and a cargo feature cannot express "not this machine".
#[cfg(target_arch = "arm")]
pub(crate) fn cycle_counter_running() -> bool {
    // Safety: `DCB` and `DWT` are Cortex-M system peripherals. `rig.rs` claims
    // `cortex_m::Peripherals::take()` in the firmware package, so this cannot count on the
    // singleton being free - which is exactly why it uses `steal()`: a panic path must not behave
    // differently depending on whether some other binary got there first. Aliasing the singleton is
    // harmless here because both users only *enable* tracing (idempotent) and read CYCCNT, and
    // neither ever writes it, so the counter's meaning does not depend on who ran first. Neither
    // this crate nor daisy-embassy claims it at all, and embassy-stm32's `Peripherals::take()`
    // returns its own generated struct, not this one.
    let mut cp = unsafe { cortex_m::peripheral::Peripherals::steal() };
    cp.DCB.enable_trace(); // DEMCR.TRCENA: CM7 may ignore CYCCNTENA without it
    cortex_m::peripheral::DWT::unlock(); // LAR: H7 locks the DWT after power-on
    if !cortex_m::peripheral::DWT::has_cycle_counter() {
        return false;
    }
    cp.DWT.enable_cycle_counter();
    // The readback is the liveness proof: `CTRL` and `CYCCNT` sit behind the same `LAR`
    // lock, so a DWT that stayed locked fails here too — no need to sample the counter
    // twice and add a timing assumption to a function whose whole job is to avoid them.
    cortex_m::peripheral::DWT::cycle_counter_enabled()
}

/// Off-device the counter registers live at 0xE000_1000, an address that exists only on the
/// target — reading it on a host would fault, and there is nothing to deliver over CDC from
/// a host anyway. Reporting "unavailable" makes the budget spend itself immediately.
#[cfg(not(target_arch = "arm"))]
pub(crate) fn cycle_counter_running() -> bool {
    false
}

/// One sample of DWT's CYCCNT. See [`cycle_counter_running`] for why ARM-only.
#[cfg(target_arch = "arm")]
pub(crate) fn cycle_count() -> u32 {
    cortex_m::peripheral::DWT::cycle_count()
}

/// Constant zero off-device; paired with [`cycle_counter_running`] reporting false, so no
/// caller ever reads this value on a host build.
#[cfg(not(target_arch = "arm"))]
pub(crate) fn cycle_count() -> u32 {
    0
}

/// A wall-clock budget for a spin that may run with interrupts masked.
///
/// Two bounds, whichever expires first wins, and neither one assumes an interrupt fires:
/// processor cycles when DWT's counter is going, and poll iterations always. See
/// [`EMIT_TIMEOUT_CYCLES`] for why the crate's own `embassy_time::Instant` cannot be the
/// clock here.
pub(crate) struct SpinBudget {
    limit_cycles: u64,
    /// Cycle samples are u32 and wrap; the elapsed total is accumulated in u64 so they may.
    elapsed_cycles: u64,
    prev_sample: u32,
    counting: bool,
    polls: u32,
}

impl SpinBudget {
    /// Start a fresh budget, bringing up the cycle counter as a side effect.
    pub(crate) fn start() -> Self {
        let counting = cycle_counter_running();
        SpinBudget {
            limit_cycles: EMIT_TIMEOUT_CYCLES,
            elapsed_cycles: 0,
            prev_sample: cycle_count(),
            counting,
            polls: 0,
        }
    }

    /// Fold one CYCCNT sample into the elapsed total and report whether the budget is spent.
    ///
    /// Monotone by construction: `elapsed_cycles` only ever grows, because a u32 delta
    /// between two adjacent samples is exact modulo 2^32 however many times the counter has
    /// wrapped — adjacent samples are microseconds apart, never 4.29e9 cycles apart.
    /// Deliberately not `now - anchor`: that form silently goes backwards on a wrap, and
    /// `embassy_time::Instant`'s own `Sub` panics on the same class of race
    /// (embassy-rs/embassy#5545), which a panic handler cannot survive — a second panic
    /// inside `#[panic_handler]` recurses with no way out.
    ///
    /// With the counter unavailable the budget reports itself spent on the first call: the
    /// message is lost either way, so exiting immediately beats spinning against a clock
    /// that will never advance.
    pub(crate) fn expired(&mut self, sample: u32) -> bool {
        if !self.counting {
            return true;
        }
        self.elapsed_cycles += sample.wrapping_sub(self.prev_sample) as u64;
        self.prev_sample = sample;
        self.polls += 1;
        self.elapsed_cycles >= self.limit_cycles || self.polls >= EMIT_TIMEOUT_MAX_POLLS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Direct construction rather than `start()`: the register seam is inert on a host, so
    /// the tests drive `expired()` with synthetic samples — the reason the sample is an
    /// argument rather than a register read inside.
    fn budget(limit_cycles: u64, counting: bool) -> SpinBudget {
        SpinBudget {
            limit_cycles,
            elapsed_cycles: 0,
            prev_sample: 0,
            counting,
            polls: 0,
        }
    }

    #[test]
    fn start_begins_fresh_and_never_panics() {
        // `start()` is the entry point the panic path actually uses; on a host it takes the
        // counter-unavailable branch, so this exercises the seam where the register access
        // would be. Its contract anywhere is "no panic, nothing counted yet".
        let mut b = SpinBudget::start();
        assert_eq!(b.polls, 0);
        assert_eq!(b.elapsed_cycles, 0);
        // Off-device the counter seam reports unavailable, which must mean "budget already
        // spent"; on-device a fresh budget still holds its full cycle allowance.
        if cycle_counter_running() {
            assert!(!b.expired(b.prev_sample.wrapping_add(7)));
            assert_eq!(b.polls, 1);
        } else {
            assert!(
                b.expired(7),
                "an absent counter must spend the budget at once"
            );
        }
    }

    #[test]
    fn wrap_does_not_send_the_budget_backwards() {
        let mut b = budget(EMIT_TIMEOUT_CYCLES, true);
        b.prev_sample = 0xFFFF_FFF0;
        // Crosses zero: the true gap is 32 cycles. A `now - anchor` form would report a
        // negative gap here and an `Instant::Sub` would panic outright.
        assert!(!b.expired(0x0000_0010));
        assert_eq!(b.elapsed_cycles, 32);
        assert!(!b.expired(0x0000_0020));
        assert_eq!(b.elapsed_cycles, 48);
    }

    #[test]
    fn counts_exactly_at_the_limit() {
        // `<` versus `>=` is the difference between "<= 3 s" and "<= 3 s + one full wrap",
        // so the boundary is asserted on both sides.
        let mut b = budget(10, true);
        assert!(!b.expired(9));
        assert_eq!(b.elapsed_cycles, 9);
        assert!(b.expired(10));
        assert_eq!(b.elapsed_cycles, 10);
    }

    #[test]
    fn frozen_counter_still_terminates() {
        // A counter that is enabled but never advances (locked mid-spin, or sampled faster
        // than it ticks): the cycle bound contributes nothing and the poll ceiling ends the
        // spin on its own.
        let mut b = budget(u64::MAX, true);
        for _ in 0..EMIT_TIMEOUT_MAX_POLLS - 1 {
            assert!(!b.expired(0x1234_5678));
        }
        assert!(b.expired(0x1234_5678));
    }

    #[test]
    fn absent_counter_expires_immediately() {
        let mut b = budget(EMIT_TIMEOUT_CYCLES, false);
        assert!(b.expired(0));
        assert_eq!(b.polls, 0, "a dead clock must not count as an iteration");
    }

    #[test]
    fn elapsed_never_decreases_across_arbitrary_samples() {
        // Deterministic LCG walk over the whole u32 space — cheaper to debug than proptest
        // and just as adversarial for this contract.
        let mut b = budget(u64::MAX, true);
        let mut s: u32 = 0x9E37_79B9;
        let mut last = b.elapsed_cycles;
        for _ in 0..10_000 {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            assert!(!b.expired(s), "poll ceiling reached in a 10k-sample test");
            assert!(
                b.elapsed_cycles >= last,
                "elapsed went backwards at sample {s:#x}"
            );
            last = b.elapsed_cycles;
        }
    }
}
