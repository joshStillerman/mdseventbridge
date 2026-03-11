use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use clap::Parser;
use mdsevent_tcp_bridge::dedupe::SeenCache;
use mdsevent_tcp_bridge::multicast::MulticastConfig;
use mdsevent_tcp_bridge::overlay::{publish, try_recv, OverlayMeta, TOPIC_PREFIX};
use mdsevent_tcp_bridge::wire::encode_udp_event;
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(about = "In-process overlay harness for mdsevent bridge")]
struct Args {
    #[arg(long, default_value = "inproc")]
    transport: String,
    #[arg(long, default_value_t = 5760)]
    base_port: u16,
    #[arg(long, default_value_t = 4000)]
    udp_port: u16,
    #[arg(long, default_value = "224.0.0.175")]
    address: String,
    #[arg(long, default_value = "TEST_EVENT")]
    event: String,
    #[arg(long, default_value = "test-payload")]
    payload: String,
    #[arg(long, default_value_t = 2)]
    messages: usize,
    #[arg(long, default_value_t = 120)]
    delay_ms: u64,
}

struct Node {
    site_id: String,
    bridge_id: String,
    forward_remote: bool,
    udp_port: u16,
    bridge_send_port: u16,
    mcast_cfg: MulticastConfig,
    seen: SeenCache,
    pub_sock: zmq::Socket,
    sub_sock: zmq::Socket,
}

impl Node {
    fn new(
        ctx: &zmq::Context,
        site_id: &str,
        pub_bind: &str,
        sub_connect: &[String],
        forward_remote: bool,
        udp_port: u16,
        mcast_cfg: &MulticastConfig,
        bridge_send_port: u16,
    ) -> Result<Self> {
        let pub_sock = ctx.socket(zmq::PUB)?;
        pub_sock.bind(pub_bind)?;
        let sub_sock = ctx.socket(zmq::SUB)?;
        sub_sock.set_subscribe(TOPIC_PREFIX.as_bytes())?;
        for endpoint in sub_connect {
            sub_sock.connect(endpoint)?;
        }

        Ok(Self {
            site_id: site_id.to_string(),
            bridge_id: format!("{site_id}:{}", Uuid::new_v4().simple()),
            forward_remote,
            udp_port,
            bridge_send_port,
            mcast_cfg: mcast_cfg.clone(),
            seen: SeenCache::new(Duration::from_secs(60), 100_000),
            pub_sock,
            sub_sock,
        })
    }

    fn publish_local(&mut self, event_name: &str, payload: &[u8]) -> Result<()> {
        let event_id = format!("{}:{}", self.site_id, Uuid::new_v4().simple());
        let datagram = encode_udp_event(event_name, payload);
        let meta = OverlayMeta {
            id: event_id.clone(),
            origin: self.site_id.clone(),
            sender: self.bridge_id.clone(),
            sender_udp_port: self.bridge_send_port,
            hops: 0,
            event: event_name.to_string(),
            ts: unix_ts_secs(),
            via: None,
        };
        let _ = self.seen.add_if_new(&event_id, Instant::now());
        publish(&self.pub_sock, &meta, &datagram)?;
        println!(
            "[{}] local event published over tcp: event={} payload_len={} id={}",
            self.site_id,
            event_name,
            payload.len(),
            event_id
        );
        Ok(())
    }

    fn pump_once(&mut self) -> Result<()> {
        let Some((mut meta, datagram)) = try_recv(&self.sub_sock)? else {
            return Ok(());
        };

        if !self.seen.add_if_new(&meta.id, Instant::now()) {
            return Ok(());
        }

        if meta.sender == self.bridge_id {
            return Ok(());
        }

        let target = self.mcast_cfg.event_to_multicast(&meta.event);
        let payload_len = parse_payload_len(&datagram)?;
        println!(
            "[{}] sending udp message - with details: event={} target={}:{} payload_len={} origin={} id={} hops={}",
            self.site_id,
            meta.event,
            target,
            self.udp_port,
            payload_len,
            meta.origin,
            meta.id,
            meta.hops
        );

        if self.forward_remote && meta.hops < 4 {
            meta.hops += 1;
            meta.via = Some(self.site_id.clone());
            meta.sender = self.bridge_id.clone();
            meta.sender_udp_port = self.bridge_send_port;
            publish(&self.pub_sock, &meta, &datagram)?;
        }

        Ok(())
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mcast_cfg = MulticastConfig::parse(&args.address)?;

    let (hub_ep, spoke1_ep, spoke2_ep) = if args.transport == "tcp" {
        (
            format!("tcp://127.0.0.1:{}", args.base_port),
            format!("tcp://127.0.0.1:{}", args.base_port + 1),
            format!("tcp://127.0.0.1:{}", args.base_port + 2),
        )
    } else if args.transport == "inproc" {
        (
            "inproc://hub".to_string(),
            "inproc://spoke1".to_string(),
            "inproc://spoke2".to_string(),
        )
    } else {
        return Err(anyhow!("transport must be 'inproc' or 'tcp'"));
    };

    println!("Starting overlay harness:");
    println!("  transport={}", args.transport);
    println!(
        "  hub pub={} subs=[{}, {}] forward_remote=true",
        hub_ep, spoke1_ep, spoke2_ep
    );
    println!(
        "  spoke1 pub={} subs=[{}] forward_remote=false",
        spoke1_ep, hub_ep
    );
    println!(
        "  spoke2 pub={} subs=[{}] forward_remote=false",
        spoke2_ep, hub_ep
    );

    let ctx = zmq::Context::new();
    let mut hub = Node::new(
        &ctx,
        "hub",
        &hub_ep,
        &[spoke1_ep.clone(), spoke2_ep.clone()],
        true,
        args.udp_port,
        &mcast_cfg,
        41001,
    )?;
    let mut spoke1 = Node::new(
        &ctx,
        "spoke1",
        &spoke1_ep,
        std::slice::from_ref(&hub_ep),
        false,
        args.udp_port,
        &mcast_cfg,
        41002,
    )?;
    let mut spoke2 = Node::new(
        &ctx,
        "spoke2",
        &spoke2_ep,
        std::slice::from_ref(&hub_ep),
        false,
        args.udp_port,
        &mcast_cfg,
        41003,
    )?;

    thread::sleep(Duration::from_millis(800));

    let payload = args.payload.into_bytes();
    for idx in 0..args.messages {
        let event_name = if args.messages == 1 {
            args.event.clone()
        } else {
            format!("{}_{}", args.event, idx + 1)
        };
        spoke1.publish_local(&event_name, &payload)?;

        let deadline = Instant::now() + Duration::from_millis(args.delay_ms.max(10));
        while Instant::now() < deadline {
            hub.pump_once()?;
            spoke1.pump_once()?;
            spoke2.pump_once()?;
            thread::sleep(Duration::from_millis(5));
        }
    }

    let flush_deadline = Instant::now() + Duration::from_millis(800);
    while Instant::now() < flush_deadline {
        hub.pump_once()?;
        spoke1.pump_once()?;
        spoke2.pump_once()?;
        thread::sleep(Duration::from_millis(5));
    }

    Ok(())
}

fn parse_payload_len(datagram: &[u8]) -> Result<u32> {
    if datagram.len() < 8 {
        return Err(anyhow!("datagram too short"));
    }
    let name_len = u32::from_be_bytes(datagram[0..4].try_into()?) as usize;
    let idx = 4 + name_len;
    if datagram.len() < idx + 4 {
        return Err(anyhow!("invalid datagram"));
    }
    Ok(u32::from_be_bytes(datagram[idx..idx + 4].try_into()?))
}

fn unix_ts_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |dur| dur.as_secs_f64())
}
