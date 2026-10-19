//! Measurement-rig binary: plays a known stimulus, captures one lane, and hands the bytes to
//! the host for objective measurement.
//!
//! This file deliberately stays close to `src/bin/main.rs`. Everything it keeps - the preamble, the
//! board setup, the codec start, the block geometry, the encode step - behaves exactly as it
//! does in the binary already proven on hardware, which is the only reason to trust the parts
//! that are new. Three things differ, and they are the whole point:
//!
//! 1. `cortex_m::Peripherals::take()` is claimed, because rig owns DWT's cycle counter and
//!    measures durations with it (a duration measured against a counter nobody enabled reads
//!    zero and looks like a fast device).
//! 2. The audio future runs on the shared audio module's interrupt-mode executor woken by SAI1,
//!    not on the thread executor's select tree, so its latency has a bound rather than a hope.
//! 3. The DSP chain is a stimulus generator chosen by cargo feature, and the input frame never
//!    reaches it: rig measures the box, not the instrument plugged into it. A generator that plays
//!    once (the sweep) is held silent until the capture's first callback, so the recording holds it
//!    whole; a periodic one plays from the first callback, as before.
//!
//! What it then does with the input is the measurement. One lane of the codec's input is truncated
//! to `i16` and copied into a 32 MiB ring in SDRAM, the ring is shipped to the host over the framed
//! console once the window closes, and the timing of every callback is measured against DWT so the
//! host can tell a clean capture from a starved one. The run is a fixed timeline with no runtime
//! control: see the `timeline` module.
//!
//! The one thing rig does read from the host is an excerpt install: framed records on
//! the CDC OUT endpoint, written sector by sector into a QSPI slot and verified by readback. See
//! the `install` module; it never runs alongside a capture.

#![no_std]
#![no_main]

include!(concat!(env!("OUT_DIR"), "/asp_pro.inc")); // cfg provenance note, see build.rs

mod dump;
#[cfg(feature = "stim-excerpt")]
mod excerpt_play;
mod fx_ctl;
mod generator;
mod geometry;
mod install;
mod report;
mod ring;
mod timeline;

use asperitas_dsp::processor::{Frame, Processor};
use asperitas_dsp::stimulus::Stimulus;
use asperitas_firmware::audio::{self, measure_time_base, AudioProcessor, TimeBase, BLOCK_LENGTH};
use asperitas_logging::{error, info};
use asperitas_wire::console;
use core::sync::atomic::Ordering;
use cortex_m::peripheral::{DWT, NVIC};
use daisy_embassy::audio::AudioConfig;
use daisy_embassy::hal::interrupt;
use daisy_embassy::hal::interrupt::{InterruptExt, Priority};
use daisy_embassy::hal::{bind_interrupts, dma, peripherals, qspi, usb};
use daisy_embassy::{hal, new_daisy_board, DaisyBoard};
use embassy_executor::Spawner;
use generator::{chain_of, ActiveGenerator, GENERATOR, RIG_FS, SAMPLE_RATE_HZ};
use geometry::{CAPTURE_SECONDS, MONO_LANE};
use install::FLASH;
use report::{emit_capmax, report_capstat};
use ring::{Producer, Ring, STIM_BLOCK, STIM_CALLBACK, STIM_STARTED};
use static_cell::StaticCell;
use timeline::run_capture;

use asperitas_firmware as _;

// rig's own USB IRQ struct. Each binary defines one: `bind_interrupts!` expands to an
// `#[export_name]` thunk per vector, so sharing the struct across crates would mean sharing its
// handlers with binaries that may not even enable the console.
//
// SAI1 is deliberately absent from this struct - see the handler written by hand below.
bind_interrupts!(pub struct RigUsbIrqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

// The QSPI flash's two vectors, for `FlashBuilder::build_async` (the shape of daisy-embassy's
// `examples/flash.rs`). A struct of their own rather than two more lines in `RigUsbIrqs`, because the
// two have nothing in common but this binary: USB is bound whatever the transport, flash whether or
// not anything installs, and folding them together would make the next reader wonder which half the
// other depends on. SAI1 stays out of both, for the reason given at its hand-written handler.
bind_interrupts!(pub struct RigFlashIrqs {
    QUADSPI => qspi::InterruptHandler<peripherals::QUADSPI>;
    MDMA => dma::InterruptHandler<peripherals::MDMA_CH0>;
});

// The SAI1 vector that drives the shared audio executor.
asperitas_firmware::audio_sai1_vector!();

/// Emit one wire-contract body to the framed console, stamped with the clock as it is now.
///
/// Two definitions rather than a `cfg` at each call site, because there are many (RIGCFG, RIGGEN,
/// CAPMAX, CAPSTAT, STIMSTART, DUMPEND) and that many copies of `#[cfg(feature = "log-usb")]` is
/// that many ways to forget one.
///
/// Under `log-defmt` without `log-usb` this discards, and that is honest rather than a gap: the
/// verbs describe the byte stream the framed console carries, and an RTT-only image captures no
/// such stream. The shim exists so that build still links (CI's second configuration) without
/// pretending a measurement record went somewhere.
#[cfg(feature = "log-usb")]
pub(crate) fn emit_console(body: &[u8]) {
    if !asperitas_logging::emit_record(asperitas_logging::Level::Info, report::now_ms(), body) {
        error!("rig: console refused a record (log pipe full)");
    }
}

#[cfg(not(feature = "log-usb"))]
pub(crate) fn emit_console(_body: &[u8]) {}

/// The audio callback, run by the shared audio executor at SAI1's priority.
///
/// `generator` is a sole `&'static mut`, so the callback needs no lock. That is deliberate - the
/// alternative, reaching the generator through `cortex_m::interrupt::free`, puts a PRIMASK
/// critical section on the highest-rate path in the firmware, where the whole point of the
/// interrupt executor is to avoid one.
///
/// ### Timer-slot budget: 6 named users of 8 slots, and none of them here
///
/// `generic-queue-8` is fixed by daisy-embassy's own `embassy-time` dependency; adding a
/// `generic-queue-N` here would collide on `const QUEUE_SIZE` and fail to compile, so eight is
/// the whole budget. Its users: boot 1 (DWT calibration, the codec's 2 ms startup delay,
/// `slow-boot`'s linger), `CAPSTAT` reporting ticker 1, LED blink 1, capture timeline 1 (arm delay,
/// window deadline, early-stop poll), dump retry 1, excerpt install 1 (the flash driver's
/// `with_timeout` on every erase and page program), USB drain 0 - deliberately zero, see `usb.rs`'s
/// parking drain. Six, as a ceiling. In practice fewer are held at once: slots are keyed by
/// *waker*, every one of those six runs inside `main`'s task (boot before the select, the rest
/// joined inside it), and so they collapse onto that task's single slot. Overflow does not panic:
/// `queue_generic.rs` evicts the furthest-out timer and wakes it early, so a blown budget shows up
/// as a mistimed `CAPSTAT` rather than an error. Count a new timer against the six above, and a
/// new *task* with a timer against the eight.
///
/// The audio task takes none of them: after `start_callback` is awaited there is nothing left to
/// await, which is the requirement rather than an observation. A task that awaited between blocks
/// would be re-spawned through the pender and turn the deterministic wake (SAI1 fired) into a
/// queueing delay.
struct RigAudio {
    generator: &'static mut ActiveGenerator,
    producer: Producer,
    /// Silence until the generator plays. A one-shot generator is held back until the capture's
    /// first callback: started at boot, it would be two seconds (ARM_DELAY_MS) through before the
    /// recording began, and the analysis needs it whole. A periodic one plays from the first
    /// callback, because any stretch of it is as good as any other.
    frames_out: [Frame; BLOCK_LENGTH],
    playing: bool,
    /// The `FX` generation last applied (see `fx_ctl::render`).
    fx_seen: u32,
}

impl RigAudio {
    fn new(generator: &'static mut ActiveGenerator, ring: Ring) -> Self {
        let playing = generator.one_shot_samples().is_none();
        Self {
            generator,
            producer: Producer::new(ring),
            frames_out: [Frame::default(); BLOCK_LENGTH],
            playing,
            fx_seen: 0,
        }
    }
}

impl AudioProcessor for RigAudio {
    fn process(&mut self, input: &[u32], output: &mut [u32]) {
        // Entry time first, exit time last, so `max_block_us` prices everything the callback
        // does, capture included.
        let entry = DWT::cycle_count();

        // Capture what came back first: the input lane, after the self-loopback cable. Before
        // the render, because whether this callback was captured is what starts a one-shot
        // generator. The order costs nothing: the output buffer is not played until the DMA
        // reaches it next period, whichever half of the callback fills it.
        let captured = self.producer.on_callback(entry, input);
        if let (false, Some(at)) = (self.playing, captured) {
            // This callback's output is the stimulus's first sample, and its input is being
            // captured at `at`. Written before the release that publishes it.
            self.generator.reset();
            STIM_BLOCK.store(at.block, Ordering::Relaxed);
            STIM_CALLBACK.store(at.callback as u32, Ordering::Relaxed);
            STIM_STARTED.store(true, Ordering::Release);
            self.playing = true;
        }

        // Render site, mirroring `src/bin/main.rs`: stimulus -> [processor slot] -> encode_block.
        // The processor slot is empty by design and is where a measured effect will hang.
        if self.playing {
            fx_ctl::render(self.generator, &mut self.fx_seen, &mut self.frames_out);
        }
        audio::encode_block(&self.frames_out, output);
        self.producer.finish(entry);
    }
}

static RIG_AUDIO: StaticCell<RigAudio> = StaticCell::new();

/// Emit the two description records - exactly one `RIGCFG`, then exactly one `RIGGEN`.
///
/// Emitted at boot, once the interface is running and immediately before the audio task is
/// spawned, because that is what `console::RigConfig`'s own contract says: "sent once at boot,
/// before any stimulus plays". The other shape available here - a flag captured by the callback,
/// rendered on its first block - buys nothing. Nothing between this call and the first callback can
/// change a parameter, and capture bytes go to the SDRAM ring rather than onto the console, so there
/// is no record ordering to win. What it would cost is real: rendering both bodies is core `fmt`
/// work, and core `fmt` takes longer over some values than others, which is precisely the kind of
/// work that does not belong on the first SAI1 interrupt of a run whose whole purpose is measuring
/// timing.
fn emit_descriptors(generator: &ActiveGenerator, time_base: TimeBase) {
    let mut body = [0u8; console::BODY_WINDOW];

    let cfg = console::RigConfig {
        lane: MONO_LANE,
        blocks: asperitas_wire::capture::RING_BLOCKS as u32,
        block_bytes: asperitas_wire::capture::RING_BLOCK_BYTES as u32,
        bytes_per_s: asperitas_wire::capture::BYTES_PER_SECOND as u32,
        capsec_us: asperitas_wire::capture::ring_duration_micros() as u32,
        window_s: CAPTURE_SECONDS as u32,
        cpu_hz: time_base.cpu_hz,
        icache: cortex_m::peripheral::SCB::icache_enabled(),
        dcache: cortex_m::peripheral::SCB::dcache_enabled(),
    };
    let n = console::rigcfg_body(&cfg, &mut body);
    emit_console(&body[..n]);

    // The invariant, stated where it can bite: these bytes must fit `RIGGEN_MAX_GEN_BYTES` (160).
    // Today's three generators at their `Default` parameters, described at 48 kHz, come to 88 bytes
    // for sine, 107 for the sweep, and 107 for the pulse train - the widest of the three - leaving
    // 53 bytes of slack. That margin is not kept by this comment: the worst-case check in
    // `crates/asperitas-dsp/tests/stimulus_tests.rs` pushes every field to its sanitized extreme and
    // asserts the result against the same 160, so a parameter change that eats the slack fails a
    // host test instead of arriving here as a clipped record.
    //
    // Which leaves what no host test can rule out: a future generator with fields nobody imagined.
    // So the length is measured against the constant rather than trusted. Describing into one byte
    // more than the verb will carry makes an overrun observable - `riggen_body` clips in place,
    // which would send `max_frequency_hz=8000.00` for a cut `8000.000000` and let the host divide
    // by a number that was never played. The spare byte turns that silent lie into a length a
    // release build can act on, which a `debug_assert!` cannot.
    let mut described = [0u8; console::RIGGEN_MAX_GEN_BYTES + 1];
    let n_desc = generator.describe(&mut described);
    if n_desc > console::RIGGEN_MAX_GEN_BYTES {
        error!(
            "rig: generator description is {n_desc} bytes, budget is {}; sending no RIGGEN rather than a clipped one",
            console::RIGGEN_MAX_GEN_BYTES
        );
        return;
    }
    let n = console::riggen_body(&described[..n_desc], &mut body);
    emit_console(&body[..n]);
}

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

/// Stop rig in a state a person can see with no probe and no host attached.
fn boot_halt() -> ! {
    asperitas_logging::led::set_global_state(asperitas_logging::led::LedState::Panicked);
    #[allow(clippy::empty_loop)]
    loop {}
}

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // Claim the core peripherals BEFORE `hal::init`, so that whatever else in the stack wants
    // them finds them gone rather than half-configured by us. The `Option` is matched below
    // rather than unwrapped or `steal()`ed: `None` means somebody else owns the core peripherals,
    // and rig continuing anyway would report every duration as zero while looking healthy.
    let core_peripherals = cortex_m::Peripherals::take();

    let config = daisy_embassy::default_rcc();
    let p = hal::init(config);

    // Read the tree's own answer while the `Peri<RCC>` is still in hand - `new_daisy_board!`
    // moves the fields out of `p`, and `rcc::clocks()` is the accessor embassy-stm32 itself uses
    // internally for exactly this.
    let declared_hz = hal::rcc::clocks(&p.RCC).sys.to_hertz().map(|hz| hz.0);

    // The flash's DMA channel, taken before the macro: `new_daisy_board!` moves the fields it uses
    // out of `p` and leaves `MDMA_CH0` behind, but `p` is gone from view after this line.
    let flash_dma = p.MDMA_CH0;

    let board: DaisyBoard<'_> = new_daisy_board!(p);

    // Discard USB peripherals - `asperitas_logging::init` steals them directly via Peri::steal(). Same
    // invariant as `src/bin/main.rs`: reading this field would create a second handle for the same
    // physical peripheral.
    let _ = board.usb_peripherals;

    // Init RGB LED - single owner for boot stages + panic handler.
    asperitas_logging::led::init(board.pins.d20, board.pins.d19, board.pins.d18);

    // Bring up logging. Records logged before this call are discarded.
    let console = asperitas_logging::init(RigUsbIrqs);
    info!("rig booting");

    // The claim is matched here rather than at the `take()` above, and the gap is the LED: a
    // failure that early has no pin to report through, so taking the peripherals first and
    // reporting the consequence once the LED exists is what makes the failure visible.
    let mut cp = match core_peripherals {
        Some(cp) => cp,
        None => {
            error!(
                "rig: cortex_m::Peripherals was already claimed; refusing to guess at durations"
            );
            boot_halt();
        }
    };

    let time_base = match measure_time_base(&mut cp, declared_hz).await {
        Some(base) => base,
        None => boot_halt(),
    };

    // Bring up the SDRAM the capture ring lives in. `sdram` stays bound for the rest of `main`,
    // which never returns: it owns the FMC instance, and `init` takes it by `&mut`. Not because
    // dropping it would release the pins - `Sdram` has no `Drop` impl (daisy-embassy `ca9bcc9`) -
    // but because nothing else may ever touch that controller again.
    let mut sdram = board.sdram.build(&mut cp.MPU, &mut cp.SCB);
    let ring = Ring::new(sdram.init(&mut embassy_time::Delay).cast::<u8>());

    // The QSPI flash, async-only, then its two vectors below the audio executor: see the `install` module
    // for both choices. Before audio starts, because `build_async` resets the chip with the driver's
    // blocking status wait; that is the only blocking flash wait in the image, and it runs once,
    // here, with nothing yet to starve.
    let flash = board.flash.build_async(flash_dma, RigFlashIrqs);
    interrupt::QUADSPI.set_priority(Priority::P7);
    interrupt::MDMA.set_priority(Priority::P7);
    *FLASH.lock().await = Some(flash);

    // Build the generator before audio starts. `ExponentialSweep`'s constructor peak-normalises
    // by scanning its whole 384,000-sample record, which is milliseconds at 48 kHz but belongs
    // here rather than in a callback under no circumstances. The log line goes first because a
    // board that died mid-scan would otherwise look idle.
    info!("rig: building stimulus (sweep builds scan their whole record)");
    //
    // `stim-excerpt` builds it from the slot instead: the header read, the pass start and both halves'
    // first fill happen here, before `prepare_interface`, so the audio start is not delayed by flash.
    #[cfg(feature = "stim-excerpt")]
    excerpt_play::zero_staging();
    #[cfg(feature = "stim-excerpt")]
    let player = match excerpt_play::load(&mut cp.SCB).await {
        Ok(player) => player,
        Err(why) => excerpt_play::halt_unplayable(why, console).await,
    };
    #[cfg(feature = "stim-excerpt")]
    let want_crc = player.want_crc16();
    #[cfg(feature = "stim-excerpt")]
    let generator = GENERATOR.init(chain_of(player));
    #[cfg(not(feature = "stim-excerpt"))]
    let generator = GENERATOR.init(chain_of(generator::BareGenerator::default()));
    generator.set_sample_rate(SAMPLE_RATE_HZ);

    // Prepare the audio interface (SAI + codec init + DMA buffers)
    let interface = board
        .audio_peripherals
        .prepare_interface(AudioConfig { fs: RIG_FS })
        .await;

    // Linger on the pre-init red so it can actually be seen. Everything above this point takes a
    // few milliseconds, so without the delay red -> green reads as "always green" and the two
    // stages can't be distinguished by eye.
    #[cfg(feature = "slow-boot")]
    embassy_time::Timer::after_secs(3).await;

    // Running now; `audio::start` moves the LED to Panicked and records the reason in
    // `audio::AUDIO_EXIT` (which `CAPSTAT` reports) if the interface then fails to start.
    asperitas_logging::led::set_global_state(asperitas_logging::led::LedState::Running);

    // Say what is about to play, and what the ring can hold, once, before anything plays it.
    emit_descriptors(generator, time_base);
    emit_capmax();
    // Read before the generator moves into the audio task: the timeline needs it to check the fit.
    let one_shot = generator.one_shot_samples();

    // P6 sits below the SAI DMA streams and the embassy-time driver (TIM5), which is the whole
    // argument that the callback can run without a critical section: the transfers it depends on
    // finish ahead of it rather than underneath it. The DMA side is P0 because `Config::default()`
    // ships `dma_interrupt_priority: Priority::P0` (embassy-stm32 0.6.0 `src/lib.rs:362`), and the
    // direction is required, not incidental: the DMA ISR is what pends SAI1, so it must outrank
    // the executor it wakes. `audio::start` sets the priority; the numbers are logged below as a
    // reading of the NVIC, not as a restatement of this comment.
    //
    // Descriptors went out above because rendering them on the first SAI1 interrupt is what the
    // `emit_descriptors` comment rules out.
    let audio_processor = RIG_AUDIO.init(RigAudio::new(generator, ring));
    audio::start(interface, audio_processor).await;

    // Effective NVIC priorities, read back after the vector was unmasked. Lower numbers win on
    // Cortex-M (P0 is the highest, seven implemented bits, no subpriority on this part), so the
    // claim being checked is `tim5 < sai1` and `dma1_* < sai1`, and the opposite for the flash:
    // `sai1 < quadspi` and `sai1 < mdma`.
    info!(
        "rig: nvic priority sai1={} dma1_stream0={} dma1_stream1={} tim5={} quadspi={} mdma={}",
        NVIC::get_priority(interrupt::SAI1),
        NVIC::get_priority(interrupt::DMA1_STREAM0),
        NVIC::get_priority(interrupt::DMA1_STREAM1),
        NVIC::get_priority(interrupt::TIM5),
        NVIC::get_priority(interrupt::QUADSPI),
        NVIC::get_priority(interrupt::MDMA),
    );

    // Thread mode is left to what must not run at audio priority: the console drain, which is also
    // the reporting task that paces `STATUS`, the boot LED, `CAPSTAT`, the capture timeline with
    // its dump, and the excerpt installer. None of them has a deadline.
    //
    // The inbound reader is switched on here and only here, before `run()` is first polled: rig is
    // the one binary with a consumer for it, and every other binary keeps EP OUT unread.
    #[cfg(feature = "log-usb")]
    let install_fut = install::run_install(asperitas_logging::usb::enable_inbound());
    // No console, no inbound path, nothing to install from, so this never wakes.
    #[cfg(not(feature = "log-usb"))]
    let install_fut = core::future::pending::<()>();
    let console_fut = console.run();
    let led_fut = asperitas_logging::led::blink_task();
    // The timeline finishes after its dump; the reporter and the installer never do, so the join
    // never completes.
    //
    // The replay refill and its one report finish after the pass, so they do not end the join either.
    #[cfg(feature = "stim-excerpt")]
    let replay_fut = excerpt_play::run(&mut cp.SCB, want_crc);
    #[cfg(not(feature = "stim-excerpt"))]
    let replay_fut = core::future::ready(());
    let rig_fut = embassy_futures::join::join4(
        report_capstat(time_base),
        run_capture(ring, time_base, one_shot),
        install_fut,
        replay_fut,
    );

    // All three loop forever, so reaching past the select means one of them returned: the thread
    // executor parks and the audio interrupt keeps playing, which is a machine still making sound
    // and reporting nothing about it. Halt rather than become that.
    embassy_futures::select::select3(console_fut, led_fut, rig_fut).await;
    error!("rig: console, LED or capture reporting returned; stopping");
    #[allow(clippy::empty_loop)]
    loop {}
}
