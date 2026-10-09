#!/usr/bin/env bash
# Guards that entering the dev shell prints nothing on stdout.
#
# Swatinem/rust-cache runs `nix develop .#default --command rustc -vV` (cmd-format) and parses
# stdout. Any line the flake shellHook prints there (lefthook install's "sync hooks: ..." did) is
# prepended to rustc's output, the action falls through to `rustup run <first token>`, and the
# cache silently never engages (TASK-064.03). shellHook output belongs on stderr.
set -euo pipefail
cd "$(dirname "$0")/.."

out=$(nix develop .#default --command true 2>/dev/null)
if [[ -n $out ]]; then
  printf 'check-devshell-stdout: dev shell entry wrote to stdout (send shellHook output to stderr):\n%s\n' "$out" >&2
  exit 1
fi
