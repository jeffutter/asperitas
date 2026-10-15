/*
 * Link layout for an image the stock Daisy bootloader copies out of QSPI. Selected by
 * ASPERITAS_LAYOUT=bootloaded (build.rs); `memory-internal.x` is the internal-flash layout and stays the
 * default for blinky, ledtest, podtest and panictest.
 *
 * Measured on Seed3 2026-10-15 (TASK-038.04.07.01, docs/reference/daisy-seed3.md "The stock Daisy
 * bootloader on Seed3"). The bootloader's app region starts at QSPI 0x90040000. It reads the reset
 * vector (word at +4): an address in AXI SRAM (0x24xxxxxx) makes it copy the image from QSPI into
 * AXI SRAM, about 480 KB, and jump. So:
 *
 *   - "FLASH" here is the AXI SRAM the image is copied into. The file `objcopy -O binary` writes is
 *     the bytes the bootloader copies, and it is flashed to 0x90040000 (`make flash LAYOUT=bootloaded`).
 *   - AXI SRAM is image space now, so .data/.bss/stack cannot live there: RAM is DTCM. cortex-m-rt
 *     sets the initial stack pointer to ORIGIN(RAM) + LENGTH(RAM), which is 0x20020000 for DTCM, the
 *     value the working trial link produced. Like memory-internal.x's note on AXI, the LENGTH is load-bearing.
 *   - DMA1/DMA2 cannot reach DTCM (embassy-rs/embassy#3747), so daisy-embassy's SAI DMA buffers
 *     (`.sram1_bss`) move to D2 SRAM (0x30000000). The rule at the foot of this file is the
 *     memory.x one with `> RAM_D2`; its rationale (NOLOAD, no AT>, INSERT AFTER .bss) is in memory-internal.x.
 *
 * QSPI budget. The image is stored from QSPI offset 0x40000 (the bootloader occupies the first
 * 0x40000 bytes of the part). Excerpt slots start at QSPI offset 0x100000
 * (`asperitas_logging::excerpt::EXCERPT_AREA_START`, TASK-038.04.01), so the image must end below
 * it. FLASH LENGTH is what bounds the image, and `scripts/check-image-load-addresses.sh` fails if
 * QSPI_APP_OFFSET + LENGTH reaches that constant. It parses both from this file and the Rust source;
 * the offset is stated once, on the marker line below.
 */
/* QSPI_APP_OFFSET = 0x40000 */
MEMORY
{
  FLASH : ORIGIN = 0x24000000, LENGTH = 480K
  RAM   : ORIGIN = 0x20000000, LENGTH = 128K
  RAM_D2 : ORIGIN = 0x30000000, LENGTH = 288K
}

SECTIONS {
  .sram1_bss (NOLOAD) : ALIGN(4) {
    *(.sram1_bss)
    *(.sram1_bss*)
  } > RAM_D2
} INSERT AFTER .bss;
