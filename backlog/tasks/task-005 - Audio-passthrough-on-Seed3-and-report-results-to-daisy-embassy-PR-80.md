---
id: TASK-005
title: 'Audio passthrough on Seed3, and report results to daisy-embassy PR #80'
status: Done
assignee:
  - '@human'
created_date: '2026-08-01 05:45'
updated_date: '2026-08-05 14:13'
labels: []
dependencies:
  - TASK-004
documentation:
  - docs/reference/daisy-seed3.md
  - docs/reference/daisy-pod.md
priority: high
type: feature
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Get audio in and out of the Seed3 in the Pod. This is the milestone that de-risks the project.

Build daisy-embassy's passthrough against `--features seed3` from the pinned PR #80 commit. The API is a block callback:

```rust
interface.start_callback(|input, output| {
    output.copy_from_slice(input);
}).await
```

Target 48 kHz with a 48-sample block, matching libDaisy's Pod default and the configuration PR #80 verified. The Seed3 codec can do 192 kHz/32-bit; there's no musical reason to spend the CPU here.

**Verify, don't assume** — reproduce the checks the PR author ran on this same Seed3-in-Pod configuration:
- board boots and resets reliably
- USB connects and reconnects
- stereo in and out at 48 kHz with left and right channels **not swapped**
- line-out to line-in loopback passes audio repeatably

Note the Pod's audio I/O is **line level, not hi-Z instrument level** — feed it from a DI, preamp, or an interface's line out. Do not plug a mandolin pickup straight in and conclude the codec is broken. See docs/reference/daisy-pod.md.

**Then post a test report as a comment on <https://github.com/daisy-embassy/daisy-embassy/pull/80>.** The PR is unreviewed and its author is looking for confirmation; an independent report on identical hardware is the highest-value, lowest-effort contribution available to this project. This satisfies the 'contribute upstream' stretch goal without writing a driver.

If passthrough does *not* work, we own `src/codec/tac5242.rs` in the pinned fork and can debug it. The codec is hardware-strapped, so there is no I²C register map to reverse — the problem would be in SAI configuration. The expected SAI settings are recorded in docs/reference/daisy-seed3.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 All subtasks complete
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Closed 2026-08-05 on the owner's authorization. Both subtasks are Done: TASK-005.01 implemented passthrough against the seed3 feature, and TASK-005.02 verified it on real hardware (audio audible at the output, L/R not swapped, line-out to line-in loopback repeatable across resets, reliable boot/reset with USB reconnecting). This is the milestone that de-risks the project, and it is met — audio goes in and out of the Seed3 in the Pod.

Verified running configuration: 48 kHz, confirmed via AudioConfig::default() == Fs::Fs48000. The SAI setup is entirely daisy-embassy's (main.rs calls prepare_interface(Default::default())) and matches docs/reference/daisy-seed3.md exactly at pinned commit ca9bcc9 — see TASK-005.02's notes for the item-by-item check.

Deviation from this ticket's stated target: block length is 32 samples, not the 48 described above. daisy-embassy hardcodes BLOCK_LENGTH = 32 (audio.rs:13). Already documented under TASK-011; not adjustable without patching the pinned fork.

NOT DONE — the upstream report was never posted. This ticket's description asks for a test report as a comment on https://github.com/daisy-embassy/daisy-embassy/pull/80, describing it as the highest-value, lowest-effort contribution available to this project, and that is the "contribute upstream" stretch goal. The ticket was closed against its sole acceptance criterion ("All subtasks complete"), which the subtasks satisfy, so the report fell outside what any AC gated. It remains outstanding and untracked as of this closure. The material needed to write it is recorded here and in TASK-005.02.
<!-- SECTION:NOTES:END -->
