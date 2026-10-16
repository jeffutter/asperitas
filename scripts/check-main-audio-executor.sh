#!/usr/bin/env bash
#
# Guard for TASK-076: main's audio callback must run on its own interrupt executor, never on the
# thread executor that also polls the USB console.
#
# Why: on the thread executor the SAI callback shared its 1.3 ms deadline with the knob task's blocking
# ADC reads and usb::run(). main lost that race at boot (AUDIO_EXIT == 3, Overrun), the audio loop
# ended, and the halt that followed left usb::run() unpolled, so the host's SETUP packets went
# unanswered and the console never enumerated in the bootloaded layout (DSTS SUSPSTS=1, core not
# faulted, LED_STATE Running). It is a source-level check because enumeration needs a board; the
# on-board proof is `asperitas-rig query` against a bootloaded main.
#
# Asserts on firmware/src/bin/main.rs:
#   1. an InterruptExecutor is declared and started on interrupt::SAI1;
#   2. a `#[interrupt] fn SAI1` vector polls it;
#   3. `start_callback` appears only inside `async fn audio_task`, which is spawned on that executor
#      (not on the thread spawner).
#
# Exit codes: 0 pass, 1 a rule failed, 2 the script could not run.

set -uo pipefail
ROOT=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
SRC=${MAIN_RS:-$ROOT/firmware/src/bin/main.rs}
[[ -r $SRC ]] || { echo "check-main-audio-executor: cannot read $SRC" >&2; exit 2; }

fail=0
need() { # <description> <fixed-or-regex pattern>
  if ! grep -Eq -- "$2" "$SRC"; then
    echo "check-main-audio-executor: FAIL: $1 (pattern: $2)" >&2
    fail=1
  fi
}

need "an InterruptExecutor static for audio" 'static AUDIO_EXECUTOR: InterruptExecutor'
need "the audio executor is started on SAI1" 'AUDIO_EXECUTOR\.start\(interrupt::SAI1\)'
need "the SAI1 vector polls the audio executor" 'AUDIO_EXECUTOR\.on_interrupt\(\)'
need "audio_task is spawned on the interrupt executor's spawner" 'audio_spawner\.spawn\(\s*$|audio_spawner\.spawn\('
if grep -Eq 'spawner\.spawn\(\s*audio_task|[^_]spawner\.spawn\(audio_task' "$SRC"; then
  echo "check-main-audio-executor: FAIL: audio_task spawned on the thread spawner" >&2
  fail=1
fi

# start_callback outside audio_task: track the enclosing `fn` by the last line that starts one.
stray=$(awk '
  /^[[:space:]]*(pub )?(async )?fn / { cur = $0 }
  /start_callback\(/ && !/^[[:space:]]*\/\// && cur !~ /async fn audio_task/ { print NR": "$0 }
' "$SRC")
if [[ -n $stray ]]; then
  echo "check-main-audio-executor: FAIL: start_callback outside audio_task:" >&2
  echo "$stray" >&2
  fail=1
fi

if (( fail )); then exit 1; fi
echo "check-main-audio-executor: ok (main's audio callback is on its own interrupt executor)"
