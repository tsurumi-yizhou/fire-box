//! Fire Box Core — stateful LLM gateway with auth, metrics and IPC.
//!
//! This crate implements the core logic:
//! - IPC server for communication with the native layer (Swift / C++)
//! - LLM provider abstraction (OpenAI / Anthropic / DashScope)
//! - App authentication and authorization management
//! - Real-time metrics collection (token usage, requests, connections)
//! - All configuration stored securely in OS keyring

pub mod auth;
pub mod config;
pub mod filesystem;
pub mod ipc;
pub mod keystore;
pub mod metrics;
pub mod models;
pub mod protocol;
pub mod protocols;
pub mod provider;
pub mod session;

use config::Config;
use models::ModelRegistry;
use session::SessionManager;
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use crate::auth::AuthManager;
use crate::metrics::Metrics;

/// Shared state for the entire core, accessible by the IPC server.
#[derive(Clone)]
pub struct CoreState {
    pub config: Arc<tokio::sync::RwLock<Config>>,
    pub http: reqwest::Client,
    pub session_manager: SessionManager,
    pub model_registry: Arc<ModelRegistry>,
    pub metrics: Arc<Metrics>,
    pub auth_manager: Arc<AuthManager>,
    pub ipc_event_tx: tokio::sync::broadcast::Sender<ipc::IpcEvent>,
}

/// Start the core (no arguments required, loads from keyring).
pub async fn run_from_args() -> anyhow::Result<()> {
    run().await
}

/// Start the core service.
pub async fn run() -> anyhow::Result<()> {
    // Load configuration from OS keyring.
    let config = Config::load_from_keyring();

    // Init logging.
    let filter =
        EnvFilter::try_new(&config.settings.log_level).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("fire-box core starting (keyring-based configuration)");
    info!(
        providers = config.providers.len(),
        models = config.models.len(),
        "Configuration loaded from keyring"
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

    // Create the IPC event channel early so we can use it during pre-flight.
    // 64-slot buffer; native layer receives via SSE.
    let (ipc_event_tx, _) = tokio::sync::broadcast::channel::<ipc::IpcEvent>(64);

    for p in &config.providers {
        if p.provider_type == config::ProtocolType::DashScope
            && let Some(creds_path) = &p.oauth_creds_path
        {
            info!(provider = %p.tag, "Checking DashScope OAuth credentials...");
            if let Err(e) = protocols::dashscope::preflight_check(
                &http,
                &p.tag,
                creds_path,
                Some(&ipc_event_tx),
            )
            .await
            {
                warn!(provider = %p.tag, error = %e, "DashScope OAuth pre-flight check failed, provider will be unavailable");
            }
        }

        if p.provider_type == config::ProtocolType::Copilot {
            info!(provider = %p.tag, "Checking GitHub Copilot credentials...");
            if let Err(e) = protocols::copilot::preflight_check(&http, &p.tag).await {
                warn!(provider = %p.tag, error = %e, "GitHub Copilot pre-flight check failed, provider will be unavailable");
            }
        }
    }

    let config = Arc::new(tokio::sync::RwLock::new(config));
    let session_manager = SessionManager::new();
    let metrics = Arc::new(Metrics::new());
    let auth_manager = Arc::new(AuthManager::new());

    let core_state = CoreState {
        config: config.clone(),
        http: http.clone(),
        session_manager: session_manager.clone(),
        model_registry: model_registry.clone(),
        metrics: metrics.clone(),
        auth_manager: auth_manager.clone(),
        ipc_event_tx: ipc_event_tx.clone(),
    };

    // Launch IPC server only (no HTTP gateway).
    let ipc_handle = ipc::launch(&core_state)?;

    info!("IPC server started, ready to accept requests from native layer");

    // Wait for IPC server.
    let _ = ipc_handle.await;

    Ok(())
}
