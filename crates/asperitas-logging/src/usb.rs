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

use embassy_stm32::{
    self as hal,
    usb::{Config as UsbConfig, Driver},
    Peri,
};
use embassy_sync::pipe::Pipe;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use static_cell::StaticCell;

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
// Lifetime extension helper
// ---------------------------------------------------------------------------

/// Extend a [`Peri`] borrow to `'static`.
///
/// # Safety
///
/// Peripherals are never dropped for the life of the device, so duplicating
/// the singleton behind a promise not to construct two live drivers from it
/// is sound. We hold the only reference; no other driver is created from these
/// peripherals elsewhere in this binary.
#[inline]
unsafe fn extend_peri_to_static<T: hal::PeripheralType>(p: Peri<'_, T>) -> Peri<'static, T> {
    // Deref to get the Copy inner value, then reconstruct with any lifetime.
    // Same primitive used by Peri::clone_unchecked internally.
    unsafe { Peri::new_unchecked(*p) }
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
/// 4. Installs the global `log::Logger` backed by a pipe → USB pipeline
///
/// After calling this, call [`run`] (typically in a spawned task) to start
/// the USB event loop.
///
/// # Panics
///
/// Panics if called more than once (static cells can only be initialized once).
pub fn init<I>(
    usb_otg_fs: Peri<'_, hal::peripherals::USB_OTG_FS>,
    dp: Peri<'_, hal::peripherals::PA12>,
    dn: Peri<'_, hal::peripherals::PA11>,
    irqs: I,
) -> UsbLoggerHandle
where
    I: hal::interrupt::typelevel::Binding<
            <hal::peripherals::USB_OTG_FS as hal::usb::Instance>::Interrupt,
            hal::usb::InterruptHandler<hal::peripherals::USB_OTG_FS>,
        > + 'static,
{
    if INITIALIZED.swap(true, core::sync::atomic::Ordering::AcqRel) {
        panic!("USB logging already initialized");
    }

    // Extend Peri lifetimes to 'static via the shared helper.
    let usb_otg_fs = unsafe { extend_peri_to_static(usb_otg_fs) };
    let dp = unsafe { extend_peri_to_static(dp) };
    let dn = unsafe { extend_peri_to_static(dn) };

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

    let cdc = CdcAcmClass::new(&mut builder, cdc_state, 64);
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
/// asperitas_logging::usb::init(usb, dp, dn, Irqs);
/// spawner.spawn(async { asperitas_logging::usb::run().await });
/// ```
pub async fn run() {
    loop {
        // Safe: CDC_REF and USB_DEV_REF were set by init() and point to
        // StaticCell-backed storage that lives for 'static. Single-core Cortex-M
        // means no concurrent access issues.
        let cdc = unsafe { &mut *CDC_REF };
        let usb_dev = unsafe { &mut *USB_DEV_REF };

        cdc.wait_connection().await;
        log::info!("USB connected");

        let usb_run = usb_dev.run();
        let mut buf = [0u8; 127];
        let drain = async {
            loop {
                match crate::pipe().try_read(&mut buf) {
                    Ok(0) | Err(embassy_sync::pipe::TryReadError::Empty) => {
                        embassy_futures::yield_now().await;
                    }
                    Ok(n) => {
                        let cdc = unsafe { &mut *CDC_REF };
                        if cdc.write_packet(&buf[..n]).await.is_err() {
                            break; // Connection lost
                        }
                    }
                }
            }
        };

        let _ = embassy_futures::select::select(usb_run, drain).await;
        log::info!("USB disconnected");
    }
}
