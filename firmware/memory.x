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
