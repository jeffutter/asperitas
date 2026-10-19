# Probe attach: TRACECLKEN wedge, reset catch and the nRESET net

Findings behind the probe rules in [flashing.md](../reference/flashing.md). Probe: ST-Link V3 MINIE
(firmware V3J15M7).

## Any `--chip STM32H7...` connect wedges the debug port

The generic attach `probe-rs info --protocol swd` (no `--chip`) works every time on a freshly
power-cycled board: DPv2, STMicroelectronics, part `0x4500`, core ROM table at `0xe000e000` readable.
Any attach naming the chip fails with `SwdDpError`, after which **even the generic attach fails until
the Seed's USB-C is unplugged and replugged**. RESET, `--connect-under-reset` and replugging the probe
all leave it stuck.

Cause, found by tracing OpenOCD's `examine-end` hook at `-d3`: it writes `DBGMCU_CR` (`0xE00E1004`
through AP2). The D1/D3 debug clocks (`0x00600000`), the sleep/stop/standby bits (`0x3F`) and the
watchdog freeze registers all succeed. The write that adds **bit 20, `TRACECLKEN`** (`0x0070003F`)
returns `STLINK_SWD_DP_ERROR` and the DP is dead. Writing `0x00100000` alone to `0x5C001004` on a fresh
board reproduces it; `0x00200000` or `0x00400000` alone does not. probe-rs's STM32H7
`debug_device_unlock` sets all those bits in one write on connect, selected purely by
`chip.name.starts_with("STM32H7")`. Why the trace clock faults here is unknown. Not the cause: the cable
(RAM write and flash read at 1 MHz are clean), the probe, the nRESET path (continuity-checked), or the
firmware image (the system bootloader fails the same way).

Remedy: do not write `TRACECLKEN`. OpenOCD with its `examine-end` hook cleared works; `defmt` over RTT
does not need the trace clock. For probe-rs, `firmware/asperitas-h750.yaml` is the stock `STM32H750IB`
entry (internal-flash algorithm only) renamed `ASPERITAS_H750IB`, which falls outside the `STM32H7`
prefix and gets the default ARM sequence. `probe-rs read b32 0x08000000 4 --chip ASPERITAS_H750IB
--chip-description-path firmware/asperitas-h750.yaml --protocol swd` returns the vector table on a
freshly power-cycled board without wedging. `probe-rs chip info` on it reports NVM
`0x08000000..0x08020000` (128 KiB) and AXI SRAM `0x24000000..0x24080000` (512 KiB), matching
`firmware/memory-internal.x`. Flashing, RTT and `--connect-under-reset` are verified with it, the last
only with the probe-rs below. Because the default sequence never sets `DBGMCU_CR`, firmware that sleeps
in WFI must set the debug-in-sleep bits itself.

Measured with it: attach 0.085 s plain and 0.21 s under reset, flash 3.55 to 3.68 s, RTT about 3000
records/s (about 65 KiB/s) without loss, a forced panic decoding to a 17-frame backtrace when stopped
with Ctrl+C and `--always-print-stacktrace`. Not measured: under-reset straight after a power cycle,
and `--speed` versus throughput. With the Seed unpowered the probe reads `VAPP` 0.58 V and attach fails.

## Under reset needs the flake's probe-rs (main, post-0.32.0)

Stock probe-rs 0.32.0 fails every `--connect-under-reset` attach on this board with `Unable to wait for
0 halted` / `Timeout while attaching to target under reset`; a plain attach always works. The cause is
probe-rs, not the reset net or the probe. Since probe-rs #3485 it arms reset vector catch
(`DEMCR.VC_CORERESET`), releases nRESET, and only then sets `DHCSR.C_DEBUGEN`. Vector catch fires only
if C_DEBUGEN is already set as the core leaves reset, so the core runs past it. C_DEBUGEN survives
nRESET and is cleared only by power-on, which is why an OpenOCD session (it leaves the bit set) let
exactly one probe-rs attach through. The stock ST sequences leave it set after a failed attach, so
stock chip names fail only once after power-up (probe-rs #4113). The renamed chip gets
`DefaultArmSequence`, whose teardown writes `DHCSR = 0`, so it failed every time. Upstream PR #4441
(merged, built from main in `flake.nix` until a release) sets C_DEBUGEN while reset is still held. With
it, 5 of 5 reads and 3 of 3 `make probe-flash` runs passed under reset, against 0 of 4 stock runs
interleaved with them. Full trace and the controlled test:
[probe-rs-connect-under-reset.md](../upstream/probe-rs-connect-under-reset.md).

## Under reset, an ST-Link gets a plain reset

probe-rs runs a target's custom reset sequence only when the probe exposes a DAP interface; the native
ST-Link driver does not, so the attach logs `Custom reset sequences are not supported on ST-Link V3.
Falling back to standard probe reset.` The replacement is the probe's generic reset-pin drive
(`JTAG_DRIVE_NRST_LOW`), so the flag still means "hold the chip in reset while I attach" with less chip
knowledge. This is the ST-Link driver, not a V3 quirk: a V2 on the same pads falls back identically.
Family-specific work is not all lost: H7's DBGMCU debug-component enable goes through the memory
interface, which the fallback leaves alone.

What remains is electrical: whether the probe pulls this board's nRESET low far enough, long enough.
That is probe-rs #3516. Its reporter's board really was the problem (an oscilloscope showed the
ST-Link's nRESET partly fighting a MIC6315 supervisor and a 74-series buffer), but the thread is open
and gained a second report on different hardware (STLINK-V3MINIE against STM32U5, `cubeprogrammer-cli`
fine on the same bench, intermittent failures), so the probe class is a live suspect on an unmodified
board. Hence: **"flashes fine without the flag, fails with it" is an expected outcome of this probe
class, not evidence the bench is broken.** probe-rs's FAQ: try with and without the flag, since some
chips need it and others do not support it.

## What hangs off nRESET

There is no Seed3 schematic to read. Electrosmith publishes a databrief, pinout PDF and CSV, 3D models
and a compliance zip, and nothing else; archived copies of the documentation page list the same five
assets. The databrief has zero occurrences of `RESET`, `NRST` or `supervisor`. The search is logged in
[seed3-schematic-search-log.md](seed3-schematic-search-log.md).

The net is traced on the public drawings, each by the revision and date in its title block. All four
name the net `RESET`:

| Drawing | Sheet date | Everything drawn on the `RESET` net |
|---|---|---|
| `ES_Daisy_Seed_Rev4.pdf` (full, 4 sheets) | 2020-06-08 | `R19` 10 K to `+3V3_D`; `S3` PTS815 button to GND; MCU `NRST`; `P6` mini-JTAG pin 10. No capacitor. |
| `ES_Daisy_Seed_Rev7.pdf` (reduced) | 2024-02-01 | The same three plus `C32` 100 nF to GND. |
| `ES_Daisy_Seed2_DFM_Rev5-REDUCED.pdf` | 2025-02-17 | `R1` 10 K to `+3V3_D`; `S1` button to GND; `P8` pin 10; MCU `NRST`. No capacitor. |
| `ES_Daisy_Patch_SM_Schematic.pdf` | 2024-02-08 | `R1` 10 K to `+3V3_D`; `S1` button to GND; `P8` pin 10. |

In each, MCU `NRST` runs straight to that net with nothing between, and debug header pin 10 (standard
Cortex Debug nRESET) is the same net as the front-panel button. Traps: the tokens on the `NRST` wire
beside the BGA symbol (`J1`, with `C6` and `D6` on `PDR_ON` and `BOOT0`) are UFBGA-169 ball
coordinates, not jumpers. The Daisy Pod schematic (2022-10-27) contains no reset net, so seating the
module in a Pod changes none of this.

Electrically that is the benign case. The probe's driver sinks only the pull-up (3.3 V across 10 K is
about 0.33 mA) plus one RC constant discharging the 100 nF Rev7 cap through the same 10 K (tau about
1 us). There is no second driver to fight. On the circuit drawn, a supervisor, buffer, gate, diode or
transistor on nRESET is ruled out across four drawings spanning five years. For the Seed3 itself that
is an inference: its drawing is unpublished and three of the four sheets are explicitly reduced, so
absence of a supervisor is weaker evidence than presence of the 10 K, button and capacitor. Against
that, the Seed3 is the same MCU in the same footprint with a new codec and USB connector, and the reset
net belongs to the part nobody says changed. Treat `--connect-under-reset` as likely to work on this
board, not known to. If it fails anyway, suspect the probe side first (V3 firmware below 3.2, a V3MINIE
still in MassStorage mode) before a supervisor no Seed has had.
