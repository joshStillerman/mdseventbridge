# MDSplus UDP/TCP/UDP Event Bridge

This repository contains bridge tooling for forwarding MDSplus UDP events across TCP and rebroadcasting them as UDP at remote destinations.

Primary bridge assets are under `testing/`:
- `testing/mdsevent_tcp_bridge.py` (Python bridge)
- `testing/mdsevent_tcp_overlay_harness.py` (Python no-UDP-send harness)
- `testing/mdsevent_tcp_bridge_rust/` (Rust compiled implementation + harness)

## Bridge Behavior

Each bridge process can do both directions:
1. `UDP -> TCP overlay publish`
2. `TCP overlay subscribe -> UDP rebroadcast`

Overlay transport: ZeroMQ PUB/SUB over TCP.

## Quick Start (Python)

Run from repo root (`/Users/jas/mdsplus/mdsshr`):

### Hub
```bash
python3 testing/mdsevent_tcp_bridge.py \
  --site-id hub \
  --bridge-id hub-1 \
  --udp-port 4000 \
  --udp-address-setting 239.10.10.10 \
  --pub-bind tcp://0.0.0.0:5600 \
  --sub-connect tcp://spoke1-host:5600 \
  --sub-connect tcp://spoke2-host:5600 \
  --forward-remote \
  --log-level DEBUG
```

### Spoke
```bash
python3 testing/mdsevent_tcp_bridge.py \
  --site-id spoke1 \
  --bridge-id spoke1-1 \
  --udp-port 4000 \
  --udp-address-setting 239.10.10.10 \
  --pub-bind tcp://0.0.0.0:5600 \
  --sub-connect tcp://hub-host:5600 \
  --log-level DEBUG
```

## One-Node Multi-Port Test

Use unique values per instance:
- `--udp-port`
- `--pub-bind`
- `--bridge-id`

```bash
# AC
python3 testing/mdsevent_tcp_bridge.py \
  --site-id AC \
  --bridge-id AC-1 \
  --udp-port 4000 \
  --pub-bind tcp://127.0.0.1:5600 \
  --sub-connect tcp://127.0.0.1:5601 \
  --forward-remote \
  --log-level DEBUG

# BD
python3 testing/mdsevent_tcp_bridge.py \
  --site-id BD \
  --bridge-id BD-1 \
  --udp-port 4001 \
  --pub-bind tcp://127.0.0.1:5601 \
  --sub-connect tcp://127.0.0.1:5600 \
  --forward-remote \
  --log-level DEBUG
```

## Harness (No UDP Send)

Python:
```bash
python3 testing/mdsevent_tcp_overlay_harness.py --transport inproc --messages 3
```

Rust:
```bash
cd testing/mdsevent_tcp_bridge_rust
cargo run --bin inproc_harness -- --transport inproc --messages 3
```

## Rust Build/Test

```bash
cd testing/mdsevent_tcp_bridge_rust
cargo check
cargo test
```

## Loop Prevention

Overlay metadata fields:
- `id`
- `origin`
- `sender`
- `sender_udp_port`
- `hops`

Current protections:
- dedupe cache by `id`
- ignore frames where `sender == this bridge_id`
- drop UDP ingress from known bridge sender ports
- optional hop limit (`--max-hops`)

## Troubleshooting

### `SyntaxError: Non-UTF-8 code ...`

This means a compiled binary is being launched with Python.

Wrong:
```bash
python3 /path/to/mdsevent_tcp_bridge
```

Right:
```bash
/path/to/mdsevent_tcp_bridge
```

Or run the Python script explicitly:
```bash
python3 testing/mdsevent_tcp_bridge.py
```

## More Details

See:
- `testing/README.md`
- `docs/udp_tcp_bridge_design.md`
