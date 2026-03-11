# mdsevent_tcp_bridge (Rust scaffold)

Rust scaffold for a compiled UDP/TCP/UDP bridge equivalent to the Python prototype.

## Modules

- `src/config.rs`: CLI/env config
- `src/wire.rs`: MDS UDP wire format parser/encoder
- `src/multicast.rs`: `mdsevent_address` parsing + event->multicast mapping
- `src/overlay.rs`: ZeroMQ overlay metadata and frame helpers
- `src/dedupe.rs`: TTL + bounded dedupe cache
- `src/bridge.rs`: main bridge loop with sender-port loop suppression
- `src/bin/inproc_harness.rs`: no-UDP-send overlay harness that prints
  `sending udp message - with details`

## Build

```bash
cd testing/mdsevent_tcp_bridge_rust
cargo check
```

## Run bridge

```bash
cargo run --bin mdsevent_tcp_bridge -- \
  --site-id AC \
  --bridge-id AC-1 \
  --udp-port 4000 \
  --udp-address-setting 239.10.10.10 \
  --pub-bind tcp://0.0.0.0:5600 \
  --sub-connect tcp://127.0.0.1:5601
```

## Run harness

```bash
cargo run --bin inproc_harness -- --transport inproc --messages 3
```

For local TCP harness run:

```bash
cargo run --bin inproc_harness -- --transport tcp --base-port 5760 --messages 3
```
