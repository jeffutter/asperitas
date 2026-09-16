#!/usr/bin/env bash
#
# One ledger of gate costs, and the check that makes every published figure a view onto it.
#
# WHY THIS EXISTS. Cost figures for the gate set lived in four files -- scripts/gates.sh's comments,
# lefthook.yml's, ci.yml's, firmware/Makefile's -- plus doc-001, and three tickets had repriced them by
# hand. TASK-064.01 saw `145.0s` one day after TASK-068 saw 143-144 s. TASK-068 published `commit 4 s`
# against a gate sum of 4.15-4.37 s. TASK-056 found a `0.4 s` claim that had never included the
# per-case cost it was arguing about. Nothing could see the drift, because every gate in this repo is a
# cargo or make invocation and none of them reads prose. TASK-070 ends it by making the figures DATA: a
# wall-clock numeral appears literally in exactly one committed file, this ledger, and prose holds KEYS
# into it.
#
# MODES:
#
#   --check     render every guarded file into a temp copy and compare bytes against disk; never
#               writes; reports every finding in one run. This is the mode that becomes a gate
#               (TASK-070.04). It reads --list and --dry-run only, so it costs milliseconds.
#   --render    substitute keys and rewrite generated regions in place. Idempotent on a rendered tree;
#               what --check simulates into a temp file.
#   --record    measure every tier once and write docs/gate-costs.json. Does not touch prose.
#   --refresh   --record then --render, the one command the docs name. Minutes long, so it stays a
#               human-invoked command and never a gate (TASK-068's non-goal, restated).
#   --selftest  drive THIS shipped script through fixtures under `mktemp -d`. Exit 0/1/2.
#
# WHAT A RENDERED FIGURE LOOKS LIKE, because one decision decides everything downstream. A cost-bearing
# sentence holds BOTH the figure and the key that owns it:
#
#   the commit tier takes ~4 s {{tier:commit}}, which is why pre-commit can afford it
#
# --render writes the figure from the ledger and LEAVES THE KEY STANDING. Substituting the token away is
# the obvious design and it does not work: prose would be rendered once and then agree with any ledger
# forever, because nothing left on disk would say which number the sentence claims, and --check's byte
# comparison would have nothing left to compare. Keeping the key is also what makes the ban on hand-typed
# figures usable -- a duration outside a generated region is legal exactly when a key owns it, so the ban
# never fires on a figure the renderer itself wrote.
#
# So writing prose is: type the sentence, with a guessed figure or with none, and name the key. --render
# puts the right number in the right shape and does not ask how you knew it. A guess that disagrees with
# the ledger is rewritten, not reported twice over; a key the ledger has no entry for is a crash.
#
# The classes are gate, tier, component and meta, and a token is ours only when its class is one of those
# AND its key is a plain slug. .github/workflows/ci.yml is a guarded file full of GitHub Actions
# expressions -- `${{ hashFiles('flake.lock') }}` at :90 -- and those pass through untouched. A token that
# looks like ours but names a class or a key the ledger has no entry for exits 2 rather than leaving a
# hole in the sentence it came from, because a sentence that keeps its shape while losing its figure is
# worse than a crash. To write ABOUT the syntax rather than price it, escape the braces: `\{{gate:key}}`
# passes through byte-for-byte and binds nothing. Stripping the backslash would leave a live token on disk
# that resolves to nothing, so the sentence documenting the key syntax would become an unknown key.
#
# Formatting lives in ONE awk function, FMT_AWK, shared by the two programs that turn seconds into text --
# the key table and the matrix generator. The renderer never sees a number: below one second, two decimals
# (`0.89 s`); at or above one second, the nearest integer under a tilde (`~4 s`). Three call sites spelling
# the rule by hand is the thing TASK-070 exists to stop.
#
# Structural blocks belong to the generator: everything between `<!-- BEGIN GENERATED: <name> -->` and
# `<!-- END GENERATED: <name> -->` is rewritten wholesale, and everything outside those markers passes
# through untouched except at a key. One generator today, `gate-matrix`, which joins `gates.sh --list`
# against the ledger's cost column -- the tier x gate table in doc-001, priced.
#
# THE RULE THAT DOES THE ACTUAL WORK: no wall-clock duration literal may appear in a guarded file
# outside a generated region unless a key owns it or the line carries a reasoned exemption. Byte-exact
# rendering catches a stale published value and a figure typed where the sentence cites nothing; this
# rule closes the remaining gap, which is a hand-typed number in prose that never learned to cite. Where a literal genuinely
# is not a published cost claim -- the quoted `Finished in 0.29s` cargo prints at firmware/Makefile:284
# -- the line carries `gate-costs:exempt reason="why"`, and the summary prints the exemption count so
# creep is greppable rather than silent. An exemption on a line with no duration in it is itself a
# finding: dead exemptions are how the marker becomes decoration. House style already does this twice
# over -- forbid a construct over the recipe text (check-elf-staleness.sh:273-274 bans `-newer`), and
# make grep the durable check rather than the prose (lefthook.yml:23-27).
#
# WHAT ELSE --CHECK ENFORCES, all of it from the ledger plus --list and --dry-run:
#
#   A1  Every live gate has a ledger entry, and every ledger entry names a live gate. Adding a gate
#       without pricing it, or deleting one and leaving its figure behind, is a finding either way.
#   A2  Each gate's stored command digest still matches its `--dry-run` command line. A changed command
#       makes the published cost a guess about work the gate no longer does. Per-gate and deliberately
#       narrow -- the same content-fingerprint idea firmware/Makefile:221-274 runs on ELF inputs -- so
#       retiering a gate or rewording its banner invalidates one measurement, not twenty.
#   A3  A gate's min_tier agrees between ledger and list, is priced in its own tier, and the ledger's
#       tier gate-counts equal `counts:` from --list. A retier silently changes WHICH observation is the
#       published figure, and that is precisely what a reader of the number cannot see.
#   A4  Every component key is cited by prose. Components have no generator to consume them, so an
#       uncited one is a figure nobody publishes, and unused figures are where drift returns. Gate
#       entries need no such rule: gate-matrix consumes all of them by construction, and making prose
#       name all twenty-two gates in sentences would buy busywork, not truth.
#   A5  Every key in prose resolves and every generated region is well-formed. Both exit 2.
#
# NO DATE-BASED STALENESS ANYWHERE. The ledger records when it was measured and components record when
# they were profiled, and NOTHING here compares those dates to a clock. TASK-056 exists because a
# freshness rule built on mtimes went red on 2975 s of pure checkout churn; a rule built on calendar
# days goes red on a Sunday. Freshness in this file is a content question -- A2's digest -- never a time
# question.
#
# GUARDED SET: the eight publishers of a cost figure. It deliberately reaches into backlog/docs/, unlike
# check-doc-artifact-names.sh:41-42, which excludes backlog/** because ticket files quote the names they
# prove broken. doc-001 is the publisher; excluding it would guard nothing that matters.
#
# DEPENDENCIES: bash, jq, awk, grep and sed, plus coreutils (sha256sum, sort, cmp, cut, seq, wc, mktemp,
# uname). Only jq comes from flake.nix's packages; the rest are what nixpkgs' stdenv puts on PATH in every
# Linux and Darwin stdenv, so they are present wherever bash is.
# python3, hyperfine and git resolve on this machine only because `nix develop` inherits the user PATH;
# none is in flake.nix's packages, so a gate that needed them would be green here and broken on a clean
# runner. --refresh additionally runs the tiers, which are cargo and make by definition, and it refuses
# to run outside the dev shell at all.
#
# BYTE-STABLE OUTPUT. The ledger is serialized canonically -- `jq -S`, two-space indent, LF, gates in
# --list declaration order -- so a non-empty diff after regeneration means a real change and never a key
# shuffle. One field is a clock, `measured_utc`, and a clock is the one thing that can make two identical
# runs differ; pin it with GATE_COSTS_MEASURED_UTC and the file comes out byte-identical, which is exactly
# what --selftest asserts. SOURCE_DATE_EPOCH is deliberately NOT honored even though it is the standard
# spelling: `nix develop` exports it (=315532800, i.e. 1980-01-01), so a ledger recorded from the very
# shell this command requires would date itself to the epoch and its provenance would be a lie on every
# run. An opt-in variable named for this script cannot be set by accident.
#
# Exit codes, exactly as the neighbours state them: 0 clean, 1 findings (all of them, in one run), 2 the
# check itself could not run -- no jq, no or malformed ledger, `gates.sh --list`/`--dry-run` failing or
# absent, an unknown key, a malformed generated region, or --refresh outside the dev shell. A check that
# cannot run never reports success, and never stays silent about why.
#
# Environment overrides, for --selftest ONLY, so a case can point the shipped script at a fixture tree
# instead of this repo: GATE_COSTS_ROOT (everything below derives from it), GATE_COSTS_LEDGER,
# GATE_COSTS_GATES_SH, GATE_COSTS_DEV_SHELL_OK, which stands in for the dev-shell probe so a fixture
# can exercise --record without a cross-target toolchain, and GATE_COSTS_MEASURED_UTC, which pins the
# ledger's one clock field. Setting them anywhere else is a way to make a check lie.

set -uo pipefail

PROG=gate-costs

# Anchoring is load-bearing, not hygiene: the guarded set is named by repo-relative path, so an
# unanchored run from firmware/ would go looking for backlog/docs/ under firmware/. Same shape as
# scripts/check-doc-artifact-names.sh:49-50.
ROOT=${GATE_COSTS_ROOT:-$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)}
cd "$ROOT" || exit 2

LEDGER=${GATE_COSTS_LEDGER:-$ROOT/docs/gate-costs.json}
GATES_SH=${GATE_COSTS_GATES_SH:-$ROOT/scripts/gates.sh}
# Absolute, because --selftest runs this file as a subprocess from inside a fixture tree, where a path
# relative to some other cwd would either miss or, worse, find a different copy of itself.
SELF_DIR=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
SELF="$SELF_DIR/$(basename -- "${BASH_SOURCE[0]}")"

# Every publisher of a cost figure. Quoted because one path has spaces in it.
GUARDED_FILES=(
  scripts/gates.sh
  lefthook.yml
  .github/workflows/ci.yml
  firmware/Makefile
  scripts/elf-provenance.sh
  scripts/check-elf-staleness.sh
  scripts/check-image-load-addresses.sh
  'backlog/docs/doc-001 - Asperitas-Project-Plan.md'
)

GENERATORS='gate-matrix'
EXEMPT_MARK='gate-costs:exempt'

VIOLATIONS=()
FATALS=()
EXEMPT_COUNT=0
WORK=''

die() {
  printf '%s\n' "$PROG: $*" >&2
  exit 2
}

usage() {
  cat <<'USAGE_END'
usage: scripts/gate-costs.sh MODE [--repeat N]

  --check     compare every guarded file against its rendered form; never writes (exit 0/1/2)
  --render    substitute keys and rewrite generated regions in place
  --record    measure every tier once and write docs/gate-costs.json (touches no prose)
  --refresh   --record then --render: the command the docs name. Minutes long, never a gate.
  --selftest  assert this script against fixtures under mktemp -d (exit 0/1/2)

  --repeat N  with --record/--refresh: N samples per tier, store the median (default 1)
USAGE_END
}

make_workdir() {
  WORK=$(mktemp -d) || die "mktemp -d failed, so there is nowhere to put a rendered temp copy"
  trap 'if [ -n "${WORK:-}" ]; then rm -rf "$WORK"; fi' EXIT
  mkdir -p "$WORK/gen" "$WORK/cmds"

  # Every derived path is set HERE rather than at file scope, because at file scope WORK is still empty
  # and each of these would name a file at / -- the failure being that the script cannot write its own
  # temp files and says so as a parse error three functions later.
  LIST_TSV="$WORK/list.tsv"              # key <TAB> min <TAB> banner   (--list order; counts row last)
  DRY_TSV="$WORK/dry.tsv"                # key <TAB> min <TAB> command
  DIGESTS="$WORK/digests.tsv"            # key <TAB> sha256(command)
  LEDGER_GATES="$WORK/ledger-gates.tsv"  # key <TAB> min_tier <TAB> digest <TAB> cost-at-min-tier
  LEDGER_TIERS="$WORK/ledger-tiers.tsv"  # tier <TAB> seconds <TAB> gate-count
  LEDGER_COMPS="$WORK/ledger-comps.tsv"  # key <TAB> display
  CITED_TOKENS="$WORK/cited.txt"         # every class-this-script-owns token found in guarded prose
}

bare() { printf '%s' "${1#"$ROOT"/}"; }

say() { printf '%s\n' "$*"; }

sha256_of_file() { # <file>
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -- "$1" | cut -d' ' -f1
  else
    shasum -a 256 -- "$1" | cut -d' ' -f1
  fi
}

# --------------------------------------------------------------------------- the live tier table
#
# Read once per run from the two machine-read interfaces scripts/gates.sh defines: `--list` for what
# exists and at which min_tier, `--dry-run` for the exact command line whose digest A2 stores. Both are
# read rather than parsed out of gates.sh's source, so adding a gate to the script makes it visible here
# with no second edit -- which is the property TASK-068 spent real effort building and this file would
# otherwise throw away.
#
# Parsing is defensive anyway: a banner may contain a pipe, and a 40-character key overruns the %-27s pad
# and shifts every column after it (gates.sh says so). So rows are recognised by shape -- first field a
# key, second a tier name -- and the banner is everything from field six rightwards, rejoined with pipes
# rather than silently truncated.

require_tools() {
  command -v jq >/dev/null 2>&1 || die "no jq on PATH. flake.nix ships it, so this is a broken shell: \
enter the dev shell with \`nix develop .#default\`"
  command -v awk >/dev/null 2>&1 || die "no awk on PATH"
  command -v grep >/dev/null 2>&1 || die "no grep on PATH"
}

read_gate_table() {
  local out nl nd
  require_tools
  [ -f "$GATES_SH" ] || die "no $(bare "$GATES_SH"). The gate list is what the ledger prices, and without \
it every figure below is unverifiable. Remedy: run from the repo root, or point GATE_COSTS_GATES_SH at \
the generator (used by --selftest only)."

  if ! out=$(bash "$GATES_SH" --list ci 2>&1); then
    printf '%s\n' "$PROG: \`$(bare "$GATES_SH") --list ci\` exited nonzero:" >&2
    printf '%s\n' "$out" >&2
    exit 2
  fi
  printf '%s\n' "$out" | awk '
    { line = $0; sub(/[[:space:]]+$/, "", line) }
    /^counts:/ { printf "counts\t%s\t\t\n", line; next }
    {
      n = split(line, f, /[[:space:]]*\|[[:space:]]*/)
      if (n < 6) next
      if (f[1] !~ /^[a-z0-9][a-z0-9-]*$/) next
      if (f[2] != "commit" && f[2] != "push" && f[2] != "ci") next
      banner = f[6]
      for (i = 7; i <= n; i++) banner = banner "|" f[i]
      printf "%s\t%s\t%s\t\n", f[1], f[2], banner
    }
  ' >"$LIST_TSV" || die "could not parse \`$(bare "$GATES_SH") --list ci\`"
  [ -s "$LIST_TSV" ] || die "\`$(bare "$GATES_SH") --list ci\` printed no gate rows I could parse"
  grep -q '^counts	' "$LIST_TSV" || die "\`$(bare "$GATES_SH") --list ci\` printed no \`counts:\` line, \
so there is no tier gate-count to compare the ledger against"

  if ! out=$(bash "$GATES_SH" --dry-run ci 2>&1); then
    printf '%s\n' "$PROG: \`$(bare "$GATES_SH") --dry-run ci\` exited nonzero:" >&2
    printf '%s\n' "$out" >&2
    exit 2
  fi
  printf '%s\n' "$out" | awk -F'\t' 'NF >= 3 {
      key = $2
      sub(/^[[:space:]]+/, "", key); sub(/[[:space:]]+$/, "", key)
      if (key !~ /^[a-z0-9][a-z0-9-]*$/) next
      cmd = $3
      for (i = 4; i <= NF; i++) cmd = cmd "\t" $i
      printf "%s\t%s\t%s\n", key, $1, cmd
    }' >"$DRY_TSV" || die "could not parse \`$(bare "$GATES_SH") --dry-run ci\`"
  [ -s "$DRY_TSV" ] || die "\`$(bare "$GATES_SH") --dry-run ci\` printed no gate rows I could parse"

  nl=$(grep -c -v '^counts	' "$LIST_TSV")
  nd=$(wc -l <"$DRY_TSV" | tr -d '[:space:]')
  [ "$nl" = "$nd" ] || die "--list offers $nl gates and --dry-run $nd. Refusing to price one list when \
the generator shows a gate to the other and hides it from the first."
  if cut -f1 "$DRY_TSV" | sort | uniq -d | grep -q .; then
    die "duplicate keys in \`--dry-run\` output: $(cut -f1 "$DRY_TSV" | sort | uniq -d | tr '\n' ' '). \
Two gates cannot share a key: the ledger keys its entries by it."
  fi
}

# sha256 of each gate's command line, staged by one awk pass and hashed by ONE sha256sum process. One
# fork per gate would spend twenty-two of them inside one commit-tier budget, which is the ratio this
# whole file has to keep honest.
#
# The digest input is the `--dry-run` command column verbatim with no trailing newline -- the command and
# nothing else. That is what lets a retier or a reworded banner keep its measurement while any change to
# what the gate actually runs invalidates it.
digest_gate_commands() {
  mkdir -p "$WORK/c" || die "could not stage command digests"
  awk -F'\t' -v d="$WORK/c" '{
      cmd = $3
      for (i = 4; i <= NF; i++) cmd = cmd "\t" $i
      fn = d "/" $1
      printf "%s", cmd > fn
      close(fn)
    }' "$DRY_TSV" || die "could not stage gate commands for hashing"
  if ! (cd "$WORK/c" && sha256sum -- *) 2>/dev/null | awk '{ printf "%s\t%s\n", $2, $1 }' >"$DIGESTS"; then
    die "sha256sum refused the staged command lines"
  fi
  [ -s "$DIGESTS" ] || die "no command digests computed, so A2 cannot run"
}

# --------------------------------------------------------------------------- shared awk fragments
#
# The format rule and the duration regexes are each spelled ONCE here and shared by every awk program that
# needs them. Three copies of "%.2f s versus ~%d s" inside one file is how TASK-070 started -- objcopy at
# ~50 ms in one comment and 82 ms in another -- and a checker of other people's drift that carries its own
# has some nerve.
#
# Two sharing mechanisms, because awk allows only one of them cleanly. The regexes and the token
# classifier are staged as files and pulled in with repeated -f, which is the spelling awk documents. The
# format rule is a shell variable concatenated onto an inline program, because these three call sites have
# a one-line body each and awk takes no kindly view on mixing -f with inline rules -- see build_key_table.

FMT_AWK='
  function fmt(v) {
    v = v + 0
    return (v < 1) ? sprintf("%.2f s", v) : sprintf("~%d s", int(v + 0.5))
  }
'

# A wall-clock duration as prose writes it, plus the two variants the checks need.
#
# The leading class is the whole difficulty. A bare `[0-9]+(ms|s)` reads printf field widths (`%-27s`, in
# gates.sh's own --list printer), bash's `local i=-1 s`, hex addresses, `sha256sum`, `x86_64`,
# `elf32-littlearm`, `TASK-070.02` and `SECONDS=2` as cost claims -- and a checker that cries wolf by the
# third line gets disabled, which is the failure mode this whole file exists to avoid. So a candidate must
# START where prose numbers start: line start, whitespace, or opening punctuation. That rejects any digit
# glued to `%`, `=`, `-`, `_`, `.`, `/`, `:` or a letter. The trailing class is what stops `256sum`,
# `8 save` and `1 sample` from reading as a duration: the unit has to end the word. En dash and tilde are
# range separators because `0.59-0.62 s` and `2-5 s` are both how this repo writes a measured spread.
#
#   LIT   a duration anywhere in a line, once owned figures and foreign braces have been blanked out
#   TRAIL a duration at the END of the text preceding a key -- the figure a render replaces
#
# TRAIL is one half of a contract; strip_owned (in the classifier fragment) is the other. The scanner
# blanks exactly what the renderer would rewrite, so a figure can never be forgiven by one rule and
# reported by the other.
REGEX_AWK='
  BEGIN {
    HEAD  = "(^|[ \t~({[*>])"
    NUM   = "[0-9][0-9,.]*"
    RANGE = "(" NUM "([~-]|–)" NUM ")"
    UNIT  = "(ms|secs|seconds|mins|minutes|hrs|hours|s)"
    TOKRE = "\\{\\{[^{}]*\\}\\}"
    LIT   = HEAD "((" RANGE ")|" NUM ")[ \t]*" UNIT "([^A-Za-z0-9_-]|$)"
    TRAIL = HEAD "((" RANGE ")|" NUM ")[ \t]*" UNIT "[ \t]*$"

    # Generated-region markers, spelled once for the renderer and the prose scanner -- both read this
    # file. They are STRINGS, not regexp constants: `X = /re/` is not an assignment of a pattern in awk,
    # it is an implicit `$0 ~ /re/`, so X silently becomes 0 or 1. Written as a constant, RE_BEGIN held
    # "0", every line containing a zero looked like a BEGIN marker, and the check reported invented
    # findings with total confidence.
    RE_BEGIN     = "^[[:space:]]*<!--[[:space:]]*BEGIN GENERATED:[[:space:]]*[A-Za-z0-9_-]+[[:space:]]*-->$"
    RE_END       = "^[[:space:]]*<!--[[:space:]]*END GENERATED:[[:space:]]*[A-Za-z0-9_-]+[[:space:]]*-->$"
    RE_LOOSE_END = "^[[:space:]]*<!--[[:space:]]*END GENERATED:"
  }
'

CLASSIFY_AWK='
  # ours    a token this script owns: known class, plain slug key
  # suspect right shape, unknown class -- almost always a typo like {{gat:foo}}, where silence would let
  #         a figure vanish from a sentence that still reads perfectly
  # foreign not ours at all: GitHub Actions writes `${{ hashFiles(...) }}` inside a guarded file
  function classify(tok) {
    if (tok ~ /^(gate|tier|component|meta):[A-Za-z0-9][A-Za-z0-9._-]*$/) return "ours"
    if (tok ~ /^[a-z][a-z_]*:[A-Za-z0-9][A-Za-z0-9._-]*$/) return "suspect"
    return "foreign"
  }
  # Is the token preceded by the backslash that means "I am writing about the syntax, not pricing it"?
  function escaped(pre) { return substr(pre, length(pre), 1) == "\\" }

  # The figure a token owns, removed from the text standing before it. Both the renderer and the prose
  # scanner need this SAME rule, so it lives here: whenever one strips something the other does not, a
  # render appends a second copy beside the first and the check calls its own output drift.
  #
  # Two shapes count. A segment ENDING with the value the token renders to today covers a display string
  # like "~50 ms per objcopy", which no duration regex can see past the words around it.
  # A wall-clock duration at the end covers a hand-typed guess, which is what the ban exists to catch.
  #
  # Reads the global V (token -> rendered text), which both callers populate. Returns the segment with
  # the owned figure cut, keeping whatever single separator or punctuation stood in front of it, so that
  # re-rendering a rendered line reproduces it byte for byte.
  function strip_owned(seg, tok,   v, t, k, p, c) {
    v = (tok in V) ? V[tok] : ""
    if (v != "" && length(seg) >= length(v)) {
      t = seg; k = 0
      while (length(t) > 0 && substr(t, length(t), 1) ~ /[ \t]/) {
        t = substr(t, 1, length(t) - 1); k++
      }
      if (length(t) >= length(v) && substr(t, length(t) - length(v) + 1) == v) {
        p = length(t) - length(v)
        return substr(seg, 1, p)
      }
    }
    if (match(seg, TRAIL)) {
      # The character TRAIL starts on is a separator, an approximation marker, or opening punctuation,
      # and the three are not treated alike: keep one space so the rewrites do not glue words, drop the
      # tilde because it belongs to the figure ("~4 s" is one claim, not a tilde wearing a figure), and
      # keep a bracket because it belongs to the sentence. A dropped "(" or a kept "~" both make the
      # next render disagree with this one, which is drift this script would then report about itself.
      c = substr(seg, RSTART, 1)
      if (c == "~" || c == "\xe2\x80\x93") return substr(seg, 1, RSTART - 1)
      return substr(seg, 1, RSTART)
    }
    return seg
  }
'

write_awk_fragments() {
  printf '%s\n' "$REGEX_AWK" >"$WORK/regex.awk" || die "could not stage the duration regexes"
  printf '%s\n' "$CLASSIFY_AWK" >"$WORK/classify.awk" || die "could not stage the token classifier"
}

# --------------------------------------------------------------------------- the ledger
#
# Shape, and why each field is there rather than merely convenient:
#
#   measured_utc, host, environment, samples_per_tier
#       Provenance for what is a claim about one machine on one afternoon. Without it a figure is
#       folklore, which is how `0.4 s` survived in check-elf-staleness.sh across two tickets.
#   approximation
#       The caveat travels with the data instead of sitting in prose that can drift away from it.
#   cost keyed by PAYING TIER
#       A cost is not intrinsic to a gate. The two firmware-clippy gates cost ~20 s in a fresh target
#       dir, ~2 s straight after a build and 0.25 s when nothing changed, and ordering rule 1 makes the
#       value depend on adjacency rather than membership. So the number belongs to the tier process that
#       paid it. The published figure for a gate is defined mechanically as the observation from its own
#       min_tier, and cross-tier arithmetic is invalid even though tiers are cumulative as sets: each
#       tier is a separate process and its clock restarts.
#   command_sha256
#       A2, recomputed by --check and never by eye.
#   components[]
#       Micro-costs an objcopy or a `cargo metadata` that prose argues with and that no tier run can
#       re-measure: a display string, the date, and the method that produced it. Explicitly approximate
#       and dated, and deduplicated by having exactly one home. Planning found gates.sh:253 claiming one
#       `cargo metadata` at ~85 ms while elf-provenance.sh:252,267 claimed two of them and :163 said
#       82 ms for the same call.
#
# Serialized canonically: `jq -S` sorts object keys recursively, arrays keep their order, two-space
# indent, LF endings, one trailing newline. A non-empty diff after regeneration must always mean a real
# change, never key reordering.

require_ledger() {
  [ -e "$LEDGER" ] || die "no $(bare "$LEDGER"). Write one with: scripts/gate-costs.sh --refresh"
  [ -r "$LEDGER" ] || die "$(bare "$LEDGER") exists but is not readable"
  jq -e '.schema == 1' "$LEDGER" >/dev/null 2>&1 \
    || die "$(bare "$LEDGER") is not a schema-1 cost ledger. \`jq -e '.schema == 1' $(bare "$LEDGER")\` \
refused it; see the shape documented above this function."
  jq -e '(.gates | type) == "array" and (.tiers | type) == "object" and (.components | type) == "array"
         and (.measured_utc | type) == "string"' "$LEDGER" >/dev/null 2>&1 \
    || die "$(bare "$LEDGER") is malformed: schema 1 needs a gates array, a tiers object, a components \
array and a measured_utc string"
}

read_ledger_tables() {
  jq -r '.gates[] | [ (.key // ""), (.min_tier // ""), (.command_sha256 // ""),
                     (((.cost // {})[.min_tier // ""]) // "" | tostring) ] | @tsv' \
     "$LEDGER" >"$LEDGER_GATES" || die "could not read .gates from $(bare "$LEDGER")"
  jq -r '.tiers | to_entries[] | [ .key, (.value.seconds | tostring), (.value.gates | tostring) ] | @tsv' \
     "$LEDGER" >"$LEDGER_TIERS" || die "could not read .tiers from $(bare "$LEDGER")"
  jq -r '.components[] | [ (.key // ""), (.display // "") ] | @tsv' \
     "$LEDGER" >"$LEDGER_COMPS" || die "could not read .components from $(bare "$LEDGER")"
}

# The key -> rendered-text table the renderer consumes: `token <TAB> text`. Seconds go through fmt(), so
# the rule that turns 0.89 into `0.89 s` and 4.3 into `~4 s` exists in exactly one place in this file --
# see FMT_AWK. Components arrive as display strings already written by a person, because a micro-cost like
# "~50 ms per objcopy" is a profiled judgement about a loop, not a number this script can re-derive.
# The format rule reaches these programs as "$FMT_AWK" rather than as `-f "$WORK/fmt.awk"`, because an
# inline program text on a command line that already carries a -f is not appended to it: gawk ignores the
# inline rules and takes them for another input file. Both lines therefore emitted no key at all, and the
# renderer reported every gate and tier in the tree as an unknown key.
build_key_table() {
  {
    awk -F'\t' "$FMT_AWK"'$4 != "" { printf "gate:%s\t%s\n", $1, fmt($4) }' "$LEDGER_GATES"
    awk -F'\t' "$FMT_AWK"'$2 != "" { printf "tier:%s\t%s\n", $1, fmt($2) }' "$LEDGER_TIERS"
    awk -F'\t' '$2 != "" { printf "component:%s\t%s\n", $1, $2 }' "$LEDGER_COMPS"
    jq -r '"meta:measured\t" + (.measured_utc | split("T")[0])' "$LEDGER"
  } >"$WORK/keys.tsv" || die "could not build the key table from $(bare "$LEDGER")"
}

# --------------------------------------------------------------------------- generated regions
#
# The tier x gate matrix, with the cost column joined from the ledger. Each gate's published figure is
# its own min_tier observation, so the matrix prints the same number a {{gate:key}} would. A gate the
# ledger has not priced prints UNPRICED instead of blanks, because a blank cell in a cost column reads
# as zero and zero is the most misleading thing a cost column can say.
generate_gate_matrix() {
  # No space between "$FMT_AWK" and the program below: concatenated they are ONE argument, awk's program.
  # With a space they are two, and awk takes the first as the program and the second as an input file --
  # which silently emits nothing here and empties the matrix.
  awk -F'\t' "$FMT_AWK"'
    NR == FNR { if ($4 != "") cost[$1] = $4; next }
    $1 == "counts" { next }
    {
      rank = ($2 == "commit" ? 1 : ($2 == "push" ? 2 : 3))
      printf "%-27s | %-6s | %-3s | %-3s | %-2s | %-7s | %s\n", $1, $2, \
        (1 <= rank ? "yes" : "-"), (2 <= rank ? "yes" : "-"), (3 <= rank ? "yes" : "-"), \
        ($1 in cost ? fmt(cost[$1]) : "UNPRICED"), $3
    }
  ' "$LEDGER_GATES" "$LIST_TSV" >"$WORK/gen/gate-matrix.rows" || die "gate-matrix generation failed"

  {
    printf '%s\n' '```text'
    printf '%-27s | %-6s | %-3s | %-3s | %-2s | %-7s | %s\n' key min com psh ci cost gate
    printf '%s\n' \
      '----------------------------+--------+-----+-----+----+---------+---------------------------------------'
    cat "$WORK/gen/gate-matrix.rows"
    grep '^counts	' "$LIST_TSV" | cut -f2
    printf '%s\n' '```'
  } >"$WORK/gen/gate-matrix" || die "could not assemble the gate-matrix region"
}

run_generators() {
  generate_gate_matrix
}

# --------------------------------------------------------------------------- the renderer
#
# One awk pass per file: keys substituted outside generated regions, generator output spliced in place of
# everything between the markers. Findings go to a TSV file rather than printing as they are found, so one
# run names every unknown key and every malformed region in the tree instead of only the first.
#
# Substitution leaves the token standing after the figure it just wrote (`~4 s {{tier:commit}}`), which is
# what makes the scheme checkable at all -- see the header. It follows that rendering a rendered line must
# reproduce it exactly, so whatever figure a token currently carries is stripped from the preceding text
# and rewritten from the ledger: idempotent when they already agree, corrective when a person edited the
# number or the ledger moved under it. A hand-typed guess is therefore never a finding twice over --
# --render fixes it, and --check reports the same disagreement as one byte difference.

RENDER_AWK='
  function emit(s,   at, rest, b, tok, cls, pre, v, out) {
    out = ""
    while ((at = index(s, "{{")) > 0) {
      pre = substr(s, 1, at - 1)
      rest = substr(s, at + 2)
      b = index(rest, "}}")
      if (b == 0) { out = out s; return out }
      tok = substr(rest, 1, b - 1)
      s = substr(rest, b + 2)

      # An escaped token passes through byte-for-byte, backslash included. Stripping the escape would put
      # a live token on disk that binds nothing, and the next --check would read the sentence that was
      # writing ABOUT the syntax as a claim about a gate named key.
      if (escaped(pre)) { out = out pre "{{" tok "}}"; continue }

      cls = classify(tok)
      if (cls == "foreign") { out = out pre "{{" tok "}}"; continue }
      if (cls == "suspect") {
        printf "SUSPECT_CLASS\t%s\t%s\ttoken %s is shaped like a cost key but names no such class; the \
classes are gate, tier, component and meta\n", NAME, FNR, tok > ERRF
        bad = 1; out = out pre "{{" tok "}}"; continue
      }
      if (!(tok in V)) {
        printf "UNKNOWN_KEY\t%s\t%s\tkey %s is not in the ledger; nothing was substituted, so the \
sentence keeps its shape while losing its figure\n", NAME, FNR, tok > ERRF
        bad = 1; out = out pre "{{" tok "}}"; continue
      }

      # V[tok] is already formatted -- build_key_table ran it through fmt() for the two classes that
      # hold seconds, and left a component display string and the measured date exactly as written.
      # Re-running fmt() here would coerce "2026-09-14" and "~50 ms" to 0 and print "0.00 s" for both.
      v = V[tok]
      out = out strip_owned(pre, tok) v " {{" tok "}}"
    }
    return out s
  }
  BEGIN {
    n = split(GENS, g, /[[:space:]]+/)
    for (i = 1; i <= n; i++) if (g[i] != "") HAVE[g[i]] = 1
  }
  NR == FNR {
    p = index($0, "\t")
    if (p > 1) V[substr($0, 1, p - 1)] = substr($0, p + 1)
    next
  }
  {
    if (inregion) {
      if ($0 ~ RE_END) { inregion = 0; print }
      next
    }
    if ($0 ~ RE_BEGIN) {
      name = $0
      sub(/^.*BEGIN GENERATED:[[:space:]]*/, "", name)
      sub(/[[:space:]]*-->[[:space:]]*$/, "", name)
      print
      openat = FNR
      openname = name
      inregion = 1
      if (!(name in HAVE)) {
        printf "MARKER_ERROR\t%s\t%s\tno generator named %s. Generators this script has: %s\n", \
          NAME, FNR, name, GENS > ERRF
        bad = 1
        next
      }
      genfile = GEN "/" name
      while ((getline gl < genfile) > 0) print gl
      close(genfile)
      next
    }
    if ($0 ~ RE_LOOSE_END) {
      printf "MARKER_ERROR\t%s\t%s\tEND GENERATED with no matching BEGIN above it\n", NAME, FNR > ERRF
      bad = 1
      print
      next
    }
    if (index($0, "{{") > 0) print emit($0)
    else print
  }
  END {
    if (inregion)
      printf "MARKER_ERROR\t%s\t%s\tBEGIN GENERATED: %s has no matching END below it\n", \
        NAME, openat, openname > ERRF
    if (bad || inregion) exit 3
  }
'

write_render_program() {
  printf '%s\n' "$RENDER_AWK" >"$WORK/render.awk" || die "could not stage the render program"
}

render_file() { # <in-file> <out-file> <err-file>
  local in=$1 out=$2 err=$3
  awk -v NAME="$(bare "$in")" -v GEN="$WORK/gen" -v GENS="$GENERATORS" -v ERRF="$err" \
      -f "$WORK/regex.awk" -f "$WORK/classify.awk" -f "$WORK/render.awk" \
      "$WORK/keys.tsv" "$in" >"$out"
}

# First line where two files differ, or a note that one is a prefix of the other. Written in awk rather
# than diff because the message wants BOTH sides quoted, and because diff exit codes carry more meaning
# than this needs.
describe_first_diff() { # <on-disk> <rendered> <label>
  awk -v label="$3" '
    NR == FNR { a[NR] = $0; n = NR; next }
    {
      if (FNR > n) { print label ": the rendered form has " FNR " lines and the file on disk has " n; exit }
      if (a[FNR] != $0) {
        printf "%s:%s: renders differently from the file on disk.\n  on disk:  %s\n  rendered: %s\n", \
          label, FNR, a[FNR], $0
        exit
      }
    }
    END { if (NR < n) printf "%s: the file on disk has %s lines and the rendered form has %s\n", label, n, NR }
  ' "$1" "$2"
}

# --------------------------------------------------------------------------- prose rules
#
# ONE awk pass per guarded file produces every prose finding and every citation the file makes. Doing it
# in bash instead -- grep for a line number, awk for that line, grep again for the marker -- costs three
# processes per hit, and with fifty-odd duration literals in the tree today that is most of this script's
# runtime spent on plumbing. The budget for --check is under a second, because the commit tier it will one
# day join is priced in the ledger's `tiers.commit`.
#
# Rows come back on stdout as KIND<TAB>field<TAB>detail and bash turns them into messages. Two things this
# pass does that are easy to get wrong and worth stating:
#
#   * Generated regions are skipped by tracking the markers inline, so a matrix cell cannot trip the
#     literal rule. Line numbers stay true because skipping still advances FNR.
#   * Before scanning for literals, owned figures (`~4 s {{tier:commit}}`) and foreign braces are blanked
#     out. Without the first half the rule reports every figure the renderer itself wrote; without the
#     second it reports GitHub Actions expressions. An escaped token, `\{{gate:key}}`, is documentation of
#     the syntax and cites nothing.

PROSE_AWK='
  BEGIN {
    EXREASON = EXMARK "[ \t]+reason=\"[^\"]{4,}\""
  }
  # First input is the key table, token <TAB> rendered text. The scanner needs the VALUES, not just the
  # tokens: an exemption or a literal is judged against what the renderer would have written here.
  NR == FNR {
    p = index($0, "\t")
    if (p > 1) V[substr($0, 1, p - 1)] = substr($0, p + 1)
    next
  }
  # A copy of the line with every brace token removed, and with each owned figure the renderer would
  # rewrite removed too. Whatever survives is what a person typed that nobody owns -- the only thing the
  # literal ban may fire on. Using strip_owned rather than a second regex here is the point: the two must
  # agree exactly, or the check reports drift the renderer itself produced.
  function unowned(s0,   s, out, at, pre, rest, b, tok) {
    out = ""; s = s0
    while ((at = index(s, "{{")) > 0) {
      pre = substr(s, 1, at - 1)
      rest = substr(s, at + 2)
      b = index(rest, "}}")
      if (b == 0) break
      tok = substr(rest, 1, b - 1)
      s = substr(rest, b + 2)
      if (escaped(pre)) { out = out substr(pre, 1, length(pre) - 1) "{{" tok "}}"; continue }
      if (classify(tok) != "ours") { out = out pre; continue }
      out = out strip_owned(pre, tok)
    }
    out = out s
    gsub(TOKRE, " ", out)
    return out
  }
  {
    if (inregion) { if ($0 ~ RE_END) inregion = 0; next }
    if ($0 ~ RE_BEGIN) { inregion = 1; next }
  }
  {
    line = $0
    t = unowned(line)
    hit = ""
    if (match(t, LIT)) hit = substr(t, RSTART, RLENGTH)

    if (index(line, EXMARK) > 0) {
      if (hit == "") printf "DEAD_EXEMPT\t%s\t\n", FNR
      else printf "%s\t%s\t%s\n", (line ~ EXREASON ? "EXEMPT" : "EXEMPT_BAD"), FNR, hit
    } else if (hit != "") {
      printf "LIT\t%s\t%s\n", FNR, hit
    }

    s = line
    while ((a = index(s, "{{")) > 0) {
      pre = substr(s, 1, a - 1)
      rest = substr(s, a + 2)
      b = index(rest, "}}")
      if (b == 0) break
      tok = substr(rest, 1, b - 1)
      s = substr(rest, b + 2)
      if (!escaped(pre) && classify(tok) == "ours") printf "CITE\t%s\t\n", tok
    }
  }
'

write_prose_program() {
  printf '%s\n' "$PROSE_AWK" >"$WORK/prose.awk" || die "could not stage the prose scanner"
}

# One pass, then the messages. The counter counts every reasoned exemption in the tree, including the ones
# whose figure is fine, because the summary line is what makes creep visible: each one is a figure this
# check cannot see.
check_prose() { # <guarded file>
  local f=$1 kind fld detail
  # Findings go through a file rather than a process substitution, because `< <(awk ...)` discards awk's
  # exit status: a program that failed to COMPILE produced no rows, and the check reported a tree it had
  # not actually read as clean. A scanner that cannot run is a FATAL, not an empty finding list.
  : >"$WORK/prose.rows"
  if ! awk -v NAME="$(bare "$f")" -v EXMARK="$EXEMPT_MARK" \
        -f "$WORK/regex.awk" -f "$WORK/classify.awk" -f "$WORK/prose.awk" \
        "$WORK/keys.tsv" "$f" >"$WORK/prose.rows"; then
    FATALS+=("$(bare "$f"): the prose scanner could not run on this file, so it was not checked. Its \
exit status, not its findings, is what this names.")
    return 1
  fi
  while IFS=$'\t' read -r kind fld detail; do
    [ -n "$kind" ] || continue
    case "$kind" in
      CITE) printf '%s\n' "$fld" >>"$CITED_TOKENS" ;;
      EXEMPT) EXEMPT_COUNT=$((EXEMPT_COUNT + 1)) ;;
      EXEMPT_BAD)
        VIOLATIONS+=("$(bare "$f"):$fld: '$detail' carries $EXEMPT_MARK but no reason of at least four \
characters. An unreasoned exemption is exactly the drift this rule exists to catch, and it is faster to \
type than a key would be.") ;;
      DEAD_EXEMPT)
        VIOLATIONS+=("$(bare "$f"):$fld: dead $EXEMPT_MARK - this line holds no wall-clock duration, so \
the exemption covers nothing. Remove it; an exemption that outlives its figure is how the marker becomes \
decoration.") ;;
      LIT)
        VIOLATIONS+=("$(bare "$f"):$fld: wall-clock duration literal '${detail:-found}' sits outside a \
generated region owned by no key. Either cite it - write \`VALUE {{gate:key}}\`, \`{{tier:commit}}\`, \
\`{{component:key}}\` or \`{{meta:measured}}\` and let --render put the number there - or, if this line is \
not publishing a cost claim, mark it: $EXEMPT_MARK reason=\"what it is instead\"") ;;
    esac
  done <"$WORK/prose.rows"
}

# --------------------------------------------------------------------------- structural rules
#
# All four join the live table against the ledger in awk rather than in a jq call per gate: twenty-two
# jq forks would be most of this script's runtime, and the joins want to run once per --check either way.

check_structure() {
  # A1/A2/A3 in one pass over (live list, digests, ledger gates).
  local findings
  findings=$(awk -F'\t' '
    function rank(t) { return (t == "commit" ? 1 : (t == "push" ? 2 : 3)) }
    FILENAME ~ /digests/     { digest[$1] = $2; next }
    FILENAME ~ /list/ && $1 != "counts" { live[$1] = $2; order[++n] = $1; next }
    FILENAME ~ /ledger-gates/ {
      seen[$1] = 1; lmin[$1] = $2; ldig[$1] = $3; lcost[$1] = $4
      next
    }
    END {
      for (i = 1; i <= n; i++) {
        k = order[i]
        if (!(k in seen)) {
          printf "GATE_UNPRICED\t%s\t%s\n", k, live[k]
          continue
        }
        if (lmin[k] != live[k])
          printf "TIER_DISAGREES\t%s\t%s\t%s\n", k, live[k], lmin[k]
        if (digest[k] != "" && ldig[k] != "" && digest[k] != ldig[k])
          printf "DIGEST_CHANGED\t%s\t%s\t%s\n", k, ldig[k], digest[k]
        if (lcost[k] == "")
          printf "GATE_UNMEASURED\t%s\t%s\n", k, live[k]
      }
      for (k in seen)
        if (!(k in live)) printf "GATE_GONE\t%s\t%s\n", k, lmin[k]
    }
  ' "$DIGESTS" "$LIST_TSV" "$LEDGER_GATES")

  local kind key a b
  while IFS=$'\t' read -r kind key a b; do
    [ -n "$kind" ] || continue
    case "$kind" in
      GATE_UNPRICED)
        VIOLATIONS+=("$PROG: live gate '$key' (min_tier $a) has no entry in $(bare "$LEDGER"). Price it \
with \`scripts/gate-costs.sh --refresh\`; a gate nobody prices is a gate whose cost prose will invent.") ;;
      GATE_GONE)
        VIOLATIONS+=("$PROG: $(bare "$LEDGER") still prices '$key', which \`$(bare "$GATES_SH") --list\` \
no longer offers. Delete the entry with \`--refresh\`: a figure for a gate that does not exist is not \
conservative, it is fiction.") ;;
      TIER_DISAGREES)
        VIOLATIONS+=("$PROG: gate '$key' is min_tier $a in \`$(bare "$GATES_SH") --list\` but $b in \
$(bare "$LEDGER"). The published figure is defined as the observation from a gate's own tier, so a retier \
that moves only one of these two silently changes which measurement the number means.") ;;
      DIGEST_CHANGED)
        VIOLATIONS+=("$PROG: gate '$key' now runs a different command than the one its stored cost \
measured (ledger ${a:0:12}..., current ${b:0:12}...). Re-measure with \`scripts/gate-costs.sh --refresh\`: \
the digest covers the command line only, so this is work added or removed, not a banner edit.") ;;
      GATE_UNMEASURED)
        VIOLATIONS+=("$PROG: gate '$key' has a ledger entry but no seconds recorded for its own tier \
$a, so there is nothing to publish. \`--refresh\` fills it.") ;;
    esac
  done <<<"$findings"

  # A3's counts half, and A1's for tiers: the ledger must carry all three tier totals, agree with --list
  # about how many gates each runs, and hold no tier that does not exist. Iterating the three tiers rather
  # than whatever the ledger happens to name is what makes a MISSING total a finding -- iterating the file
  # would find nothing at all in exactly the case worth finding.
  local want got tier want_n _rest
  want=$(grep '^counts	' "$LIST_TSV" | cut -f2 | sed -e 's/^counts: //' -e 's/, only some in ci$//')
  for tier in commit push ci; do
    got=$(awk -F'\t' -v t="$tier" '$1 == t { print $3 }' "$LEDGER_TIERS")
    want_n=$(printf '%s' "$want" | grep -oE "$tier [0-9]+" | awk '{print $2}')
    if [ -z "$got" ]; then
      FATALS+=("$PROG: $(bare "$LEDGER") has no tiers.$tier entry. Write one with \
\`scripts/gate-costs.sh --refresh\` - a tier total is what the prose cites for every budget argument.")
      continue
    fi
    if [ -n "$want_n" ] && [ "$got" != "$want_n" ]; then
      VIOLATIONS+=("$PROG: tiers.$tier.gates says $got gates but \`$(bare "$GATES_SH") --list\` counts \
$want_n for that tier. Refresh the ledger.")
    fi
  done
  # Two variables, not one: `read -r tier` puts the WHOLE line into $tier, tabs included, so every tier
  # in the ledger looked like one this script does not run.
  while IFS=$'\t' read -r tier _rest; do
    case "$tier" in commit|push|ci|"") continue ;; esac
    VIOLATIONS+=("$PROG: $(bare "$LEDGER") prices a tier '$tier' that \`$(bare "$GATES_SH")\` does not \
run. Delete it with \`--refresh\`.")
  done <"$LEDGER_TIERS"

  # A4: components have no generator consuming them, so an uncited one is a figure nobody publishes.
  local ck _display cited
  while IFS=$'\t' read -r ck _display; do
    [ -n "$ck" ] || continue
    if ! grep -qx "component:$ck" "$CITED_TOKENS"; then
      VIOLATIONS+=("$PROG: $(bare "$LEDGER") publishes component '$ck' that no guarded file cites. Either \
cite it as {{component:$ck}} where the prose argues with it, or drop it from the ledger - an unused \
figure is where drift comes back, because nothing compares it to anything.")
    fi
  done <"$LEDGER_COMPS"
  cited=$(wc -l <"$CITED_TOKENS" | tr -d '[:space:]')
  say "cited keys: ${cited:-0}"
}

# --------------------------------------------------------------------------- reporting
#
# Two classes, because they mean different things to whoever reads the terminal. A FATAL is this script
# failing to do its job (no ledger, an unresolvable key, a region whose markers do not pair); a
# VIOLATION is the tree disagreeing with the ledger, which is what the check exists to find. Both print
# in one run, and the exit code takes the worse of the two: 2 beats 1 beats 0.
report() {
  local nv=${#VIOLATIONS[@]} nf=${#FATALS[@]} ng
  ng=$(grep -c -v '^counts	' "$LIST_TSV")

  if [ "$nf" -eq 0 ] && [ "$nv" -eq 0 ]; then
    say "$PROG: clean - ${#GUARDED_FILES[@]} guarded files render byte-identical to $(bare "$LEDGER"), \
which prices all $ng live gates; ${EXEMPT_COUNT} reasoned duration exemption(s) in use"
    return 0
  fi

  {
    if [ "$nf" -gt 0 ]; then
      printf '%s\n' "$PROG: COULD NOT FINISH THE CHECK ($nf reason(s)) - these are not drift, they are \
this script being unable to answer"
      printf '\n'
      printf '%s\n' "${FATALS[@]}"
      [ "$nv" -gt 0 ] && printf '\n'
    fi
    if [ "$nv" -gt 0 ]; then
      printf '%s\n' "$PROG: $nv finding(s) against $(bare "$LEDGER")"
      printf '\n'
      printf '%s\n' "${VIOLATIONS[@]}"
      printf '\n'
    fi
    printf '%s\n' 'A figure in prose is written VALUE {{class:key}} -- classes gate, tier, component, meta. \
--render rewrites the value from the ledger; refresh the ledger itself with `scripts/gate-costs.sh \
--refresh`, which is minutes long and is never a gate.'
    printf '%s\n' "Reasoned exemptions: ${EXEMPT_COUNT}. Grep '$EXEMPT_MARK' to see them; each one is a \
figure this check cannot see."
  } >&2

  [ "$nf" -gt 0 ] && return 2
  return 1
}

# Everything --check and --render need, in the order that fails fastest: the ledger before the gate table,
# because reading the ledger is free and running gates.sh twice (--list and --dry-run) is not.
prepare() {
  require_tools
  write_awk_fragments
  write_render_program
  write_prose_program
  require_ledger
  read_gate_table
  digest_gate_commands
  read_ledger_tables
  build_key_table
  run_generators
  : >"$CITED_TOKENS"
}

guarded_files_exist() {
  local f
  for f in "${GUARDED_FILES[@]}"; do
    [ -f "$f" ] || FATALS+=("$PROG: guarded file $f does not exist. It publishes a cost figure, so its \
absence is not a reason to skip the check - either restore it or remove it from GUARDED_FILES here.")
  done
  [ "${#FATALS[@]}" -eq 0 ]
}

# Render every guarded file into $WORK/out.<n>, numbered in GUARDED_FILES order, and collect every
# renderer finding. Shared by --check and --render because the two must apply byte-identical rules: a bug
# that only exists in the checking half would report drift the fixing half cannot cure, and the reverse
# would "fix" a file --check is happy with.
#
# Returns nonzero when any file carried a fatal -- an unresolvable key, mismatched region markers. Those
# stop the run rather than piling on underneath: with a token that resolves to nothing, every downstream
# comparison in that file is describing a hole rather than a stale number, and a person reading sixty
# findings to find one typo will not fix the typo.
render_all() {
  local f i=0 rc
  for f in "${GUARDED_FILES[@]}"; do
    i=$((i + 1))
    rm -f "$WORK/r.err"
    if render_file "$f" "$WORK/out.$i" "$WORK/r.err"; then rc=0; else rc=1; fi
    if [ -s "$WORK/r.err" ]; then
      while IFS=$'\t' read -r _name _line _msg; do
        [ -n "$_name" ] && FATALS+=("$_name:$_line: $_msg")
      done <"$WORK/r.err"
    fi
    [ "$rc" -eq 0 ] || return 1
  done
  return 0
}

cmd_check() {
  local f diff_msg i=0
  prepare
  if ! guarded_files_exist; then report; return 2; fi
  if ! render_all; then report; return 2; fi

  for f in "${GUARDED_FILES[@]}"; do
    i=$((i + 1))
    if ! cmp -s "$f" "$WORK/out.$i"; then
      diff_msg=$(describe_first_diff "$f" "$WORK/out.$i" "$(bare "$f")")
      VIOLATIONS+=("$diff_msg
The text on disk is not what $(bare "$LEDGER") renders. Run \`scripts/gate-costs.sh --render\` and read \
the diff: a figure edited by hand inside a sentence that cites a key is the case this exists to catch.")
    fi
    check_prose "$f"
  done

  check_structure
  report
}

cmd_render() {
  local f i=0 changed=()
  prepare
  if ! guarded_files_exist; then
    printf '%s\n' "${FATALS[@]}" >&2
    return 2
  fi
  if ! render_all; then
    printf '%s\n' "${FATALS[@]}" >&2
    printf '%s\n' "$PROG: wrote nothing - fix the keys above and re-run --render" >&2
    return 2
  fi

  # Which files differ is decided once, from the temp copies, before anything is written: asking again
  # after writing would always answer "no". Rendering everything first also means an unknown key in the
  # seventh file cannot leave six files rewritten around it.
  for f in "${GUARDED_FILES[@]}"; do
    i=$((i + 1))
    cmp -s "$f" "$WORK/out.$i" || changed+=("$f")
  done

  i=0
  for f in "${GUARDED_FILES[@]}"; do
    i=$((i + 1))
    cmp -s "$f" "$WORK/out.$i" && continue
    cat "$WORK/out.$i" >"$f" || die "could not write $f"
  done

  if [ "${#changed[@]}" -gt 0 ]; then
    say "$PROG: rewrote ${#changed[@]} file(s):"
    printf '  %s\n' "${changed[@]#"$ROOT"/}"
  else
    say "$PROG: already rendered - all ${#GUARDED_FILES[@]} guarded files byte-identical to their \
rendered form"
  fi
  return 0
}

# --------------------------------------------------------------------------- --record
#
# The measurement, and the only reason this file cannot be a pure checker.
#
# Each tier runs ONCE as its own process with GATES_TIMINGS_FILE set, and the numbers come from that
# sink rather than from stdout: gates.sh's human `--- N.NNs` line is joinable to a gate by position
# alone, and `cargo test` printing a lookalike line would corrupt the join silently. Three runs, not
# one, because a cost belongs to the tier process that paid it -- tiers are cumulative as sets but each
# restarts the clock, so a `push` observation of a commit-tier gate is a different fact about a
# different adjacency, not a second opinion on the same one.
#
# What --record does NOT invent is `components`. Those micro-costs -- an objcopy, a `cargo metadata`,
# the fork-plus-interpreter price of a child -- were profiled ad hoc and no tier run re-derives them,
# so an existing ledger's .components is carried forward verbatim and the initial entries are authored
# in the ledger itself. That keeps this true: a wall-clock numeral is written by a person in exactly one
# file, and every other numeral in the repo is either measured by this command or rendered from one of
# the two.

require_dev_shell() {
  # Stands in for the probe so --selftest can exercise --record against a stub gate list without an
  # embedded toolchain, in BOTH directions: =1 answers yes, =0 answers no, because a refusal nobody has
  # ever provoked is a refusal that may not work. Nothing outside --selftest should set it; see the header.
  if [ -n "${GATE_COSTS_DEV_SHELL_OK:-}" ]; then
    [ "$GATE_COSTS_DEV_SHELL_OK" = "1" ] && return 0
    die "--record runs the firmware gates, and this shell has no thumbv7em-none-eabihf standard library. \
Remedy: run it inside \`nix develop .#default\`."
  fi
  local sysroot
  sysroot=$(rustc --print sysroot 2>/dev/null) || sysroot=""
  if [ -z "$sysroot" ] || [ ! -d "$sysroot/lib/rustlib/thumbv7em-none-eabihf" ]; then
    die "--record runs the firmware gates, and this rustc has no thumbv7em-none-eabihf standard library \
(its sysroot is ${sysroot:-unresolvable}). Remedy: run it inside \`nix develop .#default\`."
  fi
  command -v make >/dev/null 2>&1 || die "--record runs the firmware gates, and there is no make on \
PATH. Remedy: run it inside \`nix develop .#default\`."
}

# UTC stamp for `measured_utc`. GATE_COSTS_MEASURED_UTC is the pin that makes --record byte-reproducible,
# which --selftest asserts; an unpinned run differs by exactly this field. It takes a whole timestamp
# rather than an integer epoch because the value it writes is a string and a typo in a date-shaped string
# is caught by the shape check below, whereas any integer is a plausible epoch -- and 315532800 is exactly
# plausible enough to have shipped a 1980 ledger once.
#
# Value-producing helpers CANNOT report failure by dying. `measured=$(stamp_utc)` runs the whole function
# in a subshell, so a die() in here exits only that subshell, leaves the variable empty, and lets
# cmd_record write a ledger whose one provenance field is blank -- with exit 0. --selftest has a case for
# precisely that, because the first version of this function did exactly that. The contract instead: print
# the reason, return 1, and let the caller exit on the status. The callers therefore say `|| exit 2`
# rather than `|| die`, which would print a second and vaguer line over the precise one.
stamp_utc() {
  if [ -n "${GATE_COSTS_MEASURED_UTC:-}" ]; then
    printf '%s' "$GATE_COSTS_MEASURED_UTC" | grep -Eq '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$' \
      || { printf '%s\n' "$PROG: GATE_COSTS_MEASURED_UTC='$GATE_COSTS_MEASURED_UTC' is not a UTC \
timestamp shaped YYYY-MM-DDTHH:MM:SSZ" >&2; return 1; }
    printf '%s\n' "$GATE_COSTS_MEASURED_UTC"
    return 0
  fi
  local out
  out=$(date -u +%Y-%m-%dT%H:%M:%SZ) \
    || { printf '%s\n' "$PROG: could not read the clock" >&2; return 1; }
  printf '%s\n' "$out"
}

# Same contract as stamp_utc: called inside a command substitution, so it reports by returning 1.
describe_host() {
  local hn kr mn cores json
  hn=$(uname -n 2>/dev/null || printf unknown)
  kr=$(uname -srm 2>/dev/null || printf unknown)
  mn=$(uname -m 2>/dev/null || printf unknown)
  cores=$(nproc 2>/dev/null || printf unknown)
  json=$(jq -n --arg hostname "$hn" --arg kernel "$kr" --arg machine "$mn" --arg cores "$cores" \
        --arg toolchain "$(rustc -V 2>/dev/null || printf 'rustc unavailable')" \
    '{hostname: $hostname, system: $kernel, machine: $machine, cores: ($cores | tonumber? // $cores),
      rustc: $toolchain}') \
    || { printf '%s\n' "$PROG: could not describe the measuring host" >&2; return 1; }
  printf '%s\n' "$json"
}

# One tier, one sample, one timings file. Output goes to a log because lefthook-style buffering aside,
# four minutes of cargo is not something to dump on a terminal between two progress lines.
run_tier_sample() { # <tier> <sample-index>
  local tier=$1 s=$2 timings=$WORK/timings.$1.$2 log=$WORK/run.$1.$2.log
  : >"$timings" || die "could not open a timings file for $tier"
  say "  $tier tier, sample $s of $REPEATS (this is the slow part; log at $(bare_log "$log"))"
  if ! GATES_TIMINGS_FILE=$timings bash "$GATES_SH" "$tier" >"$log" 2>&1; then
    printf '%s\n' "$PROG: \`$(bare "$GATES_SH") $tier\` failed, so there is nothing to record. Its last \
25 lines:" >&2
    tail -n 25 "$log" >&2
    die "a ledger of costs for a tier that does not pass would be a ledger of fiction."
  fi
  [ -s "$timings" ] || die "\`$(bare "$GATES_SH") $tier\` exited 0 but wrote no timings, so the sink \
contract TASK-070.01 defines is broken and there is nothing to record."
  # Fold the sample into the aggregate table: tier, kind, subject, seconds.
  awk -F'\t' -v tier="$tier" '{
      if ($1 == "gate" || $1 == "tier") printf "%s\t%s\t%s\t%s\n", tier, $1, $2, $3
    }' "$timings" >>"$WORK/samples.tsv" || die "could not fold $tier timings into the aggregate"
}

bare_log() { printf '%s' "${1#"$WORK"/}"; }

measure_all_tiers() {
  : >"$WORK/samples.tsv"
  local tier s
  for s in $(seq 1 "$REPEATS"); do
    for tier in commit push ci; do
      run_tier_sample "$tier" "$s"
    done
  done
}

# Median per (tier, kind, subject). Median rather than mean because the interesting failure of a warm
# single sample is a page fault or a Spotlight scan, and one such outlier moves a mean outright while
# the median of N=1 is the sample itself -- which is what the default records, honestly labelled.
aggregate_samples() {
  sort -t"$(printf '\t')" -k1,1 -k2,2 -k3,3 "$WORK/samples.tsv" \
    | awk -F'\t' '
      function med(vals, n,   i, j, t, m) {
        for (i = 2; i <= n; i++) {            # insertion sort: n is samples_per_tier, never large
          t = vals[i]; j = i - 1
          while (j >= 1 && vals[j] > t) { vals[j + 1] = vals[j]; j-- }
          vals[j + 1] = t
        }
        return (n % 2) ? vals[(n + 1) / 2] : (vals[n / 2] + vals[n / 2 + 1]) / 2
      }
      function flush(   k, n, parts, v) {
        if (cur == "") return
        n = split(seen, parts, " ")
        for (k = 1; k <= n; k++) v[k] = parts[k] + 0
        printf "%s\t%s\t%s\t%.3f\n", curtier, curkind, cur, med(v, n)
        cur = ""
      }
      {
        if ($3 != cur || $2 != curkind || $1 != curtier) { flush(); cur = $3; curkind = $2; curtier = $1; seen = "" }
        seen = seen (seen == "" ? "" : " ") $4
      }
      END { flush() }
    ' >"$WORK/agg.tsv" || die "could not aggregate the timing samples"
  [ -s "$WORK/agg.tsv" ] || die "aggregation produced nothing from $(wc -l <"$WORK/samples.tsv") sample \
lines"
}

# key <TAB> min <TAB> banner <TAB> digest <TAB> cost@commit <TAB> cost@push <TAB> cost@ci
#
# Blank means "this tier never paid for this gate", which is a fact worth keeping: it is what lets
# --check tell a gate that was retiered apart from one that was never measured.
build_gate_rows() {
  awk -F'\t' '
    # agg rows are tier <TAB> kind <TAB> subject <TAB> seconds; indexed key-first so the gate row below
    # can ask for a key and a tier in the order it already has them.
    FILENAME ~ /agg/     { if ($2 == "gate") cost[$3 "\t" $1] = $4; next }
    FILENAME ~ /digests/ { digest[$1] = $2; next }
    $1 != "counts" {
      banner = $3
      gsub(/\t/, " ", banner)
      printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\n", $1, $2, banner,
        digest[$1],
        (($1 "\tcommit") in cost ? cost[$1 "\tcommit"] : ""),
        (($1 "\tpush")   in cost ? cost[$1 "\tpush"]   : ""),
        (($1 "\tci")     in cost ? cost[$1 "\tci"]     : "")
    }
  ' "$WORK/agg.tsv" "$DIGESTS" "$LIST_TSV" >"$WORK/gate-rows.tsv" \
    || die "could not join the measurements against the gate list"
}

tier_rows() {
  awk -F'\t' '
    BEGIN { n["commit"] = 0; n["push"] = 0; n["ci"] = 0 }
    FILENAME ~ /agg/ && $2 == "gate" { n[$1]++; next }
    FILENAME ~ /agg/ && $2 == "tier" { tot[$1] = $4; next }
    END {
      for (t in n) {
        if (!(t in tot)) { printf "ERR\tmissing tier total for %s\n", t; continue }
        printf "%s\t%s\t%d\n", t, tot[t], n[t]
      }
    }
  ' "$WORK/agg.tsv" >"$WORK/tier-rows.raw" || die "could not summarize the tier totals"
  if grep -q '^ERR' "$WORK/tier-rows.raw"; then
    die "$(sed -e 's/^ERR\t//' "$WORK/tier-rows.raw" | head -1). The tier line is what gates.sh appends \
after the last gate, so a tier that reported gates but no total did not reach its summary."
  fi
  # Canonical order, not whatever `for (t in n)` happened to yield: a ledger whose keys shuffle is a
  # ledger whose diffs mean nothing.
  for t in commit push ci; do
    grep "^$t	" "$WORK/tier-rows.raw" || die "no $t tier total was measured"
  done >"$WORK/tier-rows.tsv"
}

cmd_record() {
  local measured host_json approx comps_json gates_json tiers_json tmp
  REPEATS=$SAMPLES
  require_tools
  require_dev_shell
  write_awk_fragments
  read_gate_table
  digest_gate_commands

  say "$PROG: measuring $REPEATS sample(s) of each tier. This is minutes long on any host - three tiers \
are three cargo-and-make processes - which is why it is a command and never a gate. Current totals, if \
one has been recorded here: $(bare "$LEDGER")"
  measure_all_tiers
  aggregate_samples

  # Carry the hand-authored micro-costs forward rather than dropping them; on a first record there is
  # nothing to carry and the array starts empty.
  comps_json='[]'
  if [ -e "$LEDGER" ] && jq -e '.components | type == "array"' "$LEDGER" >/dev/null 2>&1; then
    comps_json=$(jq -c '.components' "$LEDGER") || die "could not read .components from $(bare "$LEDGER")"
  fi

  build_gate_rows
  tier_rows
  measured=$(stamp_utc) || exit 2
  host_json=$(describe_host) || exit 2
  if [ "$REPEATS" -eq 1 ]; then
    approx="one warm sample per tier on one machine; the last digit is noise, and a cold cache or a \
background Spotlight scan moves any figure here by tens of percent"
  else
    approx="median of $REPEATS warm samples per tier on one machine; the last digit is still noise, and \
every figure remains a claim about this host rather than about a CI runner"
  fi

  gates_json=$(jq -R -s '
      split("\n") | map(select(length > 0) | split("\t") |
        { key: .[0], min_tier: .[1], banner: .[2], command_sha256: .[3],
          cost: ( { commit: .[4], push: .[5], ci: .[6] }
                  | with_entries(select(.value != ""))
                  | map_values(tonumber) ) })' "$WORK/gate-rows.tsv") \
    || die "could not turn the gate rows into JSON"
  tiers_json=$(jq -R -s '
      split("\n") | map(select(length > 0) | split("\t") |
        { key: .[0], value: { seconds: (.[1] | tonumber), gates: (.[2] | tonumber) } })
      | from_entries' "$WORK/tier-rows.tsv") \
    || die "could not turn the tier rows into JSON"

  tmp=$WORK/ledger.json
  jq -n -S \
      --arg measured "$measured" \
      --argjson host "$host_json" \
      --arg environment "nix develop .#default" \
      --argjson samples "$REPEATS" \
      --arg approximation "$approx" \
      --argjson components "$comps_json" \
      --argjson gates "$gates_json" \
      --argjson tiers "$tiers_json" \
    '{schema: 1, measured_utc: $measured, host: $host, environment: $environment,
      samples_per_tier: $samples, approximation: $approximation, tiers: $tiers, gates: $gates,
      components: $components}' >"$tmp" \
    || die "could not serialize the ledger"

  mkdir -p "$(dirname -- "$LEDGER")" || die "could not create the directory for $(bare "$LEDGER")"
  cat "$tmp" >"$LEDGER" || die "could not write $(bare "$LEDGER")"
  say "$PROG: wrote $(bare "$LEDGER") - $(jq '.gates | length' "$LEDGER") gates priced, measured_utc \
$measured, samples_per_tier $REPEATS"
  return 0
}

cmd_refresh() {
  cmd_record || return $?
  cmd_render || return $?
  say "$PROG: refreshed - $(bare "$LEDGER") and everything rendered from it agree"
}

# --------------------------------------------------------------------------- --selftest
#
# The house pattern: drive THIS shipped file, as a subprocess, through fixtures under `mktemp -d` with
# an EXIT trap. Nothing here exercises a copy of the logic, so a case going red means the thing users
# run is broken rather than that a reimplementation of it is.
#
# Every case builds a whole fixture tree rather than a single file, because GUARDED_FILES is the unit
# under test: eight named paths, all of which must exist for a check to run at all. The fixture is
# therefore eight small files plus a stub generator plus a ledger, and the pristine baseline is produced
# by running `--render` on it -- so a case mutates a KNOWN-GOOD tree and asserts one specific finding,
# instead of trusting that a hand-typed expectation happens to match what fmt() would print.
#
# Three seams make a fixture able to exercise modes that otherwise reach outside themselves:
# GATE_COSTS_GATES_SH points at a stub gate list (so --check never compiles anything),
# GATE_COSTS_DEV_SHELL_OK=1 stands in for the embedded toolchain so --record can be driven at all, and
# GATE_COSTS_DEV_SHELL_OK=0 forces the opposite answer, because a refusal you cannot provoke is a
# refusal nobody has ever seen. GATE_COSTS_MEASURED_UTC pins the one clock field so --record's output is
# assertable byte-for-byte.

FIX=''
CASES_RUN=0
CASES_FAILED=0

fixture_gates_sh() { # <path> [extra-gate-line]
  local f=$1 extra=${2:-}
  cat >"$f" <<'STUB_END'
#!/usr/bin/env bash
# A gate list shaped like scripts/gates.sh's machine-read interfaces and nothing else: four gates, no
# cargo, no make. --check reads only --list and --dry-run, so this is the whole surface a real check
# touches; run mode exists so --record can be driven without a cross-target toolchain.
set -uo pipefail
MODE=run; TIER=ci
for a in "$@"; do
  case $a in
    --dry-run) MODE=dry ;;
    --list)    MODE=list ;;
    commit|push|ci) TIER=$a ;;
  esac
done
TRANK=3; [[ $TIER == push ]] && TRANK=2; [[ $TIER == commit ]] && TRANK=1
NC=0; NP=0; NI=0

g() {
  local min=$1 key=$2 banner=$3 cmd=$4 r
  case $min in commit) r=1 ;; push) r=2 ;; *) r=3 ;; esac
  [[ $r -le 1 ]] && NC=$((NC + 1))
  [[ $r -le 2 ]] && NP=$((NP + 1))
  NI=$((NI + 1))
  [[ $TRANK -lt $r ]] && return 0
  case $MODE in
    list)
      printf '%-27s | %-6s | %-3s | %-3s | %-2s | %s\n' "$key" "$min" \
        "$([[ $r -le 1 ]] && echo yes || echo -)" \
        "$([[ $r -le 2 ]] && echo yes || echo -)" yes "$banner" ;;
    dry)  printf '%-6s\t%-27s\t%s\n' "$min" "$key" "$cmd" ;;
    run)  printf 'gate\t%s\t0.500\n' "$key" >>"$GATES_TIMINGS_FILE" ;;
  esac
  return 0
}

[[ $MODE == list ]] && printf '%-27s | %-6s | %-3s | %-3s | %-2s | %s\n' key min com psh ci gate

g commit alpha "=== alpha ===" "cargo run --example alpha"
g commit beta  "=== beta ==="  "cargo clippy -p beta"
g push   gamma "=== gamma ===" "cargo test -p gamma"
g ci     delta "=== delta ===" "cargo test -p delta --features hw"
EXTRA_GATE_PLACEHOLDER

if [[ $MODE == list ]]; then
  printf '\ncounts: commit %d, push %d, ci %d\n' "$NC" "$NP" "$NI"
  exit 0
elif [[ $MODE == dry ]]; then
  exit 0
fi
printf 'tier\t%s\t1.500\n' "$TIER" >>"$GATES_TIMINGS_FILE"
STUB_END
  if [ -n "$extra" ]; then
    perl-free_replace 'EXTRA_GATE_PLACEHOLDER' "$extra" "$f"
  else
    perl-free_replace 'EXTRA_GATE_PLACEHOLDER' '' "$f"
  fi
  chmod +x "$f" || die "could not make the stub gate list executable"
}

# Eight guarded paths, one stub generator, one ledger. Content is chosen so that ONE `--render` makes
# the whole tree self-consistent: prose carries keys with the wrong figures deliberately typed in, and
# the assertion that the baseline is clean is then a statement about the renderer, not about my typing.
build_fixture() { # <dir>
  local d=$1 extra=${2:-}
  mkdir -p "$d/scripts" "$d/docs" "$d/firmware" "$d/.github/workflows" \
           "$d/backlog/docs" || die "could not build a fixture tree"
  fixture_gates_sh "$d/scripts/gates.sh" "$extra"
  touch "$d/lefthook.yml" "$d/scripts/elf-provenance.sh" "$d/scripts/check-elf-staleness.sh" \
        "$d/scripts/check-image-load-addresses.sh" "$d/firmware/Makefile" \
        "$d/.github/workflows/ci.yml" || die "could not stage the guarded files"

  cat >"$d/docs/gate-costs.json" <<'LEDGER_END'
{
  "approximation": "hand-written fixture; nothing here was measured",
  "components": [
    {"display": "~50 ms per objcopy", "key": "objcopy-invocation", "measured_utc": "2026-09-13", "method": "timed loop of rust-objcopy over 7 images"}
  ],
  "environment": "fixture",
  "gates": [
    {"banner": "=== alpha ===", "command_sha256": "PLACEHOLDER_ALPHA", "cost": {"ci": 0.5, "commit": 0.89, "push": 0.5}, "key": "alpha", "min_tier": "commit"},
    {"banner": "=== beta ===", "command_sha256": "PLACEHOLDER_BETA", "cost": {"ci": 0.5, "commit": 1.4, "push": 0.5}, "key": "beta", "min_tier": "commit"},
    {"banner": "=== gamma ===", "command_sha256": "PLACEHOLDER_GAMMA", "cost": {"ci": 0.5, "push": 2.0}, "key": "gamma", "min_tier": "push"},
    {"banner": "=== delta ===", "command_sha256": "PLACEHOLDER_DELTA", "cost": {"ci": 67.0}, "key": "delta", "min_tier": "ci"}
  ],
  "host": {"hostname": "fixture", "machine": "arm64"},
  "measured_utc": "2026-09-14T12:00:00Z",
  "samples_per_tier": 1,
  "schema": 1,
  "tiers": {"ci": {"gates": 4, "seconds": 68.0}, "commit": {"gates": 2, "seconds": 4.3}, "push": {"gates": 3, "seconds": 77.0}}
}
LEDGER_END

  # Real digests of the stub's own command lines, so A2 passes on the baseline and only the case that
  # means to break it breaks it.
  # Split on the tabs by hand rather than with `read -r a b c`: tab is IFS *whitespace*, so the stub's
  # %-27s key padding merges with the delimiter and the fields come back shifted. This mirrors how
  # read_gate_table takes the same columns from the real generator.
  local k cmd line h
  while IFS= read -r line; do
    cmd=${line#*$'\t'}; cmd=${cmd#*$'\t'}
    k=${line#*$'\t'}; k=${k%%$'\t'*}
    k=$(printf '%s' "$k" | tr -d '[:space:]')
    h=$(printf '%s' "$cmd" | { sha256sum 2>/dev/null || shasum -a 256; } | cut -d' ' -f1)
    perl-free_replace "PLACEHOLDER_$(printf '%s' "$k" | tr '[:lower:]' '[:upper:]')" "$h" \
      "$d/docs/gate-costs.json"
  done < <(bash "$d/scripts/gates.sh" --dry-run ci)

  # Deliberately the real path, spaces included: GUARDED_FILES is not configurable, precisely so a
  # fixture cannot shrink the guarded set and call the result clean.
  cat >"$d/backlog/docs/doc-001 - Asperitas-Project-Plan.md" <<'DOC_END'
The gate set, priced.

The commit tier takes 99 s {{tier:commit}} total, of which alpha is 99 s {{gate:alpha}} and beta is
99 s {{gate:beta}}. The push tier reaches 99 s {{tier:push}} and the ci tier 99 s {{tier:ci}}.

<!-- BEGIN GENERATED: gate-matrix -->
nothing yet
<!-- END GENERATED: gate-matrix -->

Micro-costs: one objcopy runs at 99 ms {{component:objcopy-invocation}}, and the whole ledger was
measured on {{meta:measured}}.
DOC_END

  printf '%s\n' \
    'on: [push]' \
    'jobs:' \
    '  gates:' \
    '    steps:' \
    "      - run: echo \"nix-\${{ hashFiles('flake.lock') }}\"" \
    '        # alpha, {{gate:alpha}}, is the cheapest of these.' \
    >"$d/.github/workflows/ci.yml" || die "could not stage ci.yml"

  printf '%s\n' \
    '# lefthook config' \
    'pre-commit:' \
    '  commands:' \
    '    gates:' \
    '      run: bash scripts/gates.sh commit   # ~99 s {{tier:commit}}' \
    >"$d/lefthook.yml" || die "could not stage lefthook.yml"

  printf '%s\n' \
    '# firmware Makefile' \
    '# The provenance reader costs 99 s {{gate:alpha}}' \
    '# The tool it quotes prints Finished in 0.29s # gate-costs:exempt reason="verbatim cargo output, not a published cost claim"' \
    >"$d/firmware/Makefile" || die "could not stage firmware/Makefile"

  printf '%s\n' '# stub' >"$d/scripts/gates-doc-note.txt"
  printf '%s\n' \
    '# gates.sh stand-in documentation' \
    'keys look like \{{gate:key}} in prose and bind nothing when escaped that way.' \
    >"$d/scripts/elf-provenance.sh" || die "could not stage a guarded file"
  printf '%s\n' '# stub' >"$d/scripts/check-elf-staleness.sh"
  printf '%s\n' '# stub' >"$d/scripts/check-image-load-addresses.sh"

  render_in "$d" >/dev/null || die "could not render the pristine fixture"
}

# sed -i would be one more portability question; a temp file plus rename is four lines and obvious.
perl-free_replace() { # <needle> <replacement> <file>
  local needle=$1 repl=$2 f=$3
  awk -v n="$needle" -v r="$repl" '{
      if (index($0, n) > 0) { out = ""; rest = $0
        while ((p = index(rest, n)) > 0) { out = out substr(rest, 1, p - 1) r; rest = substr(rest, p + length(n)) }
        print out rest
      } else print
    }' "$f" >"$f.tmp" && mv "$f.tmp" "$f" || die "could not substitute $needle in $f"
}

fixture_env() { # <dir> -- prints the assignments a case needs
  printf 'GATE_COSTS_ROOT=%s\nGATE_COSTS_LEDGER=%s/docs/gate-costs.json\nGATE_COSTS_GATES_SH=%s/scripts/gates.sh\n' \
    "$1" "$1" "$1"
}

# Run the shipped script against a fixture and capture BOTH results in globals: RC for the exit code and
# OUT for everything it printed. Every case asserts the code and a phrase of the message, so a case that
# forgot to capture OUT would be asserting a phrase from whichever case ran before it.
run_case() { # <dir> <args...>
  local d=$1
  shift
  OUT=$( cd "$d" && env GATE_COSTS_ROOT="$d" GATE_COSTS_LEDGER="$d/docs/gate-costs.json" \
      GATE_COSTS_GATES_SH="$d/scripts/gates.sh" GATE_COSTS_DEV_SHELL_OK=1 \
      bash "$SELF" "$@" 2>&1 )
  RC=$?
}

render_in() { run_case "$1" --render; return $?; }

# Digest every guarded file, so a case can assert the check leaves the tree alone.
tree_digest() { # <dir>
  local f
  for f in scripts/gates.sh lefthook.yml .github/workflows/ci.yml firmware/Makefile \
           scripts/elf-provenance.sh scripts/check-elf-staleness.sh \
           scripts/check-image-load-addresses.sh 'backlog/docs/doc-001 - Asperitas-Project-Plan.md'; do
    printf '%s %s\n' "$(sha256_of_file "$1/$f")" "$f"
  done | sha256_of_stdin
}

sha256_of_stdin() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum | cut -d' ' -f1
  else shasum -a 256 | cut -d' ' -f1; fi
}

expect() { # <label> <want-rc> <got-rc> [needle-in-output]
  local label=$1 want=$2 got=$3 needle=${4:-}
  CASES_RUN=$((CASES_RUN + 1))
  if [ "$want" != "$got" ]; then
    printf 'FAIL %s: exit %s, wanted %s\n%s\n' "$label" "$got" "$want" "$OUT" >&2
    CASES_FAILED=$((CASES_FAILED + 1))
    return 0
  fi
  if [ -n "$needle" ] && ! printf '%s' "$OUT" | grep -qF -- "$needle"; then
    printf 'FAIL %s: exit %s as asked, but the output never said %s\n%s\n' "$label" "$got" "$needle" \
      "$OUT" >&2
    CASES_FAILED=$((CASES_FAILED + 1))
    return 0
  fi
  printf 'ok   %s\n' "$label"
}

expect_fail_not() { # <label> <rc-must-not-be-this> <got-rc> -- "reported, but not as a crash"
  local label=$1 avoid=$2 got=$3
  CASES_RUN=$((CASES_RUN + 1))
  if [ "$avoid" = "$got" ]; then
    printf 'FAIL %s: exit %s, wanted any other code\n%s\n' "$label" "$got" "$OUT" >&2
    CASES_FAILED=$((CASES_FAILED + 1))
    return 0
  fi
  printf 'ok   %s\n' "$label"
}

# --------------------------------------------------------------------------- the cases
#
# Each is three lines: mutate the pristine tree in exactly one way, run the shipped script, assert the
# exit code AND a phrase of the message. Asserting the phrase matters as much as the code -- sixty
# findings that include the right one by luck would pass a code-only assertion, and the whole value of
# this checker is a message that names the file, the line and the remedy.

case_clean_baseline() {
  local d=$1
  run_case "$d" --check
  expect "clean fixture checks clean" 0 "$RC" "gate-costs: clean"
}

case_stale_value() {
  local d=$1 doc="$1/backlog/docs/doc-001 - Asperitas-Project-Plan.md"
  perl-free_replace '0.89 s {{gate:alpha}}' '0.31 s {{gate:alpha}}' "$doc"
  run_case "$d" --check
  expect "a figure edited by hand inside a citing sentence is reported" 1 "$RC" \
    "renders differently from the file on disk"
  printf '%s\n' "$OUT" | grep -q 'doc-001' || { printf 'FAIL stale value did not name the file\n' >&2; CASES_FAILED=$((CASES_FAILED + 1)); }
  run_case "$d" --render
  expect "and --render repairs it" 0 "$RC" "rewrote"
  run_case "$d" --check
  expect "and --render repairs it" 0 "$RC" "gate-costs: clean"
}

case_unknown_key() {
  local d=$1 doc="$1/backlog/docs/doc-001 - Asperitas-Project-Plan.md"
  printf '%s\n' 'The zeta gate, 1 s {{gate:zeta}}, was never priced.' >>"$doc"
  run_case "$d" --check
  expect "an unknown key stops the check rather than leaving a hole" 2 "$RC" "key gate:zeta is not in the ledger"
  run_case "$d" --render
  expect_fail_not "--render refuses to write anything around an unknown key" 0 "$RC"
  grep -q 'never priced' "$doc" || die "selftest integrity: --render wrote through an unresolvable key"
}

case_suspect_class() {
  local d=$1 doc="$1/backlog/docs/doc-001 - Asperitas-Project-Plan.md"
  printf '%s\n' 'A typo class: 1 s {{gat:alpha}}.' >>"$doc"
  run_case "$d" --check
  expect "a token shaped like a key but naming no such class exits 2" 2 "$RC" "names no such class"
}

case_dead_entry() {
  local d=$1
  jq '.gates += [{"banner":"=== ghost ===","command_sha256":"deadbeef","cost":{"commit":0.5},
                  "key":"ghost","min_tier":"commit"}]' "$d/docs/gate-costs.json" >"$d/l2" \
    && mv "$d/l2" "$d/docs/gate-costs.json"
  run_case "$d" --check
  expect "a ledger entry for a gate that no longer exists is fiction" 1 "$RC" "still prices 'ghost'"
}

case_gate_added_unpriced() {
  local d=$1
  fixture_gates_sh "$d/scripts/gates.sh" 'g push epsilon "=== epsilon ===" "cargo build -p epsilon"'
  run_case "$d" --check
  expect "adding a gate without pricing it goes red" 1 "$RC" "live gate 'epsilon' (min_tier push) has no entry"
}

case_gate_removed() {
  local d=$1
  # Drop delta from the list while its figure stays in the ledger: the mirror image of the case above.
  perl-free_replace 'g ci     delta "=== delta ===" "cargo test -p delta --features hw"' '' \
    "$d/scripts/gates.sh"
  run_case "$d" --check
  expect "deleting a gate and leaving its figure goes red too" 1 "$RC" "still prices 'delta'"
}

case_digest_changed() {
  local d=$1
  # A retier or a reworded banner must NOT invalidate a measurement; only the command may do that.
  perl-free_replace 'cargo clippy -p beta' 'cargo clippy -p beta --all-targets' "$d/scripts/gates.sh"
  run_case "$d" --check
  expect "a gate that runs a different command invalidates its published cost" 1 "$RC" \
    "now runs a different command"
}

case_banner_edit_keeps_cost() {
  local d=$1
  perl-free_replace '"=== beta ==="' '"=== beta, reworded for readability ==="' "$d/scripts/gates.sh"
  # The matrix region prints banners, so a reword legitimately changes what the generator renders. What
  # must NOT change is the measurement beside it: --render fixes the sentence, and no cost is
  # invalidated on the way.
  run_case "$d" --render
  run_case "$d" --check
  expect_fail_not "a reworded banner is not a changed command" 2 "$RC"
  expect "rewording a banner keeps the measurement" 0 "$RC" "gate-costs: clean"
}

case_literal_duration() {
  local d=$1 doc="$1/backlog/docs/doc-001 - Asperitas-Project-Plan.md"
  printf '%s\n' 'The gamma gate wants 2.4 s of your commit budget.' >>"$doc"
  run_case "$d" --check
  expect "a hand-typed duration outside a generated region is a finding" 1 "$RC" \
    "wall-clock duration literal"
}

case_exemption() {
  local d=$1 mk="$1/firmware/Makefile"
  run_case "$d" --check
  expect "a reasoned exemption silences the rule it covers" 0 "$RC" "1 reasoned duration exemption"
  # Same figure, same marker, reason gutted: the marker alone must not be enough to hide a number.
  # Deleting the marker instead would only test the literal rule again, which the case above does.
  perl-free_replace 'reason="verbatim cargo output, not a published cost claim"' 'reason="why"' "$mk"
  run_case "$d" --check
  expect "an exemption with no reason is itself a finding" 1 "$RC" "carries gate-costs:exempt but no reason"
}

case_dead_exemption() {
  local d=$1 mk="$1/firmware/Makefile"
  printf '%s\n' '# nothing timed here # gate-costs:exempt reason="left over from a deleted line"' >>"$mk"
  run_case "$d" --check
  expect "an exemption that outlived its duration is a finding" 1 "$RC" "dead gate-costs:exempt"
}

case_mismatched_markers() {
  local d=$1 doc="$1/backlog/docs/doc-001 - Asperitas-Project-Plan.md"
  perl-free_replace '<!-- END GENERATED: gate-matrix -->' '' "$doc"
  run_case "$d" --check
  expect "a BEGIN with no END stops the check" 2 "$RC" "has no matching END"
  build_fixture "$d"
  perl-free_replace '<!-- BEGIN GENERATED: gate-matrix -->' '' "$doc"
  run_case "$d" --check
  expect "an END with no BEGIN stops the check" 2 "$RC" "no matching BEGIN"
}

case_unknown_generator() {
  local d=$1 doc="$1/backlog/docs/doc-001 - Asperitas-Project-Plan.md"
  perl-free_replace 'BEGIN GENERATED: gate-matrix' 'BEGIN GENERATED: tier-table' "$doc"
  run_case "$d" --check
  expect "a region naming a generator this script does not have stops the check" 2 "$RC" \
    "no generator named tier-table"
}

case_generated_region_is_owned() {
  local d=$1 doc="$1/backlog/docs/doc-001 - Asperitas-Project-Plan.md"
  grep -q 'cargo test -p gamma\|gamma' "$doc" || { printf 'FAIL generated region holds no gate rows\n' >&2; CASES_FAILED=$((CASES_FAILED+1)); }
  CASES_RUN=$((CASES_RUN + 1))
  if grep -c 'UNPRICED' "$doc" | grep -qv '^0$'; then
    printf 'FAIL the matrix printed UNPRICED for a priced gate\n' >&2; CASES_FAILED=$((CASES_FAILED + 1))
  else
    printf 'ok   the generated matrix joins ledger costs into the gate table\n'
  fi
}

case_matrix_cell_is_not_a_finding() {
  local d=$1 doc="$1/backlog/docs/doc-001 - Asperitas-Project-Plan.md"
  # Every cell of the cost column is a duration. Inside a generated region they must read as owned.
  run_case "$d" --check
  expect "figures inside a generated region are the generator's, not findings" 0 "$RC" "gate-costs: clean"
}

case_idempotent_render() {
  local d=$1 before after
  before=$(tree_digest "$d")
  run_case "$d" --render
  expect "rendering a rendered tree writes nothing" 0 "$RC" "already rendered"
  after=$(tree_digest "$d")
  CASES_RUN=$((CASES_RUN + 1))
  if [ "$before" != "$after" ]; then
    printf 'FAIL --render moved bytes on a tree it had already rendered\n%s\n%s\n' "$before" "$after" >&2
    CASES_FAILED=$((CASES_FAILED + 1))
  else
    printf 'ok   --render is idempotent at the byte level\n'
  fi
}

case_check_never_writes() {
  local d=$1 before after
  # The point of the whole design: a hook that rewrote a tracked file would commit the stale version
  # silently, and this repo has no dirty-tree discipline to catch that.
  perl-free_replace '0.89 s {{gate:alpha}}' '0.31 s {{gate:alpha}}' \
    "$d/backlog/docs/doc-001 - Asperitas-Project-Plan.md"
  before=$(tree_digest "$d")
  run_case "$d" --check
  expect_fail_not "a stale tree still exits nonzero" 0 "$RC"
  after=$(tree_digest "$d")
  CASES_RUN=$((CASES_RUN + 1))
  if [ "$before" != "$after" ]; then
    printf 'FAIL --check WROTE to a guarded file\n' >&2; CASES_FAILED=$((CASES_FAILED + 1))
  else
    printf 'ok   --check leaves every guarded file byte-identical, findings and all\n'
  fi
}

case_clean_tree_is_byte_identical() {
  local d=$1 before after
  before=$(tree_digest "$d")
  run_case "$d" --check
  expect "clean tree, clean exit" 0 "$RC" "gate-costs: clean"
  after=$(tree_digest "$d")
  CASES_RUN=$((CASES_RUN + 1))
  if [ "$before" != "$after" ]; then
    printf 'FAIL --check moved bytes on a clean tree\n' >&2; CASES_FAILED=$((CASES_FAILED + 1))
  else
    printf 'ok   every guarded file byte-identical before and after a clean --check\n'
  fi
}

case_foreign_braces_pass_through() {
  local d=$1 yml="$d/.github/workflows/ci.yml"
  run_case "$d" --render
  CASES_RUN=$((CASES_RUN + 1))
  if grep -qF "hashFiles('flake.lock')" "$yml"; then
    printf 'ok   a GitHub Actions expression inside a guarded file is left alone\n'
  else
    printf 'FAIL the renderer ate a ${{ hashFiles(...) }} expression\n' >&2
    CASES_FAILED=$((CASES_FAILED + 1))
  fi
}

case_escaped_token_binds_nothing() {
  local d=$1 f="$d/scripts/elf-provenance.sh"
  CASES_RUN=$((CASES_RUN + 1))
  if grep -qF '\{{gate:key}}' "$f" && ! grep -qF '0.89 s {{gate:key}}' "$f"; then
    printf 'ok   an escaped token documents the syntax and binds nothing\n'
  else
    printf 'FAIL an escaped token got substituted anyway: %s\n' "$(grep -n 'gate:key' "$f")" >&2
    CASES_FAILED=$((CASES_FAILED + 1))
  fi
}

case_uncited_component() {
  local d=$1
  jq '.components += [{"display":"~2 ms per fork","key":"fork","measured_utc":"2026-09-13",
                      "method":"timed loop of a command substitution"}]' "$d/docs/gate-costs.json" \
    >"$d/l2" && mv "$d/l2" "$d/docs/gate-costs.json"
  run_case "$d" --check
  expect "a component nobody cites is a figure nobody publishes" 1 "$RC" "publishes component 'fork'"
}

case_tier_count_disagrees() {
  local d=$1
  jq '.tiers.commit.gates = 9' "$d/docs/gate-costs.json" >"$d/l2" && mv "$d/l2" "$d/docs/gate-costs.json"
  run_case "$d" --check
  expect "the ledger cannot claim a different gate count than the list" 1 "$RC" "counts"
}

case_retier_disagrees() {
  local d=$1
  jq '(.gates[] | select(.key == "gamma") | .min_tier) = "ci"' "$d/docs/gate-costs.json" >"$d/l2" \
    && mv "$d/l2" "$d/docs/gate-costs.json"
  run_case "$d" --check
  expect "a retier that moves only one side goes red" 1 "$RC" "is min_tier push in"
}

case_missing_ledger() {
  local d=$1
  rm "$d/docs/gate-costs.json"
  run_case "$d" --check
  expect "no ledger means no verdict" 2 "$RC" "no docs/gate-costs.json"
}

case_malformed_ledger() {
  local d=$1
  printf '{ "schema": 1, "gates": [\n' >"$d/docs/gate-costs.json"
  run_case "$d" --check
  expect "a truncated ledger exits 2 rather than reporting zero findings" 2 "$RC" "is not a schema-1 cost ledger"
}

case_missing_generator() {
  local d=$1
  rm "$d/scripts/gates.sh"
  run_case "$d" --check
  expect "no gate list means no verdict" 2 "$RC" "The gate list is what the ledger prices"
}

case_list_and_dry_run_disagree() {
  local d=$1
  # A gate visible to one machine-read interface and hidden from the other: refusing beats guessing.
  perl-free_replace 'dry)  printf' 'dry)  [[ $key == beta ]] && return 0; printf' "$d/scripts/gates.sh"
  run_case "$d" --check
  expect "one gate visible to --list and another to --dry-run refuses to price either" 2 "$RC" \
    "Refusing to price one list"
}

case_record_is_reproducible() {
  local d=$1 a b
  ( cd "$d" && env GATE_COSTS_ROOT="$d" GATE_COSTS_LEDGER="$d/docs/gate-costs.json" \
      GATE_COSTS_GATES_SH="$d/scripts/gates.sh" GATE_COSTS_DEV_SHELL_OK=1 \
      GATE_COSTS_MEASURED_UTC=2026-09-14T12:00:00Z bash "$SELF" --record ) >"$d/rec1.log" 2>&1
  RC=$?
  OUT=$(cat "$d/rec1.log")
  expect "--record measures each tier once and writes the ledger" 0 "$RC" "gates priced"
  cp "$d/docs/gate-costs.json" "$d/rec1.json"
  ( cd "$d" && env GATE_COSTS_ROOT="$d" GATE_COSTS_LEDGER="$d/docs/gate-costs.json" \
      GATE_COSTS_GATES_SH="$d/scripts/gates.sh" GATE_COSTS_DEV_SHELL_OK=1 \
      GATE_COSTS_MEASURED_UTC=2026-09-14T12:00:00Z bash "$SELF" --record ) >"$d/rec2.log" 2>&1
  CASES_RUN=$((CASES_RUN + 1))
  if cmp -s "$d/rec1.json" "$d/docs/gate-costs.json"; then
    printf 'ok   two records of the same run are byte-identical, so a non-empty diff always means a real change\n'
  else
    printf 'FAIL --record is not byte-stable under a pinned clock\n' >&2
    diff "$d/rec1.json" "$d/docs/gate-costs.json" | head -20 >&2
    CASES_FAILED=$((CASES_FAILED + 1))
  fi
  CASES_RUN=$((CASES_RUN + 1))
  if jq -e '.components | length == 1' "$d/docs/gate-costs.json" >/dev/null; then
    printf 'ok   --record carries the hand-authored components forward instead of dropping them\n'
  else
    printf 'FAIL --record lost the components array\n' >&2; CASES_FAILED=$((CASES_FAILED + 1))
  fi
  CASES_RUN=$((CASES_RUN + 1))
  if jq -e '[.gates[].cost | has("commit")] | length == 4' "$d/docs/gate-costs.json" >/dev/null \
     && jq -e '.gates[] | select(.key=="delta") | .cost | has("commit") | not' "$d/docs/gate-costs.json" >/dev/null; then
    printf 'ok   a cost is recorded per paying tier, and a ci-only gate has no commit-tier figure\n'
  else
    printf 'FAIL the cost map is not keyed by paying tier\n%s\n' "$(jq -c '.gates[] | {key, cost}' "$d/docs/gate-costs.json")" >&2
    CASES_FAILED=$((CASES_FAILED + 1))
  fi
  # A record moves the figures, so the prose that cites them is stale the moment it finishes -- that is
  # the whole reason --refresh exists as one command rather than two that people forget to chain.
  run_case "$d" --check
  expect "a fresh record leaves the prose citing figures the old ledger held" 1 "$RC" "renders differently"
  run_case "$d" --render
  run_case "$d" --check
  expect "and rendering once makes a freshly recorded tree clean" 0 "$RC" "gate-costs: clean"
}

# The regression this exists for: `nix develop` exports SOURCE_DATE_EPOCH=315532800, so honoring the
# standard pin meant every ledger written from the shell that --record requires claimed 1980-01-01.
# Provenance that is wrong on every single run is worse than no provenance field at all, because it looks
# checked. A record taken with the epoch exported anyway must still carry the real clock.
case_ambient_epoch_ignored() {
  local d=$1 stamp
  rm -f "$d/docs/gate-costs.json"
  ( cd "$d" && env GATE_COSTS_ROOT="$d" GATE_COSTS_LEDGER="$d/docs/gate-costs.json" \
      GATE_COSTS_GATES_SH="$d/scripts/gates.sh" GATE_COSTS_DEV_SHELL_OK=1 \
      SOURCE_DATE_EPOCH=315532800 bash "$SELF" --record ) >"$d/epoch.log" 2>&1
  RC=$?
  OUT=$(cat "$d/epoch.log")
  expect "--record runs with SOURCE_DATE_EPOCH exported" 0 "$RC" "gates priced"
  stamp=$(jq -r .measured_utc "$d/docs/gate-costs.json")
  CASES_RUN=$((CASES_RUN + 1))
  if [ "${stamp#1970-}" = "$stamp" ] && [ "${stamp#1980-}" = "$stamp" ]; then
    printf 'ok   an ambient SOURCE_DATE_EPOCH does not date the ledger to the epoch (%s)\n' "$stamp"
  else
    printf 'FAIL SOURCE_DATE_EPOCH leaked into measured_utc: %s\n' "$stamp" >&2
    CASES_FAILED=$((CASES_FAILED + 1))
  fi
  # The explicit pin still wins, which is what keeps --record byte-reproducible for --selftest.
  ( cd "$d" && env GATE_COSTS_ROOT="$d" GATE_COSTS_LEDGER="$d/docs/gate-costs.json" \
      GATE_COSTS_GATES_SH="$d/scripts/gates.sh" GATE_COSTS_DEV_SHELL_OK=1 \
      SOURCE_DATE_EPOCH=315532800 GATE_COSTS_MEASURED_UTC=2026-09-14T12:00:00Z \
      bash "$SELF" --record ) >"$d/pin.log" 2>&1
  CASES_RUN=$((CASES_RUN + 1))
  if [ "$(jq -r .measured_utc "$d/docs/gate-costs.json")" = "2026-09-14T12:00:00Z" ]; then
    printf 'ok   and the explicit pin beats both the ambient epoch and the clock\n'
  else
    printf 'FAIL GATE_COSTS_MEASURED_UTC did not pin measured_utc: %s\n' \
      "$(jq -r .measured_utc "$d/docs/gate-costs.json")" >&2
    CASES_FAILED=$((CASES_FAILED + 1))
  fi
}

case_bad_stamp_shape() {
  local d=$1
  ( cd "$d" && env GATE_COSTS_ROOT="$d" GATE_COSTS_LEDGER="$d/docs/gate-costs.json" \
      GATE_COSTS_GATES_SH="$d/scripts/gates.sh" GATE_COSTS_DEV_SHELL_OK=1 \
      GATE_COSTS_MEASURED_UTC="last tuesday" bash "$SELF" --record ) >"$d/badstamp.log" 2>&1
  RC=$?
  OUT=$(cat "$d/badstamp.log")
  expect "a pin that is not shaped like a timestamp exits 2 rather than writing folklore into the ledger" 2 "$RC" \
    "not a UTC timestamp"
}

case_record_refuses_outside_dev_shell() {
  local d=$1
  ( cd "$d" && env GATE_COSTS_ROOT="$d" GATE_COSTS_LEDGER="$d/docs/gate-costs.json" \
      GATE_COSTS_GATES_SH="$d/scripts/gates.sh" GATE_COSTS_DEV_SHELL_OK=0 \
      bash "$SELF" --record ) >"$d/refuse.log" 2>&1
  RC=$?
  OUT=$(cat "$d/refuse.log")
  expect "--record refuses outside the dev shell with the remedy on one line" 2 "$RC" \
    "nix develop .#default"
}

case_refresh_runs_both() {
  local d=$1
  ( cd "$d" && env GATE_COSTS_ROOT="$d" GATE_COSTS_LEDGER="$d/docs/gate-costs.json" \
      GATE_COSTS_GATES_SH="$d/scripts/gates.sh" GATE_COSTS_DEV_SHELL_OK=1 \
      GATE_COSTS_MEASURED_UTC=2026-09-14T12:00:00Z bash "$SELF" --refresh ) >"$d/refresh.log" 2>&1
  RC=$?
  OUT=$(cat "$d/refresh.log")
  expect "--refresh records then renders" 0 "$RC" "refreshed"
}

case_repeat_median() {
  local d=$1
  ( cd "$d" && env GATE_COSTS_ROOT="$d" GATE_COSTS_LEDGER="$d/docs/gate-costs.json" \
      GATE_COSTS_GATES_SH="$d/scripts/gates.sh" GATE_COSTS_DEV_SHELL_OK=1 \
      GATE_COSTS_MEASURED_UTC=2026-09-14T12:00:00Z bash "$SELF" --record --repeat 3 ) >"$d/repeat.log" 2>&1
  RC=$?
  OUT=$(cat "$d/repeat.log")
  expect "--repeat N samples per tier" 0 "$RC" ""
  CASES_RUN=$((CASES_RUN + 1))
  if jq -e '.samples_per_tier == 3 and (.approximation | test("median of 3"))' "$d/docs/gate-costs.json" >/dev/null; then
    printf 'ok   --repeat records the sample count and says the figure is a median\n'
  else
    printf 'FAIL --repeat did not record how many samples it took\n%s\n' "$(jq -c '{samples_per_tier, approximation}' "$d/docs/gate-costs.json")" >&2
    CASES_FAILED=$((CASES_FAILED + 1))
  fi
}

case_bad_usage() {
  local d=$1
  run_case "$d" --frobnicate
  expect "an unknown mode is usage, not silence" 2 "$RC" "usage: scripts/gate-costs.sh"
  run_case "$d"
  expect "no mode at all is usage too" 2 "$RC" "usage: scripts/gate-costs.sh"
}

cmd_selftest() {
  require_tools
  FIX=$(mktemp -d) || die "mktemp -d failed"
  if [ -n "${GATE_COSTS_KEEP_FIXTURES:-}" ]; then
    # A debugging seam, and the only honest way to look at a failing case: the fixtures are deleted by
    # the trap otherwise, so a red case leaves nothing behind but its own message.
    printf 'fixtures kept under %s\n' "$FIX"
  else
    trap 'if [ -n "${FIX:-}" ]; then rm -rf "$FIX"; fi' EXIT
  fi

  local cases=(clean_baseline stale_value unknown_key suspect_class dead_entry gate_added_unpriced
               gate_removed digest_changed banner_edit_keeps_cost literal_duration exemption
               dead_exemption mismatched_markers unknown_generator generated_region_is_owned
               matrix_cell_is_not_a_finding idempotent_render check_never_writes
               clean_tree_is_byte_identical foreign_braces_pass_through escaped_token_binds_nothing
               uncited_component tier_count_disagrees retier_disagrees missing_ledger
               malformed_ledger missing_generator list_and_dry_run_disagree record_is_reproducible
               ambient_epoch_ignored bad_stamp_shape
               record_refuses_outside_dev_shell refresh_runs_both repeat_median bad_usage)
  local c n=0
  for c in "${cases[@]}"; do
    n=$((n + 1))
    local d=$FIX/case-$n
    mkdir -p "$d" || die "could not stage a fixture directory"
    build_fixture "$d"
    printf '%s\n' "--- $c"
    "case_$c" "$d"
  done

  printf '\n%s: %d case(s), %d failure(s)\n' "$PROG" "$CASES_RUN" "$CASES_FAILED"
  [ "$CASES_FAILED" -eq 0 ] || return 1
  return 0
}

# --------------------------------------------------------------------------- dispatch

make_workdir() {
  WORK=$(mktemp -d) || die "mktemp -d failed, so there is nowhere to put a rendered temp copy"
  trap 'if [ -n "${WORK:-}" ]; then rm -rf "$WORK"; fi' EXIT
  mkdir -p "$WORK/gen" "$WORK/cmds"

  # Every derived path is set HERE rather than at file scope, because at file scope WORK is still empty
  # and each of these would name a file at / -- the failure being that the script cannot write its own
  # temp files and says so as a parse error three functions later.
  LIST_TSV="$WORK/list.tsv"              # key <TAB> min <TAB> banner   (--list order; counts row last)
  DRY_TSV="$WORK/dry.tsv"                # key <TAB> min <TAB> command
  DIGESTS="$WORK/digests.tsv"            # key <TAB> sha256(command)
  LEDGER_GATES="$WORK/ledger-gates.tsv"  # key <TAB> min_tier <TAB> digest <TAB> cost-at-min-tier
  LEDGER_TIERS="$WORK/ledger-tiers.tsv"  # tier <TAB> seconds <TAB> gate-count
  LEDGER_COMPS="$WORK/ledger-comps.tsv"  # key <TAB> display
  CITED_TOKENS="$WORK/cited.txt"         # every class-this-script-owns token found in guarded prose
}

MODE=''
SAMPLES=1
while [ $# -gt 0 ]; do
  case $1 in
    --check)   MODE=check ;;
    --render)  MODE=render ;;
    --record)  MODE=record ;;
    --refresh) MODE=refresh ;;
    --selftest) MODE=selftest ;;
    --repeat)
      shift
      [ $# -gt 0 ] || die "--repeat wants a number"
      case $1 in
        ''|*[!0-9]*) die "--repeat wants a positive integer, got: $1" ;;
      esac
      [ "$1" -ge 1 ] || die "--repeat wants at least 1 sample"
      SAMPLES=$1
      ;;
    -h|--help) usage; exit 0 ;;
    *) printf '%s\n' "$PROG: unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done
[ -n "$MODE" ] || { printf '%s\n' "$PROG: pick a mode" >&2; usage >&2; exit 2; }

case $MODE in
  selftest) cmd_selftest; exit $? ;;
  check)    make_workdir; cmd_check; exit $? ;;
  render)   make_workdir; cmd_render; exit $? ;;
  # --record writes the ledger and --refresh renders from it, so both re-enter prepare() internally:
  # the input the renderer reads must be the ledger this run just produced, not the copy on disk before
  # it. cmd_refresh is therefore record-then-render rather than one function that measures and writes.
  record)   make_workdir; cmd_record; exit $? ;;
  refresh)  make_workdir; cmd_refresh; exit $? ;;
esac
