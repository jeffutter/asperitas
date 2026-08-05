//! USB CDC-ACM serial logging backend.
//!
//! Implements logging over the Seed3's onboard USB-C using the CDC-ACM class.
//! Logs are sent through a lock-free pipe to a background USB task, avoiding
//! async calls from the synchronous `log::Log` trait.
//!
//! # Architecture
//!
//! ```text
//! log::info!("msg") → FacadeLogger → Pipe (in lib.rs) → USB drain task → CDC-ACM
//! ```

use core::future::Future;
use core::task::{Context, Waker};

use embassy_stm32::{
    self as hal,
    usb::{Config as UsbConfig, Driver},
};
use embassy_sync::pipe::Pipe;
use embassy_time::{Duration, Instant};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use static_cell::StaticCell;

/// Maximum CDC-ACM packet size, in bytes.
///
/// The endpoint rejects any oversize write outright rather than splitting it:
/// `EndpointIn::write` returns `EndpointError::BufferOverflow` when
/// `buf.len() > max_packet_size` (see `embassy-usb-synopsys-otg`). Every write
/// path must therefore chunk to this size, so it is defined once here instead of
/// being restated at each call site — the two copies previously disagreed, and a
/// pipe read larger than the endpoint read back as a disconnect.
const MAX_PACKET_SIZE: u16 = 64;

/// How long [`emit_blocking`] will try before giving up.
///
/// Bounds the panic path against a board with no host attached: the message is
/// lost, but the board halts with its red LED rather than spinning here forever.
const EMIT_TIMEOUT: Duration = Duration::from_secs(3);

/// Error returned when USB connection is lost.
#[derive(Debug)]
pub struct Disconnected;

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => Disconnected,
            EndpointError::Disabled => Disconnected,
        }
    }
}

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

/// Cached reference to the initialized CDC class.
/// Set during init(), read by run(). Safe on single-core Cortex-M.
static mut CDC_REF: *mut CdcAcmClass<'static, UsbDrv> = core::ptr::null_mut();

/// Cached reference to the initialized USB device.
/// Set during init(), read by run(). Safe on single-core Cortex-M.
static mut USB_DEV_REF: *mut embassy_usb::UsbDevice<'static, UsbDrv> = core::ptr::null_mut();

/// Handle to the running USB logger. Returned by [`init`].
pub struct UsbLoggerHandle;

/// Internal flag indicating whether init() has been called.
static INITIALIZED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

// ---------------------------------------------------------------------------
// Public init
// ---------------------------------------------------------------------------

/// Initialize the USB CDC-ACM logging backend.
///
/// This function:
/// 1. Creates the USB driver with FS speed configuration
/// 2. Sets up Windows-compatible composite device descriptors
/// 3. Creates the CDC-ACM class with 64-byte packet size
/// 4. Installs the global `log::Logger` backed by a pipe → USB pipeline
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

    // Initialize static storage and cache the references.
    // StaticCell::init returns &'static mut T, which we store as raw pointers
    // for later access in run(). Safe on single-core Cortex-M with controlled access.
    let cdc_ref = CDC_STORAGE.init(cdc);
    let usb_dev_ref = USB_DEV_STORAGE.init(usb_device);
    unsafe {
        CDC_REF = cdc_ref;
        USB_DEV_REF = usb_dev_ref;
    }

    // --- Initialize the log pipe ---
    unsafe {
        *(&raw mut crate::LOG_PIPE) = Some(Pipe::new());
    }

    // Install the global logger and switch backend to Usb
    crate::install_logger();
    unsafe {
        crate::GLOBAL_BACKEND = crate::Backend::Usb;
    }

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
    // Safe: USB_DEV_REF was set by init() and points to StaticCell-backed
    // storage that lives for 'static. Single-core Cortex-M means no concurrent
    // access issues.
    let usb_dev = unsafe { &mut *USB_DEV_REF };

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
    let usb_fut = usb_dev.run();

    // Connection/drain loop — runs alongside usb_fut, not before it. Handles
    // repeated connect/disconnect cycles without ever dropping usb_fut.
    let drain_fut = async {
        // Sized to the endpoint, not to the pipe. A larger buffer here lets a
        // busy pipe hand `write_packet` more than one packet's worth, which the
        // endpoint rejects as BufferOverflow — indistinguishable below from a
        // genuine disconnect, so the log data was dropped and the loop fell back
        // to waiting for a reconnect that had never happened.
        let mut buf = [0u8; MAX_PACKET_SIZE as usize];
        loop {
            let cdc = unsafe { &mut *CDC_REF };
            cdc.wait_connection().await;
            log::info!("USB connected");

            loop {
                match crate::pipe().try_read(&mut buf) {
                    Ok(0) | Err(embassy_sync::pipe::TryReadError::Empty) => {
                        embassy_futures::yield_now().await;
                    }
                    Ok(n) => {
                        let cdc = unsafe { &mut *CDC_REF };
                        if cdc.write_packet(&buf[..n]).await.is_err() {
                            break; // Connection lost — wait for reconnect.
                        }
                    }
                }
            }

            log::info!("USB disconnected");
        }
    };

    embassy_futures::join::join(usb_fut, drain_fut).await;
}

/// Send `msg` to the host synchronously, without the async executor.
///
/// This exists for the panic handler, and the pipe-based path cannot serve it.
/// [`run`]'s drain loop is the only thing that normally moves bytes from the log
/// pipe to the endpoint, and it lives inside a future; a panic halts the executor
/// for good, so anything written to the pipe afterwards sits there unread until
/// the board is reset. This function bypasses the pipe and drives the device and
/// the endpoint write itself.
///
/// It can do that because the USB interrupt handler is still installed and still
/// firing during the panic spin, so the driver's hardware state keeps advancing.
/// Only the *future* is missing someone to poll it, which is what the loop below
/// provides.
///
/// Returns once the message is sent, or after [`EMIT_TIMEOUT`] if the host is not
/// listening. Silently does nothing if [`init`] never ran.
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
    let usb_dev = unsafe { &mut *USB_DEV_REF };
    let cdc = unsafe { &mut *CDC_REF };

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
    };

    let mut fut = core::pin::pin!(embassy_futures::select::select(device_fut, write_fut));

    // Busy-poll with a no-op waker until the writes finish or we run out of time.
    //
    // The timeout is a plain `Instant::now()` comparison and deliberately NOT an
    // `embassy_time::Timer`: `Timer::poll` calls `schedule_wake(.., cx.waker())`
    // on every `Pending` poll, so using one here would push a no-op waker into
    // the time driver's queue on every iteration of this loop. `Instant::now()`
    // only reads a counter and cannot fail.
    let deadline = Instant::now() + EMIT_TIMEOUT;
    let mut cx = Context::from_waker(Waker::noop());
    while Instant::now() < deadline {
        if fut.as_mut().poll(&mut cx).is_ready() {
            return;
        }
    }
}
