#!/usr/bin/env bash
set -euo pipefail

if [ -n "${PYTHON:-}" ]; then
  PYTHON="$PYTHON"
elif [ -x ".venv/bin/python" ]; then
  PYTHON=".venv/bin/python"
else
  PYTHON="python3"
fi
BRIDGE="${BRIDGE:-./mdsevent_tcp_bridge.py}"

mkdir -p logs pids

# Clean out stale pid files if the processes are gone
for f in pids/*.pid; do
  [ -e "$f" ] || continue
  pid="$(cat "$f" 2>/dev/null || true)"
  if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
    echo "Bridge already running from $f (pid $pid)" >&2
    exit 1
  else
    rm -f "$f"
  fi
done

# AC side bridge
stdbuf -oL -eL  "$BRIDGE" \
  --site-id AC \
  --udp-address-setting 239.10.10.10 \
  --udp-port 4001 \
  --pub-bind tcp://127.0.0.1:5600 \
  --sub-connect tcp://127.0.0.1:5601 \
  --log-level DEBUG \
  > >(sed -u 's/^/[AC] /' > logs/bridge_AC.log) 2>&1 &
echo $! > pids/bridge_AC.pid

# BD side bridge
stdbuf -oL -eL "$BRIDGE" \
  --site-id BD \
  --udp-address-setting 239.10.10.11 \
  --udp-port 4002 \
  --pub-bind tcp://127.0.0.1:5601 \
  --sub-connect tcp://127.0.0.1:5600 \
  --log-level DEBUG \
  > >(sed -u 's/^/[BD] /' > logs/bridge_BD.log) 2>&1 &
echo $! > pids/bridge_BD.pid

sleep 1

check_bridge() {
  local pid_file="$1"
  local log_file="$2"
  local name="$3"
  local pid
  pid="$(cat "$pid_file" 2>/dev/null || true)"
  if [ -z "${pid:-}" ] || ! kill -0 "$pid" 2>/dev/null; then
    echo "$name failed to start; recent log output:" >&2
    tail -n 40 "$log_file" >&2 || true
    exit 1
  fi
}

check_bridge pids/bridge_AC.pid logs/bridge_AC.log "AC bridge"
check_bridge pids/bridge_BD.pid logs/bridge_BD.log "BD bridge"

echo "Started bridges:"
echo "  AC bridge pid $(cat pids/bridge_AC.pid)"
echo "  BD bridge pid $(cat pids/bridge_BD.pid)"
echo
echo "Logs:"
echo "  logs/bridge_AC.log"
echo "  logs/bridge_BD.log"
