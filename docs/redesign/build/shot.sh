#!/usr/bin/env bash
# Serialized screenshot: parallel UI packages share ui/dist and desktop/out, so builds and launches take turns.
# Usage: docs/redesign/build/shot.sh <screenshot.mjs options...>   (same options; see screenshot.mjs's header)
# Holds a mkdir lock for the whole build + launch; waits up to 20 minutes for another agent's turn.
set -uo pipefail
cd "$(dirname "$0")/../../.."
lock=target/screenshot.lock
for _ in $(seq 1200); do
  if mkdir "$lock" 2>/dev/null; then
    trap 'rmdir "$lock"' EXIT
    node docs/redesign/build/screenshot.mjs "$@"
    exit $?
  fi
  # A lock older than 15 minutes belongs to a dead run.
  if [ -n "$(find "$lock" -maxdepth 0 -mmin +15 2>/dev/null)" ]; then rmdir "$lock" 2>/dev/null; fi
  sleep 1
done
echo "screenshot lock busy for 20 minutes" >&2
exit 1
