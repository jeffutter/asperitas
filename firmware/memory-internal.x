MEMORY
{
  /* Internal flash — 128 KB, the region used by DFU bootloader.
     QSPI flash (8 MB at 0x90000000) is not included here; add it via
     a separate MEMORY region or linker script when using the Daisy
     bootloader for larger applications. */
  FLASH : ORIGIN = 0x08000000, LENGTH = 128K

  /* AXI SRAM — 512 KB, NOT 1 MB.
     The STM32H750's advertised "1 MB RAM" is the total across all domains
     (AXI 512K + D2 288K + D3 64K + DTCM 128K + ITCM 64K). Only the AXI SRAM
     is contiguous at 0x24000000.

     This length is load-bearing: cortex-m-rt sets the initial stack pointer to
     ORIGIN + LENGTH. Overstating it as 1M puts the SP at 0x24100000, past the
     end of physical RAM, so the first push after reset takes a BusFault and the
     board hard-faults before reaching main — indistinguishable from a board
     that never booted. Matches embassy-stm32's generated memory.x and
     daisy-embassy's own linker script. */
  RAM   : ORIGIN = 0x24000000, LENGTH = 512K
}

/*
 * Placement for daisy-embassy's SAI DMA buffers.
 *
 * `daisy-embassy/src/audio.rs` tags its two `GroundedArrayCell::uninit()` statics
 * with `#[unsafe(link_section = ".sram1_bss")]`. Without a rule here, that name is an
 * *orphan* output section: rust-lld places it itself, right after `.data`, and gives it
 * LMA == VMA because its previous section (`.data`) is not in the default load region.
 * `llvm-objcopy -O binary` writes a memory image spanning lowest to highest *load*
 * address, so those 1 KB of buffers made every audio-bearing image cover
 * 0x08000000..0x24000598 and come out at 469,763,480 bytes of mostly zeros.
 *
 * The rule has to live in this file, not upstream: cortex-m-rt's `link.x` performs
 * exactly one `INCLUDE memory.x`, and this crate's `OUT_DIR` comes first on the `-L`
 * search path, so ours is the only one the linker ever reads. daisy-embassy ships its own
 * `memory.x` with a `(NOLOAD)` rule for this section, and none of it - neither the rule
 * nor its MEMORY block - applies to our images.
 *
 * What each part is doing:
 *
 *   - `(NOLOAD)` is what makes lld emit `SHT_NOBITS`. rustc emits a custom-named section
 *     as `PROGBITS` however it is initialised - only names beginning `.bss.` become
 *     `NOBITS` - so `MaybeUninit` alone buys nothing, and `-R .bss -R .uninit` cannot
 *     remove a section that isn't either.
 *   - `> RAM` means *our* AXI SRAM at 0x24000000, which is where DMA1 reaches its
 *     descriptors. Do not copy daisy-embassy's regions instead: their `memory.x` does
 *     `REGION_ALIAS(RAM, DTCMRAM)` and sends these buffers to `RAM_D2`, and DMA1/DMA2
 *     cannot reach DTCM (embassy-rs/embassy#3747), so audio would stop with no compile
 *     error and no panic.
 *   - No `AT> FLASH`. `link.x` says of this injection point: "do not change output region
 *     or load region in those user sections!" Giving the section a flash load address
 *     also fixes the image size but writes ~1 KB of zeros into flash that nothing copies,
 *     since cortex-m-rt copies only `__sidata..__edata`.
 *   - `INSERT AFTER .bss` puts the buffers past `__ebss`, so they are not inside the range
 *     cortex-m-rt zero-fills at startup and do not stretch the `.data` copy loop. That
 *     would have been harmless anyway - `prepare_interface` initialises both buffers
 *     before use - but a layout that relies on a dependency's housekeeping is not one to
 *     choose.
 */
SECTIONS {
  .sram1_bss (NOLOAD) : ALIGN(4) {
    *(.sram1_bss)
    *(.sram1_bss*)
  } > RAM
} INSERT AFTER .bss;
