//! Excerpt replay: the `stim-excerpt` generator, which plays a slot installed by the `install`
//! module.
//!
//! The audio callback never touches QSPI: it pops 32-sample blocks from `asperitas_replay::ReplayCore`, a two-half ping-pong in
//! internal RAM whose accounting (which side may touch which half, the underrun policy, the CRC) is
//! host-tested in that crate. Everything here is the part a host cannot test: the flash read that fills
//! a half, the cache argument, and the reporting.
//!
//! ### Boot order
//!
//! `load` runs in `main` between the flash build and `prepare_interface`: it reads and validates the
//! slot header, starts the pass, and fills BOTH halves before any audio exists. The callback therefore
//! finds a `Ready` half on its first block and the SAI start timing is unchanged. An empty or invalid slot
//! does not play silence: `halt_unplayable` sends `EXCPLAYFAIL`, turns the LED red and serves the console
//! forever with no audio.
//!
//! ### When the pass runs
//!
//! `ExcerptPlayer::one_shot_samples` is `Some`, so the audio task holds the player silent until the
//! capture's first callback and then records where it started (`STIMSTART`), exactly as for the sweep:
//! the recording holds the excerpt whole. `reset()` is therefore a no-op - the pass was started and
//! staged at boot, and restarting it from the callback would mean flash work at audio priority.
//!
//! ### Where the staging lives, and the D-cache
//!
//! The staging is `REPLAY`, placed by name in the `.replay_bss` section (NOLOAD, zeroed at boot by
//! `zero_staging`) and always in AXI SRAM: the whole RAM region under `memory-internal.x`, and the 32 KB
//! `AXI_TAIL` above the 480 KB bootloaded image under `memory-bootloaded.x`. AXI SRAM is cacheable by the
//! default memory map. It is deliberately not the DTCM that plain `.bss` is under the bootloaded layout:
//! MDMA, the master `Flash::read_async` uses, reaches the TCMs only through its separate AHBS
//! port, DMA1/DMA2 cannot reach DTCM at all, and nothing here confirmed from RM0433 that an MDMA refill into
//! DTCM lands. `scripts/firmware-variants.sh` (the `replay` action) checks the symbol's address in the bootloaded image.
//! The argument for either cache state, which `fill_half` carries out:
//!
//! * D-cache off (nothing in this stack enables it today; RIGCFG reports the live bit): MDMA's writes
//!   are the only writes, loads go to memory, no maintenance is needed and none is done.
//! * D-cache on (the buffer is in AXI SRAM): MDMA writes memory behind the cache. The CPU never writes a
//!   staging byte (the callback only reads; `take_block` takes `&self` and the halves are written only
//!   through a `FillClaim`), so no line over a half is ever dirty and nothing can be written back over
//!   the DMA's data. The only hazard is a stale clean line from the half's previous contents, so after
//!   each read completes `fill_half` invalidates the half's cache lines before publishing it. The
//!   range is rounded up to a whole line; halves are 32-byte aligned and a multiple of 32 long
//!   (compile-time asserts in `replay`), so the rounding stays inside the half.
//!
//! ### The interrupt budget the callback keeps
//!
//! Added to the SAI1 callback by this path: one `take_block` per callback. Worst case it is a CRC-16
//! over 64 bytes (`crc16_ccitt_update` is the bitwise form: 8 shift-and-conditional-xor steps per byte,
//! priced pessimistically at 6 cycles each, so 64 x 8 x 6 = 3 072 cycles), 32 sample unpacks and 32
//! float conversions (about 10 cycles each, 640), a few atomic loads and stores (under 100), and the
//! block-boundary half hand-off (about 50): roughly 3 900 cycles, 8.2 us at 480 MHz. That is about 1.2%
//! of the 666 us period against the ~590 us of headroom the sine generator leaves today, and it replaces
//! the generator's own per-sample work rather than adding to it, so `max_block_us` is not worsened by
//! more than the estimate and is expected to fall. An estimate, not a reading: the observed figure is
//! `CAPSTAT`'s `max_block_us` on hardware. No bench claim is made here.
//!
//! ### What EXCPLAY's CRC says
//!
//! It is the CRC of the samples handed to the DAC encoder (`asperitas_replay::PassReport::crc16`). With `want`
//! (the stored header CRC) and `clean=1` it says QSPI-to-encoder delivered the excerpt intact and
//! unbroken. It says nothing about the codec, the analog path or the cable.

use crate::emit_console;
use crate::install::{flash_of, FLASH};
use asperitas_dsp::processor::{Frame, Processor};
use asperitas_dsp::stimulus::Stimulus;
use asperitas_logging::{error, info};
use asperitas_replay::{
    FillClaim, PassReport, ReplayCore, ASSUMED_SCHEDULING_LATENCY_MICROS, BLOCK_SAMPLES,
    BYTES_PER_SAMPLE,
};
use asperitas_wire::console;
use asperitas_wire::excerpt::{self, SlotHeader, HEADER_LEN};
use core::fmt::Write as _;
use cortex_m::peripheral::SCB;

/// The slot this build plays: `ASP_RIG_EXCERPT_SLOT` when set, otherwise 0.
///
/// Read with `option_env!`, so rustc tracks the variable and changing it rebuilds.
pub const SLOT: u8 = match option_env!("ASP_RIG_EXCERPT_SLOT") {
    Some(text) => parse_slot(text),
    None => 0,
};

/// Parse a slot number at compile time. Anything but a whole number below `SLOT_COUNT` fails the
/// build, naming the variable, instead of falling back to a slot nobody asked for.
const fn parse_slot(text: &str) -> u8 {
    let bytes = text.as_bytes();
    assert!(!bytes.is_empty(), "ASP_RIG_EXCERPT_SLOT is set but empty");
    let mut slot = 0u32;
    let mut i = 0;
    while i < bytes.len() {
        assert!(
            bytes[i].is_ascii_digit(),
            "ASP_RIG_EXCERPT_SLOT must be a whole slot number"
        );
        slot = slot * 10 + (bytes[i] - b'0') as u32;
        // Bail before the accumulator can overflow on a long digit string.
        assert!(
            slot < excerpt::SLOT_COUNT as u32,
            "ASP_RIG_EXCERPT_SLOT is not below excerpt::SLOT_COUNT"
        );
        i += 1;
    }
    slot as u8
}

const _: () = assert!(parse_slot("0") == 0);
const _: () = assert!(parse_slot("13") == 13);

/// Flash address of the slot's first PCM byte.
const PCM_BASE: u32 = match excerpt::slot_pcm_base(SLOT) {
    Some(base) => base,
    None => panic!("SLOT is validated by parse_slot"),
};

/// How long the refill task sleeps when no half is free to claim, in milliseconds.
///
/// Chosen against the staging table in `replay`: a half plays for 85 ms, and the table budgets
/// `ASSUMED_SCHEDULING_LATENCY_MICROS` (10 ms) for the refill task to get to run. A poll period
/// of at most half of that keeps the poll's own contribution to the latency inside the budget it
/// already assumed, and a wake every 2 ms on an otherwise idle thread executor is negligible.
const REFILL_POLL_MS: u64 = 2;
const _: () = assert!(
    REFILL_POLL_MS * 1_000 <= ASSUMED_SCHEDULING_LATENCY_MICROS as u64 / 2,
    "the refill poll period eats the scheduling latency the staging table budgets"
);

/// The staging ping-pong. See "Where the staging lives" in the module docs for what region it is in.
///
/// `.replay_bss` is NOLOAD in both link layouts, so nothing initialises it: [`zero_staging`] must
/// run before the first use. All-zero is a valid idle state (`PHASE_IDLE`, `HalfState::Empty`).
#[link_section = ".replay_bss"]
static REPLAY: ReplayCore = ReplayCore::new();

/// Zero the staging. Called once from `main`, before [`load`] and before any other task exists.
pub fn zero_staging() {
    // SAFETY: called once at boot, single-threaded, before the SAI interrupt or any refill can
    // touch the core. `addr_of!` takes the address without making a reference. The core is
    // plain integers and atomics for which all-zero bytes are valid.
    unsafe {
        core::ptr::write_bytes(core::ptr::addr_of!(REPLAY).cast_mut(), 0, 1);
    }
}

/// Whether the pass has been started and has not yet completed.
///
/// `load` starts the pass before anything else can ask, so "no report yet" means "open". Only the
/// install gate asks, and that exists only with the console.
#[cfg(feature = "log-usb")]
pub fn pass_open() -> bool {
    REPLAY.finish().is_none()
}

/// One cache line, aligned like one, for the header read's destination.
#[repr(align(32))]
struct Line([u8; 32]);
const _: () = assert!(HEADER_LEN <= 32);

/// Discard any cached copy of `buf`'s lines after a DMA wrote it. See the section comment.
fn invalidate_after_dma(scb: &mut SCB, buf: &mut [u8]) {
    if !SCB::dcache_enabled() {
        return;
    }
    let addr = buf.as_ptr() as usize;
    debug_assert!(addr.is_multiple_of(32), "DMA buffer is not line aligned");
    // SAFETY: `buf` starts on a line and every caller passes a buffer inside a 32-byte-aligned
    // region whose length is a multiple of 32 (`Line`, and a staging half), so rounding the
    // length up stays inside that region and invalidates no neighbour's lines. Nothing in it is
    // dirty: the CPU never writes these bytes.
    unsafe { scb.invalidate_dcache_by_address(addr, buf.len().next_multiple_of(32)) };
}

/// Read the claimed half's source range from QSPI, invalidate its cache lines, publish it.
async fn fill_half(scb: &mut SCB, mut claim: FillClaim<'_>) {
    let address = PCM_BASE + claim.source().start;
    assert!(
        address >= excerpt::slot_base(SLOT).unwrap_or(u32::MAX),
        "rig: replay read below its slot"
    );
    let buffer = claim.buffer();
    {
        // Held across this one read and nothing else, as `FLASH`'s rule says.
        let mut guard = FLASH.lock().await;
        flash_of(&mut guard).read_async(address, buffer).await;
    }
    invalidate_after_dma(scb, buffer);
    claim.finish();
}

/// Read and validate the slot header, start the pass and stage both halves.
///
/// Everything that can make a slot unplayable is decided here, before audio exists.
pub async fn load(scb: &mut SCB) -> Result<ExcerptPlayer, console::PlayRefusal> {
    let mut line = Line([0; 32]);
    {
        let mut guard = FLASH.lock().await;
        let base = excerpt::slot_base(SLOT).expect("SLOT is validated by parse_slot");
        flash_of(&mut guard).read_async(base, &mut line.0).await;
    }
    invalidate_after_dma(scb, &mut line.0);
    let header = SlotHeader::decode(&line.0).map_err(|why| match why {
        // Blank flash reads 0xFF and an install in progress erased the header sector first: both
        // are "nothing here". Everything else is something that is not a header.
        excerpt::HeaderError::Magic => console::PlayRefusal::Empty,
        _ => console::PlayRefusal::Invalid,
    })?;
    REPLAY
        .start(header.pcm_bytes())
        .expect("rig: a validated header has an even length, and no pass is running at boot");
    // Both halves, so the callback's first block finds one `Ready` (see "Boot order" in the module docs).
    while let Some(claim) = REPLAY.claim_fill() {
        fill_half(scb, claim).await;
    }
    Ok(ExcerptPlayer { header })
}

/// Refill halves as the callback drains them; after the pass, report it once.
///
/// Returns after emitting `EXCPLAY`. Polls rather than waking on a signal: a wake from the
/// callback would be a lock or a pender call at audio priority, and the poll's cost is in
/// `REFILL_POLL_MS`.
pub async fn run(scb: &mut SCB, want: u16) {
    loop {
        if let Some(report) = REPLAY.finish() {
            emit_pass(&report, want);
            return;
        }
        match REPLAY.claim_fill() {
            Some(claim) => fill_half(scb, claim).await,
            None => embassy_time::Timer::after_millis(REFILL_POLL_MS).await,
        }
    }
}

/// Send the one `EXCPLAY` record for a completed pass.
fn emit_pass(report: &PassReport, want: u16) {
    let play = console::ExcerptPlay {
        slot: SLOT,
        bytes: report.samples.saturating_mul(BYTES_PER_SAMPLE as u32),
        crc16: report.crc16,
        want,
        underruns: report.underruns,
        clean: report.is_clean(),
    };
    let mut body = [0u8; console::BODY_WINDOW];
    let n = console::exc_play_body(&play, &mut body);
    info!(
        "rig: replay pass done: slot {} crc16 {:04x} want {:04x} underruns {}",
        play.slot, play.crc16, play.want, play.underruns
    );
    if play.crc16 != play.want {
        error!("rig: replay CRC differs from the stored one");
    }
    emit_console(&body[..n]);
}

/// The generator the audio task holds: a thin front for the shared [`REPLAY`] core.
pub struct ExcerptPlayer {
    header: SlotHeader,
}

impl ExcerptPlayer {
    /// The CRC the slot header stored: the pass's `want`.
    pub fn want_crc16(&self) -> u16 {
        self.header.pcm_crc16()
    }
}

impl Processor for ExcerptPlayer {
    type Params = ();

    fn set_sample_rate(&mut self, _hz: f32) {
        // The excerpt is 48 kHz by construction (`excerpt::WAV_SAMPLE_RATE`), checked at install.
    }

    fn set_params(&mut self, _params: &()) {}

    /// Silence: this source is block-granular, because the CRC and the half hand-off are both
    /// defined per 32-sample block. Present only because the trait requires it.
    fn tick(&mut self, _input: Frame) -> Frame {
        [0.0; 2]
    }

    /// See "When the pass runs" in the module docs: the pass was started at boot.
    fn reset(&mut self) {}

    /// The whole audio-path cost of replay: `take_block` and a sample conversion. No QSPI, no
    /// flash mutex, no await, no allocation. Samples convert as `s / 32768` into both channels.
    fn process_block(&mut self, output: &mut [Frame]) {
        let mut block = [0i16; BLOCK_SAMPLES];
        let _ = REPLAY.take_block(&mut block);
        for (frame, sample) in output.iter_mut().zip(block) {
            let value = f32::from(sample) / 32768.0;
            *frame = [value, value];
        }
    }
}

/// A `fmt::Write` that stops at the end of its buffer, like the dsp crate's private one.
struct Fill<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl core::fmt::Write for Fill<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let room = self.buf.len() - self.pos;
        let n = s.len().min(room);
        self.buf[self.pos..self.pos + n].copy_from_slice(&s.as_bytes()[..n]);
        self.pos += n;
        Ok(())
    }
}

impl Stimulus for ExcerptPlayer {
    /// `name=excerpt sample_rate_hz=48000 slot=<n> tag=<name> bytes=<n> crc16=<4 hex>`: at most
    /// 83 bytes against `RIGGEN`'s 160. No level: the excerpt's level is its own.
    fn describe(&self, out: &mut [u8]) -> usize {
        let mut w = Fill { buf: out, pos: 0 };
        let tag = core::str::from_utf8(self.header.name().as_bytes()).unwrap_or("?");
        let _ = write!(
            w,
            "name=excerpt sample_rate_hz={} slot={} tag={} bytes={} crc16={:04x}",
            excerpt::WAV_SAMPLE_RATE,
            SLOT,
            tag,
            self.header.pcm_bytes(),
            self.header.pcm_crc16(),
        );
        w.pos
    }

    /// The excerpt plays once, so the player starts it inside the capture and says where.
    fn one_shot_samples(&self) -> Option<u32> {
        Some(self.header.pcm_bytes() / BYTES_PER_SAMPLE as u32)
    }
}

/// Refuse to play: say why, show red, and keep serving the console so the record can leave.
///
/// `boot_halt` spins and so would strand the record in the pipe; this keeps the drain and the LED
/// tasks running and starts no audio, which is the point - a slot with nothing playable is a
/// configuration mistake to surface, not a reason to play silence and look healthy.
#[cfg(feature = "stim-excerpt")]
pub async fn halt_unplayable(why: console::PlayRefusal, console: asperitas_logging::Console) -> ! {
    error!(
        "rig: excerpt slot {} is not playable ({}); not starting audio",
        SLOT,
        why.as_str()
    );
    let mut body = [0u8; console::BODY_WINDOW];
    let n = console::exc_play_refused_body(SLOT, why, &mut body);
    emit_console(&body[..n]);
    asperitas_logging::led::set_global_state(asperitas_logging::led::LedState::Panicked);
    embassy_futures::select::select(console.run(), asperitas_logging::led::blink_task()).await;
    #[allow(clippy::empty_loop)]
    loop {}
}
