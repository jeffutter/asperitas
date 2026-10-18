# Wire protocol index

The framed USB console and everything that rides on it is specified in rustdoc, next to the code
that encodes and parses it. Each grammar has exactly one owner; this page only points at them.

Build the docs with `cargo doc -p asperitas-wire --no-deps --open` (add `--document-private-items`
to see the field-offset constants).

| Grammar | Owner |
|---|---|
| Record framing, fields, CRC-16/CCITT-FALSE parameters, sanitisation, resynchronisation, the USB short-packet rule for reader authors | `asperitas_wire::frame` |
| `BOOT` and `STATUS` bodies, counter semantics, and the rig records `RIGCFG`, `RIGGEN`, `CAPMAX`, `CAPSTAT`, `STIMSTART`, `DUMPEND` | `asperitas_wire::console` |
| `AUDIO` and `AUDEND` (the base64 audio dump) | `asperitas_wire::dump` |
| Capture ring geometry and the derived capture budgets | `asperitas_wire::capture` |
| Excerpt flash layout, slot header, `EXCSTART`/`EXCDATA`/`EXCEND`, `EXCOK`/`EXCFAIL`/`EXCPLAY`/`EXCPLAYFAIL`, flash rules, replay CRC posture | `asperitas_wire::excerpt` |
| Host control verbs `CTL ...` and replies `CTLR ...` | `asperitas_wire::control` |
| Firmware wiring of the control channel (consumer task, override merge, version, reset reason, restart, panic path) | `firmware/src/host_control.rs` module docs |
| Device-side console runtime (ring, drain task, ZLP handling, panic path) | `asperitas-logging` |

Measured hardware figures (SDRAM timings, QSPI install costs, link layouts) stay in
[daisy-seed3.md](./daisy-seed3.md). The host capture runner is [asperitas-rig.md](./asperitas-rig.md).
