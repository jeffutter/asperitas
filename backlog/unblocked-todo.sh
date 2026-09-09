#!/usr/bin/env bash
# Thin delegation to ralph's deployed copy of this script. There is no logic here on purpose.
#
# Two loops read this repo's backlog and must agree on what "ready to pick up" means:
#   - the pi extension (~/.pi/agent/extensions/ralph/index.ts), which resolves the deployed
#     script by absolute path out of $HOME
#   - .claude/workflows/ralph-backlog-loop.js, which invokes ./backlog/unblocked-todo.sh
# For a while they did not agree. This copy got the archive/ ID-collision ordering fix (bc5fe61)
# while the deployed one went on shadowing live tasks with archived stubs, and the assignee
# filter later landed on one side first too. The same rules implemented twice drift; only one
# of the two copies gets noticed when that happens.
#
# Semantics therefore live in exactly one file:
#   ~/.config/home-manager/modules/home/languages/ai/pi-extensions/ralph/unblocked-todo.sh
# deployed by ai.nix to ~/.pi/agent/extensions/ralph/unblocked-todo.sh. It lists tasks in a
# status whose dependencies are all Done, holds back containers with an unfinished child, and
# splits the result by owner: `--assignee agent|human|all`, default `agent`.
#
# Usage: unblocked-todo.sh [status] [--assignee agent|human|all]
set -euo pipefail

# The deployed script cds into a *relative* `backlog/`, so anchor on this file's own repo root
# rather than trusting the caller's cwd. Both known call sites run it from the root already;
# this keeps it working from anywhere, as the previous self-contained copy did.
cd "$(dirname "${BASH_SOURCE[0]}")/.."

target="${HOME}/.pi/agent/extensions/ralph/unblocked-todo.sh"
if [ ! -x "$target" ]; then
  echo "backlog/unblocked-todo.sh: $target is missing or not executable." >&2
  echo "Edit the source at ~/.config/home-manager/modules/home/languages/ai/pi-extensions/ralph/unblocked-todo.sh and run ~/bin/rebuild." >&2
  exit 1
fi

exec "$target" "$@"
