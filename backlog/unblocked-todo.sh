#!/usr/bin/env bash
# List tasks in a given status (default "To Do") whose dependencies (if any) are all Done.
# Usage: unblocked-todo.sh [status]
# e.g. `unblocked-todo.sh Blocked` finds tasks parked in "Blocked" status whose dependencies
# have since completed — status doesn't update itself when a blocker ships.
set -euo pipefail

# Locate the backlog directory rather than assuming it is the script's own, so this keeps
# working if the script is ever relocated (it has been, twice). Walking up from the
# script's own location — not $PWD — also makes it independent of the caller's working
# directory, which matters because both Ralph loops invoke it from the repo root.
#
# Failing loudly matters here: when the globs below match nothing this script exits 0 with
# no output, which is indistinguishable to the caller from "every ticket is blocked" — a
# silent, total stall of the loop. A missing backlog must be an error, not empty output.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
backlog_dir=""
dir="$script_dir"
while :; do
  if [ -f "$dir/backlog/config.yml" ]; then
    backlog_dir="$dir/backlog"
    break
  fi
  [ "$dir" = "/" ] && break
  dir="$(dirname "$dir")"
done

if [ -z "$backlog_dir" ]; then
  echo "unblocked-todo.sh: no backlog/config.yml found in any parent of $script_dir" >&2
  exit 1
fi
cd "$backlog_dir"

if [ ! -d tasks ]; then
  echo "unblocked-todo.sh: $backlog_dir/tasks does not exist" >&2
  exit 1
fi

TARGET_STATUS="${1:-To Do}"

frontmatter() {
  awk '/^---$/{c++; next} c==1' "$1"
}

# Least authoritative directory first, most authoritative last, because assignment
# overwrites on ID collision and the live task must win.
#
# IDs are not guaranteed unique across these directories: archiving a task keeps its
# ID and its status verbatim, so an archived stub can share an ID with a real task.
# Observed live — an abandoned scratch task sat in archive/ as "To Do" under the same
# ID as a completed task, and being read last it overwrote the real "Done". Everything
# depending on that ID looked permanently blocked, so the loop quietly starved with no
# error anywhere. `backlog doctor` cannot catch this: it only scans active and
# completed tasks, not archive/.
declare -A status_of
for f in archive/tasks/*.md completed/*.md tasks/*.md; do
  [ -f "$f" ] || continue
  fm=$(frontmatter "$f")
  id=$(printf '%s\n' "$fm" | yq -r '.id')
  st=$(printf '%s\n' "$fm" | yq -r '.status')
  status_of["$id"]="$st"
done

for f in tasks/*.md; do
  [ -f "$f" ] || continue
  fm=$(frontmatter "$f")
  st=$(printf '%s\n' "$fm" | yq -r '.status')
  [ "$st" = "$TARGET_STATUS" ] || continue

  id=$(printf '%s\n' "$fm" | yq -r '.id')
  title=$(printf '%s\n' "$fm" | yq -r '.title')
  mapfile -t deps < <(printf '%s\n' "$fm" | yq -o=json '.dependencies // []' | jq -r '.[]')

  blocked=false
  for d in "${deps[@]:-}"; do
    [ -z "$d" ] && continue
    if [ "${status_of[$d]:-MISSING}" != "Done" ]; then
      blocked=true
      break
    fi
  done

  if [ "$blocked" = false ]; then
    echo "$id - $title"
  fi
done
