# Development workflow

How the repo is built, gated and run in CI. The gate set itself is defined in one place,
`scripts/gates.sh`; this document explains the shape and the reasons, and records nothing that
the script can print (`scripts/gates.sh --list` shows every gate, the tier that first runs it and
the banner it prints). Timings are not recorded here either: each run prints a time per gate and
per tier.

## Nix

A single flake provides everything: a Rust toolchain with `thumbv7em-none-eabihf`, `clippy`,
`rustfmt`, `rust-analyzer`, plus `dfu-util`, `probe-rs`, `cargo-binutils`, `lefthook`, and the
ALSA/pkg-config dependencies `cpal` needs. `direnv` is wired up (`.envrc` contains `use flake`).
The toolchain comes from `fenix` or `rust-overlay` because nixpkgs' `rustc` does not carry the
embedded target.

`flake.nix` defines two shells through `mkDevShell`: `default`, and `ci`, which is the same shell
without `probe-rs-tools`. That package builds from probe-rs main (it carries the
`--connect-under-reset` support, since merged upstream) and took almost the whole dev-shell build
time; no gate uses probe-rs, so CI enters `.#ci`.

## Two workspaces

The host crates and `firmware/` are separate Cargo workspaces (see doc-001). Every root-level
cargo command stops at that exclusion, so firmware has its own gates in `scripts/gates.sh`:
firmware fmt, cross clippy over all six bins in both cfg sets (console and RTT-only), and two
cross-compiles. Tests are the honest gap: a `no_std` target has no test harness to link, so
firmware correctness rests on the cross-compile, the lints and the rig. Residuals named rather than
glossed: firmware docs are ungated (`cargo doc` covers `crates/*` only), pedantic clippy lints are
unadopted, and the `pod-hw` test runs in CI alone.

## lefthook

Both hook stages run one command each: `bash scripts/gates.sh commit` and
`bash scripts/gates.sh push`. The gate set is defined once, in that script: every check is named
once and tagged with the cheapest tier that runs it, so neither the hooks nor CI can restate it and
drift. Restating it was the bug - `lefthook.yml` once held 25 hand-written `run:` lines and CI kept
a second copy of the same list, and every check added after that landed on one side only.

Nothing filters paths: no `root`, `glob`, `files` or `local` key survives in `lefthook.yml`, and no
job carries a staged-files template. A filtered job whose set is empty exits 0 without running, so
a lint behind one reads as green having checked nothing. Grep the file for those keys and count
zero; that is the durable check. One skip survives and it is benign: lefthook ignores a stage whose
staged-file set is empty, so `git commit --allow-empty` runs no gates and a manual
`lefthook run <stage>` on an empty index needs `-f`. An empty commit changes no tree, so the only
thing it could gate is HEAD's tree, already paid for by the commit that made it.

### Tiers

`commit`, `push` and `ci`, ascending. `ci` is the default when no tier is given, because forgetting
the argument must never mean "ran less than everything". Order within a tier is whatever the script
declares, cheapest-first, with one rule that outranks cost: the cross-clippy gates follow both
cross-builds so they reuse the artifacts.

Firmware build order otherwise does not matter. Every firmware cargo invocation passes a
`--target-dir` named by `scripts/variant-dir.sh` from the link layout, log mode and feature set
(`firmware/target/internal-default-seed3`, `firmware/target/internal-nodefault-log-defmt+seed3`, ...),
so no two variants write the same `release/<bin>`. The cost is disk space and a cold first build per
variant; the `make` ELF path is derived from the same knobs, so `make probe-*` cannot decode with
another variant's ELF.

That rule is not expressible in hook config - lefthook sorts a stage by priority, then leading
digits, then command name, never by declaration order - which is why the definition is a script
and not YAML.

### Selftest gates and artifact gates

Three selftest gates run ahead of every cargo invocation: `elf-provenance --selftest`,
`elf-staleness --selftest` and `load-addresses --selftest`. They compile nothing and read no
artifact, which is what cheapest-first does with a check that has no inputs to wait for. Each
selftest grades the reading (the parser, against hand-emitted fixtures); the push-tier gate with the
same subject grades the artifact (the six linked images). They share no state and neither makes the
other redundant. Their cases and cost rules are stated in the scripts' own headers.

- `=== image load addresses ===` (push tier, since pre-commit builds no firmware; it reads the RTT-only variant, the one that links all six binaries) asserts that no
  file-backed section loads outside the FLASH region of the linker script in use
  (`firmware/memory-internal.x` or `firmware/memory-bootloaded.x`), that `.sram1_bss` stays NOBITS
  while the script claims to place it `(NOLOAD)`, and that each plain `-O binary` image is exactly as
  long as the highest flash load-address end implies. The invariant exists because objcopy writes
  from the lowest to the highest load address: a section loading in AXI SRAM once made `main.bin`
  469,763,480 bytes of mostly zeros with every other gate green. It uses bare
  `rust-objdump` / `rust-objcopy` only.
- `scripts/check-doc-artifact-names.sh` keeps reader-facing documentation to image names the
  firmware Makefile really builds and keeps the objcopy recipe out of prose.

What the one-command shape costs: lefthook buffers a command's stdout and replays it when the
command finishes, so a hook prints nothing until its tier has finished. In exchange the log carries
per-gate headers and wall times, and the run stops at the first failure naming the gate that died.

## CI

GitHub Actions runs the same script at full tilt - `nix develop .#ci --command bash
scripts/gates.sh ci` - so the repo holds exactly one list and the runner executes it verbatim. The
workflow names no checks at all. (CI once kept its own copy inline in a single-quoted `bash -c`
string, a shape that cannot carry comments: an apostrophe closed the quote and the step died at end
of file, unnoticed for days because nothing had been pushed.)

Exactly one gate lives in the `ci` tier alone: `cargo test -p asperitas-pod --features pod-hw`. The
feature adds no tests and no other crate depends on `asperitas-pod`, so the gate runs only that
crate's suite; the compile-time half (`clippy --features asperitas-pod/pod-hw`) runs on push. CI
stays the authority either way, immune to `--no-verify`.

Runner behavior worth knowing, from observed runs on GitHub-hosted runners:

- The `rust-cache` action works for clippy and cross builds (clippy gates fell from 4-15 s to
  under 2 s) but not for the two `cargo test` runs, which stay near 100 s each.
- A dev shell whose stdout is polluted breaks rust-cache's `rustc -vV` probe;
  `scripts/check-devshell-stdout.sh` guards that.
- Entering `.#ci` instead of `.#default` took the dev-shell step from about 10 minutes to under one,
  and a warm job from about 16 minutes to about 5.
- The workflow has no `workflow_dispatch`, so runner-side figures can only be observed on push.
