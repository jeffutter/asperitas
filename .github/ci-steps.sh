#!/usr/bin/env bash
#
# The CI check list, as one shell script.
#
# This used to be an inline `nix develop .#default --command bash -c '...'` string inside
# .github/workflows/ci.yml. That shape cannot hold comments: the whole list is one single-quoted
# argument, so an apostrophe anywhere in a comment closes the quote and the outer shell re-parses
# the rest of the file. bash executes a script command by command as it reads it, so the damage
# is invisible until the end -- every check appears to run, then the step dies with "unexpected
# EOF while looking for matching `''". Six comments had drifted into that state, and the class
# went unnoticed because the last CI run predates the first of them by a day.
#
# In a real file the comments are just comments, `bash -n` covers it (lefthook runs that on every
# commit), and the list diffs on its own. Keep it that way: if you need another check, add it
# here, not back in the workflow YAML.
#
# Costs in comments are LOCAL warm figures, aarch64-darwin, measured inside nix develop
# .#default. Observed CI timings belong to TASK-052.

set -euo pipefail

echo "=== cargo fmt ==="
cargo fmt --all --check

# Root Cargo.toml:3 declares exclude = ["firmware"], so the check above never meant all:
# firmware/ is its own workspace and has drifted under this gate twice (TASK-044, TASK-060).
# Same command as lefthook's fmt-check-firmware. ~0.4 s warm, local figure.
echo "=== cargo fmt (firmware workspace) ==="
cargo fmt --manifest-path firmware/Cargo.toml --all --check

echo "=== cargo clippy ==="
cargo clippy --workspace --all-targets -- -D warnings

# Default features build the crate without `log-usb`, so every record-path function in
# asperitas-logging goes unlinted above. This is the only gate that sees them.
echo "=== cargo clippy (asperitas-logging log-usb) ==="
cargo clippy -p asperitas-logging --features log-usb --lib -- -D warnings

# The other transport has the same blind spot: the default feature set excludes `log-defmt`,
# so defmt_log.rs -- the bridge behind the probe's lossless log channel -- is compiled by no
# other check here.
echo "=== cargo clippy (asperitas-logging log-defmt) ==="
cargo clippy -p asperitas-logging --features log-defmt --lib -- -D warnings

echo "=== cargo clippy (asperitas-pod pod-hw feature) ==="
cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings

# Rustdoc cross-references. Each member's `[lints] workspace = true` already denies the two
# intra-doc-link lints via the root table; RUSTDOCFLAGS raises that to every rustdoc warning.
# Two runs because neither feature set is a superset: default features document logging's
# fn.init() but not usb.rs / led.rs / panic_handler.rs, while --all-features gains those (and
# pod's led / pins) and drops fn.init().
echo "=== cargo doc (workspace) ==="
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

echo "=== cargo doc (workspace, all features) ==="
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

echo "=== cargo test ==="
cargo test --workspace

echo "=== cargo test (asperitas-pod pod-hw feature) ==="
cargo test --workspace --features asperitas-pod/pod-hw

# Exit code is the verdict: 0 means every block the synthetic captures promised, they
# proved. Runs the shipped read/decode/assemble path, so no board is attached.
echo "=== dump_reassemble --selftest ==="
cargo run -p asperitas-logging --example dump_reassemble -- --selftest

# Docs name firmware image files. Legal names come from the build itself via `make -n`,
# so this is the only place in CI that invokes make -- and it stays a dry run, never a
# cross-compile. Landed with TASK-058.
echo "=== docs artifact names ==="
scripts/check-doc-artifact-names.sh

# One subshell for everything that must run with cwd firmware/, where .cargo/config.toml
# supplies the thumbv7em target and link args (cargo #9670: discovered from the CWD, not
# from --manifest-path). Grouping beats inheriting a cd from the line above.
(
  echo "=== firmware cross-compile ==="
  cd firmware && cargo build --release --features seed3

  # The RTT-only image is the only build that links build.rs's `-Tdefmt.x` fragment and the
  # `#[cfg(not(feature = "log-defmt"))]` logger stubs in the same binary; the console build
  # above leaves both uncompiled. DEFMT_LOG is deliberately unset: it selects which frames get
  # compiled in, not whether this compiles.
  echo "=== firmware cross-compile (RTT-only, log-defmt) ==="
  cargo build --release --no-default-features --features "seed3 log-defmt"

  # Lint the same two cfg sets the builds above just compiled. Placed after them so clippy
  # reuses their artifacts -- local: 20 s in a fresh target dir, 11 s right after a cold
  # build, ~2 s warm. --bins is the whole package here (no lib target, six entries under
  # src/bin/), and --all-targets is unusable on a no_std target with no test harness to link.
  echo "=== firmware clippy (all bins) ==="
  cargo clippy --release --features seed3 --bins -- -D warnings

  echo "=== firmware clippy (all bins, RTT-only, log-defmt) ==="
  cargo clippy --release --no-default-features --features "seed3 log-defmt" --bins -- -D warnings
)
