use anyhow::Result;
use clap::Parser;
use mdsevent_tcp_bridge::bridge::Bridge;
use mdsevent_tcp_bridge::config::{BridgeConfig, Cli};
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = BridgeConfig::from_cli(cli.clone());

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new(cfg.log_level.clone()))
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let mut bridge = Bridge::new(cfg)?;
    bridge.run()
}
