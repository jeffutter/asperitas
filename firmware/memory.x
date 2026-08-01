MEMORY
{
  /* Internal flash — 128 KB, the region used by DFU bootloader.
     QSPI flash (8 MB at 0x90000000) is not included here; add it via
     a separate MEMORY region or linker script when using the Daisy
     bootloader for larger applications. */
  FLASH : ORIGIN = 0x08000000, LENGTH = 128K
  RAM   : ORIGIN = 0x24000000, LENGTH = 1M
}
