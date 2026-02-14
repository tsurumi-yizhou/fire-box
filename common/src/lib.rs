//! Fire Box Core — stateful LLM gateway with auth and metrics.
//!
//! This crate implements the core logic:
//! - LLM provider abstraction (OpenAI / Anthropic / DashScope / GitHub Copilot)
//! - App authentication and authorization management
//! - Real-time metrics collection (token usage, requests, connections)
//! - Configuration stored securely in OS keyring
//! - HTTP gateway for local OpenAI/Anthropic-compatible API

pub mod auth;
pub mod config;
pub mod error;
pub mod filesystem;
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

/// Shared state for the entire core.
#[derive(Clone)]
pub struct CoreState {
    pub config: Arc<tokio::sync::RwLock<Config>>,
    pub http: reqwest::Client,
    pub session_manager: SessionManager,
    pub model_registry: Arc<ModelRegistry>,
    pub metrics: Arc<Metrics>,
    pub auth_manager: Arc<AuthManager>,
}

impl CoreState {
    /// Create a new CoreState by loading configuration from keyring.
    pub async fn new() -> anyhow::Result<Self> {
        let config = Config::load_from_keyring();

        // Init logging.
        let filter = EnvFilter::try_new(&config.settings.log_level)
            .unwrap_or_else(|_| EnvFilter::new("info"));
        let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();

        info!("fire-box core initializing (keyring-based configuration)");
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

        let http = reqwest::Client::new();

        // Pre-flight check: verify provider credentials at startup.
        for p in &config.providers {
            if p.provider_type == config::ProtocolType::DashScope
                && let Some(creds_path) = &p.oauth_creds_path
            {
                info!(provider = %p.tag, "Checking DashScope OAuth credentials...");
                if let Err(e) =
                    protocols::dashscope::preflight_check(&http, &p.tag, creds_path).await
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

        Ok(Self {
            config: Arc::new(tokio::sync::RwLock::new(config)),
            http,
            session_manager: SessionManager::new(),
            model_registry,
            metrics: Arc::new(Metrics::new()),
            auth_manager: Arc::new(AuthManager::new()),
        })
    }
}
