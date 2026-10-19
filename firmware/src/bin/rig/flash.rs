//! The rig's QSPI flash handle, which offers the async calls and nothing else.
//!
//! daisy-embassy's `Flash<Async>` still carries the blocking `read` / `write` / `erase`, which wait on
//! the chip's WIP bit in an unbounded busy loop. Wrapping it in a type with a private field and only
//! async methods makes the blocking forms unreachable from the rest of the binary: nothing else can
//! name the inner `Flash`, and the only constructor is `build`, which goes through `build_async`. See
//! the `install` module for why rig wants the async calls only. Which callers exist depends on the
//! `log-usb` and `stim-excerpt` features, hence the conditional `dead_code` allowances.

use daisy_embassy::flash::{Flash, FlashBuilder};
use daisy_embassy::hal::qspi::{Instance, InterruptHandler, QuadDma};
use daisy_embassy::hal::{self, dma, mode::Async, peripherals::QUADSPI, Peri};

#[cfg_attr(
    not(any(feature = "log-usb", feature = "stim-excerpt")),
    allow(dead_code)
)]
pub(crate) struct AsyncFlash(Flash<'static, Async>);

#[cfg_attr(
    not(any(feature = "log-usb", feature = "stim-excerpt")),
    allow(dead_code)
)]
impl AsyncFlash {
    /// Build the flash in async mode. Resets the chip with the driver's blocking status wait, so call
    /// it once at boot before audio starts.
    pub(crate) fn build<D, I>(
        builder: FlashBuilder<'static>,
        dma_ch: Peri<'static, D>,
        irq: I,
    ) -> Self
    where
        D: QuadDma<QUADSPI>,
        I: hal::interrupt::typelevel::Binding<D::Interrupt, dma::InterruptHandler<D>>
            + hal::interrupt::typelevel::Binding<
                <QUADSPI as Instance>::Interrupt,
                InterruptHandler<QUADSPI>,
            > + 'static,
    {
        Self(builder.build_async(dma_ch, irq))
    }

    pub(crate) async fn read_async(&mut self, address: u32, buffer: &mut [u8]) {
        self.0.read_async(address, buffer).await
    }

    #[cfg_attr(not(feature = "log-usb"), allow(dead_code))]
    pub(crate) async fn write_async(&mut self, address: u32, data: &[u8]) {
        self.0.write_async(address, data).await
    }

    #[cfg_attr(not(feature = "log-usb"), allow(dead_code))]
    pub(crate) async fn erase_async(&mut self, address: u32, length: u32) {
        self.0.erase_async(address, length).await
    }
}
