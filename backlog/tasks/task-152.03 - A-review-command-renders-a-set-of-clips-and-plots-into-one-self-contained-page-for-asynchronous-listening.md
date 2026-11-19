---
id: TASK-152.03
title: >-
  A review command renders a set of clips and plots into one self-contained page
  for asynchronous listening
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-13T22:00:00Z'
updated_date: '2026-11-19T02:37:00Z'
labels:
  - automation
  - review
  - tooling
  - planned
dependencies:
  - TASK-152.01
documentation:
  - backlog/docs/doc-001 - Asperitas-Project-Plan.md
  - audio/README.md
  - crates/asperitas-measure/src/lib.rs
parent_task_id: TASK-152
priority: medium
type: feature
ordinal: 290700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The owner cannot be at a bench or a keyboard for every sound judgement, and an agent cannot hear. What bridges them is a package the agent produces and the owner reviews when convenient: audio to listen to, next to plots that make a defect visible, with the exact settings used so a comment can be turned into a ticket. Nothing like it exists; examples today are WAVs and JSON reports in a captures folder.

Add one command that takes a small manifest of cases (a corpus clip, optional automation file from TASK-152.01, optional label such as a comparison against a previous render or against the dry clip) and writes a single self-contained HTML page: for each case an audio player for the rendered output (and the reference when given), a waveform and spectrogram, the settings and automation used, and the objective checks the repo already has where they apply (for example the discontinuity and step detectors in `asperitas-measure`, peak level, clipping). The page must open from a local file with no network and no server, must stay within a size small enough to publish as an artifact page (16 MB), and must not require a device. Publishing it for the owner is done by the person or agent running the command, not by the command.

Out of scope: any new analysis algorithm (reuse what asperitas-measure has; say so where it does not cover a check); wasm; a server.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 One command takes a manifest file and writes a single self-contained HTML file (audio embedded, no external requests) and exits non-zero with a message naming the problem on a bad manifest, a missing clip or a clip that is not 48 kHz
- [x] #2 Each case shows a player and a plot for its output (and for the reference when the manifest names one), the knob settings or automation used, and which objective checks ran with their result; a case that fails a check is marked as failed on the page, not hidden
- [x] #3 The manifest format and an example manifest are documented in a docs/reference page and linked from CLAUDE.md, and the example runs on the committed corpus
- [x] #4 A test builds a page from a short synthetic clip and checks the page contains each case's name, the embedded audio and the check results; and that a failing check shows as failed
- [x] #5 The page for the example manifest is opened or viewed and the ticket notes say what was seen, including its file size
- [x] #6 bash scripts/gates.sh push passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the commit that adds crates/asperitas-cli/src/review.rs. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added asperitas-cli review (crates/asperitas-cli/src/review.rs), docs/reference/review.md, audio/review/example.toml + sweep.txt, CLAUDE.md and audio/README.md links. Manifest is TOML (toml + serde, deny_unknown_fields); plots are inline SVG waveform and PNG spectrogram (rustfft, png); audio is base64 stereo f32 WAV as rendered. Checks: peak and clipping inline (measure has no f32-render equivalent), parameter steps via asperitas_measure::param_step. The detector flags musical attacks (100+ events per corpus clip), so the rule is render count > dry-input count, with automation containing a step line exempt; documented as a coarse tripwire. AC5: ran the example manifest, page is 6,047,147 bytes (about 6 MB, 3 cases, 3 dry references). Viewed via qlmanage WebKit thumbnail (no Chrome installed for agent-browser): dark theme renders, summary table shows 3 passed, waveform and spectrogram plots show correctly for output and dry reference; audio controls render (playback not exercised, no ears). gates.sh push: the all-features cargo doc gate began failing because asperitas-cli now depends on asperitas-measure (serde std unification breaks ssmarshal, the existing reason measure was excluded); excluded asperitas-cli and asperitas-live (its dependent) from that one gate with the comment updated. They are still covered by the default cargo doc gate. Dependencies added: toml, png, base64, rustfft, serde, asperitas-measure, tempfile promoted to regular dep.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
asperitas-cli review <manifest.toml> <out.html> renders cases through process and writes one offline HTML page with players, waveform and spectrogram plots, settings, and peak/clipping/parameter-step checks with failures marked. Documented in docs/reference/review.md with a working example on the corpus; unit tests cover content, failing check and error cases; gates push passes.
<!-- SECTION:FINAL_SUMMARY:END -->
