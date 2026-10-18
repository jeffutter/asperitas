# Rejected routes for freeing internal flash

The install-path `rig` (excerpt install) linked at 143,312 B (seed3), 145,104 B (`stim-ess`) and
143,536 B (`stim-pulse`) against 131,072 B of internal flash, about 18.3 KB over the 125,016 B baseline
at opt-level 3.

| Route | Result (seed3) | Why not |
|---|---|---|
| whole-workspace `opt-level = "s"` | 116,856 B | fits, but changes callback codegen for every binary, invalidating the audio timings |
| whole-workspace `opt-level = "z"` | 111,548 B | same |
| `asperitas-logging` at `"s"` alone | 131,904 B | still 832 B over |
| plus `embassy-usb` and `embassy-usb-synopsys-otg` at `"s"` | 132,720 B | worse; generic async code is monomorphized into the bin crate |
| `embassy-stm32` at `"s"` | 142,064 B | barely moves |
| split rig into its own package with a per-package override | not built | still changes rig's callback codegen, and leaves little room for growth |

Chosen: the Daisy bootloader route ([bootloaded-layout.md](./bootloaded-layout.md)). It moves the
callback's code and SAI buffers (AXI SRAM and D2), so timings taken from the internal layout predate it
and need re-measuring from the bootloaded one.
