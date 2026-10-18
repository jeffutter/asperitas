# RTT behaviour and the D-cache hazard

Why the probe runbook ([flashing.md](../reference/flashing.md)) says what it says about `defmt`
over RTT. Everything here concerns the `log-defmt` build with `defmt-rtt` 1.3.0 and probe-rs.

## Four regimes an RTT stream runs in

`defmt-rtt` initialises its up-channel to `NON_BLOCKING_TRIM`; probe-rs flips it to block-if-full on
attach, and `defmt_rtt::in_blocking_mode()` tells firmware which is in effect.

| Host state | Mode in effect | What happens |
|---|---|---|
| No host attached | `NON_BLOCKING_TRIM` | frames truncated or dropped, silently, with no counter anywhere |
| Attached, host keeping up | block-if-full | nothing lost |
| Attached, host stalled | block-if-full | target spins in the write loop with interrupts disabled; the application freezes, audio first |
| Host detached mid-run | still block-if-full as far as the target knows | the same freeze, from the ordinary cause of closing a terminal |

Every `defmt` frame is written inside `critical_section::acquire()`, against a 667 us audio block
deadline (32 samples at 48 kHz). Hence the absolute rule: **nothing may log from the audio callback.**
The logger also panics rather than nesting, so reentrancy is fatal.

Row four is `defmt-rtt`'s own documented behaviour: probe-rs puts RTT into blocking mode, so the
implementation may block forever if probe-rs disconnects at runtime, and `defmt::flush` would block
too. The mechanism is one assumption deep: the channel decides a host is connected from the mode bits
alone, so a host that set `BLOCK_IF_FULL` and vanished leaves the target writing as though someone
still drained it. Whether probe-rs restores the flags on detach is **not measured**. To measure it:
attach, stream, kill the reader, read `_SEGGER_RTT.up_channel.flags`.

Levers:

- Host side, for a deliberately slow host: `--rtt-channel-mode no-block-skip` drops a whole frame
  when it does not fit; `no-block-trim` writes what fits. `block-if-full` is the default.
- Target side: `defmt-rtt`'s `disable-blocking-mode` feature forces the non-blocking write path even
  after probe-rs sets BlockIfFull. It is the only lever that reaches row four (no host is left to
  pass a flag to), and it costs losslessness while attached. Insurance under consideration, not a
  default; the no-logging-from-the-callback rule is the primary defence.

## Driver frames are compiled into the RTT image

Selecting `log-defmt` turns defmt on across `daisy-embassy`'s whole dependency stack, so driver frames
are compiled in whether or not we write them (`cargo tree -e features -i defmt@0.3.100 --features
seed3,log-defmt` is the receipt quoted in `firmware/Cargo.toml`). The RTT image carries 82 defmt frame
symbols, 75 of them `defmt_error`:

```bash
cd firmware && CARGO_TARGET_DIR=$(mktemp -d) cargo nm --release --no-default-features \
  --features "seed3 log-defmt" --bin main -- | grep -c '"package"'
```

The flags are load-bearing because `cargo nm` re-runs the build; omit them and it builds the console
image, which links the no-op logger and contains no `SEGGER` magic. To tell the two images apart, scan
bytes (`strings -a <ELF> | grep -c SEGGER`), not symbols. Better: every ELF carries a non-allocated
`.asp.prov` note written by `firmware/build.rs` naming its cfg set; `scripts/elf-provenance.sh show
<ELF>` prints it. When dumping it by hand, pass an explicit output file
(`rust-objcopy --dump-section .asp.prov=/dev/stdout <ELF> /dev/null`); without one, objcopy rewrites
the input ELF in place (identical bytes, fresh mtime). The script's `/dev/null` guard stays because a
read command must not rewrite the artifact it reads.

One ERROR frame worth knowing: `Ringbuffer broken invariants detected!` comes from
`embassy-stm32`'s SAI `impl From<ringbuffer::Error> for Error`, fired on `DmaUnsynced`. `daisy-embassy`
reaches it from `start_callback`'s `codec.read`/`codec.write` loop, so it runs in task context, not the
audio callback. It is still an ERROR-level frame, the one level that survives an unset `DEFMT_LOG`,
written inside the RTT critical section at the moment audio is already going wrong. No filter is
proposed; a path filter such as `DEFMT_LOG=off,crate=off` yields roughly 300 proc-macro errors.

## Sleep modes and RTT discovery

On several STM32 parts, unless `DBG_SLEEP`/`DBG_STANDBY`/`DBG_STOP` are set in `DBGMCU_CR`, the RTT
control block is never found once the core has slept (probe-rs #350). This stack sleeps:
embassy-executor runs `wfe` after every `poll()` that finds nothing to do, with no feature to opt
out. Nothing in `firmware/src` sets the `DBGMCU_CR` bits, so the hazard is live whenever the core idles.
The ST-Link path works anyway because the probe attaches and halts the core rather than finding the
block while it sleeps. Treat any change that lengthens idle time, or moves `_SEGGER_RTT`, as needing a
fresh attach test.

## The D-cache is the other way to lose the control block

Latent, not present: nothing enables I- or D-cache anywhere in this stack (no cache or MPU call in
embassy-stm32's `src/`, in daisy-embassy's boot path or in cortex-m-rt's startup, and no cache symbol
in the linked image). daisy-embassy's SDRAM builder enables the MPU with a cacheable region at
`0xD000_0000`, but an MPU region is not the D-cache, and that region does not cover the SDRAM (see the
SDRAM section of [daisy-seed3.md](../reference/daisy-seed3.md)). Whoever enables caching for DSP
headroom breaks RTT silently, and will not suspect the cache.

`_SEGGER_RTT` lands wherever the linker puts `.data`: `0x24000008`, 48 bytes, in a `log-defmt` release
image, inside the AXI SRAM region `0x24000000..0x24080000`. Enable the Cortex-M7 D-cache over it and the
host stops seeing what the core wrote: writes sit in the cache until evicted while debug-port reads go
to the memory system. Either the `SEGGER RTT` magic is not there yet so discovery fails, or the block
looks present with stale write index and ring. SEGGER forum thread 5360 is the symptom record (block at
`0x24000000` missed by auto-search, found manually but no data, worked once moved to DTCM at
`0x20000000`); it does not mention caching, so read it for the symptom, not the explanation.

SEGGER's RTT knowledge-base page (*Cortex-M specifics*) requires, if the CPU implements caches: the
control block and all buffers cache-line aligned and a multiple of a cache line in size (using the
largest line size of any cache level); the application to clean + invalidate the control block and
buffers after segment init but before first use; virtual address equal to physical address; and an
uncached address alias provided for the control block and buffers.

Measured against that list (release `main`, `log-defmt`; other binaries land elsewhere; re-run to
check):

```bash
cd firmware && CARGO_TARGET_DIR=$(mktemp -d) cargo nm --release --no-default-features \
  --features "seed3 log-defmt" --bin main -- | grep -Ei "SEGGER_RTT|defmt_rtt.*BUFFER"
```

| Object | Address | Size | Output section | Bytes into its 32-byte cache line |
|---|---|---|---|---|
| `_SEGGER_RTT` | `0x24000008` | 48 | `.data`, which starts at `0x24000000` | 8 |
| `defmt_rtt::BUFFER` | `0x240010e4` | 1024 | `.uninit` | 4 |

Neither starts cache-line aligned, and the 48-byte control block straddles a line boundary and shares
lines with unrelated `.data`. It gets away with that only because the cache is off. `defmt-rtt` offers
no handle: its only features are `disable-blocking-mode` and `drop-on-contention`. The nearest hook is
that the ring is emitted into the named input section `.uninit.defmt-rtt.BUFFER` and the channel name
into `.data.defmt-rtt.NAME`, which a linker script can select, so an upstream align-and-section patch
is plausible. The two objects sit about 4 KB apart with unrelated data between, so one non-cacheable
MPU window covering both makes those neighbours uncached too, and partial-line sharing defeats
invalidate-by-address on an M7 regardless. Two credible routes remain: an MPU non-cacheable window over
the start of AXI SRAM, or the upstream patch. Linker work alone is necessary and not sufficient; the
clean + invalidate and the uncached alias are application work. Neither route is scheduled; the day the
SDRAM/DSP work wants the cache on is the day to plan it.

Trap on hand-moving the block to DTCM: a `NOLOAD` section placed naively outside DTCM drags `__ebss`
across an unmapped gap and bus-faults before `main`. That passes in Renode (which backs the gap) and
fails only on silicon; daisy-rs documents it on the same H750 in `docs/memory-placement.md`. A relocated
block stays findable: `probe-rs chip info ASPERITAS_H750IB --chip-description-path
firmware/asperitas-h750.yaml` lists DTCM `0x20000000..0x20020000` among the RAM regions.

## Why `--verify` stays on

pyOCD issue #1700 ("STM32H750 flash corruption with double buffering enabled") is the closest
published account of a bad download on this part: a 30 kB image corrupted roughly one attempt in five
with no sign of failure, pages partially written with the tail left `0xff`, traced to double buffering
under either erase mode. A second reporter hit it on a Daisy Seed (about one in three tries across three
probes) while OpenOCD programmed and verified the same bench flawlessly. pyOCD closed it by disabling
double buffering for flash algorithms by default. pyOCD runs its own flash loader, so this is evidence
about the technique and the chip, not probe-rs's implementation; what it establishes is the failure's
shape, silent partial writes, which on a single 128 KB sector means a board that boots wrong or not at
all. Read-back is the only protection, so no flash recipe drops `--verify`, and a timing-out download
should try `--disable-double-buffering` before the bench is blamed.
