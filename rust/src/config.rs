use std::env;

use clap::Parser;

const DEFAULT_UDP_PORT: u16 = 4000;
const DEFAULT_UDP_ADDRESS: &str = "224.0.0.175";

#[derive(Debug, Clone, Parser)]
#[command(name = "mdsevent_tcp_bridge")]
#[command(about = "Bridge MDSplus UDP events to/from ZeroMQ TCP")]
pub struct Cli {
    #[arg(long, env = "MDS_BRIDGE_SITE_ID", default_value = "localhost")]
    pub site_id: String,

    #[arg(long, env = "MDS_BRIDGE_ID")]
    pub bridge_id: Option<String>,

    #[arg(long, env = "mdsevent_port", default_value_t = DEFAULT_UDP_PORT)]
    pub udp_port: u16,

    #[arg(long, env = "mdsevent_address", default_value = DEFAULT_UDP_ADDRESS)]
    pub udp_address_setting: String,

    #[arg(long, default_value = "0.0.0.0")]
    pub udp_bind_ip: String,

    #[arg(long, default_value = "0.0.0.0")]
    pub udp_send_bind_ip: String,

    #[arg(long, env = "mdsevent_interface")]
    pub udp_interface_ip: Option<String>,

    #[arg(long, env = "mdsevent_ttl", default_value_t = 1)]
    pub udp_ttl: u32,

    #[arg(long, env = "mdsevent_loop", default_value_t = true)]
    pub udp_loop: bool,

    #[arg(long, default_value = "tcp://0.0.0.0:5600")]
    pub pub_bind: String,

    #[arg(long)]
    pub sub_connect: Vec<String>,

    #[arg(long, default_value_t = false)]
    pub forward_remote: bool,

    #[arg(long, default_value_t = 4)]
    pub max_hops: u32,

    #[arg(long, default_value_t = 60)]
    pub seen_ttl_seconds: u64,

    #[arg(long, default_value_t = 200_000)]
    pub seen_max_entries: usize,

    #[arg(long, default_value_t = 10)]
    pub poll_ms: u64,

    #[arg(long, default_value = "info")]
    pub log_level: String,
}

#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub site_id: String,
    pub bridge_id: String,
    pub udp_port: u16,
    pub udp_address_setting: String,
    pub udp_bind_ip: String,
    pub udp_send_bind_ip: String,
    pub udp_interface_ip: Option<String>,
    pub udp_ttl: u32,
    pub udp_loop: bool,
    pub pub_bind: String,
    pub sub_connect: Vec<String>,
    pub forward_remote: bool,
    pub max_hops: u32,
    pub seen_ttl_seconds: u64,
    pub seen_max_entries: usize,
    pub poll_ms: u64,
    pub log_level: String,
}

impl BridgeConfig {
    #[must_use]
    pub fn from_cli(cli: Cli) -> Self {
        let default_bridge_id = format!("{}:{}", hostname_fallback(), std::process::id());
        Self {
            site_id: cli.site_id,
            bridge_id: cli.bridge_id.unwrap_or(default_bridge_id),
            udp_port: cli.udp_port,
            udp_address_setting: cli.udp_address_setting,
            udp_bind_ip: cli.udp_bind_ip,
            udp_send_bind_ip: cli.udp_send_bind_ip,
            udp_interface_ip: cli.udp_interface_ip,
            udp_ttl: cli.udp_ttl,
            udp_loop: cli.udp_loop,
            pub_bind: cli.pub_bind,
            sub_connect: cli.sub_connect,
            forward_remote: cli.forward_remote,
            max_hops: cli.max_hops,
            seen_ttl_seconds: cli.seen_ttl_seconds,
            seen_max_entries: cli.seen_max_entries,
            poll_ms: cli.poll_ms,
            log_level: cli.log_level,
        }
    }
}

fn hostname_fallback() -> String {
    env::var("HOSTNAME").unwrap_or_else(|_| "localhost".to_string())
}
