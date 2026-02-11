mod channel;
mod config;
mod filesystem;
mod protocol;
mod protocols;
mod provider;
mod router;
mod session;

use config::Config;
use session::SessionManager;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load config from command line argument
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: {} <config_file>", args[0]);
        std::process::exit(1);
    }

    let config_path = &args[1];
    let config = Config::load(config_path)?;

    // Init logging.
    let filter = EnvFilter::try_new(&config.log.level).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("fire-box LLM gateway starting");
    info!(
        config_file = %config_path,
        providers = config.providers.len(),
        channels = config.channels.len(),
        routes = config.routes.len(),
        "Configuration loaded"
    );

    for p in &config.providers {
        info!(tag = %p.tag, r#type = ?p.provider_type, base_url = %p.base_url, "Provider configured");
    }
    for c in &config.channels {
        info!(tag = %c.tag, r#type = ?c.channel_type, port = c.port, "Channel configured");
    }

    let config = Arc::new(config);
    let session_manager = SessionManager::new();

    // Launch all channel servers.
    let handles = channel::launch_all(config, session_manager);

    info!("All channels started, gateway is ready");

    // Wait for all servers.
    for h in handles {
        let _ = h.await;
    }

    Ok(())
}
