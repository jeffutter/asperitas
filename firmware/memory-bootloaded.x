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
  /* The 32 KB of AXI SRAM above the 480 KB image the bootloader copies. Nothing is loaded here; it is
   * the one RAM region both MDMA and the CPU are known to reach without a caveat (see .replay_bss). */
  AXI_TAIL : ORIGIN = 0x24000000 + 480K, LENGTH = 32K
}

SECTIONS {
  .sram1_bss (NOLOAD) : ALIGN(4) {
    *(.sram1_bss)
    *(.sram1_bss*)
  } > RAM_D2
} INSERT BEFORE .bss;

/* Rig's excerpt replay staging (`REPLAY`, firmware/src/bin/rig.rs, TASK-074). Not DTCM: whether MDMA, the
 * master `Flash::read_async` refills it with, can write DTCM at 0x20000000 was not confirmed from RM0433
 * (the TCMs hang off MDMA's separate AHBS port, and a transfer must be steered to it; nothing in this tree
 * does that), and a refill that lands nowhere would play zeros. AXI SRAM is reached by MDMA's ordinary
 * master port. NOLOAD, no AT>: the image is not grown, and rig zeroes the section itself at boot
 * (`excerpt_play::zero_staging`) because cortex-m-rt zeroes only [__sbss, __ebss). The region LENGTH is the
 * overflow check: the linker refuses a staging that does not fit. scripts/check-bootloaded-builds.sh
 * checks the symbol's final address. */
SECTIONS {
  .replay_bss (NOLOAD) : ALIGN(32) {
    *(.replay_bss)
    *(.replay_bss*)
  } > AXI_TAIL
} INSERT BEFORE .bss;

/* cortex-m-rt zeroes [__sbss, __ebss) and sets __ebss, and the stack end, from the location counter
 * after .bss. A section inserted AFTER .bss that lives in another region leaves that counter in the
 * other region, so the zeroing loop ran from DTCM across unmapped memory to D2 and took an imprecise
 * bus fault before main (measured 2026-10-15, TASK-038.04.07.04: HFSR FORCED, CFSR 0x400, stacked PC
 * in Reset, __ebss = 0x30000400). .sram1_bss is NOLOAD and needs no zeroing; inserting it BEFORE .bss leaves
 * .bss as the last section placed, so the counter ends in RAM, and this assert keeps __ebss inside RAM. */
ASSERT(__ebss <= ORIGIN(RAM) + LENGTH(RAM), "__ebss left RAM: a section inserted after .bss lives in another region")
