//! The host's handle on the effect stage that follows the generator. Compiled on every build, not
//! only `stim-excerpt`: the sine, sweep and pulse builds run the same stage.
//!
//! `CTL FX f=<2 hex> v=<4 hex>` selects the effect stage that follows the generator: f 00 bypass (the
//! default), 01 the `asperitas_dsp::Asperitas` patch, the same filter-then-gain chain `main` runs, at
//! its default knob positions (filter open, 0 dB). v is accepted for wire stability and ignored.
//! Bypass touches no sample, so default captures are bit-exact with the bare generator. The
//! effect runs after `take_block`, so EXCPLAY's CRC still describes the raw stored samples.
//! The pass is held until the capture's first callback, which is `ARM_DELAY_MS` after audio starts,
//! so the control must arrive in that window; it is refused with `busy` once a capture is armed.
//! `select_fx` publishes under a critical section and the callback picks it up at the next block.
//!
//! `CTL SET k=<00|01> v=<4 hex>` / `CTL REL k=<00|01>` override knob 0 (filter cutoff) or knob 1
//! (output gain) through `host_control`'s override table, the one the shipped firmware uses. Unlike `FX` it is accepted while a capture is armed, so `asperitas-rig`'s override sweep
//! moves the effect mid-capture. Each callback merges the overrides over the patch's default knob
//! positions (the "pot reading") and calls `ReplayFx::set_knobs`, which acts only when the merged
//! pair changed; the override therefore enters UPSTREAM of the effect's smoothing, exactly where
//! `main` applies it. Releasing returns a knob to its default. Bypass ignores both. Knob ids other
//! than 0 and 1 get `bad-knob`.
//!
//! Feature `unsmoothed-params` builds the effect without smoothing (`ReplayFx::new_unsmoothed`): the
//! parameters step on the next sample. Deliberately click-prone, for proving the discontinuity
//! detector sees steps; the default build keeps the shipped smoothing.
//!
//! Callback cost: when nothing changes, one critical section around a two-element copy plus a
//! compare, a few tens of cycles. While a value moves, one `set_controls` per block re-derives the
//! parameters (a `powf` for gain, a `powf` and `expf` for the filter).

use asperitas_dsp::fx_chain::FxChain;
use asperitas_dsp::processor::{Frame, Processor};
use core::cell::Cell;
use core::sync::atomic::{AtomicU32, Ordering};
use cortex_m::interrupt::Mutex;

/// The pending effect selection `(kind code, value)` and a counter the callback compares with the
/// one it last applied, so the common case is a single relaxed atomic load.
static FX_REQUEST: Mutex<Cell<(u8, u16)>> = Mutex::new(Cell::new((0, 0)));
static FX_GENERATION: AtomicU32 = AtomicU32::new(0);

/// Request effect `code` at `value`. `false` for an unknown code, changing nothing.
#[cfg(feature = "log-usb")]
pub fn select_fx(code: u8, value: u16) -> bool {
    if asperitas_dsp::replay_fx::FxKind::from_code(code).is_none() {
        return false;
    }
    cortex_m::interrupt::free(|cs| FX_REQUEST.borrow(cs).set((code, value)));
    FX_GENERATION.fetch_add(1, Ordering::Release);
    true
}

/// Render one block: the generator, then the effect, applying a pending `FX` first.
///
/// `fx_seen` is the generation this stage last applied. The callback's cost beyond the bare
/// generator is one relaxed atomic load in bypass; with the patch selected it adds 32 effect
/// ticks per block. No await, no flash access, no allocation; the
/// critical sections are a handful of cycles and only run when a request is pending or an
/// effect is selected.
pub fn render<G: Processor>(chain: &mut FxChain<G>, fx_seen: &mut u32, block: &mut [Frame]) {
    let generation = FX_GENERATION.load(Ordering::Acquire);
    if generation != *fx_seen {
        *fx_seen = generation;
        let (code, value) = cortex_m::interrupt::free(|cs| FX_REQUEST.borrow(cs).get());
        chain.select(code, value);
    }
    chain.process_block_with(block, |base| {
        // Upstream of smoothing, as `main` does.
        asperitas_firmware::host_control::merge(base)
    });
}
