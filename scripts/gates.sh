#!/usr/bin/env bash
#
# The gate set, defined once.
#
# Every check this repo enforces is named here exactly once, tagged with the cheapest tier that runs
# it. CI asks for `ci`; lefthook's pre-commit and pre-push ask for `commit` and `push`. Tiers are
# cumulative, and no argument means `ci`, so forgetting it never means "ran less than everything".
#
# A script rather than lefthook config because lefthook orders a stage by priority and name, not
# declaration order, and two orderings matter here: cheapest-first fail-fast, and cross-builds before
# cross-clippy so the lints reuse the build artifacts. Lefthook also buffers stdout, so a hook prints
# nothing until its tier ends; per-gate headers and times are what come back for that.
#
# Adding a check: add one `gate` line where it should run. A gate's key is its identity (rewording a
# banner does not rename it); keys match ^[a-z0-9][a-z0-9-]{0,39}$ and are unique, enforced at call time.
#
# With GATES_TIMINGS_FILE set, a run appends one tab-separated line per gate (`gate<TAB>key<TAB>seconds`)
# and one for the tier, so nothing has to parse the human `--- N.NNs` lines.
#
# Exit codes: 0 every gate passed, 1 a gate failed or the tier matched no gate, 2 bad usage,
# 3 this toolchain cannot build for thumbv7em at all.

set -euo pipefail

# bash 5 only: the per-gate timing reads EPOCHREALTIME, which bash 3.2 (the macOS system shell)
# leaves empty. Fail here with a sentence rather than arithmetic-erroring on the first gate.
if [[ -z "${EPOCHREALTIME:-}" ]]; then
  printf 'gates.sh: needs bash 5 for sub-second gate timings (found: %s)\n' "${BASH_VERSION:-unknown}" >&2
  exit 2
fi

usage() {
  cat <<'EOF'
usage: scripts/gates.sh [--dry-run | --list] [commit|push|ci]

  run       execute the tier, sequentially, cheapest-first, stopping at the first failure
  --dry-run print the commands the tier would run, executing nothing
  --list    print the tier x gate matrix (see docs/development.md)
  no tier argument means ci: forgetting it must never mean "ran less than everything"
EOF
}

MODE=run
TIER=ci
while [[ $# -gt 0 ]]; do
  case $1 in
    --dry-run) MODE=dry ;;
    --list)    MODE=list ;;
    -h|--help) usage; exit 0 ;;
    commit|push|ci) TIER=$1 ;;
    *) printf 'gates.sh: unknown argument: %s\n\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

# Anchoring at the repo root is load-bearing: run from firmware/, host-workspace commands such as
# `cargo fmt --all --check` silently check the wrong workspace and exit 0.
ROOT=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT" || exit 1

# vd <LAYOUT> <NO_DEFAULT> <FEATURES> [TAG] prints the `--target-dir` value for a firmware gate. The
# firmware gates run with `-C firmware`, so the path it prints (relative to firmware/) resolves there.
vd() { scripts/variant-dir.sh "$@"; }

case $TIER in
  commit) RANK=1 ;;
  push)   RANK=2 ;;
  ci)     RANK=3 ;;
esac

N_COMMIT=0 N_PUSH=0 N_CI=0 RAN=0

# gate <min-tier> <key> <banner> [-C dir] <command...>
# <min-tier> is the cheapest tier that runs the gate; declaration order is execution order.
# -C runs the command in a subshell in dir: cargo finds .cargo/config.toml from the CWD, and a bare
# `cd` would leave later host gates pointed at the firmware workspace.
KEY_RE='^[a-z0-9][a-z0-9-]{0,39}$'
declare -A SEEN_KEYS=()

gate() {
  local min=$1 key=$2 banner=$3 rank dir="" out a us t
  local called=${BASH_LINENO[0]}

  # Checked in every mode so a bad key fails even --list.
  if [[ ! $key =~ $KEY_RE ]]; then
    printf 'gates.sh: bad gate key "%s" at line %s\nkeys match ^[a-z0-9][a-z0-9-]{0,39}$.\n' "$key" "$called" >&2
    exit 2
  fi
  if [[ -n ${SEEN_KEYS[$key]+set} ]]; then
    printf 'gates.sh: duplicate gate key "%s": used at line %s and again at line %s\nTwo gates cannot share one key -- it is the gate identity.\n' \
      "$key" "${SEEN_KEYS[$key]}" "$called" >&2
    exit 2
  fi
  SEEN_KEYS[$key]=$called

  case $min in
    commit) rank=1; N_COMMIT=$(( N_COMMIT + 1 )); N_PUSH=$(( N_PUSH + 1 )); N_CI=$(( N_CI + 1 )) ;;
    push)   rank=2; N_PUSH=$(( N_PUSH + 1 ));    N_CI=$(( N_CI + 1 )) ;;
    ci)     rank=3; N_CI=$(( N_CI + 1 )) ;;
    *) printf 'gates.sh: bad min-tier: %s\n' "$min" >&2; exit 2 ;;
  esac
  shift 3
  if [[ $1 == "-C" ]]; then dir=$2; shift 2; fi

  out=$1
  for a in "${@:2}"; do
    if [[ $a == *[[:space:]]* ]]; then out+=" \"$a\""; else out+=" $a"; fi
  done
  [[ -z $dir ]] || out="cd $dir && $out"

  if [[ $MODE != run ]]; then
    if (( RANK >= rank )); then
      # Machine-read output: column order is part of the format (key, tier, rest).
      if [[ $MODE == list ]]; then
        printf '%-27s | %-6s | %-3s | %-3s | %-2s | %s\n' "$key" "$min" \
          "$([[ 1 -ge $rank ]] && echo yes || echo -)" \
          "$([[ 2 -ge $rank ]] && echo yes || echo -)" \
          "$([[ 3 -ge $rank ]] && echo yes || echo -)" "$banner"
      else
        printf '%-6s\t%-27s\t%s\n' "$min" "$key" "$out"
      fi
    fi
    return 0
  fi

  (( RANK >= rank )) || return 0
  RAN=$(( RAN + 1 ))
  printf '\n%s\n' "$banner"
  t=${EPOCHREALTIME/./}
  if ! ( cd "${dir:-$ROOT}" && exec "$@" ); then
    printf '\n*** gate failed: %s\n*** tier: %s (%d of %d gates completed before it)\n' \
      "$banner" "$TIER" $(( RAN - 1 )) "$RAN" >&2
    exit 1
  fi
  us=$(( ${EPOCHREALTIME/./} - t ))
  awk -v us="$us" 'BEGIN { printf "--- %.2fs\n", us / 1000000 }'
  if [[ -n ${GATES_TIMINGS_FILE:-} ]]; then
    awk -v us="$us" -v key="$key" 'BEGIN { printf "gate\t%s\t%.3f\n", key, us / 1000000 }' \
      >> "$GATES_TIMINGS_FILE"
  fi
}

if [[ $MODE == list ]]; then
  printf '%-27s | %-6s | %-3s | %-3s | %-2s | %s\n' key min com psh ci gate
  printf '%s\n' '----------------------------+--------+-----+-----+----+------------------------------------------------'
fi

# Without the embedded std every firmware gate dies as E0463, which reads like a code bug. Run mode
# only: --dry-run and --list must work anywhere.
if [[ $MODE == run ]]; then
  if [[ ! -d "$(rustc --print sysroot)/lib/rustlib/thumbv7em-none-eabihf" ]]; then
    printf 'gates.sh: this rustc has no thumbv7em-none-eabihf standard library, so every firmware\n' >&2
    printf 'gate below would fail with E0463 no matter what the code looks like.\n' >&2
    printf 'Enter the project shell first: nix develop .#default\n' >&2
    exit 3
  fi
  printf 'tier: %s\n' "$TIER"
  # Read after the sysroot probe so shell startup stays out of the tier total.
  TIER_START_US=${EPOCHREALTIME/./}
fi

# A syntax error in this file should be the cheapest possible failure. The path is repo-relative so
# --list and --dry-run output carries no machine-specific directory.
gate commit gate-definition-parses "=== gate definition parses ===" bash -n scripts/gates.sh

# Docs may only name images the build produces. Legal names come from `make -n` (a dry run, the only
# make invocation in the gate set); the script's header explains why -n is load-bearing.
gate commit docs-artifact-names "=== docs artifact names ===" scripts/check-doc-artifact-names.sh

# Reader of the provenance note in an ELF. Hand-emitted fixtures, no build; cheap, so it runs ahead of
# every cargo gate. Cases and cost rules are in scripts/elf-provenance.sh's header.
gate commit elf-provenance-selftest "=== elf-provenance --selftest ===" scripts/elf-provenance.sh --selftest

# The staleness check hashes sources against the digest embedded in the ELF; its cases drive the
# shipped Makefile recipe through command-line overrides. No cargo, fixtures in a temp dir.
gate commit elf-staleness-selftest "=== elf-staleness --selftest ===" scripts/check-elf-staleness.sh --selftest

# The load-address checker reads objdump columns by position, and a parser that stops matching turns
# the push-tier gate green for the wrong reason. This runs the parser against fixtures and literal
# objdump rows; the push-tier gate over real ELFs stays the backstop. Rules in the script's selftest header.
gate commit load-addresses-selftest "=== load-addresses --selftest ===" scripts/check-image-load-addresses.sh --selftest

gate commit cargo-fmt "=== cargo fmt ===" cargo fmt --all --check

# firmware/ is its own workspace, so the gate above does not cover it. `--check` is required: a bare
# `cargo fmt` would rewrite ELF inputs and send `make elf-check` red for a change nobody made.
gate commit cargo-fmt-firmware "=== cargo fmt (firmware workspace) ===" \
  cargo fmt --manifest-path firmware/Cargo.toml --all --check

gate commit cargo-clippy "=== cargo clippy ===" cargo clippy --workspace --all-targets -- -D warnings

# Default features build asperitas-logging without `log-usb`; only this gate lints those paths.
gate commit cargo-clippy-log-usb "=== cargo clippy (asperitas-logging log-usb) ===" \
  cargo clippy -p asperitas-logging --features log-usb --lib -- -D warnings

# Same blind spot for `log-defmt`, the bridge behind the probe's lossless log channel.
gate commit cargo-clippy-log-defmt "=== cargo clippy (asperitas-logging log-defmt) ===" \
  cargo clippy -p asperitas-logging --features log-defmt --lib -- -D warnings

# Compiles the hardware-backed Pod paths no default-feature build sees. The runtime half is the
# ci-only `cargo-test-pod-hw` gate.
gate push clippy-pod-hw "=== cargo clippy (asperitas-pod pod-hw feature) ===" \
  cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings

# The capture decoder's verdict on synthetic captures, through the shipped decode path. No board.
gate push dump-reassemble-selftest "=== dump_reassemble --selftest ===" \
  cargo run -p asperitas-host --bin dump_reassemble -- --selftest

# Rustdoc warnings are errors. Two runs because neither feature set is a superset of the other
# (default documents logging's `init()`, --all-features documents usb/led/panic_handler); do not merge them.
gate push cargo-doc "=== cargo doc (workspace) ===" \
  env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# asperitas-measure is excluded: --all-features unifies serde's `std` with logging's `log-usb` ->
# embassy-usb -> ssmarshal, which stops compiling. The crate has no features, so its default build
# (`cargo-doc` above) already covers it.
gate push cargo-doc-all-features "=== cargo doc (workspace, all features) ===" \
  env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude asperitas-measure --no-deps --all-features

# Every firmware variant is a row of scripts/firmware-variants.table; these gates run the driver over
# it. This check is source-level and builds nothing: it fails when a src/bin file has no row.
gate commit firmware-variants-coverage "=== firmware variants cover every src/bin ===" \
  scripts/firmware-variants.sh check

# Each variant builds into its own target directory, so bench `make probe-*` decodes the ELF of the
# variant it names.
gate push firmware-builds "=== firmware builds (every variant) ===" scripts/firmware-variants.sh build

# No file-backed section may load outside the layout's region, `.sram1_bss` stays NOBITS, and each
# plain `-O binary` image has exactly the length its flash LMA range implies. Reads the artifacts the
# build gate wrote with bare rust-objdump/rust-objcopy (the cargo forms rebuild); a missing ELF is a
# failure, never a skip. Push tier because pre-commit builds no firmware.
gate push image-load-addresses "=== image load addresses (every variant) ===" scripts/firmware-variants.sh grade

# After the builds so clippy reuses their dependency artifacts. --bins because the package has no lib
# target and --all-targets cannot link on a no_std target.
gate commit firmware-clippy "=== firmware clippy (every variant) ===" scripts/firmware-variants.sh clippy

gate push cargo-test "=== cargo test ===" cargo test --workspace

# Runtime coverage of the pod-hw feature; compile coverage is `clippy-pod-hw` in the push tier.
gate ci cargo-test-pod-hw "=== cargo test (asperitas-pod pod-hw feature) ===" cargo test -p asperitas-pod --features pod-hw

if [[ $MODE == list ]]; then
  printf '\ncounts: commit %d, push %d, ci %d\n' "$N_COMMIT" "$N_PUSH" "$N_CI"
  exit 0
elif [[ $MODE == dry ]]; then
  exit 0
fi

if (( RAN == 0 )); then
  printf 'gates.sh: tier "%s" matched no gate. That is a bug in the tier table, not a pass.\n' "$TIER" >&2
  exit 1
fi

TIER_US=$(( ${EPOCHREALTIME/./} - TIER_START_US ))

if [[ -n ${GATES_TIMINGS_FILE:-} ]]; then
  awk -v us="$TIER_US" -v tier="$TIER" 'BEGIN { printf "tier\t%s\t%.3f\n", tier, us / 1000000 }' \
    >> "$GATES_TIMINGS_FILE"
fi

# Formatted from EPOCHREALTIME microseconds; bash's integer SECONDS would truncate.
printf '\ntier %s: %d gates, %.1fs\n' "$TIER" "$RAN" \
  "$(awk -v us="$TIER_US" 'BEGIN { printf "%.1f", us / 1000000 }')"
