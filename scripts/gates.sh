#!/usr/bin/env bash
#
# The gate set, defined once.
#
# Every fmt / lint / doc / test / cross-build check this repo enforces is named here exactly once,
# tagged with the cheapest tier that runs it. Three callers each ask for one tier and add nothing:
# .github/workflows/ci.yml asks for `ci`, and lefthook's pre-commit and pre-push ask for `commit`
# and `push`. Before this file the same list existed twice, by hand -- 25 `run:` lines in
# lefthook.yml plus a second copy in CI's step -- and every check added since TASK-018 drifted
# between them. TASK-060 itself exists because firmware was invisible to both copies, and four
# closed tickets declined to notice.
#
# Why a script and not "CI just runs the hooks": lefthook sorts the commands in a stage by
# priority, then leading digits in the name, then name ascending -- NOT by declaration order -- so
# two decisions this repo has measured money behind are simply unexpressible in hook config:
# cheapest-gate-first fail-fast, and cross-build-before-cross-clippy so the lints reuse the
# artifacts the builds just produced. A sequential script can say both; YAML cannot. It also buys
# deterministic ordering, per-gate timings, and fail-fast that alphabetical names cannot give.
# What it gives up: lefthook captures a command's stdout and replays it when the command finishes,
# so a hook now prints nothing until its tier ends (~4 s commit, ~77 s push). Read the per-gate
# headers and times below as the trade.
#
# Tiers are cumulative: `commit` is a subset of `push`, `push` a subset of `ci`. No argument means
# `ci` -- forgetting the argument must never mean "ran less than everything".
#
# Two ordering rules outrank cheapest-first, and both are load-bearing:
#
#   1. The console cross-build comes immediately before the RTT-only cross-build, in that order,
#      and no gate builds firmware after them. Whichever build ran last is what
#      firmware/target/thumbv7em-none-eabihf/release/main names -- the two images are hardlinks to
#      different deps/main-<hash> artifacts -- and every `make probe-*` decodes whatever that path
#      currently holds. That residue is no longer folklore: `=== firmware ELF cfg provenance ===`
#      runs after the pair and refuses any ELF that is not the RTT-only image, so swapping the two
#      builds or adding a gate that compiles firmware after them turns that gate red naming both cfg
#      sets (TASK-062). Do not "fix" that blindness anywhere else and do not collapse the pair.
#   2. The two cross-clippy gates come after both cross-builds so clippy reuses their artifacts.
#      They stay `commit`-tier gates, so in the commit tier they simply run with no build above
#      them, which is what pre-commit does today.
#
# Cost figures in the comments are LOCAL warm numbers, aarch64-darwin, measured inside
# `nix develop .#default` on a clean tree. One figure per gate even where two call sites used to
# quote different ones. Runner-side figures belong to TASK-052 and TASK-063: main sits ~91 commits
# ahead of origin/main and ci.yml has no workflow_dispatch, so no agent can observe a runner.
#
# Adding a check: add one `gate` line in the position you want it to run, with the tier that should
# start running it. Nothing else in the repo names checks.
#
# Exit codes: 0 every gate passed, 1 a gate failed or the tier matched no gate at all, 2 bad usage,
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
  --list    print the tier x gate matrix that doc-001 embeds
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

# Anchoring is load-bearing, not hygiene. Run from firmware/ an unanchored list produces
# wrong-workspace passes that exit 0: `cargo fmt --all --check` checks the firmware workspace
# instead of the host one, `cargo clippy --workspace` means the firmware crates, and
# `-p asperitas-logging` errors out. Same shape as scripts/check-doc-artifact-names.sh:37-38.
ROOT=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT" || exit 1

case $TIER in
  commit) RANK=1 ;;
  push)   RANK=2 ;;
  ci)     RANK=3 ;;
esac

N_COMMIT=0 N_PUSH=0 N_CI=0 RAN=0

# gate <min-tier> <banner> [-C dir] <command...>
#
# One check, one line. <min-tier> is the cheapest tier that runs it; every higher tier inherits it,
# which is what makes the three call sites one list. Declaration order IS execution order, so the
# cheap stuff goes first and the ordering rules in the header are visible as file positions.
#
# -C dir runs the command in a subshell whose cwd is dir. Firmware gates need it: .cargo/config.toml
# supplies the thumbv7em target and link args and cargo discovers that file from the CWD, not from
# --manifest-path (cargo #9670). A bare `cd` in the parent would leave every later host gate
# pointed at the firmware workspace -- where `cargo fmt --all --check` and `cargo clippy
# --workspace` both pass happily on the wrong code with exit 0 -- so the directory dies with the
# gate that needed it.
gate() {
  local min=$1 banner=$2 rank dir="" out a us t
  case $min in
    commit) rank=1; N_COMMIT=$(( N_COMMIT + 1 )); N_PUSH=$(( N_PUSH + 1 )); N_CI=$(( N_CI + 1 )) ;;
    push)   rank=2; N_PUSH=$(( N_PUSH + 1 ));    N_CI=$(( N_CI + 1 )) ;;
    ci)     rank=3; N_CI=$(( N_CI + 1 )) ;;
    *) printf 'gates.sh: bad min-tier: %s\n' "$min" >&2; exit 2 ;;
  esac
  shift 2
  if [[ $1 == "-C" ]]; then dir=$2; shift 2; fi

  out=$1
  for a in "${@:2}"; do
    if [[ $a == *[[:space:]]* ]]; then out+=" \"$a\""; else out+=" $a"; fi
  done
  [[ -z $dir ]] || out="cd $dir && $out"

  if [[ $MODE != run ]]; then
    if (( RANK >= rank )); then
      if [[ $MODE == list ]]; then
        printf '%-6s | %-3s | %-3s | %-2s | %s\n' "$min" \
          "$([[ 1 -ge $rank ]] && echo yes || echo -)" \
          "$([[ 2 -ge $rank ]] && echo yes || echo -)" \
          "$([[ 3 -ge $rank ]] && echo yes || echo -)" "$banner"
      else
        printf '%-6s\t%s\n' "$min" "$out"
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
}

# Before anything cross-target runs: without the embedded std every firmware gate dies as E0463
# ("can't find crate for `core`"), which reads like a code bug rather than a missing toolchain.
# Outside `nix develop .#default` a user's own cargo has no thumbv7em std installed, and the shim
# lefthook installs honours LEFTHOOK=0 as a total bypass -- a confusing red hook is exactly what
# teaches someone to set it. Checked in run mode only: --dry-run and --list answer questions about
# the list and must work anywhere.
if [[ $MODE == list ]]; then
  printf 'min    | com | psh | ci | gate\n'
  printf '%s\n' '-------+-----+-----+----+------------------------------------------------'
fi

if [[ $MODE == run ]]; then
  if [[ ! -d "$(rustc --print sysroot)/lib/rustlib/thumbv7em-none-eabihf" ]]; then
    printf 'gates.sh: this rustc has no thumbv7em-none-eabihf standard library, so every firmware\n' >&2
    printf 'gate below would fail with E0463 no matter what the code looks like.\n' >&2
    printf 'Enter the project shell first: nix develop .#default\n' >&2
    exit 3
  fi
  printf 'tier: %s\n' "$TIER"
fi

# The list is a shell script, so the class of bug that ate CI for three days (an inline
# single-quoted `bash -c '\''...'\''` string cannot carry an apostrophe: the quote closes, and
# because bash executes a script line by line as it parses it, every check appears to run before
# the step dies on "unexpected EOF while looking for matching `''" -- 6d7d38a to 70c6fc6, unseen
# because nothing had been pushed since the last green run) is worth ten milliseconds to rule out
# locally. Cheapest gate, so a broken definition is the fastest possible failure.
gate commit "=== gate definition parses ===" bash -n "${BASH_SOURCE[0]}"

# Docs name firmware image files; legal names come from the build rules themselves via `make -n`,
# so this is the only place in the gate set that invokes make, and it stays a dry run -- see the
# load-bearing warning at scripts/check-doc-artifact-names.sh:25-28. Covers names only: it does not
# read a line of Rust. Landed with TASK-058.
gate commit "=== docs artifact names ===" scripts/check-doc-artifact-names.sh

# The provenance reader's own checks (TASK-067). TASK-062.02's commit message described these cases as
# asserted and named their exit codes; nothing ran them, so the next edit to build.rs's blob encoding
# or to the feature normalizer had nothing between it and an ELF that reads as the wrong cfg set at the
# bench. Hand-emitted ELF fixtures, no build, nothing touched outside a `mktemp -d`: the cases and the
# cost rules they obey (at most five child invocations, one `cargo metadata`, never a `cargo build` /
# `clippy` / `objcopy` / `objdump` / `make`, never a path under firmware/target/) are spelled out in
# scripts/elf-provenance.sh's own header.
#
# Third gate, ahead of every cargo invocation, because it compiles nothing and needs no artifact: the
# one figure below is what cheapest-first says to do with a check that has no inputs to wait for.
# Placing it beside the push-tier provenance gate "for symmetry" would misrepresent both - they share
# no state, one grades the reader and the other grades the stamp - and ordering rule 1 is untouched,
# since no firmware gets built here. Measured 0.89 s warm, which is the price of seven objcopy
# invocations (~50 ms each) and one `cargo metadata` (~85 ms), every one of them load-bearing.
# Outside `nix develop .#default` there is no rust-objcopy, but gates.sh already exits 3 before any
# gate for the missing thumbv7em std, so the script's own exit-2 message is a direct-run affordance
# rather than a hook failure.
gate commit "=== elf-provenance --selftest ===" scripts/elf-provenance.sh --selftest

# The staleness mechanism's own checks (TASK-056): elf-check decides "was this ELF built from these
# sources?" by hashing the input set against a stamp build-elf wrote, and a check of that kind is only
# as good as the case that would notice it quietly passing. Ten cases drive the SHIPPED recipe through
# make's command-line overrides (ELF / ELF_INPUTS / MAIN_SRC / CARGO / PROV), so the thing graded is
# the file that ships rather than a copy of its logic; one of them is the false positive itself, red
# against any Makefile that compares mtimes again. Fixtures under `mktemp -d`, no cargo, nothing
# outside that directory touched -- rules and reasoning in scripts/check-elf-staleness.sh's header.
# Fourth in the list on the same cheapest-first rule as the gate above: it builds nothing and reads no
# artifact. Measured 0.9 s warm for ten cases, most of it make parsing the Makefile ten times.
gate commit "=== elf-staleness --selftest ===" scripts/check-elf-staleness.sh --selftest

# The load-address checker's own checks (TASK-068). The push-tier `=== image load addresses ===` gate far
# below reads one column of `rust-objdump -h --show-lma` by POSITION, and a parser that stops matching
# does not turn that gate red: an empty section table satisfies all three rules by having nothing to fail,
# so it turns it green for the wrong reason. Four traps of that kind were measured while planning this
# ticket, and none of them had a case until now. Bash matches leftmost-longest, so dropping the trim
# leaves objdump's column padding on every captured name and R2 answers `.sram1_bss=absent` for all six
# binaries -- `main` and `rig` included, exit still 0. defmt names sections after JSON records full of
# spaces, braces and commas, which no left-to-right field split survives. The Type column is composed
# flags joined by `, `, and prints nothing at all for an unallocated row, where "not loaded" and "a word
# nobody recognizes" must not be read the same way. And `.gnu.sgstubs` is zero-size ALLOC sitting ABOVE
# the real flash high-water mark, so counting it breaks R3's length equality by 8 bytes on a build that
# is correct.
#
# Hand-emitted ELF fixtures plus literal objdump row text, everything in a `mktemp -d`: seventeen cases,
# five of them real child processes, because the exit code that reaches this file is part of what is being
# asserted. Rules and reasoning in scripts/check-image-load-addresses.sh's selftest header. What they
# cannot prove is the limit every fixture has -- that the linker emits anything like these files -- and
# the push-tier gate over six linked ELFs stays the backstop for that, which is why both exist rather
# than one. Fifth here on the same cheapest-first rule as the two above: compiles nothing, reads no
# artifact, writes nothing under firmware/target/. Measured 0.6 s warm, all of it in the five children and
# their objdumps (~58 ms each). Eight deliberate mutations each turned at least one case red and left the
# rest green: trim dropped, zero-size counted, empty Type read as loaded, unknown Type word downgraded to
# a warning, VMA column read instead of LMA, refusal removed before measuring, `.sram1_bss` matched as a
# substring, and no-rows-parsed downgraded to a warning. Two of them needed a case of their own that no
# real binary could have provoked -- the substring match and the VMA swap, the latter visible only through
# a fixture whose `.data` runs from RAM and loads from flash.
gate commit "=== load-addresses --selftest ===" scripts/check-image-load-addresses.sh --selftest

gate commit "=== cargo fmt ===" cargo fmt --all --check

# Root Cargo.toml:3 declares exclude = ["firmware"], so the check above never meant all: firmware/
# is its own workspace and has drifted under this gate twice (TASK-044, TASK-060). `--check` is not
# optional, and not because of any expiry date: a bare `cargo fmt` would rewrite firmware/src/**/*.rs,
# which are ELF inputs (ELF_INPUTS in firmware/Makefile), so a commit would move the very bytes the
# next `make elf-check` hashes and send the bench red for a change it never made. A commit has no
# business rewriting the tree it is committing either way.
gate commit "=== cargo fmt (firmware workspace) ===" \
  cargo fmt --manifest-path firmware/Cargo.toml --all --check

gate commit "=== cargo clippy ===" cargo clippy --workspace --all-targets -- -D warnings

# Default features build asperitas-logging without `log-usb`, so every record-path function in it
# goes unlinted above. This is the only gate that sees them. Landed with TASK-047.
gate commit "=== cargo clippy (asperitas-logging log-usb) ===" \
  cargo clippy -p asperitas-logging --features log-usb --lib -- -D warnings

# The other transport has the same blind spot: the default feature set excludes `log-defmt`, so
# defmt_log.rs -- the bridge behind the probe's lossless log channel -- is compiled by no other gate
# here.
gate commit "=== cargo clippy (asperitas-logging log-defmt) ===" \
  cargo clippy -p asperitas-logging --features log-defmt --lib -- -D warnings

# pod-hw gates the hardware-backed Pod paths no default-feature build compiles. Its compile-time
# half runs in the push tier; its runtime half is the one deliberate CI-only item, below.
gate push "=== cargo clippy (asperitas-pod pod-hw feature) ===" \
  cargo clippy --workspace --all-targets --features asperitas-pod/pod-hw -- -D warnings

# The capture decoder's own verdict: exit 0 means every block the synthetic captures promised, they
# proved. Exercises the shipped read/decode/assemble path, so it needs no board.
gate push "=== dump_reassemble --selftest ===" \
  cargo run -p asperitas-logging --example dump_reassemble -- --selftest

# Rustdoc cross-references. Each member's `[lints] workspace = true` already denies the two
# intra-doc-link lints via the root table; RUSTDOCFLAGS raises that to every rustdoc warning.
# Two runs because neither feature set is a superset of the other: default features document
# logging's fn.init() but not usb.rs / led.rs / panic_handler.rs, while --all-features gains those
# (and pod's led / pins) and drops fn.init(). Landed with TASK-049. Do not merge them.
gate push "=== cargo doc (workspace) ===" \
  env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

gate push "=== cargo doc (workspace, all features) ===" \
  env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

# One spelling of the RTT-only cfg set, shared by the build below and the provenance gate after it.
# Kept as two literals they are free to drift, and a drifted expectation makes the gate refuse the
# exact image its own build line just produced.
RTT_ONLY_FEATURES="seed3 log-defmt"

# Both cross-builds, console first: see ordering rule 1 in the header. Whichever of the two runs
# last is the ELF the bench then flashes and decodes with, and because both lines are `push`-tier
# every tier that runs them ends on the RTT-only image -- which is what the provenance gate below
# asserts rather than what any comment here claims. So this pair stays adjacent, in this order, with
# nothing building firmware after it.
gate push "=== firmware cross-compile ===" -C firmware cargo build --release --features seed3

# The RTT-only image is the only build that links build.rs's `-Tdefmt.x` fragment and the
# `#[cfg(not(feature = "log-defmt"))]` logger stubs in the same binary; the console build above
# leaves both uncompiled. DEFMT_LOG is deliberately unset: it selects which frames get compiled in,
# not whether this compiles.
gate push "=== firmware cross-compile (RTT-only, log-defmt) ===" -C firmware \
  cargo build --release --no-default-features --features "$RTT_ONLY_FEATURES"

# Rule 1's residue, checked rather than promised. `release/main` is one name for two images, so
# "the pair ran in this order" and "the path holds the RTT-only ELF" are the same claim stated two
# ways -- and until this gate only the first one was enforced, which meant the symptom of violating
# it was a bench decoding defmt frames against symbols for a binary that is not on the board. Now a
# swapped pair or a later firmware build fails here, naming the cfg set found and the one expected.
#
# Reads only, on purpose: it shells out to `rust-objcopy` inside scripts/elf-provenance.sh and asks
# cargo for nothing. `cargo objcopy`, `cargo objdump` and `make build-elf` all rebuild, and a rebuild
# re-points the very path this gate is auditing -- a cfg switch finishes in 0.07 s by re-uplifting the
# cached hardlink, no relink -- so the check would grade its own side effect instead of the pair.
# Asking with NO_DEFAULT=1 also keeps `cargo metadata` out of it entirely: that derivation only runs
# when defaults are on. Measured warm: 0.08 s inside a `ci` run, 0.09 s inside `push`.
#
# `push` tier and placed where it is because it asserts something only the pair above can make true.
# In the `commit` tier there is no build before it, so the path would hold whatever the last push
# left, and a check of somebody else's residue is a check that fails for reasons nobody caused.
gate push "=== firmware ELF cfg provenance ===" \
  bash scripts/elf-provenance.sh check \
    firmware/target/thumbv7em-none-eabihf/release/main "$RTT_ONLY_FEATURES" 1

# Where the six images load. Asserts no file-backed section loads outside the FLASH region parsed from
# firmware/memory.x, that `.sram1_bss` stays NOBITS when present, and that each plain `-O binary`
# image is exactly as long as the highest flash LMA end implies -- the invariant whose absence made
# `main.bin` 469,763,536 bytes of mostly zeros with every gate green (TASK-059). Reads the artifacts the
# pair above just wrote, including whatever cfg set the provenance gate named, and asks cargo for
# nothing: bare `rust-objdump` / `rust-objcopy` only, for the same reason the gate above has.
#
# `push` tier and placed here because pre-commit builds no firmware, so the ELFs it reads may not exist
# at all -- and inventing a skip path for that is the blind spot TASK-062 had to remove from elf-check.
# Measured 0.70 s warm on its own, 0.71 s inside a `push` run: twelve child processes, six objdumps
# (~58 ms each) and six objcopies (~30 ms). Its parser's own assertions are a separate commit-tier gate,
# `=== load-addresses --selftest ===`, placed with the other selftests ahead of every cargo invocation --
# which is where a check with no inputs to wait for belongs; this one stays the only place that grades the
# six linked images.
gate push "=== image load addresses ===" bash scripts/check-image-load-addresses.sh

# Lint the same two cfg sets the two builds above just compiled, placed after them so clippy reuses
# their artifacts: ~20 s in a fresh target dir, ~2 s directly after a build, 0.25 s when nothing
# changed. --bins is the whole package over there (no lib target, six entries under src/bin/), and
# --all-targets is unusable on a no_std target with no test harness to link. firmware/ is a second
# workspace, so every host clippy above says nothing about the six bins that actually run on the
# board. The RTT-only cfg set is where TASK-036.03's warnings actually surfaced, so linting only the
# console build leaves it unchecked. Landed with TASK-060.03.
gate commit "=== firmware clippy (all bins) ===" -C firmware \
  cargo clippy --release --features seed3 --bins -- -D warnings

gate commit "=== firmware clippy (all bins, RTT-only, log-defmt) ===" -C firmware \
  cargo clippy --release --no-default-features --features "seed3 log-defmt" --bins -- -D warnings

gate push "=== cargo test ===" cargo test --workspace

# THE ONE DELIBERATELY CI-ONLY GATE, and it is priced rather than merely absent (TASK-061 AC #3).
# 67 s local warm -- more than every other push-tier gate combined -- to re-run the whole host suite
# under one non-default feature flag. Its compile-time half DOES run in the push tier, as the
# pod-hw clippy above, so what stays remote is runtime coverage of pod-hw code paths. Accepted
# because CI is the authority for that coverage, TASK-018.01's fixup made the same split on purpose
# (commit c44b9c1), and the loop that writes most commits here never pushes: pre-commit is where its
# work gets gated. Reopen condition, stated as a condition: if pushes become routine, or the
# autonomous loop starts pushing, re-measure and reconsider. Until then this exclusion is a
# decision with a price attached, not an oversight.
gate ci "=== cargo test (asperitas-pod pod-hw feature) ===" cargo test --workspace --features asperitas-pod/pod-hw

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

printf '\ntier %s: %d gates, %.1fs\n' "$TIER" "$RAN" \
  "$(awk -v s="$SECONDS" 'BEGIN { printf "%.1f", s }')"
