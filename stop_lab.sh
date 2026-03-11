#!/usr/bin/env bash
set -euo pipefail

for f in pids/*.pid; do
  [ -e "$f" ] || continue
  pid="$(cat "$f" 2>/dev/null || true)"
  if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
    kill "$pid" || true
    echo "Stopped $f (pid $pid)"
  fi
  rm -f "$f"
done
