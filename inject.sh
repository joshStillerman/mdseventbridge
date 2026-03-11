#!/usr/bin/env bash
set -euo pipefail

SIDE="${1:?usage: inject.sh SIDE EVENT MSG}"
EVENT="${2:?usage: inject.sh SIDE EVENT MSG}"
MSG="${3:?usage: inject.sh SIDE EVENT MSG}"

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

echo "[$SIDE] Emitting event '$EVENT' with data: $MSG on ${mdsevent_address}:${mdsevent_port}"
setevent "$EVENT" "$MSG"

