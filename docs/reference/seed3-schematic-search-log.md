# Seed3 schematic search: what is public, and what isn't

The evidence trail behind the nRESET answer in
[daisy-seed3.md](./daisy-seed3.md#flashing-and-logging-over-an-st-link-probe), kept so the negative
results don't have to be rediscovered. Assembled on 2026-09-10 while working TASK-050. Every URL below
was fetched (curl HEAD/GET, or in-memory download plus text extraction); nothing here is inferred from
vendor marketing. Section 3 lists the drawings that do exist; the reset net itself is traced in
daisy-seed3.md, using vector geometry and pixel-level reads of those sheets rather than text extraction
alone.

## Summary

**There is no publicly available Seed3 circuit schematic.** Electrosmith publishes only a databrief,
pinout and compliance files for Seed3 — no PDF schematic, no KiCad/Eagle source, no block diagram with
net names. Consequently **no public document shows the Seed3 nRESET/NRST net at all**, so what sits on
this board's reset line can only be inferred from its predecessors, not read off a drawing. That
inference is now as far as documents allow it to go, and it comes out benign: every public
Seed-family sheet that does show the net draws a 10 KΩ pull-up, one tactile button to GND and the debug
header's pin 10, with no supervisor and no buffer anywhere. Closing the last step needs a written answer
from Electrosmith (TASK-051) or a scope. The nearest public schematics are the *reduced* Seed Rev7 sheet
(2024-02-01) and the *full* Seed Rev4 sheet (2020-06-08, recovered from a GitHub fork of the deleted
`electro-smith/Hardware` repo); Rev4 does show an `NRST` net, and neither shows any discrete reset-supervisor IC.

## Findings

### 1. Seed3 official documentation set contains no schematic (verified live and archived)

- Product page body_html links to exactly one doc URL: `https://daisy.audio/hardware/Seed3/`, which now
  404s (`404 text/html`). Statuses verified: `https://daisy.audio/products/seed3.json` → 200 JSON;
  `https://docs.daisy.audio/hardware/Seed3/` → 200 HTML.
- `https://docs.daisy.audio/hardware/Seed3/` lists these assets and nothing else resembling a schematic:

  | Asset | URL | Verified status |
  | --- | --- | --- |
  | Databrief | `.../products/seed3/Daisy_Seed3_datasheet.pdf` | 200, `application/pdf`, 7,058,096 B, last-modified 2026-08-28 |
  | Pinout | `.../products/seed3/Seed3-Pinout.pdf` | 200, pdf, 693,668 B, 2026-03-12 |
  | Pinout CSV | `.../products/seed3/Seed3_pinout.csv` | 200 |
  | 3D models | `.../products/seed3/Seed3-models.zip` | 200 |
  | Compliance | `.../products/seed3/Compliance.zip` | 200 (contents: FCC test report, FCC cert, RoHS, REACH — no design files) |
  base = `https://daisy.nyc3.cdn.digitaloceanspaces.com`.

- Wayback proves this is not a recent removal. CDX for the whole bucket domain returns only these
  seed3 keys ever archived: `Daisy_Seed3_datasheet.pdf`, `seed3-pinout-dark.png`, `seed3-pinout-light.png`,
  `seed3_2x1.png`, `spin-gif.gif`. The archived doc page
  `http://web.archive.org/web/20260819205157/https://docs.daisy.audio/hardware/Seed3/` (200, fetched)
  lists the identical asset set — no schematic link in the past either.
- ~25 plausible schematic key names probed against the bucket (`ES_Daisy_Seed3.pdf`, `Seed3_schematic.pdf`,
  `products/seed3/ES_Daisy_Seed3_schematic.pdf`, `Seed3.sch`, `products/seed/ES_Daisy_Seed_Rev8.pdf`, …):
  all `403 application/xml` (that bucket's "key absent" response), while known-good keys return 200.

### 2. The Seed3 databrief itself carries zero reset information (I extracted its text)

Method: downloaded in memory, inflated every stream, pulled literal strings, joined without spaces to
defeat kerning-split glyph runs. Calibration: `TAC5242` is found exactly once, `STM32H750` present, so
extraction works. Counts in `Daisy_Seed3_datasheet.pdf`:
`RESET` 0, `NRST` 0, `reset` 0, `schematic` 0, `supervisor` 0, `button` 12 (audio-input/button examples).
So the 38-page databrief has no reset net, no supervisor part, no reset button description. Same scan on
the current Seed (rev7) databrief `products/seed/Daisy_Seed_datasheet.pdf` (2,516,631 B, created 2026-03-18):
`RESET`/`NRST` also absent (it does contain PCM3060 external-codec example circuits and revision-pin table).

### 3. Public schematics that DO exist (with visible revision/date from their own title blocks)

All are EAGLE exports; I decoded their embedded CID→Unicode CMaps and read the actual label text.

| Board | URL | Status | What I saw in the file |
| --- | --- | --- | --- |
| Seed Rev7 (**reduced**) | `https://daisy.nyc3.cdn.digitaloceanspaces.com/products/seed/ES_Daisy_Seed_Rev7.pdf` | 200, pdf, 26,085 B, last-mod 2024-02-01 | Title block `ES_Daisy_Seed_Rev7-Reduced`, `2/1/2024 10:20 AM`, `Sheet: 1/1`, creator `EAGLE Version 9.6.2`. Nets incl. `RESET`, `BOOT_FLASH`, `3V3A_IN`, `SEED V1.2 VERSION PIN`, `VERSION PIN`; parts `PTS815` (tactile switch), `RED LED R0603`, `2.2UH SRP2010-2R2M`. **No `NRST` net, no supervisor IC, no MCU part number printed.** |
| Seed Rev4 (**full**, 4 sheets) | `https://raw.githubusercontent.com/jull-taragan/Hardware/master/reference/daisy_seed/ES_Daisy_Seed_Rev4.pdf` | 200, 97,215 B | Title block `ES_Daisy_Seed_Rev4`, `6/8/2020 10:47 AM`, `1/4`…`4/4`. MCU sheet exposes **both `NRST` and `RESET`**, plus `PDR_ON`, `BOOT0`, `BOOT_FLASH`, `VBAT`, `VCAP_1`, `VCAP2`, `VDDUSB`, crystal `ABM8-16`, tactile switch `PTS815`, diode `NSR1020MW2T3G`, SDRAM `U2*`, flash `IS25LP 8MB`, codec pins `AK4556_PDN`. Annotated notes verbatim: "PDR_ON pin is new on BGA packages / On QFP-100, it is tied internally to Vdd / On BGA a lower voltage Vdd is possible because of this pin by disabling the power-down-reset supervisor. We don't need that feature, though. So we'll leave it tied to 3v3", and the BOOT_FLASH DFU trick (set PG3 high, software reset, etc.). **No discrete reset-supervisor reference designator or part number appears anywhere in the extracted label set.** |
| Patch Initial | `https://daisy.nyc3.cdn.digitaloceanspaces.com/products/patch-init/patch_init_schematic.pdf` | 200 pdf (note: underscore; the hyphenated `patch-init_schematic.pdf` is 403) | not opened beyond status |
| Pod Rev5 | `https://daisy.nyc3.cdn.digitaloceanspaces.com/products/pod/ES_Daisy_Pod_Rev5.pdf` | 200 pdf | carrier-level, not Seed internals |
| Patch SM | `https://daisy.nyc3.cdn.digitaloceanspaces.com/products/patch-sm/ES_Daisy_Patch_SM_Schematic.pdf` | 200 pdf | submodule, not Seed3 |
| Seed2 DFM | `.../products/seed2-dfm/Daisy_Seed2_DFM.pdf` (200, 2,781,697 B, Adobe-produced databrief, created 2026-02-20) and `.../ES_Daisy_Seed2_DFM_Rev5-REDUCED.pdf` (200) | reduced/databrief only; my text scan found no `RESET`/`NRST` strings | — |
| Patch / Field | `.../products/patch/ES_Daisy_Patch_Rev8.pdf`, `.../products/field/daisy_field_schematic_rev4.pdf` | listed on docs pages | carriers |

### 4. Dead / negative leads, so nobody re-runs them

- **No public hardware repo.** `https://github.com/electro-smith/Hardware` → 404; `https://github.com/electro-smith/DaisyKiCad` → 404; org listing (2 pages) has no Hardware/KiCad/Schematics repo; GitHub repo search for `Hardware|Schematics|KiCad in:name` scoped to electro-smith → `total_count: 0`.
- **The published KiCad library has no schematics.** `https://daisy.nyc3.cdn.digitaloceanspaces.com/libraries/DaisyKiCad-main.zip` (200, 33,402 B) unzips to exactly `Daisy-Boards.kicad_sym` plus six `.kicad_mod` footprints (`DAISY_SEED`, `DAISY_SEED_SMT`, `DAISY_SEED2_DFM*`, `DAISY_PATCH_SM*`) and LICENSE/README. Symbols + footprints only — no `.kicad_sch`.
- **Old repo contents in Wayback are HTML only.** CDX shows `github.com/electro-smith/Hardware/blob/master/doc/daisy_seed/Daisy_Seed_Rev5_sch.pdf` archived 20221031021054 as `text/html` (blob page), and `raw.githubusercontent.com/electro-smith/Hardware/...` snapshots are 404 captures. The Rev4/Rev5 PDF bytes were never archived there.
- **Forum moved, and holds no Seed3 schematic.** Current Discourse is `https://community.daisy.audio` (`/search.json` → 200 JSON); `forum.daisy.audio` → 301, `discourse.daisy.audio` → no DNS. Searches: `seed3 schematic` → 2 topics (9518 noise, 9539 power filtering), both about the *user's* circuit; `Seed3 reset` → unrelated; TAC5242 → announcement topic 9440. I read `/raw/9440`, `/raw/9518`, `/raw/9539`: no staff attachment, no schematic link, no promise of one. Topic 4961 ("Where did the github hardware repo go?") names the surviving fork `https://github.com/jull-taragan/Hardware` (200) whose tree holds `reference/daisy_seed/ES_Daisy_Seed_Rev4.pdf` and nothing newer for Seed.
- **GitHub issue search**: `"Seed3" "schematic"` across all repos → 1 hit, unrelated (`skngh/How-to-Make-a-Guitar-Pedal#1`). No open issue/PR in libDaisy/DaisyBootloader/DaisyDuino/oopsy/DaisyToolchain asks for a Seed3 schematic.
- **Distributors mirror nothing**: only reseller product pages exist (`https://foundsound.com.au/products/43206`, shop.app mirror); no design files.
- **FCC filings carry no schematic.** `Seed3_FCC_Cert.pdf` (495,921 B) and `Seed3_FCC_Test report.pdf` (2,512,846 B): my string scan found no "schematic", "circuit description", "block diagram" text; the test report embeds 14 JPEG streams (photos), i.e. a visual route at best, not a netlist.

### 5. nRESET specifically — what can and cannot be claimed

- **Verified absent from every public Seed3 document**: no schematic exists, and the databrief contains no reset text at all (counts above). I could not open a Seed3 schematic because there is none to open.
- **Verified present in public *earlier* schematics**: Seed Rev4 full sheet has nets `NRST` and `RESET` and a `PTS815` tactile switch; Seed Rev7 reduced sheet has `RESET` only (it is explicitly a reduced drawing — staff have said the public Seed schematic is intentionally incomplete).
- **Connectivity, once the sheets were opened properly**: text extraction recovers label strings, not wires, so it cannot say what sits on `NRST`. Rendering the sheets as vectors and reading them at high zoom does: on Rev4 (2020-06-08), Rev7 (2024-02-01), Seed2 DFM Rev5-reduced (2025-02-17) and Patch SM (2024-02-08) the `RESET` net carries a 10 KΩ pull-up to `+3V3_D`, one tactile switch to GND, the MCU's `NRST` pin and the 10-pin mini-JTAG header's pin 10 — and Rev7 alone adds a 100 nF capacitor to GND. Values, designators and the full trace are in [daisy-seed3.md](./daisy-seed3.md#flashing-and-logging-over-an-st-link-probe).
- **No supervisor or buffer part number observed** in any of those four drawings, by label scan *or* by looking at the drawn net. Still absence of evidence rather than proof about the Seed3: three of the four are explicitly reduced sheets, Seed3's own sheet is not public, and a supervisor could in principle hide as an unlabelled footprint there.

## Sources

Kept:

- Seed3 docs page — <https://docs.daisy.audio/hardware/Seed3/> — authoritative asset list (no schematic).
- Seed3 databrief — <https://daisy.nyc3.cdn.digitaloceanspaces.com/products/seed3/Daisy_Seed3_datasheet.pdf> — text-scanned for reset/schematic terms.
- Wayback capture of the same page — <http://web.archive.org/web/20260819205157/https://docs.daisy.audio/hardware/Seed3/> — proves the asset list never included a schematic.
- Seed Rev7 reduced schematic — <https://daisy.nyc3.cdn.digitaloceanspaces.com/products/seed/ES_Daisy_Seed_Rev7.pdf> — title block + net labels read directly.
- Seed Rev4 full schematic (fork of deleted upstream repo) — <https://raw.githubusercontent.com/jull-taragan/Hardware/master/reference/daisy_seed/ES_Daisy_Seed_Rev4.pdf> — only public full Seed schematic found; shows `NRST`.
- Fork pointer thread — <https://community.daisy.audio/t/where-did-the-github-hardware-repo-go/4961> — how the fork was located.
- Seed3 announcement — <https://community.daisy.audio/t/seed3-is-here/9440> — staff-stated changes, no schematic.
- KiCad library zip — <https://daisy.nyc3.cdn.digitaloceanspaces.com/libraries/DaisyKiCad-main.zip> — proves symbols-only.

Dropped:

- forum.electro-smith.com threads about the Rev7 schematic (moved host; content duplicated at community.daisy.audio, and predates Seed3).
- daisy.audio/pages/legacy PDF links (Eurorack submodules only, no Seed internals).
- SEO mirrors (`github.laiyagushi.com`, scribd datasheet copy) — third-party copies of primary docs.
- Reseller pages (foundsound, thonk, schneidersladen) — no design data.

## Gaps / next steps

1. **Ask Electrosmith directly** (community.daisy.audio, ideally replying in topic 9440 or a new Hardware topic): will the Seed3 schematic be published as the Rev7 one was ("Seed Rev7 Schematic Is Now Live!" precedent, topic 4846)? The narrow question left is only whether the Seed3 kept its predecessors' reset net — a 10 KΩ pull-up to `+3V3_D`, the front-panel button to GND, and (on Rev7) a 100 nF capacitor, with no supervisor or buffer anywhere.
2. **Bench route, now demoted to a tiebreaker**: an oscilloscope on header pin 10 during an under-reset attach settles the Seed3 question directly. It is worth doing only if `--connect-under-reset` actually misbehaves — TASK-037 passing on the first try is itself evidence, and a scope adds nothing to a pass.
3. ~~Open the two PDFs visually~~ — done, along with the Seed2 DFM and Patch SM sheets; see the trace in [daisy-seed3.md](./daisy-seed3.md#flashing-and-logging-over-an-st-link-probe). What product photography cannot do is settle the Seed3 question: at the resolution of the published images a SOT23-class supervisor near the reset button would be barely a package outline, let alone a readable marking, and the compliance zip holds FCC photographs rather than design data.
4. Re-check `https://docs.daisy.audio/hardware/Seed3/` periodically; a schematic would appear as a new `products/seed3/*.pdf` key, and the bucket's absent-key response (403) makes probing cheap.
