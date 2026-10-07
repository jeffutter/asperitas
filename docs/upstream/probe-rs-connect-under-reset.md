# probe-rs `--connect-under-reset` never halts a Cortex-M whose DHCSR.C_DEBUGEN is clear

Status: root cause found and a fix verified on the bench, 2026-10-07. Not reported upstream yet.
Everything below is measured on one board and one probe unless marked otherwise.

## Summary

Since probe-rs PR #3485 (merged 2025-09-10, "call `enable_arm_debug` sequence after srst
deassertion"), the connect-under-reset path in `Session::attach_arm_debug_interface` runs:

1. assert nRESET
2. attach SWD, `debug_device_unlock`
3. `reset_catch_set`: for Cortex-M, sets only `DEMCR.VC_CORERESET`
4. deassert nRESET
5. `debug_core_start`: sets `DHCSR.C_DEBUGEN`
6. wait 100 ms for `DHCSR.S_HALT`

Reset vector catch only fires when halting debug is enabled (`DHCSR.C_DEBUGEN = 1`) at the moment
the core leaves reset. In this order C_DEBUGEN is set only after reset is released. Unless an earlier
session left C_DEBUGEN on, the core runs straight past the catch, never halts, and the attach fails
with `Unable to wait for 0 halted` / `Timeout while attaching to target under reset`.

C_DEBUGEN is cleared only by a power-on reset, not by nRESET. That explains all the earlier
observations:

- **OpenOCD "primes" it.** OpenOCD sets C_DEBUGEN while it examines the target and leaves it set on
  shutdown, so the next probe-rs under-reset attach works.
- **Only once.** When a probe-rs session is dropped, `DefaultArmSequence::debug_core_stop` writes
  `DHCSR = 0`, so the attach after that fails again.
- **Stock ST chip names fail only on the first try after power-up.** The ST vendor sequences
  (`Stm32Armv6`, `Stm32Armv7`, `Stm32h7`) override `debug_core_stop` and never clear DHCSR. A
  failed attach still runs step 5, so C_DEBUGEN stays set and the retry works. This matches upstream
  issue **#4113** (G0, H5, L5 Nucleo: "fails after power cycle, then succeeds"). Inference: those
  reports have not been checked against this mechanism.
- **Our renamed chip fails every time.** `ASPERITAS_H750IB` deliberately falls outside the `STM32H7`
  prefix match (see "Related problem"), so it gets `DefaultArmSequence` and its DHCSR-clearing
  `debug_core_stop`.

The probe, the reset wiring and the flash contents were never involved.

## Environment

| | |
|---|---|
| Target | STM32H750IBK6 on a Daisy Seed3 module, seated in a Daisy Pod |
| Probe | ST-Link V3, OpenOCD reports `STLINK V3J15M7 (API v3) VID:PID 0483:3754` |
| probe-rs | 0.32.0 (crates.io build). Master as of 2026-10-07 has the same attach order. |
| OpenOCD | 0.12.0 (nixpkgs) |
| Host | macOS, Apple silicon |
| Wiring | 14-pad debug footprint with a soldered pin header, straight-through 1:1 ribbon to the probe's STDC14. nRESET on pin 10 of the 10-pin layout, 10 K pull-up to 3V3. VAPP 3.25 V. |
| Chip entry | `firmware/asperitas-h750.yaml`: the stock `STM32H750IB` entry renamed to `ASPERITAS_H750IB`, passed with `--chip-description-path` |

## Evidence

### Trace order

`RUST_LOG=probe_rs=trace` on a failing `probe-rs read ... --connect-under-reset` shows these spans
in this order: `reset_catch_set` (DEMCR write to `0xE000EDFC`), `reset_hardware_deassert`
(ST-Link command `f2 3c 01`), `debug_core_start` (DHCSR write), then 46 DHCSR reads in
`wait_for_core_halted` until the 100 ms timeout. All the reads succeed; the "ARM specific error"
is the timeout, not a bus fault.

### Controlled test: DHCSR, not the probe

OpenOCD ran before every probe-rs attempt, so any probe-side effect of OpenOCD is the same on both
arms. The only difference was the DHCSR value OpenOCD left behind (`mww 0xE000EDF0 ...`, read back
with `mdw`). Each line is the stock probe-rs 0.32.0 `read ... --connect-under-reset` that followed:

| OpenOCD leaves DHCSR | read back | probe-rs under reset |
|---|---|---|
| `0xA05F0000` (C_DEBUGEN = 0) | `01010000` | timeout, 3 of 3 |
| `0xA05F0001` (C_DEBUGEN = 1) | `01010001` | success, 3 of 3 |

The runs alternated, 6 in total, and every one matched the prediction.

### Fix verified on hardware

Patch against probe-rs 0.32.0: set C_DEBUGEN inside the Cortex-M `reset_catch_set`, while reset is
still held. The Cortex-M SCS is reachable under reset, which the existing DEMCR write in the same
function already relies on. Cortex-A/R cores, the case #3485 was written for, use different
functions and are untouched.

```diff
--- a/probe-rs/src/architecture/arm/sequences.rs
+++ b/probe-rs/src/architecture/arm/sequences.rs
@@ -369,6 +369,17 @@
 fn cortex_m_reset_catch_set(core: &mut dyn ArmMemoryInterface) -> Result<(), ArmError> {
     use crate::architecture::arm::core::armv7m::{Demcr, Dhcsr};
 
+    // Vector catch only fires when halting debug is enabled. DebugCoreStart now
+    // runs after reset is released, so enable C_DEBUGEN here (SCS is reachable
+    // under reset on Cortex-M) or the core runs straight past the catch.
+    let dhcsr = Dhcsr(core.read_word_32(Dhcsr::get_mmio_address())?);
+    if !dhcsr.c_debugen() {
+        let mut dhcsr = Dhcsr(0);
+        dhcsr.set_c_debugen(true);
+        dhcsr.enable_write();
+        core.write_word_32(Dhcsr::get_mmio_address(), dhcsr.into())?;
+    }
+
     // Request halt after reset
     let mut demcr = Demcr(core.read_word_32(Demcr::get_mmio_address())?);
     demcr.set_vc_corereset(true);
```

Built as probe-rs-tools 0.32.0 with `[patch.crates-io] probe-rs = { path = ... }`. No OpenOCD was
run during this test. Every patched session ends with the stock `debug_core_stop`, which writes
`DHCSR = 0`, so every patched attempt started with C_DEBUGEN clear.

| Binary | `read ... --connect-under-reset` |
|---|---|
| stock 0.32.0 | timeout, 4 of 4 |
| patched | success, 8 of 8 (interleaved with the stock runs) |
| patched, `download --connect-under-reset --verify --reset` | success, 2 of 2 back to back, 3.55 s each |

The fix is a candidate for upstream. Where it belongs is the maintainers' call: in
`cortex_m_reset_catch_set` as above, or by running `debug_core_start` before reset release for
Cortex-M only.

## Hypotheses refuted along the way

- Stale target state: a power cycle changed nothing. Power-on clears C_DEBUGEN, so it could not have.
- Firmware misbehaving at startup: a blank chip failed the same way.
- Debug-domain clocks off: `DBGMCU_CR` already read `0x00600007`, and writing `0x0060003F` changed
  nothing.
- ST-Link state left by OpenOCD: ruled out by the controlled test above.

## Related problem (separate upstream issue candidate, still open)

On this board, writing `DBGMCU_CR.TRACECLKEN` (bit 20) kills the debug port until the Seed's USB-C is
unplugged and replugged. probe-rs's STM32H7 `debug_device_unlock` sets all the debug bits in one write
(`0x0070003F`) on every connect, selected by `chip.name.starts_with("STM32H7")`, so any
`--chip STM32H750IBKx` attach wedges the port. OpenOCD's `stm32h7x.cfg` `examine-end` hook does the
same write and fails the same way (`STLINK_SWD_DP_ERROR` on that write; writes of `0x00600000` and
`0x3F` succeed; writing `0x00100000` alone to `0x5C001004` on a fresh board reproduces it). The cause
is unknown. The renamed chip entry works around it, and that rename is why this board always hit the
DHCSR problem above rather than only after power-up. As far as we know the two problems are unrelated.

## Where the evidence lives

Bench notes are in backlog ticket TASK-037
(`backlog/tasks/task-037 - Attach-the-ST-Link-and-verify-the-probe-path-on-hardware.md`). The pad
layout and the TRACECLKEN finding are in `docs/reference/daisy-seed3.md`.
