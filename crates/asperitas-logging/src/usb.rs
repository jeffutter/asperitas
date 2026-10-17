//! USB CDC-ACM serial logging backend.
//!
//! Implements logging over the Seed3's onboard USB-C using the CDC-ACM class. Records are
//! committed to a framed ring by the crate-private `emit_at()` and carried to the host by
//! [`UsbConsole::run`]'s drain task, which is the only thing that touches the endpoint under
//! normal operation.
//!
//! # Architecture
//!
//! ```text
//! log::info!("msg") → FacadeLogger → LOG_PIPE (framed records) → drain task → CDC-ACM
//!                                                        ↑ STATUS rides in here too
//! panic → panic_handler → emit_panic_record → emit_blocking ─────────┘ (ring bypassed)
//!
//! host → CDC OUT → inbound reader → inbound::InboundFeed → INBOUND_CHANNEL → binary
//!                  (only after enable_inbound())
//! ```
//!
//! The CDC class is split at `init` into its IN half (`CdcTx`, used by the drain task and
//! the panic path) and its OUT half (`CdcRx`, used only by the inbound reader), so the two
//! concurrent tasks each borrow a distinct object instead of aliasing one `&mut`.
//!
//! [`UsbConsole`] owns all three objects, and `run` consumes it. The one thing that outlives
//! that ownership is the panic path: it needs the IN half and the device after the executor
//! has stopped, so `init` also records raw pointers to them in two atomics. That is the only
//! shared access; see `emit_blocking`.

use core::cell::UnsafeCell;
use core::future::Future;
use core::sync::atomic::{AtomicPtr, Ordering};
use core::task::{Context, Waker};

use embassy_stm32::{
    self as hal,
    usb::{Config as UsbConfig, Driver},
};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver};
use embassy_time::Instant;
use embassy_usb::class::cdc_acm::{self, CdcAcmClass, State};
use static_cell::StaticCell;

use crate::inbound::{InboundFeed, InboundRecord, INBOUND};
use crate::spin_budget::{cycle_count, SpinBudget};
use asperitas_wire::{console, frame};

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
// Static state — initialized once by init()
// ---------------------------------------------------------------------------

static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
static CDC_STATE: StaticCell<State> = StaticCell::new();
static EP_OUT_BUFFER: StaticCell<[u8; 256]> = StaticCell::new();

/// Type alias for the USB driver.
type UsbDrv = Driver<'static, hal::peripherals::USB_OTG_FS>;

/// The USB device, whose `run` answers enumeration and control transfers.
type UsbDev = embassy_usb::UsbDevice<'static, UsbDrv>;

/// The CDC class's IN half: host-bound bytes.
type CdcTx = cdc_acm::Sender<'static, UsbDrv>;

/// The CDC class's OUT half: device-bound bytes.
type CdcRx = cdc_acm::Receiver<'static, UsbDrv>;

/// Storage for the CDC-ACM IN half. Initialized once by `init`.
static CDC_TX_STORAGE: StaticCell<CdcTx> = StaticCell::new();

/// Storage for the CDC-ACM OUT half. Initialized once by `init`.
static CDC_RX_STORAGE: StaticCell<CdcRx> = StaticCell::new();

/// Storage for the USB device. Initialized once by `init`.
static USB_DEV_STORAGE: StaticCell<UsbDev> = StaticCell::new();

/// The CDC IN half, for the panic path only.
///
/// Null until `init` has run, which is how `emit_blocking` knows whether there is a console to
/// write to. Points into [`CDC_TX_STORAGE`], and aliases the `&'static mut` that
/// [`UsbConsole`] hands to the drain task: sound only because the panic path runs after the
/// executor has stopped, so the drain task's borrow is dead.
static PANIC_CDC: AtomicPtr<CdcTx> = AtomicPtr::new(core::ptr::null_mut());

/// The USB device, for the panic path only. Same contract as [`PANIC_CDC`].
static PANIC_DEV: AtomicPtr<UsbDev> = AtomicPtr::new(core::ptr::null_mut());

/// Frame buffer for the panic path — deliberately **not** [`crate::LOG_PIPE`] and not the
/// shared [`crate`] record buffers.
///
/// A panic means the executor is gone, so the drain task that would carry a ring write to
/// the endpoint never runs again: anything put in the ring after that sits there until
/// reset. The record buffers belong to the commit lock, which the panic path must not take
/// (AC #6 in TASK-030.02), so this path gets storage of its own instead. 228 bytes of
/// `.bss` buys a final record that decodes like every other one, and keeps a `MAX_FRAME`
/// local off a stack that may already be nearly exhausted.
static PANIC_FRAME: PanicFrame = PanicFrame(UnsafeCell::new([0; frame::MAX_FRAME]));

/// `Sync` wrapper for [`PANIC_FRAME`]: the panic path is its only user and runs single-threaded.
struct PanicFrame(UnsafeCell<[u8; frame::MAX_FRAME]>);

// Safety: only `emit_panic_record` touches it, from the panic handler, with the executor stopped.
unsafe impl Sync for PanicFrame {}

/// The USB device, both CDC halves and nothing else: what [`UsbConsole::run`] drives.
pub(crate) struct UsbConsole {
    dev: &'static mut UsbDev,
    tx: &'static mut CdcTx,
    rx: &'static mut CdcRx,
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

/// Build the USB CDC-ACM device: FS driver, Windows-compatible composite descriptors and a
/// CDC-ACM class with 64-byte packets. The crate-level `init` is the caller and goes on to
/// switch the log backend and emit `BOOT`.
///
/// # Panics
///
/// Panics if called more than once (static cells can only be initialized once).
pub(crate) fn init<I>(irqs: I) -> UsbConsole
where
    I: hal::interrupt::typelevel::Binding<
            <hal::peripherals::USB_OTG_FS as hal::usb::Instance>::Interrupt,
            hal::usb::InterruptHandler<hal::peripherals::USB_OTG_FS>,
        > + 'static,
{
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

    let (cdc_tx, cdc_rx) = CdcAcmClass::new(&mut builder, cdc_state, MAX_PACKET_SIZE).split();
    let usb_device = builder.build();

    let tx = CDC_TX_STORAGE.init(cdc_tx);
    let rx = CDC_RX_STORAGE.init(cdc_rx);
    let dev = USB_DEV_STORAGE.init(usb_device);

    // Publish the panic path's view last: a non-null pointer is its proof that everything
    // above is built. The pointers are taken before the `&mut`s move into `UsbConsole`.
    PANIC_CDC.store(core::ptr::from_mut(&mut *tx), Ordering::Release);
    PANIC_DEV.store(core::ptr::from_mut(&mut *dev), Ordering::Release);

    // No pipe to initialize: `LOG_PIPE` is a `const`-constructed static, so there is no
    // window in which a producer could find it missing.

    UsbConsole { dev, tx, rx }
}

impl UsbConsole {
    /// Run the USB device event loop, log drain and inbound reader.
    ///
    /// This future loops forever, handling connect/disconnect cycles gracefully.
    pub(crate) async fn run(self) {
        let UsbConsole { dev, tx, rx } = self;
        // `dev.run()` is what drives enumeration: control transfers, descriptor
        // requests, address assignment. It must be polled CONCURRENTLY with any
        // `wait_connection()`, never after it.
        //
        // Awaiting wait_connection() first deadlocks the device: wait_connection()
        // only completes once the host has configured the device, and the host can
        // only configure a device whose run() future is being polled to answer it.
        // Neither side can advance, and the board never appears on the USB bus at
        // all. This mirrors the upstream daisy-embassy usb_serial example, which
        // joins the two futures rather than sequencing them.
        let usb_fut = dev.run();

        // Connection/drain loop — runs alongside usb_fut, not before it. Handles
        // repeated connect/disconnect cycles without ever dropping usb_fut.
        let drain_fut = async {
            let mut buf = [0u8; DRAIN_BUF_SIZE];
            let mut status_gate = console::StatusGate::new();

            loop {
                tx.wait_connection().await;
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
                                if tx.write_packet(&[]).await.is_err() {
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
                        if tx.write_packet(chunk).await.is_err() {
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

        embassy_futures::join::join3(usb_fut, drain_fut, inbound_reader(rx)).await;
    }
}

// ---------------------------------------------------------------------------
// Inbound (host → device) records
// ---------------------------------------------------------------------------

/// How many validated inbound records may wait for the consumer.
///
/// Four is enough for a consumer that takes one record per sector-sized unit of work; more
/// only buys the host a longer burst before flow control engages, which nothing needs. A
/// fuller channel costs nothing but latency: see `inbound_reader`.
pub const INBOUND_DEPTH: usize = 4;

/// Records from the host, in arrival order, waiting for the binary.
static INBOUND_CHANNEL: Channel<CriticalSectionRawMutex, InboundRecord, INBOUND_DEPTH> =
    Channel::new();

/// Whether a binary asked for inbound records. Read once, when the console starts running.
static INBOUND_ENABLED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// The receiving end of the inbound record channel.
pub type InboundReceiver = Receiver<'static, CriticalSectionRawMutex, InboundRecord, INBOUND_DEPTH>;

/// Turn on the CDC OUT reader and return the receiver its records arrive on.
///
/// Call before the console starts running. Only a binary with a consumer should call this: without one
/// the channel fills after [`INBOUND_DEPTH`] records, the reader stops reading, and the host's
/// writes stall until the board resets. That is flow control doing its job — nothing is
/// dropped — but it is not a state a binary should be able to enter by accident, so binaries
/// that never call this keep today's behaviour exactly: EP OUT is never read and host writes
/// are NAKed. Calling it twice returns the same channel.
///
/// Every record is whole and CRC-checked; rejects and junk are counted in
/// [`crate::inbound::INBOUND`], never delivered. The body means whatever the consumer
/// decides; excerpt install parses it with `asperitas_wire::excerpt::parse_record`.
pub fn enable_inbound() -> InboundReceiver {
    INBOUND_ENABLED.store(true, Ordering::Release);
    INBOUND_CHANNEL.receiver()
}

/// Read EP OUT, decode, and hand records to [`INBOUND_CHANNEL`]. Never returns while enabled.
///
/// Reads are re-armed as soon as a packet's records are in the channel, so the endpoint is
/// drained promptly and USB flow control, not loss, paces the host. When the channel is full
/// the reader awaits space *before* reading again: the device then NAKs further OUT packets
/// and the host's `write()` blocks. Backpressure therefore costs the host time, never bytes.
///
/// Fairness with the drain task: both are arms of one `join3` on one executor, each awaits
/// its own endpoint, and neither busy-loops, so neither can starve the other. Neither
/// touches the audio executor.
///
/// Every connection starts with [`InboundFeed::reset`], which abandons a half record a
/// dropped link left behind so it cannot splice onto the next session's bytes.
async fn inbound_reader(rx: &'static mut CdcRx) {
    if !INBOUND_ENABLED.load(Ordering::Acquire) {
        return;
    }
    let sender = INBOUND_CHANNEL.sender();
    let mut feed = InboundFeed::new(&INBOUND);
    let mut packet = [0u8; MAX_PACKET_SIZE as usize];

    loop {
        rx.wait_connection().await;
        feed.reset();

        loop {
            let n = match rx.read_packet(&mut packet).await {
                Ok(n) => n,
                Err(_) => {
                    INBOUND.endpoint_error();
                    break;
                }
            };

            // Drain-then-retry: a refused record is held by the feed, awaited into the
            // channel, and the untaken remainder offered again. See `inbound`'s module docs.
            let mut off = 0;
            loop {
                off += feed.feed(&packet[off..n], |r| sender.try_send(*r).is_ok());
                match feed.take_pending() {
                    Some(r) => sender.send(r).await,
                    None => break,
                }
            }
        }
    }
}

/// Frame a panic message as one v1 record and push it straight to the endpoint.
///
/// The body arrives as plain text from [`crate::panic_handler`]; wrapping it here means a
/// `PANIC:` line validates like every other record and stays legible in a raw terminal, so
/// the README's "read the last line" procedure still means what it says.
///
/// Built in the static `PANIC_FRAME` **without taking the record lock**, and allocating nothing: the
/// executor may have died part-way through holding it, and the panic handler runs on a
/// stack that may be nearly exhausted. Sharing the buffer is acceptable for the same reason
/// — with the executor halted nothing else is formatting, and the one theoretical overlap
/// (a panic raised inside the commit critical section) is designed out: record-path callers
/// crash only *outside* the record lock, where `commit_records` defers its stall `panic!`
/// until `RECORD_BUFS.lock` has returned. Nothing on the record path can raise while `PRIMASK` is
/// set; the `#[no_panic]` functions it runs make the release build fail to link if anything starts to.
pub fn emit_panic_record(body: &[u8]) {
    // Safety: see [`PANIC_FRAME`]. Takes no lock by design; `frame!` writes are pure byte
    // arithmetic into the buffer we were given.
    let out = unsafe { &mut *PANIC_FRAME.0.get() };
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
/// The drain loop is the only thing that normally moves bytes from the log
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
/// first — if the host is not listening. Silently does nothing if `init` never ran.
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
    // Without init() both pointers are null. A panic this early has only the LED to report
    // through.
    let (dev, tx) = (
        PANIC_DEV.load(Ordering::Acquire),
        PANIC_CDC.load(Ordering::Acquire),
    );
    if dev.is_null() || tx.is_null() {
        return;
    }

    // Safety: both point at StaticCell-backed storage that lives for 'static, on a single
    // core. The executor is halted by the time we are called, so the drain task's `&mut`s to
    // the same objects are dead and cannot alias ours.
    let (usb_dev, cdc) = unsafe { (&mut *dev, &mut *tx) };

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
        if !msg.is_empty() && msg.len().is_multiple_of(MAX_PACKET_SIZE as usize) {
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
