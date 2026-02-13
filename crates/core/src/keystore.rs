//! Secure credential storage via the OS keyring.
//!
//! Uses the `keyring` crate to store and retrieve:
//! - LLM provider API keys (so they don't have to sit in the config as plaintext)
//! - Authenticated app authorization data (persisted across restarts)
//! - Full service configuration (providers, models, settings)
//!
//! Keys are stored under service name `"fire-box"` with descriptive entry IDs.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, warn};

const SERVICE: &str = "fire-box";
const APP_AUTH_KEY: &str = "app_authorizations";
const PROVIDERS_KEY: &str = "providers_config";
const MODELS_KEY: &str = "models_config";
const SETTINGS_KEY: &str = "service_settings";

// ─── Configuration structures (keyring-persisted) ───────────────────────────

/// Provider metadata (credentials stored separately via provider_entry_name).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub tag: String,
    #[serde(rename = "type")]
    pub provider_type: ProviderType,
    /// Base URL for OpenAI / Anthropic providers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Path to OAuth credentials JSON file for DashScope providers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_creds_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    OpenAI,
    Anthropic,
    DashScope,
    Copilot,
}

impl ProviderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderType::OpenAI => "openai",
            ProviderType::Anthropic => "anthropic",
            ProviderType::DashScope => "dashscope",
            ProviderType::Copilot => "copilot",
        }
    }
}

/// Model mapping: which provider(s) serve a unified model tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMapping {
    pub provider: String,
    pub model_id: String,
}

/// Service runtime settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSettings {
    pub log_level: String,
    pub ipc_pipe: String,
}

impl Default for ServiceSettings {
    fn default() -> Self {
        Self {
            log_level: "info".to_owned(),
            ipc_pipe: default_ipc_pipe(),
        }
    }
}

fn default_ipc_pipe() -> String {
    #[cfg(windows)]
    {
        "fire-box-ipc".to_owned()
    }
    #[cfg(not(windows))]
    {
        "/tmp/fire-box-ipc.sock".to_owned()
    }
}

// ─── Provider API key helpers ───────────────────────────────────────────────

fn provider_entry_name(tag: &str, kind: &str) -> String {
    format!("provider:{}:{}", tag, kind)
}

/// Store a provider's API key in the OS keyring.
pub fn store_provider_key(tag: &str, key: &str) -> anyhow::Result<()> {
    let entry_name = provider_entry_name(tag, "api_key");
    let entry =
        keyring::Entry::new(SERVICE, &entry_name).context("failed to create keyring entry")?;
    entry
        .set_password(key)
        .context("failed to store API key in keyring")?;
    debug!(provider = tag, "API key stored in keyring");
    Ok(())
}

/// Retrieve a provider's API key from the OS keyring.
/// Returns `None` if no key is stored.
pub fn get_provider_key(tag: &str) -> Option<String> {
    let entry_name = provider_entry_name(tag, "api_key");
    let entry = keyring::Entry::new(SERVICE, &entry_name).ok()?;
    match entry.get_password() {
        Ok(pw) if !pw.is_empty() => Some(pw),
        Ok(_) => None, // Treat empty string as no key
        Err(keyring::Error::NoEntry) => None,
        Err(e) => {
            warn!(provider = tag, error = %e, "failed to read API key from keyring");
            None
        }
    }
}

/// Delete a provider's API key from the OS keyring.
pub fn delete_provider_key(tag: &str) -> anyhow::Result<()> {
    let entry_name = provider_entry_name(tag, "api_key");
    let entry =
        keyring::Entry::new(SERVICE, &entry_name).context("failed to create keyring entry")?;
    match entry.delete_credential() {
        Ok(()) => {
            debug!(provider = tag, "API key deleted from keyring");
            Ok(())
        }
        Err(keyring::Error::NoEntry) => Ok(()), // Already deleted
        Err(e) => Err(e).context("failed to delete API key from keyring"),
    }
}

/// Store a provider's auth token (e.g. Anthropic x-api-key) in the OS keyring.
pub fn store_auth_token(tag: &str, token: &str) -> anyhow::Result<()> {
    let entry_name = provider_entry_name(tag, "auth_token");
    let entry =
        keyring::Entry::new(SERVICE, &entry_name).context("failed to create keyring entry")?;
    entry
        .set_password(token)
        .context("failed to store auth token in keyring")?;
    debug!(provider = tag, "Auth token stored in keyring");
    Ok(())
}

/// Retrieve a provider's auth token from the OS keyring.
pub fn get_auth_token(tag: &str) -> Option<String> {
    let entry_name = provider_entry_name(tag, "auth_token");
    let entry = keyring::Entry::new(SERVICE, &entry_name).ok()?;
    match entry.get_password() {
        Ok(pw) => Some(pw),
        Err(keyring::Error::NoEntry) => None,
        Err(e) => {
            warn!(provider = tag, error = %e, "failed to read auth token from keyring");
            None
        }
    }
}

// ─── App authorization persistence ──────────────────────────────────────────

/// Save the full app authorization map (JSON-serialized) to the OS keyring.
pub fn save_app_authorizations(json: &str) -> anyhow::Result<()> {
    let entry =
        keyring::Entry::new(SERVICE, APP_AUTH_KEY).context("failed to create keyring entry")?;
    entry
        .set_password(json)
        .context("failed to persist app authorizations to keyring")?;
    debug!("App authorizations persisted to keyring");
    Ok(())
}

/// Load the app authorization map (JSON string) from the OS keyring.
/// Returns `None` if nothing has been stored yet.
pub fn load_app_authorizations() -> Option<String> {
    let entry = keyring::Entry::new(SERVICE, APP_AUTH_KEY).ok()?;
    match entry.get_password() {
        Ok(json) => Some(json),
        Err(keyring::Error::NoEntry) => None,
        Err(e) => {
            warn!(error = %e, "failed to load app authorizations from keyring");
            None
        }
    }
}

/// Resolve the effective API key for a provider: prefer keyring, fallback to config.
pub fn resolve_api_key(tag: &str, config_key: Option<&str>) -> Option<String> {
    // Keyring takes precedence.
    if let Some(key) = get_provider_key(tag) {
        return Some(key);
    }
    // Fall back to plaintext config value — and opportunistically save it.
    if let Some(key) = config_key {
        if let Err(e) = store_provider_key(tag, key) {
            warn!(provider = tag, error = %e, "could not migrate API key to keyring");
        }
        return Some(key.to_owned());
    }
    None
}

/// Resolve the effective auth token for a provider: prefer keyring, fallback to config.
pub fn resolve_auth_token(tag: &str, config_token: Option<&str>) -> Option<String> {
    if let Some(token) = get_auth_token(tag) {
        return Some(token);
    }
    if let Some(token) = config_token {
        if let Err(e) = store_auth_token(tag, token) {
            warn!(provider = tag, error = %e, "could not migrate auth token to keyring");
        }
        return Some(token.to_owned());
    }
    None
}

// ─── Full configuration persistence ─────────────────────────────────────────

/// Save providers list to keyring (credentials are stored separately).
pub fn save_providers(providers: &[ProviderInfo]) -> anyhow::Result<()> {
    let json = serde_json::to_string(providers).context("failed to serialize providers")?;
    let entry =
        keyring::Entry::new(SERVICE, PROVIDERS_KEY).context("failed to create keyring entry")?;
    entry
        .set_password(&json)
        .context("failed to persist providers to keyring")?;
    debug!(count = providers.len(), "Providers config saved to keyring");
    Ok(())
}

/// Load providers list from keyring.
pub fn load_providers() -> Vec<ProviderInfo> {
    let entry = match keyring::Entry::new(SERVICE, PROVIDERS_KEY) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    match entry.get_password() {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(keyring::Error::NoEntry) => Vec::new(),
        Err(e) => {
            warn!(error = %e, "failed to load providers from keyring");
            Vec::new()
        }
    }
}

/// Save models configuration to keyring.
pub fn save_models(models: &HashMap<String, Vec<ProviderMapping>>) -> anyhow::Result<()> {
    let json = serde_json::to_string(models).context("failed to serialize models")?;
    let entry =
        keyring::Entry::new(SERVICE, MODELS_KEY).context("failed to create keyring entry")?;
    entry
        .set_password(&json)
        .context("failed to persist models to keyring")?;
    debug!(count = models.len(), "Models config saved to keyring");
    Ok(())
}

/// Load models configuration from keyring.
pub fn load_models() -> HashMap<String, Vec<ProviderMapping>> {
    let entry = match keyring::Entry::new(SERVICE, MODELS_KEY) {
        Ok(e) => e,
        Err(_) => return HashMap::new(),
    };
    match entry.get_password() {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(keyring::Error::NoEntry) => HashMap::new(),
        Err(e) => {
            warn!(error = %e, "failed to load models from keyring");
            HashMap::new()
        }
    }
}

/// Save service settings to keyring.
pub fn save_settings(settings: &ServiceSettings) -> anyhow::Result<()> {
    let json = serde_json::to_string(settings).context("failed to serialize settings")?;
    let entry =
        keyring::Entry::new(SERVICE, SETTINGS_KEY).context("failed to create keyring entry")?;
    entry
        .set_password(&json)
        .context("failed to persist settings to keyring")?;
    debug!("Service settings saved to keyring");
    Ok(())
}

/// Load service settings from keyring.
pub fn load_settings() -> ServiceSettings {
    let entry = match keyring::Entry::new(SERVICE, SETTINGS_KEY) {
        Ok(e) => e,
        Err(_) => return ServiceSettings::default(),
    };
    match entry.get_password() {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(keyring::Error::NoEntry) => ServiceSettings::default(),
        Err(e) => {
            warn!(error = %e, "failed to load settings from keyring");
            ServiceSettings::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_entry_name() {
        assert_eq!(
            provider_entry_name("OpenAI", "api_key"),
            "provider:OpenAI:api_key"
        );
        assert_eq!(
            provider_entry_name("Anthropic", "auth_token"),
            "provider:Anthropic:auth_token"
        );
    }
}
