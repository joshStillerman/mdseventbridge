#!/usr/bin/env python3
"""
Prototype bridge between MDSplus UDP events and a TCP overlay.

Local path:
  UDP multicast -> ZeroMQ PUB (TCP)
Remote path:
  ZeroMQ SUB (TCP) -> UDP multicast rebroadcast

The UDP payload is kept byte-for-byte compatible with current UdpEvents.c:
  [name_len: u32 be][event_name bytes][buf_len: u32 be][payload bytes]
"""

from __future__ import annotations

import argparse
import json
import logging
import os
import re
import signal
import socket
import struct
import time
import uuid
from collections import OrderedDict
from dataclasses import dataclass
from typing import Optional

import zmq


DEFAULT_UDP_PORT = 4000
DEFAULT_UDP_ADDRESS = "224.0.0.175"
TOPIC_PREFIX = "mdsevent."


@dataclass(frozen=True)
class MulticastConfig:
    address_format: str
    range_start: int
    range_end: int


class SeenCache:
    def __init__(self, ttl_seconds: float, max_entries: int) -> None:
        self.ttl_seconds = ttl_seconds
        self.max_entries = max_entries
        self._seen: OrderedDict[str, float] = OrderedDict()

    def _prune(self, now: float) -> None:
        expire_before = now - self.ttl_seconds
        while self._seen:
            first_key = next(iter(self._seen))
            if self._seen[first_key] >= expire_before:
                break
            self._seen.popitem(last=False)
        while len(self._seen) > self.max_entries:
            self._seen.popitem(last=False)

    def add_if_new(self, event_id: str, now: float) -> bool:
        self._prune(now)
        if event_id in self._seen:
            self._seen.move_to_end(event_id, last=True)
            self._seen[event_id] = now
            return False
        self._seen[event_id] = now
        return True


def parse_mds_address(setting: Optional[str]) -> MulticastConfig:
    if not setting:
        return MulticastConfig("224.0.0.%d", 175, 175)
    lower = setting.strip().lower()
    if lower == "compat":
        return MulticastConfig("225.0.0.%d", 0, 255)
    match = re.fullmatch(
        r"\s*(\d+)\.(\d+)\.(\d+)\.(\d+)(?:-(\d+))?\s*", setting
    )
    if not match:
        raise ValueError(
            "Invalid mdsevent_address format. Use 'compat' or n.n.n.n[-n]."
        )
    p1, p2, p3, p4, p5 = (int(x) if x is not None else None for x in match.groups())
    if any(part < 0 or part > 255 for part in (p1, p2, p3, p4)):
        raise ValueError("Each IPv4 octet must be in [0,255].")
    end = p4 if p5 is None else p5
    if end < p4 or end > 255:
        raise ValueError("Address range end must satisfy start <= end <= 255.")
    return MulticastConfig(f"{p1}.{p2}.{p3}.%d", p4, end)


def parse_udp_port(value: Optional[str]) -> int:
    if not value:
        return DEFAULT_UDP_PORT
    try:
        port = int(value, 10)
    except ValueError:
        try:
            port = socket.getservbyname(value, "udp")
        except OSError as exc:
            raise ValueError(
                f"Unsupported mdsevent_port '{value}'. Use integer port or UDP service name."
            ) from exc
    if port < 1 or port > 65535:
        raise ValueError("UDP port must be in [1,65535].")
    return port


def event_to_multicast(event_name: str, cfg: MulticastConfig) -> str:
    # Keep compatible with C implementation hash style.
    hval = sum(event_name.encode("utf-8", errors="ignore"))
    span = cfg.range_end - cfg.range_start + 1
    mapped = int(cfg.range_start + ((hval % 256) / 256.0) * span)
    mapped = max(cfg.range_start, min(cfg.range_end, mapped))
    return cfg.address_format % mapped


def decode_udp_event(datagram: bytes) -> Optional[str]:
    if len(datagram) < 8:
        return None
    (name_len,) = struct.unpack("!I", datagram[:4])
    if name_len > 65535:
        return None
    after_name = 4 + name_len
    if len(datagram) < after_name + 4:
        return None
    name_bytes = datagram[4:after_name]
    (buf_len,) = struct.unpack("!I", datagram[after_name : after_name + 4])
    expected = after_name + 4 + buf_len
    if len(datagram) != expected:
        return None
    return name_bytes.decode("utf-8", errors="replace")


class MdseventTcpBridge:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.site_id = args.site_id
        self.bridge_id = args.bridge_id or f"{socket.gethostname()}:{os.getpid()}"
        self.mcast_cfg = parse_mds_address(args.udp_address_setting)
        self.udp_port = args.udp_port
        self.seen = SeenCache(args.seen_ttl_seconds, args.seen_max_entries)
        self._stop = False

        self.udp_recv_sock = self._create_udp_receiver()
        self.udp_send_sock = self._create_udp_sender()
        self.bridge_send_port = self.udp_send_sock.getsockname()[1]
        self.known_bridge_sender_ports: set[int] = {self.bridge_send_port}

        self.ctx = zmq.Context.instance()
        self.pub = self.ctx.socket(zmq.PUB)
        self.pub.bind(args.pub_bind)
        self.sub = self.ctx.socket(zmq.SUB)
        self.sub.setsockopt_string(zmq.SUBSCRIBE, TOPIC_PREFIX)
        for endpoint in args.sub_connect:
            self.sub.connect(endpoint)

        logging.info(
            "Bridge site_id=%s bridge_id=%s udp_port=%d mcast=%s[%d-%d] pub_bind=%s sub_connect=%s",
            self.site_id,
            self.bridge_id,
            self.udp_port,
            self.mcast_cfg.address_format,
            self.mcast_cfg.range_start,
            self.mcast_cfg.range_end,
            args.pub_bind,
            args.sub_connect,
        )

    def _create_udp_receiver(self) -> socket.socket:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        if hasattr(socket, "SO_REUSEPORT"):
            try:
                sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
            except OSError:
                pass
        sock.bind((self.args.udp_bind_ip, self.udp_port))
        iface = (
            socket.inet_aton(self.args.udp_interface_ip)
            if self.args.udp_interface_ip
            else socket.inet_aton("0.0.0.0")
        )
        for i in range(self.mcast_cfg.range_start, self.mcast_cfg.range_end + 1):
            group = self.mcast_cfg.address_format % i
            mreq = socket.inet_aton(group) + iface
            sock.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP, mreq)
        sock.setblocking(False)
        return sock

    def _create_udp_sender(self) -> socket.socket:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
        sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_TTL, self.args.udp_ttl)
        sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_LOOP, self.args.udp_loop)
        if self.args.udp_interface_ip:
            sock.setsockopt(
                socket.IPPROTO_IP,
                socket.IP_MULTICAST_IF,
                socket.inet_aton(self.args.udp_interface_ip),
            )
        sock.bind((self.args.udp_send_bind_ip, 0))
        sock.setblocking(False)
        return sock

    def _publish_overlay(self, meta: dict, udp_datagram: bytes) -> None:
        out_meta = dict(meta)
        out_meta["sender"] = self.bridge_id
        out_meta["sender_udp_port"] = self.bridge_send_port
        topic = f"{TOPIC_PREFIX}{out_meta['event']}".encode("utf-8", errors="replace")
        meta_raw = json.dumps(out_meta, separators=(",", ":")).encode("utf-8")
        self.pub.send_multipart([topic, meta_raw, udp_datagram])

    def _rebroadcast_udp(self, event_name: str, datagram: bytes) -> None:
        target_ip = event_to_multicast(event_name, self.mcast_cfg)
        self.udp_send_sock.sendto(datagram, (target_ip, self.udp_port))
        logging.debug("rebroadcast udp event=%s group=%s", event_name, target_ip)

    def _drain_udp(self) -> None:
        now = time.monotonic()
        while True:
            try:
                datagram, src = self.udp_recv_sock.recvfrom(65535)
            except BlockingIOError:
                return
            src_ip, src_port = src[0], src[1]
            # Ignore any UDP datagram emitted by a known bridge sender socket.
            # This prevents tcp->udp rebroadcast packets from being re-published.
            if src_port in self.known_bridge_sender_ports:
                logging.debug(
                    "dropping udp from known bridge sender port src=%s:%d",
                    src_ip,
                    src_port,
                )
                continue
            event_name = decode_udp_event(datagram)
            if event_name is None:
                logging.debug("dropping invalid udp datagram len=%d from=%s", len(datagram), src)
                continue
            event_id = f"{self.site_id}:{uuid.uuid4().hex}"
            meta = {
                "id": event_id,
                "origin": self.site_id,
                "hops": 0,
                "event": event_name,
                "ts": time.time(),
            }
            self.seen.add_if_new(event_id, now)
            self._publish_overlay(meta, datagram)
            logging.debug("udp->tcp event=%s id=%s", event_name, event_id)

    def _drain_overlay(self) -> None:
        now = time.monotonic()
        while True:
            try:
                parts = self.sub.recv_multipart(flags=zmq.NOBLOCK)
            except zmq.Again:
                return
            if len(parts) != 3:
                logging.debug("dropping overlay message with %d frames", len(parts))
                continue
            _, meta_raw, datagram = parts
            try:
                meta = json.loads(meta_raw.decode("utf-8"))
                event_id = meta["id"]
                event_name = meta["event"]
                origin = meta["origin"]
                sender = meta.get("sender")
                sender_udp_port = meta.get("sender_udp_port")
                hops = int(meta.get("hops", 0))
            except (KeyError, ValueError, TypeError, json.JSONDecodeError):
                logging.debug("dropping malformed overlay metadata")
                continue
            if isinstance(sender_udp_port, int) and 1 <= sender_udp_port <= 65535:
                self.known_bridge_sender_ports.add(sender_udp_port)
            if not self.seen.add_if_new(event_id, now):
                continue

            # Never process or relay events this process sent itself.
            if sender == self.bridge_id:
                continue

            self._rebroadcast_udp(event_name, datagram)
            logging.debug("tcp->udp event=%s id=%s", event_name, event_id)

            if (
                self.args.forward_remote
                and hops < self.args.max_hops
            ):
                meta["hops"] = hops + 1
                meta["via"] = self.site_id
                self._publish_overlay(meta, datagram)

    def run(self) -> None:
        def _stop_handler(signum: int, _frame: object) -> None:
            logging.info("received signal=%d, shutting down", signum)
            self._stop = True

        signal.signal(signal.SIGINT, _stop_handler)
        signal.signal(signal.SIGTERM, _stop_handler)

        sleep_s = max(0.001, self.args.poll_ms / 1000.0)
        while not self._stop:
            self._drain_udp()
            self._drain_overlay()
            time.sleep(sleep_s)

    def close(self) -> None:
        try:
            self.udp_recv_sock.close()
        except OSError:
            pass
        try:
            self.udp_send_sock.close()
        except OSError:
            pass
        try:
            self.sub.close(0)
        except Exception:  # noqa: BLE001
            pass
        try:
            self.pub.close(0)
        except Exception:  # noqa: BLE001
            pass
        try:
            self.ctx.term()
        except Exception:  # noqa: BLE001
            pass


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Bridge MDSplus UDP events to/from ZeroMQ TCP."
    )
    parser.add_argument(
        "--site-id",
        default=socket.gethostname(),
        help="Unique bridge/site identifier used for loop suppression.",
    )
    parser.add_argument(
        "--bridge-id",
        default=None,
        help="Unique sender id for this process instance (defaults to hostname:pid).",
    )
    parser.add_argument(
        "--udp-port",
        type=int,
        default=parse_udp_port(os.getenv("mdsevent_port")),
        help="UDP multicast port used by mdsevent (default: env mdsevent_port or 4000).",
    )
    parser.add_argument(
        "--udp-address-setting",
        default=os.getenv("mdsevent_address", DEFAULT_UDP_ADDRESS),
        help="Address mode compatible with mdsevent_address: compat or n.n.n.n[-n].",
    )
    parser.add_argument(
        "--udp-bind-ip",
        default="0.0.0.0",
        help="Local IP for UDP receive bind.",
    )
    parser.add_argument(
        "--udp-send-bind-ip",
        default="0.0.0.0",
        help="Local IP for UDP send socket bind (source IP/port).",
    )
    parser.add_argument(
        "--udp-interface-ip",
        default=os.getenv("mdsevent_interface"),
        help="IPv4 of multicast interface for join/send.",
    )
    parser.add_argument(
        "--udp-ttl",
        type=int,
        default=int(os.getenv("mdsevent_ttl", "1")),
        help="UDP multicast TTL for rebroadcast.",
    )
    parser.add_argument(
        "--udp-loop",
        type=int,
        choices=(0, 1),
        default=int(os.getenv("mdsevent_loop", "1")),
        help="Set IP_MULTICAST_LOOP on send socket.",
    )
    parser.add_argument(
        "--pub-bind",
        default="tcp://0.0.0.0:5600",
        help="ZeroMQ PUB endpoint to bind (outbound overlay stream).",
    )
    parser.add_argument(
        "--sub-connect",
        action="append",
        default=[],
        help="ZeroMQ PUB endpoint to connect SUB socket to. Repeat for multiple destinations.",
    )
    parser.add_argument(
        "--forward-remote",
        action=argparse.BooleanOptionalAction,
        default=False,
        help="Republish remote overlay events to other destinations (hub behavior).",
    )
    parser.add_argument(
        "--max-hops",
        type=int,
        default=4,
        help="Maximum overlay hop count when forwarding remote events.",
    )
    parser.add_argument(
        "--seen-ttl-seconds",
        type=float,
        default=60.0,
        help="Seconds to retain seen event IDs for dedupe.",
    )
    parser.add_argument(
        "--seen-max-entries",
        type=int,
        default=200000,
        help="Maximum remembered event IDs for dedupe.",
    )
    parser.add_argument(
        "--poll-ms",
        type=int,
        default=10,
        help="Main-loop poll sleep in milliseconds.",
    )
    parser.add_argument(
        "--log-level",
        default="INFO",
        choices=("DEBUG", "INFO", "WARNING", "ERROR"),
        help="Logging level.",
    )
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    logging.basicConfig(
        level=getattr(logging, args.log_level, logging.INFO),
        format="%(asctime)s %(levelname)s %(message)s",
    )
    bridge = MdseventTcpBridge(args)
    try:
        bridge.run()
        return 0
    finally:
        bridge.close()


if __name__ == "__main__":
    raise SystemExit(main())
