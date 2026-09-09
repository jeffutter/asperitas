---
id: TASK-038
title: Generate stimulus and capture audio on-device for self-loopback measurement
status: To Do
assignee:
  - '@agent'
created_date: '2026-09-09 02:12'
labels: []
dependencies: []
documentation:
  - docs/reference/daisy-seed3.md
  - docs/reference/daisy-pod.md
modified_files:
  - firmware/src/bin/podtest.rs
  - crates/asperitas-logging/src/usb.rs
priority: high
type: feature
ordinal: 44500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Pod self-loopback in TASK-034 closes the electrical path but leaves nobody generating stimulus and nobody recording the return. That is this ticket, and it is the work that replaces buying an external audio interface — cost moved from money to engineering time deliberately.

The shape follows from hardware already present. Stimulus is generated digitally, so its level is a constant rather than a pad switch someone hunts, and clipping is decided in code. Capture lands in the 64 MB SDRAM, which at 48 kHz mono float holds roughly five minutes — enough for the multi-minute dropout runs TASK-019.03 asks for without streaming in real time. Dump happens after the run rather than during it, because full-speed USB CDC has neither the sustained rate nor the isolation from logging traffic that a live stream would need; recording into memory and draining afterwards turns a bandwidth problem into a latency problem.

Real playing material needs no host audio output either: excerpts from audio/instruments/ fit in the 8 MB QSPI flash and replay as exact stimulus. Keep the excerpt small and named — storing a corpus wholesale would spend the whole flash and leave nothing for firmware images.

One constraint worth stating before implementation starts: the dump must not be able to corrupt live audio, and live audio must not be able to corrupt the dump. Sharing DMA buffers between them is how a test passes while measuring its own tearing.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A build mode plays deterministic stimulus out of the Pod jack — sine at a selectable level, swept sine, and an impulse train — generated in the DSP domain so its amplitude is exact and its phase starts at a sample boundary.
- [ ] #2 Input samples are recorded into SDRAM with the footprint stated in bytes per second, and recording cannot overwrite a block that is being dumped.
- [ ] #3 Recorded blocks reach the host through the framed transport from TASK-030 with a checksum that either matches or the host reports loss, and dumping does not starve the audio callback — verified by the drop counter staying at zero during a dump.
- [ ] #4 Maximum capturable duration is reported by the device rather than guessed by the caller, with headroom over live audio buffers stated.
- [ ] #5 A named excerpt from audio/instruments/ can be stored in QSPI and replayed as stimulus bit-exactly, so playing-real-material tests need no host analog path; storage cost per second is documented.
- [ ] #6 Host-side tests parse the dump format from synthetic data with no board attached.
- [ ] #7 Usage is documented alongside the other host tooling, and the SDRAM and QSPI budgets land in docs/reference/daisy-seed3.md.
<!-- AC:END -->
