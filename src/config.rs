use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub log: LogConfig,
    pub service: ServiceConfig,
    pub providers: Vec<ProviderConfig>,
    pub models: HashMap<String, Vec<ProviderMapping>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogConfig {
    pub level: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServiceConfig {
    #[cfg_attr(not(unix), allow(dead_code))]
    pub uds: String,
    pub tcp: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub tag: String,
    #[serde(rename = "type")]
    pub provider_type: ProtocolType,
    /// API key for OpenAI-compatible providers.
    #[serde(default)]
    pub api_key: Option<String>,
    /// Base URL for OpenAI / Anthropic providers.
    #[serde(default)]
    pub base_url: Option<String>,
    /// Auth token for Anthropic providers (sent as x-api-key header).
    #[serde(default)]
    pub auth_token: Option<String>,
    /// Path to OAuth credentials JSON file for DashScope providers.
    /// Supports ~ expansion. The file contains access_token, refresh_token,
    /// resource_url, and expiry_date.
    #[serde(default)]
    pub oauth_creds_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderMapping {
    pub provider: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProtocolType {
    OpenAI,
    Anthropic,
    DashScope,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&content)?;
        Ok(config)
    }
}
