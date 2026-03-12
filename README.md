# MDSplus UDP/TCP Event Bridge Lab

This directory is a runnable local lab for forwarding MDSplus UDP events across a ZeroMQ TCP overlay and rebroadcasting them on the remote UDP multicast side.

## What Is Here

- `mdsevent_tcp_bridge.py`: Python bridge implementation.
- `mdsevent_tcp_bridge`: compiled bridge executable (Mach-O arm64 in this workspace).
- `start_bridges.sh`: starts two bridge instances (`AC` and `BD`) and writes logs/PIDs.
- `listener.sh`: listens for one event on side `A|B|C|D` using `wfevent`.
- `inject.sh`: injects one event on side `A|B|C|D` using `setevent`.
- `smoke_test.sh`: starts four listeners, injects from A and B, and tails/records output.
- `stop_lab.sh`: stops bridge/listener processes from `pids/*.pid`.
- `rust/`: Rust source crate for the bridge and harness/tests.

## Topology Used By Scripts

- A/C side multicast: `239.10.10.10:4001`
- B/D side multicast: `239.10.10.11:4002`
- AC bridge:
  - `--pub-bind tcp://127.0.0.1:5600`
  - `--sub-connect tcp://127.0.0.1:5601`
- BD bridge:
  - `--pub-bind tcp://127.0.0.1:5601`
  - `--sub-connect tcp://127.0.0.1:5600`

## Requirements

- MDSplus CLI tools on `PATH`: `setevent`, `wfevent`
- For Python bridge: Python 3 + `pyzmq`
- For Rust development: Rust toolchain (`cargo`)

## Quick Start

1. Start the two bridge processes:

```bash
./start_bridges.sh
```

2. Run the smoke test (starts listeners, injects events):

```bash
./smoke_test.sh
```

3. Stop all lab processes:

```bash
./stop_lab.sh
```

## Use Rust Binary Instead of Python Script

`start_bridges.sh` defaults to `./mdsevent_tcp_bridge.py`.  
To use the compiled executable:

```bash
BRIDGE=./mdsevent_tcp_bridge ./start_bridges.sh
```

Do not run the binary with `python3`; execute it directly.

## Logs and PIDs

- Bridge logs:
  - `logs/bridge_AC.log`
  - `logs/bridge_BD.log`
- Listener logs (smoke test):
  - `logs/A.log`, `logs/B.log`, `logs/C.log`, `logs/D.log`
- PID files:
  - `pids/*.pid`

## Rust Development

```bash
cd rust
cargo check
cargo test
```
