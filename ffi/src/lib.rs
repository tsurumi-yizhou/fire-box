//! Fire Box FFI - UniFFI interface for Fire Box core

use std::sync::Arc;
use std::fmt;

pub struct FireBox {
    state: Arc<common::CoreState>,
    runtime: tokio::runtime::Runtime,
}

#[derive(Debug)]
pub enum FireBoxError {
    InitializationFailed(String),
    ConfigurationError(String),
    ProviderError(String),
    AuthError(String),
    NetworkError(String),
    InternalError(String),
}

impl fmt::Display for FireBoxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FireBoxError::InitializationFailed(msg) => write!(f, "Initialization failed: {}", msg),
            FireBoxError::ConfigurationError(msg) => write!(f, "Configuration error: {}", msg),
            FireBoxError::ProviderError(msg) => write!(f, "Provider error: {}", msg),
            FireBoxError::AuthError(msg) => write!(f, "Auth error: {}", msg),
            FireBoxError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            FireBoxError::InternalError(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for FireBoxError {}
impl From<anyhow::Error> for FireBoxError {
    fn from(e: anyhow::Error) -> Self {
        FireBoxError::InternalError(e.to_string())
    }
}

#[derive(Clone, Copy)]
pub enum ProviderType {
    OpenAI,
    Anthropic,
    DashScope,
    Copilot,
}

pub struct ProviderInfo {
    pub tag: String,
    pub provider_type: ProviderType,
    pub base_url: Option<String>,
    pub oauth_creds_path: Option<String>,
}

pub struct ProviderMapping {
    pub provider: String,
    pub model_id: String,
}

/// Local metrics snapshot for FFI - uses JSON strings for complex data
pub struct MetricsSnapshot {
    pub total_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_errors: u64,
    pub active_connections: u64,
    pub per_model_json: String,
    pub per_provider_json: String,
    pub per_app_json: String,
}

pub struct ModelMapping {
    pub model_tag: String,
    pub provider_tag: String,
    pub provider_model: String,
}

pub struct AppInfo {
    pub app_id: String,
    pub app_name: Option<String>,
    pub approved: bool,
    pub created_at: u64,
}

impl FireBox {
    pub fn new() -> Result<Arc<Self>, FireBoxError> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| FireBoxError::InitializationFailed(e.to_string()))?;
        
        let state = runtime.block_on(async { common::CoreState::new().await })
            .map_err(|e| FireBoxError::InitializationFailed(e.to_string()))?;

        Ok(Arc::new(FireBox {
            state: Arc::new(state),
            runtime,
        }))
    }

    pub fn list_providers(&self) -> Result<Vec<ProviderInfo>, FireBoxError> {
        let config = self.runtime.block_on(async {
            self.state.config.read().await.clone()
        });

        Ok(config.providers
            .iter()
            .map(|p| ProviderInfo {
                tag: p.tag.clone(),
                provider_type: match p.provider_type {
                    common::config::ProtocolType::OpenAI => ProviderType::OpenAI,
                    common::config::ProtocolType::Anthropic => ProviderType::Anthropic,
                    common::config::ProtocolType::DashScope => ProviderType::DashScope,
                    common::config::ProtocolType::Copilot => ProviderType::Copilot,
                },
                base_url: p.base_url.clone(),
                oauth_creds_path: p.oauth_creds_path.clone(),
            })
            .collect())
    }

    pub fn add_provider(
        &self,
        tag: String,
        provider_type: ProviderType,
        api_key: Option<String>,
        base_url: Option<String>,
    ) -> Result<(), FireBoxError> {
        let mut config = self.runtime.block_on(async {
            self.state.config.write().await.clone()
        });

        let pt = match provider_type {
            ProviderType::OpenAI => common::config::ProtocolType::OpenAI,
            ProviderType::Anthropic => common::config::ProtocolType::Anthropic,
            ProviderType::DashScope => common::config::ProtocolType::DashScope,
            ProviderType::Copilot => common::config::ProtocolType::Copilot,
        };

        config.providers.push(common::config::ProviderConfig {
            tag: tag.clone(),
            provider_type: pt,
            base_url,
            oauth_creds_path: if pt == common::config::ProtocolType::DashScope {
                Some(format!(".fire-box-creds/{}.json", tag))
            } else {
                None
            },
        });

        if let Some(key) = api_key
            && let Err(e) = common::keystore::store_provider_key(&tag, &key)
        {
            return Err(FireBoxError::ConfigurationError(e.to_string()));
        }

        config.save_to_keyring()
            .map_err(|e| FireBoxError::ConfigurationError(e.to_string()))?;

        Ok(())
    }

    pub fn remove_provider(&self, tag: String) -> Result<(), FireBoxError> {
        let mut config = self.runtime.block_on(async {
            self.state.config.write().await.clone()
        });

        let before = config.providers.len();
        config.providers.retain(|p| p.tag != tag);
        let after = config.providers.len();

        if before == after {
            return Err(FireBoxError::ProviderError(format!("Provider '{}' not found", tag)));
        }

        config.save_to_keyring()
            .map_err(|e| FireBoxError::ConfigurationError(e.to_string()))?;

        Ok(())
    }

    pub fn list_models(&self) -> Result<Vec<ModelMapping>, FireBoxError> {
        let config = self.runtime.block_on(async {
            self.state.config.read().await.clone()
        });

        let mut mappings = Vec::new();
        for (model_tag, model_mappings) in &config.models {
            for mapping in model_mappings {
                mappings.push(ModelMapping {
                    model_tag: model_tag.clone(),
                    provider_tag: mapping.provider.clone(),
                    provider_model: mapping.model_id.clone(),
                });
            }
        }

        Ok(mappings)
    }

    pub fn add_model_mapping(
        &self,
        model_tag: String,
        provider_tag: String,
        provider_model: String,
    ) -> Result<(), FireBoxError> {
        let mut config = self.runtime.block_on(async {
            self.state.config.write().await.clone()
        });

        let mapping = common::config::ProviderMapping {
            provider: provider_tag,
            model_id: provider_model,
        };

        config
            .models
            .entry(model_tag)
            .or_default()
            .push(mapping);

        config.save_to_keyring()
            .map_err(|e| FireBoxError::ConfigurationError(e.to_string()))?;

        Ok(())
    }

    pub fn remove_model_mapping(&self, model_tag: String) -> Result<(), FireBoxError> {
        let mut config = self.runtime.block_on(async {
            self.state.config.write().await.clone()
        });

        if config.models.remove(&model_tag).is_none() {
            return Err(FireBoxError::ProviderError(format!(
                "Model '{}' not found",
                model_tag
            )));
        }

        config.save_to_keyring()
            .map_err(|e| FireBoxError::ConfigurationError(e.to_string()))?;

        Ok(())
    }

    pub fn list_apps(&self) -> Result<Vec<AppInfo>, FireBoxError> {
        // Placeholder: apps are managed at API level
        Ok(Vec::new())
    }

    pub fn revoke_app(&self, _app_id: String) -> Result<(), FireBoxError> {
        Err(FireBoxError::AuthError(
            "App authorization is handled at the API level".to_string(),
        ))
    }

    pub fn get_metrics(&self) -> Result<MetricsSnapshot, FireBoxError> {
        let snapshot = self.runtime.block_on(async {
            self.state.metrics.snapshot().await
        });

        let per_model_json = serde_json::to_string(&snapshot.per_model)
            .unwrap_or_else(|_| "{}".to_string());
        let per_provider_json = serde_json::to_string(&snapshot.per_provider)
            .unwrap_or_else(|_| "{}".to_string());
        let per_app_json = serde_json::to_string(&snapshot.per_app)
            .unwrap_or_else(|_| "{}".to_string());

        Ok(MetricsSnapshot {
            total_requests: snapshot.total_requests,
            total_input_tokens: snapshot.total_input_tokens,
            total_output_tokens: snapshot.total_output_tokens,
            total_errors: snapshot.total_errors,
            active_connections: snapshot.active_connections,
            per_model_json,
            per_provider_json,
            per_app_json,
        })
    }
}

pub fn create_gateway() -> Result<Arc<FireBox>, FireBoxError> {
    FireBox::new()
}

// Include the UniFFI generated code
uniffi::include_scaffolding!("fire_box");
