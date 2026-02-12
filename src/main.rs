mod config;
mod filesystem;
mod models;
mod protocol;
mod protocols;
mod provider;
mod server;
mod session;

use config::Config;
use models::ModelRegistry;
use session::SessionManager;
use std::sync::Arc;
use tracing::{info, warn};
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
        models = config.models.len(),
        "Configuration loaded"
    );

    for p in &config.providers {
        info!(tag = %p.tag, r#type = ?p.provider_type, base_url = %p.base_url.as_deref().unwrap_or("-"), "Provider configured");
    }
    for (tag, mappings) in &config.models {
        info!(tag = %tag, providers = mappings.len(), "Model configured");
    }

    // Load model metadata from models.dev
    info!("Loading model metadata from models.dev...");
    let model_registry = match ModelRegistry::load_from_models_dev().await {
        Ok(registry) => {
            info!(
                models_loaded = registry.len(),
                "Model metadata loaded successfully"
            );
            Arc::new(registry)
        }
        Err(e) => {
            warn!(error = %e, "Failed to load model metadata from models.dev, using empty registry");
            Arc::new(ModelRegistry::new())
        }
    };

    // Pre-flight check: verify DashScope OAuth credentials at startup.
    let http = reqwest::Client::new();
    for p in &config.providers {
        if p.provider_type == config::ProtocolType::DashScope
            && let Some(creds_path) = &p.oauth_creds_path
        {
            info!(provider = %p.tag, "Checking DashScope OAuth credentials...");
            if let Err(e) = protocols::dashscope::preflight_check(&http, &p.tag, creds_path).await {
                warn!(provider = %p.tag, error = %e, "DashScope OAuth pre-flight check failed, provider will be unavailable");
            }
        }
    }

    let config = Arc::new(config);
    let session_manager = SessionManager::new();

    // Launch gateway servers (1 Unix socket + 1 TCP port).
    let handles = server::launch_all(config, session_manager, model_registry)?;

    info!("Gateway servers started, ready to accept requests");

    // Wait for all servers.
    for h in handles {
        let _ = h.await;
    }

    Ok(())
}
