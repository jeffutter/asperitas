//! USB CDC-ACM serial logging backend.
//!
//! Implements logging over the Seed3's onboard USB-C using the CDC-ACM class. Records are
//! committed to a framed ring by [`crate::emit`] and carried to the host by [`run`]'s drain
//! task, which is the only thing that touches the endpoint under normal operation.
//!
//! # Architecture
//!
//! ```text
//! log::info!("msg") → FacadeLogger → LOG_PIPE (framed records) → run() drain task → CDC-ACM
//!                                                        ↑ STATUS rides in here too
//! panic → panic_handler → emit_panic_record → emit_blocking ─────────┘ (ring bypassed)
//! ```

use core::future::Future;
use core::task::{Context, Waker};

use embassy_stm32::{
    self as hal,
    usb::{Config as UsbConfig, Driver},
};
use embassy_time::Instant;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use static_cell::StaticCell;

use crate::spin_budget::{cycle_count, SpinBudget};
use crate::{console, frame};

/// Maximum CDC-ACM packet size, in bytes.
///
/// The endpoint rejects any oversize write outright rather than splitting it:
/// `EndpointIn::write` returns `EndpointError::BufferOverflow` when
/// `buf.len() > max_packet_size` (see `embassy-usb-synopsys-otg`). Every write
/// path must therefore chunk to this size, so it is defined once here instead of
/// being restated at each call site — the two copies previously disagreed, and a
/// pipe read larger than the endpoint read back as a disconnect.
const MAX_PACKET_SIZE: u16 = 64;

/// How many bytes the drain task pulls from the ring per wakeup.
///
/// Several endpoint packets' worth, deliberately larger than [`MAX_PACKET_SIZE`]: reading
/// one packet at a time made every record cost a full trip through the executor. Reads
/// shorter than the ring's contiguous run are normal (a wrap ends a run early) and are not
/// a loss signal. Whatever the size, the write side chunks to the endpoint, so the
/// BufferOverflow rule above cannot be reached from here.
const DRAIN_BUF_SIZE: usize = 256;

// The panic-path spin budget (`EMIT_TIMEOUT_CYCLES`, `EMIT_TIMEOUT_MAX_POLLS`) lives in
// `crate::spin_budget` beside the code that enforces it; `emit_blocking` only asks it for a
// budget.

// ---------------------------------------------------------------------------
// Static state — initialized once by init(), consumed by run()
// ---------------------------------------------------------------------------

static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
static CDC_STATE: StaticCell<State> = StaticCell::new();
static EP_OUT_BUFFER: StaticCell<[u8; 256]> = StaticCell::new();

/// Type alias for the USB driver.
type UsbDrv = Driver<'static, hal::peripherals::USB_OTG_FS>;

/// Storage for the CDC-ACM class. Initialized once by [`init`].
static CDC_STORAGE: StaticCell<CdcAcmClass<'static, UsbDrv>> = StaticCell::new();

/// Storage for the USB device. Initialized once by [`init`].
static USB_DEV_STORAGE: StaticCell<embassy_usb::UsbDevice<'static, UsbDrv>> = StaticCell::new();

/// Cached pointer to the initialized CDC class. Set during [`init`], read by [`cdc`].
static mut CDC_REF: *mut CdcAcmClass<'static, UsbDrv> = core::ptr::null_mut();

/// Cached pointer to the initialized USB device. Set during [`init`], read by [`usb_dev`].
static mut USB_DEV_REF: *mut embassy_usb::UsbDevice<'static, UsbDrv> = core::ptr::null_mut();

/// Frame buffer for the panic path — deliberately **not** [`crate::LOG_PIPE`] and not the
/// shared [`crate`] record buffers.
///
/// A panic means the executor is gone, so the drain task that would carry a ring write to
/// the endpoint never runs again: anything put in the ring after that sits there until
/// reset. The record buffers belong to the commit lock, which the panic path must not take
/// (AC #6 in TASK-030.02), so this path gets storage of its own instead. 228 bytes of
/// `.bss` buys a final record that decodes like every other one, and keeps a `MAX_FRAME`
/// local off a stack that may already be nearly exhausted.
static mut PANIC_FRAME: [u8; frame::MAX_FRAME] = [0; frame::MAX_FRAME];

/// Handle to the running USB logger. Returned by [`init`].
pub struct UsbLoggerHandle;

/// Internal flag indicating whether init() has been called.
static INITIALIZED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Borrow the CDC class.
///
/// Each caller uses what it gets and drops it; nothing here holds one across another
/// call's borrow.
///
/// # Safety of the pattern
///
/// Points at [`CDC_STORAGE`], which lives for `'static`, on a single core, populated once
/// by [`init`] before the executor starts. `addr_of_mut!` + `read_volatile` is how this
/// reads a `static mut` without creating a reference to it — the form the
/// `static_mut_refs` lint sanctions, so the crate needs no blanket allow.
fn cdc() -> &'static mut CdcAcmClass<'static, UsbDrv> {
    // Safety: see above. Null only if `init` never ran; `run` documents the ordering and
    // `emit_blocking` checks `INITIALIZED`.
    unsafe { &mut *core::ptr::read_volatile(core::ptr::addr_of_mut!(CDC_REF)) }
}

/// Borrow the USB device. See [`cdc`] for why the access pattern is sound.
///
/// # Safety contract at the call site
///
/// The returned reference must be polled concurrently with — never after — any
/// [`cdc`] borrow that is awaiting a transfer, because enumeration is driven by
/// `UsbDevice::run`.
fn usb_dev() -> &'static mut embassy_usb::UsbDevice<'static, UsbDrv> {
    // Safety: as [`cdc`].
    unsafe { &mut *core::ptr::read_volatile(core::ptr::addr_of_mut!(USB_DEV_REF)) }
}

/// Milliseconds since boot, truncated to the width of the wire's `t_ms` field.
///
/// Stamps from the same GP16 time driver the panic-path spin refuses to trust, so under the
/// identical condition — the time-driver ISR unable to run — this field goes stale. That is
/// cosmetic: loss detection keys on `seq`, not `t_ms`, and the frame CRC does not care. A
/// wrong millisecond on a final record is a nuisance; an unbounded spin is not.
fn now_ms() -> u32 {
    Instant::now().as_millis() as u32
}

// ---------------------------------------------------------------------------
// Public init
// ---------------------------------------------------------------------------

/// Initialize the USB CDC-ACM logging backend.
///
/// This function:
/// 1. Creates the USB driver with FS speed configuration
/// 2. Sets up Windows-compatible composite device descriptors
/// 3. Creates the CDC-ACM class with 64-byte packet size
/// 4. Switches the log backend to USB and emits the `BOOT` banner
///
/// After calling this, call [`run`] (typically in a spawned task) to start
/// the USB event loop.
///
/// # Panics
///
/// Panics if called more than once (static cells can only be initialized once).
pub fn init<I>(irqs: I) -> UsbLoggerHandle
where
    I: hal::interrupt::typelevel::Binding<
            <hal::peripherals::USB_OTG_FS as hal::usb::Instance>::Interrupt,
            hal::usb::InterruptHandler<hal::peripherals::USB_OTG_FS>,
        > + 'static,
{
    if INITIALIZED.swap(true, core::sync::atomic::Ordering::AcqRel) {
        panic!("USB logging already initialized");
    }

    // Safety: peripherals are never dropped for the life of the device.
    // steal() conjures a fresh 'static Peri for each peripheral listed below.
    // daisy_embassy::DaisyBoard also hands out these same peripherals via
    // board.usb_peripherals, but callers MUST discard that field unread
    // (as done at both call sites in main.rs and blinky.rs). That discard is
    // the invariant that keeps steal() safe here — no two live drivers will
    // ever exist simultaneously because the board's copy is explicitly thrown away.
    let usb_otg_fs = unsafe { hal::peripherals::USB_OTG_FS::steal() };
    let dp = unsafe { hal::peripherals::PA12::steal() };
    let dn = unsafe { hal::peripherals::PA11::steal() };

    // --- Configure USB driver ---
    let mut usb_config = UsbConfig::default();
    usb_config.vbus_detection = false; // Pod has no VBUSEN pin

    let ep_out_buffer = EP_OUT_BUFFER.init([0; 256]);

    let driver = Driver::new_fs(usb_otg_fs, irqs, dp, dn, ep_out_buffer, usb_config);

    // --- Device descriptors (Windows-compatible composite) ---
    let mut usb_device_config = embassy_usb::Config::new(0x1209, 0x1234);
    usb_device_config.manufacturer = Some("Asperitas");
    usb_device_config.product = Some("Asperitas Debug Console");
    usb_device_config.device_class = 0xEF;
    usb_device_config.device_sub_class = 0x02;
    usb_device_config.device_protocol = 0x01;
    usb_device_config.composite_with_iads = true;

    // --- Build USB device ---
    let config_desc = CONFIG_DESC.init([0; 256]);
    let bos_desc = BOS_DESC.init([0; 256]);
    let control_buf = CONTROL_BUF.init([0; 64]);
    let cdc_state = CDC_STATE.init(State::new());

    let mut builder = embassy_usb::Builder::new(
        driver,
        usb_device_config,
        config_desc,
        bos_desc,
        &mut [], // no MSOS descriptors
        control_buf,
    );

    let cdc = CdcAcmClass::new(&mut builder, cdc_state, MAX_PACKET_SIZE);
    let usb_device = builder.build();

    // Initialize static storage and cache the pointers.
    // StaticCell::init returns &'static mut T, which we keep as raw pointers for the
    // accessors above. Single-core Cortex-M, set before the executor starts.
    let cdc_ref = CDC_STORAGE.init(cdc);
    let usb_dev_ref = USB_DEV_STORAGE.init(usb_device);
    unsafe {
        core::ptr::write(core::ptr::addr_of_mut!(CDC_REF), cdc_ref);
        core::ptr::write(core::ptr::addr_of_mut!(USB_DEV_REF), usb_dev_ref);
    }

    // No pipe to initialize: `LOG_PIPE` is a `const`-constructed static, so there is no
    // window in which a producer could find it missing.

    // Install the global logger, switch the backend, then announce the console. The order
    // is load-bearing: `BOOT` before the backend switch reaches `Backend::NoOp` and is
    // discarded without a trace, and the counters cannot report a record that was never
    // offered to them.
    crate::install_logger();
    crate::set_backend_usb();
    crate::emit_boot();

    UsbLoggerHandle
}

/// Run the USB device event loop and log-drain task.
///
/// Must be called after [`init`] and typically spawned as a background task.
/// This future loops forever, handling connect/disconnect cycles gracefully.
///
/// # Example
///
/// ```ignore
/// asperitas_logging::usb::init(Irqs);
/// spawner.spawn(async { asperitas_logging::usb::run().await });
/// ```
pub async fn run() {
    // `usb_dev.run()` is what drives enumeration: control transfers, descriptor
    // requests, address assignment. It must be polled CONCURRENTLY with any
    // `wait_connection()`, never after it.
    //
    // Awaiting wait_connection() first deadlocks the device: wait_connection()
    // only completes once the host has configured the device, and the host can
    // only configure a device whose run() future is being polled to answer it.
    // Neither side can advance, and the board never appears on the USB bus at
    // all. This mirrors the upstream daisy-embassy usb_serial example, which
    // joins the two futures rather than sequencing them.
    let usb_fut = usb_dev().run();

    // Connection/drain loop — runs alongside usb_fut, not before it. Handles
    // repeated connect/disconnect cycles without ever dropping usb_fut.
    let drain_fut = async {
        let mut buf = [0u8; DRAIN_BUF_SIZE];
        let mut status_gate = console::StatusGate::new();

        loop {
            cdc().wait_connection().await;
            log::info!("USB connected");
            // Whether the last packet handed to the endpoint filled it exactly. A new
            // connection starts with no unfinished transaction, so it lives per connection.
            // See the short-packet rule below: this flag is what keeps a capture's tail from
            // being withheld by the host.
            let mut last_packet_was_full = false;

            loop {
                let n = match crate::LOG_PIPE.try_read(&mut buf) {
                    Ok(n) if n > 0 => n,
                    // Nothing buffered: this is the only place a STATUS record or a
                    // zero-length packet belongs, because both mean "the ring is empty and
                    // I am about to stop sending".
                    _ => {
                        let now = now_ms();
                        let snap = console::CONSOLE.snapshot();
                        if status_gate.due(now, &snap) {
                            crate::emit_status(&snap);
                            status_gate.mark_sent(&console::CONSOLE.snapshot());
                            continue; // The record just queued is now waiting to go out.
                        }

                        // USB bulk transactions must end with a short packet. A 64-byte
                        // packet is held in the host's driver until something shorter
                        // follows, so parking while the last packet was full silently loses
                        // the tail of every capture — loss our own framing would faithfully
                        // report as a `seq` gap while the cause sat in this loop.
                        if last_packet_was_full {
                            if cdc().write_packet(&[]).await.is_err() {
                                console::CONSOLE.endpoint_error();
                                break;
                            }
                            last_packet_was_full = false;
                        }

                        // Park until a byte exists instead of `yield_now()`-spinning, which
                        // was stealing executor slots from the audio loop. Consumes bytes
                        // only in the poll that returns Ready, so this cannot eat a record.
                        crate::LOG_PIPE.read(&mut buf).await
                    }
                };

                // Hand the endpoint at most one packet per write — see MAX_PACKET_SIZE.
                let mut link_lost = false;
                for chunk in buf[..n].chunks(MAX_PACKET_SIZE as usize) {
                    if cdc().write_packet(chunk).await.is_err() {
                        console::CONSOLE.endpoint_error();
                        link_lost = true;
                        break;
                    }
                    last_packet_was_full = chunk.len() == MAX_PACKET_SIZE as usize;
                }

                if link_lost {
                    // Bytes pulled in this read but not yet written die with the link. That
                    // is at most one DRAIN_BUF_SIZE read per reconnect and it surfaces as an
                    // `ep_err` plus a `seq` gap, never silently.
                    break;
                }
            }

            log::info!("USB disconnected");
        }
    };

    embassy_futures::join::join(usb_fut, drain_fut).await;
}

/// Frame a panic message as one v1 record and push it straight to the endpoint.
///
/// The body arrives as plain text from [`crate::panic_handler`]; wrapping it here means a
/// `PANIC:` line validates like every other record and stays legible in a raw terminal, so
/// the README's "read the last line" procedure still means what it says.
///
/// Built in [`PANIC_FRAME`] **without taking the record lock**, and allocating nothing: the
/// executor may have died part-way through holding it, and the panic handler runs on a
/// stack that may be nearly exhausted. Sharing the buffer is acceptable for the same reason
/// — with the executor halted nothing else is formatting, and the one theoretical overlap
/// (a panic raised inside the commit critical section) is designed out: record-path callers
/// crash only *outside* the record lock, where `commit_records` defers its stall panic until
/// `RECORD_BUFS.lock` has returned. Debug-profile-only assertions still living inside that
/// lock are TASK-047's to move; their worst case is this same garbled final line, not memory
/// unsafety.
pub fn emit_panic_record(body: &[u8]) {
    // Safety: see [`PANIC_FRAME`]. Takes no lock by design; `frame!` writes are pure byte
    // arithmetic into the buffer we were given.
    let out = unsafe { &mut *core::ptr::addr_of_mut!(PANIC_FRAME) };
    let encoded = frame::encode(
        log::Level::Error,
        console::CONSOLE.take_seq(),
        now_ms(),
        body,
        out,
    );
    emit_blocking(&out[..encoded.len]);
}

/// Send `bytes` to the host synchronously, without the async executor.
///
/// This exists for the panic handler, and the pipe-based path cannot serve it.
/// [`run`]'s drain loop is the only thing that normally moves bytes from the log
/// pipe to the endpoint, and it lives inside a future; a panic halts the executor
/// for good, so anything written to the pipe afterwards sits there unread until
/// the board is reset. This function bypasses the pipe and drives the device and
/// the endpoint write itself.
///
/// It can do that only because the USB interrupt handler is still installed and still
/// firing when it runs — the assumption is that interrupts are live; this function does not
/// make them live, and cannot recover a core whose `PRIMASK` a masked-context panic left
/// set. Only the *future* is missing someone to poll it, which is what the loop below
/// provides. The `log-defmt` sibling is immune to this failure mode for a different reason:
/// RTT is polled by the probe rather than by any interrupt of ours (and `docs/reference/
/// daisy-seed3.md` records its own opposite hazard, a stalled RTT host), which is why the
/// USB path is the fragile twin.
///
/// Returns once the bytes are sent, or once the `SpinBudget` is spent — processor cycles
/// (`EMIT_TIMEOUT_CYCLES`) or poll iterations (`EMIT_TIMEOUT_MAX_POLLS`), whichever expires
/// first — if the host is not listening. Silently does nothing if [`init`] never ran.
///
/// Two assumptions live here and they are distinct, because conflating them is how the
/// clock bug survived review. The *transport* assumption is the one above: CDC needs a live
/// USB interrupt to advance the write future, and this function does not make interrupts
/// live. The *clock* assumption used to be "the time-driver ISR runs", which the old
/// `embassy_time` deadline shared with the transport; the budget now counts processor cycles and
/// poll iterations instead, so only the transport assumption remains.
///
/// # Panics
///
/// Never. This is called from `#[panic_handler]`, where a second panic recurses
/// with no way out, so every step here is written to fail quietly instead.
pub fn emit_blocking(msg: &[u8]) {
    // Without init() the static cells are empty and the refs are null. A panic
    // this early has only the LED to report through.
    if !INITIALIZED.load(core::sync::atomic::Ordering::Acquire) {
        return;
    }

    // Safe on the same grounds as run(): single-core, and these point at
    // StaticCell-backed storage that lives for 'static. The executor is halted by
    // the time we are called, so run()'s borrows are dead and cannot alias ours.
    let usb_dev = usb_dev();
    let cdc = cdc();

    // `usb_dev.run()` must be polled alongside the writes, not before them — it
    // is what answers the host's control transfers, and without it the endpoint
    // can stall part-way through a message. Same constraint as in run().
    let device_fut = usb_dev.run();
    let write_fut = async {
        for chunk in msg.chunks(MAX_PACKET_SIZE as usize) {
            if cdc.write_packet(chunk).await.is_err() {
                return; // Host went away — nothing useful left to do.
            }
        }
        // Same short-packet rule as the drain loop, and nowhere near as forgiving if
        // missed: a framed message is 28–228 bytes, and whenever it lands on an exact
        // multiple of 64 the host keeps the whole final record in its driver buffer. The
        // most important line in the capture would be the one that never arrives.
        if !msg.is_empty() && msg.len() % MAX_PACKET_SIZE as usize == 0 {
            let _ = cdc.write_packet(&[]).await;
        }
    };

    let mut fut = core::pin::pin!(embassy_futures::select::select(device_fut, write_fut));

    // Busy-poll with a no-op waker until the writes finish or the spin budget is spent.
    // The budget reads DWT's cycle counter and a poll ceiling rather than `embassy_time`
    // for reasons spelled out on `EMIT_TIMEOUT_CYCLES` in `crate::spin_budget` — including
    // why this must not regress to an `embassy_time::Timer` or any other time-driver read.
    let mut cx = Context::from_waker(Waker::noop());
    let mut budget = SpinBudget::start();
    while !budget.expired(cycle_count()) {
        if fut.as_mut().poll(&mut cx).is_ready() {
            return;
        }
    }
}
