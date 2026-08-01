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
// Static state — initialized once by init()
// ---------------------------------------------------------------------------

static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
static CDC_STATE: StaticCell<State> = StaticCell::new();
static EP_OUT_BUFFER: StaticCell<[u8; 256]> = StaticCell::new();

/// Handle to the running USB logger. Returned by [`init`].
pub struct UsbLoggerHandle;

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
/// # Returns
///
/// A tuple of `(future, handle)` where:
/// - The **future** runs the USB device and log-drain loop. It loops forever,
///   handling connect/disconnect cycles gracefully. Must be spawned or joined.
/// - The **handle** provides access to logger-specific operations.
///
/// # Example
///
/// ```ignore
/// use daisy_embassy::hal::{bind_interrupts, peripherals, usb};
///
/// bind_interrupts!(pub struct Irqs {
///     OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
/// });
///
/// let p = hal::init(daisy_embassy::default_rcc());
/// let board = daisy_embassy::new_daisy_board!(p);
///
/// let (usb_fut, _handle) = asperitas_logging::usb::init(
///     board.usb_peripherals.usb_otg_fs,
///     board.usb_peripherals.pins.DP,
///     board.usb_peripherals.pins.DN,
///     Irqs,
/// );
/// spawner.spawn(async { usb_fut.await }).ok();
/// ```
pub fn init<I>(
    usb_otg_fs: Peri<'static, hal::peripherals::USB_OTG_FS>,
    dp: Peri<'static, hal::peripherals::PA12>,
    dn: Peri<'static, hal::peripherals::PA11>,
    irqs: I,
) -> (impl core::future::Future<Output = ()>, UsbLoggerHandle)
where
    I: hal::interrupt::typelevel::Binding<
            <hal::peripherals::USB_OTG_FS as hal::usb::Instance>::Interrupt,
            hal::usb::InterruptHandler<hal::peripherals::USB_OTG_FS>,
        > + 'static,
{
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

    let mut cdc = CdcAcmClass::new(&mut builder, cdc_state, 64);

    let mut usb_device = builder.build();

    // --- Initialize the log pipe ---
    unsafe {
        *(&raw mut crate::LOG_PIPE) = Some(Pipe::new());
    }

    // Install the global logger and switch backend to Usb
    crate::install_logger();
    unsafe {
        crate::GLOBAL_BACKEND = crate::Backend::Usb;
    }

    // --- Build the USB run future ---
    // We inline the USB + drain logic here to avoid lifetime issues with
    // passing cdc/usb_device across async boundaries.
    let usb_future = async move {
        loop {
            cdc.wait_connection().await;
            log::info!("USB connected");

            // Run USB device and drain logs concurrently
            let usb_run = usb_device.run();
            let mut buf = [0u8; 127];
            let drain = async {
                loop {
                    match crate::pipe().try_read(&mut buf) {
                        Ok(0) | Err(embassy_sync::pipe::TryReadError::Empty) => {
                            embassy_futures::yield_now().await;
                        }
                        Ok(n) => {
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
    };

    (usb_future, UsbLoggerHandle)
}
