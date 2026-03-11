#!/usr/bin/env bash
set -euo pipefail

EVENT="${EVENT:-TEST_EVENT}"
mkdir -p logs pids

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing command: $1" >&2
    exit 1
  }
}

need_cmd setevent
need_cmd stdbuf
need_cmd tee
need_cmd grep

cleanup() {
  for f in pids/listener_*.pid; do
    [ -e "$f" ] || continue
    pid="$(cat "$f" 2>/dev/null || true)"
    if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" || true
    fi
    rm -f "$f"
  done
}
#trap cleanup EXIT INT TERM

start_listener() {
  local side="$1"
  : > "logs/${side}.log"
  stdbuf -oL -eL ./listener.sh "$side" "$EVENT" | tee -a "logs/${side}.log" &
  echo $! > "pids/listener_${side}.pid"
}

echo "Starting listeners..."
start_listener A
start_listener B
start_listener C
start_listener D
sleep 1

echo
echo "Injecting from A..."
MSG_A="msg-from-A-$(date +%s%N)"
./inject.sh A "$EVENT" "$MSG_A" >/dev/null
sleep 1

echo "Injecting from B..."
MSG_B="msg-from-B-$(date +%s%N)"
./inject.sh B "$EVENT" "$MSG_B" >/dev/null
sleep 1

echo
echo "Check logs:"
echo "  logs/A.log"
echo "  logs/B.log"
echo "  logs/C.log"
echo "  logs/D.log"
echo
echo "Bridge logs:"
echo "  logs/bridge_AC.log"
echo "  logs/bridge_BD.log"
echo
echo "Expected with the bridge running:"
echo "  A-side event appears in A.log and C.log locally, then is bridged to B.log and D.log"
echo "  B-side event appears in B.log and D.log locally, then is bridged to A.log and C.log"
echo
echo "Press Ctrl-C to stop listeners, or inspect logs in another shell."
wait
