#!/usr/bin/env bash
set -euo pipefail

SIDE="${1:?usage: listener.sh SIDE EVENT}"
EVENT="${2:?usage: listener.sh SIDE EVENT}"

case "$SIDE" in
  A|C)
    export mdsevent_address="239.10.10.10"
    export mdsevent_port="4001"
    ;;
  B|D)
    export mdsevent_address="239.10.10.11"
    export mdsevent_port="4002"
    ;;
  *)
    echo "SIDE must be one of A B C D" >&2
    exit 1
    ;;
esac

echo "[$SIDE] listening for '$EVENT' on ${mdsevent_address}:${mdsevent_port}"
while IFS= read -r event_data; do
  echo "[$SIDE] GOT '$EVENT': $event_data"
done < <(wfevent "$EVENT" -d)
