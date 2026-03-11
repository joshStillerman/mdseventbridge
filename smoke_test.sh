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

wait_for() {
  local file="$1"
  local text="$2"
  local timeout="${3:-5}"
  for _ in $(seq 1 $((timeout * 10))); do
    if grep -Fq "$text" "$file"; then
      return 0
    fi
    sleep 0.1
  done
  return 1
}

ensure_listener_ready() {
  local side="$1"
  local pid_file="pids/listener_${side}.pid"
  local pid=""

  if [ -e "$pid_file" ]; then
    pid="$(cat "$pid_file" 2>/dev/null || true)"
  fi

  if [ -z "${pid:-}" ] || ! kill -0 "$pid" 2>/dev/null; then
    echo "Listener $side not running; restarting..."
    start_listener "$side"
  fi

  if ! wait_for "logs/${side}.log" "[$side] listening for '$EVENT'" 5; then
    echo "Listener $side did not become ready in time" >&2
    exit 1
  fi
}

echo "Starting listeners..."
start_listener A
start_listener B
start_listener C
start_listener D
ensure_listener_ready A
ensure_listener_ready B
ensure_listener_ready C
ensure_listener_ready D

echo
echo "Injecting from A..."
MSG_A="msg-from-A-$(date +%s%N)"
./inject.sh A "$EVENT" "$MSG_A" >/dev/null
sleep 1

echo "Re-waiting listeners before B injection..."
ensure_listener_ready A
ensure_listener_ready B
ensure_listener_ready C
ensure_listener_ready D

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
