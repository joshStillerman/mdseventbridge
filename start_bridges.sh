#!/usr/bin/env bash
set -euo pipefail

PYTHON="${PYTHON:-python3}"
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
stdbuf -oL -eL "$BRIDGE" \
  --site-id AC \
  --udp-address-setting 239.10.10.10 \
  --udp-port 4001 \
  --pub-bind tcp://127.0.0.1:5600 \
  --sub-connect tcp://127.0.0.1:5601 \
  --log-level DEBUG \
  > logs/bridge_AC.log 2>&1 &
echo $! > pids/bridge_AC.pid

# BD side bridge
stdbuf -oL -eL "$BRIDGE" \
  --site-id BD \
  --udp-address-setting 239.10.10.11 \
  --udp-port 4002 \
  --pub-bind tcp://127.0.0.1:5601 \
  --sub-connect tcp://127.0.0.1:5600 \
  --log-level DEBUG \
  > logs/bridge_BD.log 2>&1 &
echo $! > pids/bridge_BD.pid

sleep 1

echo "Started bridges:"
echo "  AC bridge pid $(cat pids/bridge_AC.pid)"
echo "  BD bridge pid $(cat pids/bridge_BD.pid)"
echo
echo "Logs:"
echo "  logs/bridge_AC.log"
echo "  logs/bridge_BD.log"
