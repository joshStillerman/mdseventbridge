use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use tracing::{debug, info};
use uuid::Uuid;

use crate::config::BridgeConfig;
use crate::dedupe::SeenCache;
use crate::multicast::MulticastConfig;
use crate::overlay::{publish, try_recv, OverlayMeta, TOPIC_PREFIX};
use crate::wire::decode_udp_event;

pub struct Bridge {
    cfg: BridgeConfig,
    mcast_cfg: MulticastConfig,
    seen: SeenCache,
    udp_recv: UdpSocket,
    udp_send: UdpSocket,
    bridge_send_port: u16,
    known_bridge_sender_ports: HashSet<u16>,
    _ctx: zmq::Context,
    pub_sock: zmq::Socket,
    sub_sock: zmq::Socket,
}

impl Bridge {
    pub fn new(cfg: BridgeConfig) -> Result<Self> {
        let mcast_cfg = MulticastConfig::parse(&cfg.udp_address_setting)?;
        let udp_recv = create_udp_receiver(&cfg, &mcast_cfg)?;
        let udp_send = create_udp_sender(&cfg)?;
        let bridge_send_port = udp_send.local_addr()?.port();

        let ctx = zmq::Context::new();
        let pub_sock = ctx.socket(zmq::PUB)?;
        pub_sock.bind(&cfg.pub_bind)?;

        let sub_sock = ctx.socket(zmq::SUB)?;
        sub_sock.set_subscribe(TOPIC_PREFIX.as_bytes())?;
        for endpoint in &cfg.sub_connect {
            sub_sock.connect(endpoint)?;
        }

        info!(
            site_id = cfg.site_id,
            bridge_id = cfg.bridge_id,
            udp_port = cfg.udp_port,
            pub_bind = cfg.pub_bind,
            ?cfg.sub_connect,
            "bridge initialized"
        );

        let mut known_bridge_sender_ports = HashSet::new();
        known_bridge_sender_ports.insert(bridge_send_port);

        Ok(Self {
            seen: SeenCache::new(
                Duration::from_secs(cfg.seen_ttl_seconds),
                cfg.seen_max_entries,
            ),
            cfg,
            mcast_cfg,
            udp_recv,
            udp_send,
            bridge_send_port,
            known_bridge_sender_ports,
            _ctx: ctx,
            pub_sock,
            sub_sock,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let shutdown = Arc::new(AtomicBool::new(false));
        {
            let flag = shutdown.clone();
            ctrlc::set_handler(move || {
                flag.store(true, Ordering::SeqCst);
            })?;
        }

        let poll_sleep = Duration::from_millis(self.cfg.poll_ms.max(1));
        while !shutdown.load(Ordering::SeqCst) {
            self.drain_udp()?;
            self.drain_overlay()?;
            thread::sleep(poll_sleep);
        }
        info!("shutdown requested");
        Ok(())
    }

    fn drain_udp(&mut self) -> Result<()> {
        let now = Instant::now();
        let mut buf = [0_u8; 65535];
        loop {
            let (size, src) = match self.udp_recv.recv_from(&mut buf) {
                Ok(v) => v,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
                Err(err) => return Err(err).context("udp recv failed"),
            };

            let src_port = src.port();
            if self.known_bridge_sender_ports.contains(&src_port) {
                debug!(src = %src, "drop udp from known bridge sender port");
                continue;
            }

            let datagram = &buf[..size];
            let decoded = match decode_udp_event(datagram) {
                Ok(value) => value,
                Err(err) => {
                    debug!(src = %src, error = %err, "drop invalid udp datagram");
                    continue;
                }
            };

            let event_id = format!("{}:{}", self.cfg.site_id, Uuid::new_v4().simple());
            let meta = OverlayMeta {
                id: event_id.clone(),
                origin: self.cfg.site_id.clone(),
                sender: self.cfg.bridge_id.clone(),
                sender_udp_port: self.bridge_send_port,
                hops: 0,
                event: decoded.event_name.clone(),
                ts: unix_ts_secs(),
                via: None,
            };

            let _ = self.seen.add_if_new(&event_id, now);
            publish(&self.pub_sock, &meta, datagram)?;
            debug!(event = meta.event, id = meta.id, "udp->tcp");
        }
    }

    fn drain_overlay(&mut self) -> Result<()> {
        let now = Instant::now();
        loop {
            let recv = match try_recv(&self.sub_sock) {
                Ok(v) => v,
                Err(err) => {
                    debug!(error = %err, "drop malformed overlay message");
                    continue;
                }
            };
            let Some((mut meta, datagram)) = recv else {
                return Ok(());
            };

            self.known_bridge_sender_ports.insert(meta.sender_udp_port);
            if !self.seen.add_if_new(&meta.id, now) {
                continue;
            }

            // Never process or relay events this process sent itself.
            if meta.sender == self.cfg.bridge_id {
                continue;
            }

            let target_ip = self.mcast_cfg.event_to_multicast(&meta.event);
            let target = SocketAddrV4::new(target_ip, self.cfg.udp_port);
            self.udp_send.send_to(&datagram, target)?;
            debug!(event = meta.event, id = meta.id, %target, "tcp->udp");

            if self.cfg.forward_remote && meta.hops < self.cfg.max_hops {
                meta.hops += 1;
                meta.via = Some(self.cfg.site_id.clone());
                meta.sender = self.cfg.bridge_id.clone();
                meta.sender_udp_port = self.bridge_send_port;
                publish(&self.pub_sock, &meta, &datagram)?;
            }
        }
    }
}

fn create_udp_receiver(cfg: &BridgeConfig, mcast_cfg: &MulticastConfig) -> Result<UdpSocket> {
    let bind_ip = Ipv4Addr::from_str(&cfg.udp_bind_ip)
        .with_context(|| format!("invalid udp_bind_ip: {}", cfg.udp_bind_ip))?;

    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;

    #[cfg(unix)]
    {
        let _ = socket.set_reuse_port(true);
    }

    let addr = SocketAddr::V4(SocketAddrV4::new(bind_ip, cfg.udp_port));
    socket.bind(&addr.into())?;

    let udp = UdpSocket::from(socket);
    udp.set_nonblocking(true)?;

    let interface = cfg
        .udp_interface_ip
        .as_deref()
        .map(Ipv4Addr::from_str)
        .transpose()
        .map_err(|_| anyhow!("invalid udp_interface_ip"))?
        .unwrap_or(Ipv4Addr::UNSPECIFIED);

    for idx in mcast_cfg.range_start..=mcast_cfg.range_end {
        let group = mcast_cfg.ip_for_index(idx);
        udp.join_multicast_v4(&group, &interface)?;
    }

    Ok(udp)
}

fn create_udp_sender(cfg: &BridgeConfig) -> Result<UdpSocket> {
    let bind_ip = Ipv4Addr::from_str(&cfg.udp_send_bind_ip)
        .with_context(|| format!("invalid udp_send_bind_ip: {}", cfg.udp_send_bind_ip))?;

    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    {
        let _ = socket.set_reuse_port(true);
    }

    let addr = SocketAddr::V4(SocketAddrV4::new(bind_ip, 0));
    socket.bind(&addr.into())?;

    let udp = UdpSocket::from(socket);
    udp.set_nonblocking(true)?;
    udp.set_multicast_ttl_v4(cfg.udp_ttl)?;
    udp.set_multicast_loop_v4(cfg.udp_loop)?;

    Ok(udp)
}

fn unix_ts_secs() -> f64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(dur) => dur.as_secs_f64(),
        Err(_) => {
            debug!("system time before unix epoch, clamping to 0.0");
            0.0
        }
    }
}
