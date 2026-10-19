# Pod listening session with `main`

Historical record of an owner listening session at the bench (2026-10-16). The durable facts are
summarised in [daisy-pod.md](../reference/daisy-pod.md#listening-with-main).

Recorded 2026-10-16. The owner listened to a mandolin loop through the Pod on headphones and
agreed that knob 1 (one-pole low-pass, `20 * 1000^n` Hz) and knob 2 (gain, `-60 + 72 n` dB) change the
sound in the intended direction with the intended character, with no zipper noise, crackle, wobble
or hiss, while moving or held still, and that behaviour survives a power cycle.

- **Use broadband material for knob 1.** On a pure sine the low-pass only acts as a gain control once
  its cutoff passes the fundamental, so knob 1 sounds like knob 2. Judge the filter on an
  instrument recording, not a tone.
- **Mac as the source:** the Mac headphone path is far quieter than line level (Mac output at 25%
  gave about -55 dB at the Pod input). Play at 50 to 80% for listening. To record the Pod's output
  back with the Mac mic input, keep input volume at 5 to 15%; 50% clips on the Pod's line-level
  output. A 60 Hz hum near -52 dBFS showed up on the Mac mic recordings.
- **The audio callback must run on an interrupt executor.** `main` runs it on SAI1 at priority P6
  (as `rig` does). A callback has about 1.3 ms after a half-transfer or `read` returns
  `sai::Error::Overrun`, and on the thread executor the knob, console and LED tasks made it miss.
  After an overrun `main`'s audio loop ends and stays ended (`AUDIO_EXIT` holds the reason); there
  is no recovery policy yet.
- **Do not attach a probe while listening.** Attaching halts the core briefly, which overruns the
  SAI ring and ends the audio loop until reset. Reset without a probe attached
  (`probe-rs reset`, then detach) before listening. Reading statics after listening is fine.
- **Half-plugged input jack:** in one session the output was audible on the Pod headphone jack only
  with the input cable half inserted. Two cables behaved the same; a fully seated cable worked
  fine in the later session. The cause (jack or cable) was not found. Seat plugs fully.
