# cpal duplex audio on macOS, and how DSP blocks relate to host buffers

Evidence for the `asperitas-live` design (TASK-145.01). Everything below comes from a throwaway program
run on this Mac on 2026-10-19; the program lives outside the repo and is not committed. It opened
`cpal` streams on the default devices, counted what the callbacks received, and ran a bit-exact
block-size equivalence check against `asperitas-dsp`.

## Setup

- `cpal` 0.18.2 (CoreAudio host), `ringbuf` 0.5.3 for the SPSC ring, `asperitas-dsp` by path.
- Release build, run from a terminal. Host: macOS (Darwin 27.0.0).
- Output was silence in every run so a headphone-jack microphone could not feed back. Timing and
  callback counts do not depend on the output content.

## Devices and supported configurations

Default input: `coreaudio:BuiltInHeadphoneInputDevice`, described as "External Microphone".
Default output: `coreaudio:BuiltInHeadphoneOutputDevice`, described as "External Headphones".

| | Input | Output |
|---|---|---|
| Channels | 1 | 2 |
| Sample formats | f32 only | f32 only |
| Sample rates | four discrete rates: 44100, 48000, 88200, 96000 (no continuous range) | same four |
| Buffer size range | min 15, max 4096 frames | min 15, max 4096 frames |
| Default config | 48000 Hz, f32 | 48000 Hz, f32 |

48 kHz f32 is available on both sides and is the default, so the effect's rate needs no resampling.
The input is mono, the output stereo. `asperitas-live` must therefore accept a mono input and
duplicate it (or apply a channel policy) rather than assume two matching channel counts. The devices
were the built-in jack pair at the time; an audio interface would report its own channel counts and
ranges, so enumerate at start rather than hard-coding these.

## Fixed buffer sizes

A 48 kHz f32 input stream and output stream were opened together with `BufferSize::Fixed(n)`, run for
6 s each, and every callback's frame count was recorded.

| Requested | Opens (in and out) | Callback length received | Distinct lengths seen |
|---|---|---|---|
| 32 | yes | exactly 32 | 1 |
| 64 | yes | exactly 64 | 1 |
| 128 | yes | exactly 128 | 1 |
| 256 | yes | exactly 256 | 1 |
| 512 | yes | exactly 512 | 1 |
| `Default` | yes | exactly 512 | 1 |

Every callback in every run had the requested length, with no short or irregular callbacks, and no
stream errors were reported. 32 frames (the device's `BLOCK_LENGTH`) opens, but its callback rate
(1500 per second) is the heaviest.

## Do input and output share a clock

cpal on macOS gives two independent streams (one input callback, one output callback), not a single
duplex callback. In these runs both sides ran on the same built-in device clock:

- 35 s run at 128 frames: input 13168 callbacks / 1685504 frames, output 13126 callbacks / 1680128
  frames. The difference of 5376 frames is a constant startup offset (the input stream began
  delivering about 110 ms before the output stream's first callback), not a rate difference.
- The ring fill, measured at every output callback after a 2 s warm-up, stayed at 256 frames (two host
  buffers) for the whole run, with one dip to 128 frames. Zero underruns, zero overruns. A drift between
  clocks would show as a steadily growing or shrinking fill; none appeared. The same held in each 6 s
  run at every buffer size: the fill stayed constant for the whole run (the value was set by how far the input
  stream started ahead of the output) with zero underruns.

So with the default built-in pair the two streams are rate-locked, but they are still two callbacks
on separate threads with a start-up offset and per-callback jitter of about one buffer. A ring between
them is required. Two different devices (for example USB interface in, built-in out) were not tested
and would have independent clocks; the ring and a fill-based drift check would be needed there.

Recommended ring: a lock-free SPSC ring of mono or stereo frames, capacity at least 8 host buffers and
no less than 4096 frames, with the output side holding back until the ring contains one host buffer
before it starts consuming (a prefill of one buffer; the observed steady fill was two buffers at 128 to 512 frames and larger where the input started further ahead).
Trim any surplus above the prefill at start, because the input stream starts first and would otherwise
leave extra frames in the ring as permanent latency (this showed as a 60 ms fill at 32 frames in the
untrimmed spike). Count underruns and overruns and surface them.

## Round-trip latency

Reported by cpal timestamps, averaged over each run: for input the time from `capture` to `callback`,
for output the time from `callback` to `playback`.

| Buffer (frames) | Input capture to callback | Output callback to playback | Sum | Sum in frames |
|---|---|---|---|---|
| 32 | 3.333 ms | 3.708 ms | 7.04 ms | 338 |
| 64 | 4.000 ms | 4.375 ms | 8.38 ms | 402 |
| 128 | 5.333 ms | 5.708 ms | 11.04 ms | 530 |
| 256 | 8.000 ms | 8.375 ms | 16.38 ms | 786 |
| 512 | 13.333 ms | 13.708 ms | 27.04 ms | 1298 |
| Default (512) | 13.333 ms | 13.708 ms | 27.04 ms | 1298 |

The fit is exact: input = buffer + 128 frames, output = buffer + 146 frames, so the cpal-reported path
is `2 x buffer + 274 frames` (about 5.7 ms of fixed device and safety offset). To that add the ring
prefill (one host buffer with the recommendation above) for the full input-to-output delay inside
`asperitas-live`, roughly `3 x buffer + 274 frames`: about 12.4 ms at 64 frames and 14.0 ms at 128.

These are cpal's own reported figures. **No physical loopback (cable or acoustic) was measured.** The
cpal numbers describe what CoreAudio says about its buffers and do not include converter or analog
latency of the jack.

## Microphone permission

Input was not blocked: the input callbacks delivered non-zero samples (peak about 0.025, RMS about
0.0095) in every run, so the terminal had microphone access. The level is consistent with the jack's
noise and room signal; it was not checked against an intentional test tone, so this shows the permission
is not blocking, not that the signal is acoustically correct.

## Decision: DSP block versus host buffer

**Decouple the DSP block size from the host buffer size, with no re-chunking and no added latency.**
`asperitas-live` processes whatever length the host callback hands it in one call, and reproduces the
device's control timing by latching a pending control change only at frame indices that are multiples
of 32 counted from stream start, splitting the callback's processing at that index.

Reasons:

- `Asperitas` is per-sample: `process_block` is bit-identical to repeated `tick`
  (`matches_manual_composition_bit_for_bit` in `crates/asperitas-dsp/src/asperitas.rs`). The only
  block-dependent behaviour is when `set_controls` takes effect, because firmware calls `set_controls`
  once before each 32-frame block.
- Checked directly: 200 trials over a 5 s noise signal, each with a control change requested at a
  random frame. Path (a) is the device: 32-frame blocks, `set_controls` before the first block at or
  after the request. Path (b) is the proposed host path: host buffers of random length 1 to 1024,
  `set_controls` applied at the same 32-multiple index by splitting the buffer there. All 200 trials
  matched bit for bit.
- The alternative, re-chunking host buffers into 32-frame blocks through a FIFO, adds up to 31 frames
  (0.65 ms) of latency and a stateful buffer for no change in output, and still needs the ring.
- A fixed host buffer is also not guaranteed on other devices (the range is 15 to 4096 here, and
  `Default` gave 512), so the engine must accept arbitrary lengths anyway.

The 32-multiple counter must run from the stream start and advance by the frames processed, so the
latch points do not depend on how the host chops the stream. Offline equivalence with
`asperitas-cli process` follows from the same rule.

## Implications for asperitas-live

- Request 48 kHz f32 and a fixed buffer of 64 to 128 frames (about 12 to 14 ms in-to-out). 32 frames
  opens but gains little over 64. Fall back to `Default` and accept whatever length arrives.
- Accept mono input and stereo output (or any channel counts) by enumerating, and treat the
  channel mapping as an explicit policy.
- Use a ring between the input and output streams as described above, with underrun and overrun
  counters.
- Never assume a loopback figure from these numbers; a physical loopback measurement remains
  unmeasured.
