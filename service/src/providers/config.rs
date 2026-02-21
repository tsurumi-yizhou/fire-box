//! Unified provider configuration with encrypted-file persistence.
//!
//! Each provider variant holds the minimum parameters required to instantiate
//! a backend.  Configuration is saved to and loaded from an AES-256-GCM
//! encrypted local file.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::middleware::store;
use crate::providers::ProviderDyn;
use crate::providers::anthropic::AnthropicProvider;
use crate::providers::copilot::CopilotProvider;
use crate::providers::dashscope::DashScopeProvider;
use crate::providers::llamacpp::{LlamaCppConfig, LlamaCppProvider};
use crate::providers::openai::OpenAiProvider;

// ---------------------------------------------------------------------------
// Provider index — ordered list of profile IDs stored in the encrypted file
// ---------------------------------------------------------------------------

/// Load every configured profile ID, in insertion order.
pub fn load_provider_index() -> Vec<String> {
    store::load().map(|d| d.provider_index).unwrap_or_default()
}

/// Add `profile_id` to the index (no-op if already present).
pub fn add_to_provider_index(profile_id: &str) -> Result<()> {
    store::update(|d| {
        if !d.provider_index.iter().any(|id| id == profile_id) {
            d.provider_index.push(profile_id.to_string());
        }
    })
    .map_err(|e| anyhow::anyhow!("failed to add provider to index: {e}"))
}

/// Remove `profile_id` from the index (no-op if absent).
pub fn remove_from_provider_index(profile_id: &str) -> Result<()> {
    store::update(|d| {
        d.provider_index.retain(|id| id != profile_id);
    })
    .map_err(|e| anyhow::anyhow!("failed to remove provider from index: {e}"))
}

/// One-time migration: populate the index from legacy hard-coded profiles.
///
/// Checks the five well-known legacy profile IDs and, for OAuth providers,
/// migrates credentials from their provider-specific keyring locations.
/// Safe to call on every startup — idempotent.
pub fn migrate_legacy_providers() {
    let index = load_provider_index();

    let has = |id: &str| index.iter().any(|x| x == id);

    // — API-key providers (already use configure_provider) ——————————————
    for id in ["openai", "anthropic", "llamacpp"] {
        if !has(id) && provider_is_configured(id) {
            let _ = add_to_provider_index(id);
        }
    }

    // — Copilot: migrate from "fire-box-copilot"/"github-oauth" ————————
    if !has("copilot")
        && let Ok(token) =
            crate::middleware::keyring::get_password("fire-box-copilot", "github-oauth")
    {
        let cfg = ProviderConfig::copilot(token, None);
        if configure_provider("copilot", &cfg).is_ok() {
            let _ = add_to_provider_index("copilot");
        }
    }

    // — DashScope: migrate from "fire-box-dashscope"/"oauth-credentials" —
    if !has("dashscope")
        && let Ok(json) =
            crate::middleware::keyring::get_password("fire-box-dashscope", "oauth-credentials")
        && let Ok(creds) =
            serde_json::from_str::<crate::providers::dashscope::OAuthCredentials>(&json)
    {
        let cfg = ProviderConfig::dashscope_oauth(
            creds.access_token,
            creds.refresh_token.unwrap_or_default(),
            creds.expiry_date.unwrap_or(0),
            creds.resource_url,
        );
        if configure_provider("dashscope", &cfg).is_ok() {
            let _ = add_to_provider_index("dashscope");
        }
    }
}

// ---------------------------------------------------------------------------
// Per-provider parameter types
// ---------------------------------------------------------------------------

/// Parameters for an API-key-based provider (OpenAI-compatible).
///
/// Used for: openai, ollama, vllm, and any OpenAI-compatible endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyConfig {
    /// API key (stored in keyring, **never** in plain config files).
    /// Empty for providers that don't require authentication (e.g., Ollama).
    pub api_key: String,
    /// Optional custom base URL.  `None` means use the default endpoint.
    pub base_url: Option<String>,
}

/// Parameters for the Anthropic provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    /// Anthropic API key.
    pub api_key: String,
    /// Optional custom base URL.
    pub base_url: Option<String>,
}

/// Parameters for the GitHub Copilot provider (OAuth).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotConfig {
    /// GitHub OAuth access token obtained via the device flow.
    ///
    /// `None` indicates the device flow has not completed yet; the IPC layer
    /// must run `OAuthStart { provider: Copilot }` before using this provider.
    pub oauth_token: Option<String>,
    /// Optional custom endpoint.
    pub endpoint: Option<String>,
}

/// Parameters for Alibaba Cloud DashScope (used by Qwen / qwen-coder).
///
/// Only OAuth authentication is supported.  Credentials are obtained via the
/// Qwen OAuth2 device flow at `chat.qwen.ai`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashScopeConfig {
    /// OAuth2 access token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    /// OAuth2 refresh token for silent renewal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// `resource_url` returned by the token endpoint (per-user DashScope URL).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_url: Option<String>,
    /// Unix timestamp (ms) at which `access_token` expires.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry_date: Option<i64>,
    /// Optional custom base URL.  Defaults to the Chinese mainland endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

/// Parameters for the local llama.cpp runner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlamaCppProviderConfig {
    /// Absolute path to the GGUF model file.
    pub model_path: PathBuf,
    /// Context window size in tokens (default: 4096).
    #[serde(default = "default_context_size")]
    pub context_size: u32,
    /// Number of layers to offload to GPU.
    pub gpu_layers: Option<u32>,
    /// Number of threads for inference.
    pub threads: Option<u32>,
}

fn default_context_size() -> u32 {
    4096
}

// ---------------------------------------------------------------------------
// Unified config enum
// ---------------------------------------------------------------------------

/// All supported provider *types* in a single serialisable enum.
///
/// Each variant represents a distinct provider type.  Multiple named profiles
/// of the same type can be stored in the keyring under different profile keys
/// via [`configure_provider`].
///
/// Serialise with `serde_json` to store in the keyring.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum ProviderConfig {
    /// OpenAI or any OpenAI-compatible endpoint.
    OpenAi(ApiKeyConfig),
    /// Anthropic Claude API.
    Anthropic(AnthropicConfig),
    /// GitHub Copilot via OAuth device flow.
    Copilot(CopilotConfig),
    /// Alibaba Cloud DashScope (Qwen / qwen-coder).
    DashScope(DashScopeConfig),
    /// Local llama.cpp runner (no network, no key).
    LlamaCpp(LlamaCppProviderConfig),
}

impl ProviderConfig {
    // -----------------------------------------------------------------------
    // Introspection
    // -----------------------------------------------------------------------

    /// Canonical slug for this provider type (`"openai"`, `"anthropic"`, …).
    pub fn type_slug(&self) -> &'static str {
        match self {
            Self::OpenAi(_) => "openai", // Covers ollama, vllm too
            Self::Anthropic(_) => "anthropic",
            Self::Copilot(_) => "copilot",
            Self::DashScope(_) => "dashscope",
            Self::LlamaCpp(_) => "llamacpp",
        }
    }

    /// Returns the custom base URL if configured, None otherwise.
    pub fn base_url(&self) -> Option<String> {
        match self {
            Self::OpenAi(c) => c.base_url.clone(),
            Self::Anthropic(c) => c.base_url.clone(),
            Self::Copilot(c) => c.endpoint.clone(),
            Self::DashScope(c) => c.resource_url.clone(),
            Self::LlamaCpp(_) => None,
        }
    }

    /// Human-readable display name for a (profile_id, type) pair.
    pub fn display_name(profile_id: &str, type_slug: &str) -> String {
        // First check if there's a custom display name stored
        if let Ok(data) = crate::middleware::store::load()
            && let Some(custom_name) = data.display_names.get(profile_id)
        {
            return custom_name.clone();
        }

        let base = match type_slug {
            "openai" => "OpenAI",
            "anthropic" => "Anthropic",
            "copilot" => "GitHub Copilot",
            "dashscope" => "DashScope (Qwen)",
            "llamacpp" => "llama.cpp",
            other => other,
        };
        // If the profile is named after its type (legacy single-instance), show
        // just the base name.  Otherwise append the profile id for disambiguation.
        if profile_id == type_slug
            || profile_id == type_slug.replace('_', "")
            || profile_id == type_slug.replace('_', "-")
        {
            base.to_string()
        } else {
            format!("{base} — {profile_id}")
        }
    }

    // -----------------------------------------------------------------------
    // Convenience constructors
    // -----------------------------------------------------------------------

    /// OpenAI with an optional custom base URL.
    pub fn openai(api_key: impl Into<String>, base_url: Option<String>) -> Self {
        Self::OpenAi(ApiKeyConfig {
            api_key: api_key.into(),
            base_url,
        })
    }

    /// Ollama (no authentication required).
    pub fn ollama(base_url: Option<String>) -> Self {
        Self::OpenAi(ApiKeyConfig {
            api_key: String::new(), // No API key
            base_url,
        })
    }

    /// vLLM with optional API key.
    pub fn vllm(api_key: Option<String>, base_url: Option<String>) -> Self {
        Self::OpenAi(ApiKeyConfig {
            api_key: api_key.unwrap_or_default(),
            base_url,
        })
    }

    /// Generic OpenAI-compatible provider.
    pub fn openai_compatible(api_key: Option<String>, base_url: Option<String>) -> Self {
        Self::OpenAi(ApiKeyConfig {
            api_key: api_key.unwrap_or_default(),
            base_url,
        })
    }

    /// Anthropic with an optional custom base URL.
    pub fn anthropic(api_key: impl Into<String>, base_url: Option<String>) -> Self {
        Self::Anthropic(AnthropicConfig {
            api_key: api_key.into(),
            base_url,
        })
    }

    /// GitHub Copilot with an existing OAuth token.
    pub fn copilot(oauth_token: impl Into<String>, endpoint: Option<String>) -> Self {
        Self::Copilot(CopilotConfig {
            oauth_token: Some(oauth_token.into()),
            endpoint,
        })
    }

    /// GitHub Copilot placeholder awaiting a device-flow OAuth login.
    pub fn copilot_pending(endpoint: Option<String>) -> Self {
        Self::Copilot(CopilotConfig {
            oauth_token: None,
            endpoint,
        })
    }

    /// DashScope with OAuth tokens obtained via the Qwen OAuth2 device flow.
    pub fn dashscope_oauth(
        access_token: impl Into<String>,
        refresh_token: impl Into<String>,
        expiry_date: i64,
        resource_url: Option<String>,
    ) -> Self {
        Self::DashScope(DashScopeConfig {
            access_token: Some(access_token.into()),
            refresh_token: Some(refresh_token.into()),
            resource_url,
            expiry_date: Some(expiry_date),
            base_url: None,
        })
    }

    /// Local llama.cpp runner with sensible defaults.
    pub fn llamacpp(model_path: impl Into<PathBuf>) -> Self {
        Self::LlamaCpp(LlamaCppProviderConfig {
            model_path: model_path.into(),
            context_size: default_context_size(),
            gpu_layers: None,
            threads: None,
        })
    }

    // -----------------------------------------------------------------------
    // Serialisation helpers (used for keyring persistence)
    // -----------------------------------------------------------------------

    /// Serialise to a compact JSON string for keyring storage.
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    /// Deserialise from a JSON string loaded from the keyring.
    pub fn from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    // -----------------------------------------------------------------------
    // Instantiation
    // -----------------------------------------------------------------------

    /// Instantiate a concrete provider from this configuration.
    pub fn build(&self) -> Arc<dyn ProviderDyn> {
        match self {
            Self::OpenAi(c) => {
                let api_key = if c.api_key.is_empty() {
                    None
                } else {
                    Some(c.api_key.clone())
                };
                let base_url = c.base_url.clone().unwrap_or_else(|| {
                    // Default to OpenAI
                    "https://api.openai.com/v1".to_string()
                });
                let p = OpenAiProvider::with_base_url(api_key, base_url);
                Arc::new(p)
            }
            Self::Anthropic(c) => {
                let p = match &c.base_url {
                    Some(url) => AnthropicProvider::with_base_url(c.api_key.clone(), url.clone()),
                    None => AnthropicProvider::new(c.api_key.clone()),
                };
                Arc::new(p)
            }
            Self::Copilot(c) => {
                let token = c.oauth_token.clone().unwrap_or_default();
                let p = match &c.endpoint {
                    Some(ep) => CopilotProvider::with_endpoint(token, ep.clone()),
                    None => CopilotProvider::new(token),
                };
                Arc::new(p)
            }
            Self::DashScope(c) => {
                let p = DashScopeProvider::from_config(c);
                Arc::new(p)
            }
            Self::LlamaCpp(c) => {
                let cfg = LlamaCppConfig {
                    model_path: c.model_path.clone(),
                    context_size: c.context_size,
                    gpu_layers: c.gpu_layers,
                    threads: c.threads,
                    server_url: None,
                };
                Arc::new(LlamaCppProvider::new(cfg))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public keyring-backed interface
// ---------------------------------------------------------------------------

/// Persist a [`ProviderConfig`] in the encrypted local store under the given
/// profile name.
///
/// The profile name may be any valid UTF-8 string (including CJK characters).
/// A second call with the same profile name silently overwrites the previous
/// configuration (no "duplicate item" errors).
///
/// # Errors
/// Returns an error if the store file cannot be written or serialisation fails.
pub fn configure_provider(profile: &str, config: &ProviderConfig) -> Result<()> {
    let json = config.to_json()?;
    let profile = profile.to_string();
    store::update(move |d| {
        d.providers.insert(profile.clone(), json.clone());
    })
    .map_err(|e| anyhow::anyhow!("failed to store provider config: {e}"))
}

/// Load a [`ProviderConfig`] from the encrypted local store and instantiate the
/// provider.
///
/// # Errors
/// Returns an error if the profile does not exist or the stored JSON cannot be
/// deserialised.
pub fn load_provider(profile: &str) -> Result<Arc<dyn ProviderDyn>> {
    let config = load_provider_config(profile)?;
    Ok(config.build())
}

/// Load the raw [`ProviderConfig`] from the encrypted local store without
/// instantiating.
///
/// Useful for inspecting or modifying an existing profile.
pub fn load_provider_config(profile: &str) -> Result<ProviderConfig> {
    let data = store::load().map_err(|e| anyhow::anyhow!("failed to load store: {e}"))?;
    let json = data
        .providers
        .get(profile)
        .ok_or_else(|| anyhow::anyhow!("provider profile '{}' not found", profile))?;
    ProviderConfig::from_json(json)
}

/// Remove a provider profile from the encrypted local store.
pub fn remove_provider(profile: &str) -> Result<()> {
    let profile = profile.to_string();
    store::update(move |d| {
        d.providers.remove(&profile);
    })
    .map_err(|e| anyhow::anyhow!("failed to delete provider config: {e}"))
}

/// Update provider metadata (name and/or base_url) without re-authentication.
///
/// This loads the existing config, updates the specified fields, and saves it back.
/// Only the provided fields are updated (None means keep current value).
///
/// # Errors
/// Returns an error if the profile doesn't exist or serialization fails.
pub fn update_provider_metadata(
    profile: &str,
    new_name: Option<String>,
    new_base_url: Option<String>,
) -> Result<()> {
    let mut config = load_provider_config(profile)?;

    // Update base_url if provided
    if let Some(base_url) = new_base_url {
        match &mut config {
            ProviderConfig::OpenAi(c) => {
                c.base_url = Some(base_url);
            }
            ProviderConfig::Anthropic(c) => {
                c.base_url = Some(base_url);
            }
            _ => {
                // Other providers don't support base_url customization
            }
        }
        configure_provider(profile, &config)?;
    }

    // Update display name if provided
    if let Some(name) = new_name {
        let mut data = store::load().map_err(|e| anyhow::anyhow!("failed to load store: {e}"))?;
        data.display_names.insert(profile.to_string(), name);
        store::update(move |d| {
            d.display_names.extend(data.display_names);
        })
        .map_err(|e| anyhow::anyhow!("failed to update display name: {e}"))?;
    }

    Ok(())
}

/// Returns `true` if a provider profile is present (and parseable) in the store.
pub fn provider_is_configured(profile: &str) -> bool {
    load_provider_config(profile).is_ok()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_config_roundtrip() {
        let cfg = ProviderConfig::openai("sk-test", None);
        let json = cfg.to_json().unwrap();
        assert!(json.contains("open_ai"));
        assert!(json.contains("sk-test"));
        let restored = ProviderConfig::from_json(&json).unwrap();
        let ProviderConfig::OpenAi(c) = restored else {
            panic!("wrong variant");
        };
        assert_eq!(c.api_key, "sk-test");
        assert!(c.base_url.is_none());
    }

    #[test]
    fn openai_config_with_custom_url() {
        let cfg = ProviderConfig::openai("sk-test", Some("https://custom.example.com/v1".into()));
        let json = cfg.to_json().unwrap();
        let restored = ProviderConfig::from_json(&json).unwrap();
        let ProviderConfig::OpenAi(c) = restored else {
            panic!("wrong variant");
        };
        assert_eq!(c.base_url.as_deref(), Some("https://custom.example.com/v1"));
    }

    #[test]
    fn anthropic_config_roundtrip() {
        let cfg = ProviderConfig::anthropic("ant-key", None);
        let json = cfg.to_json().unwrap();
        let restored = ProviderConfig::from_json(&json).unwrap();
        let ProviderConfig::Anthropic(c) = restored else {
            panic!("wrong variant");
        };
        assert_eq!(c.api_key, "ant-key");
    }

    #[test]
    fn copilot_config_roundtrip() {
        let cfg = ProviderConfig::copilot("gh-oauth-token", None);
        let json = cfg.to_json().unwrap();
        let restored = ProviderConfig::from_json(&json).unwrap();
        let ProviderConfig::Copilot(c) = restored else {
            panic!("wrong variant");
        };
        assert_eq!(c.oauth_token.as_deref(), Some("gh-oauth-token"));
    }

    #[test]
    fn dashscope_oauth_config_roundtrip() {
        let cfg = ProviderConfig::dashscope_oauth("at-abc", "rt-xyz", 9999999999_i64, None);
        let json = cfg.to_json().unwrap();
        let restored = ProviderConfig::from_json(&json).unwrap();
        let ProviderConfig::DashScope(c) = restored else {
            panic!("wrong variant");
        };
        assert_eq!(c.access_token.as_deref(), Some("at-abc"));
        assert_eq!(c.refresh_token.as_deref(), Some("rt-xyz"));
        assert_eq!(c.expiry_date, Some(9999999999_i64));
    }

    #[test]
    fn llamacpp_config_roundtrip() {
        let cfg = ProviderConfig::llamacpp("/models/qwen.gguf");
        let json = cfg.to_json().unwrap();
        let restored = ProviderConfig::from_json(&json).unwrap();
        let ProviderConfig::LlamaCpp(c) = restored else {
            panic!("wrong variant");
        };
        assert_eq!(c.model_path, PathBuf::from("/models/qwen.gguf"));
        assert_eq!(c.context_size, 4096);
    }

    #[test]
    fn build_openai_provider() {
        let cfg = ProviderConfig::openai("sk-build-test", None);
        let _provider = cfg.build();
        // Just needs to not panic.
    }

    #[test]
    fn build_llamacpp_provider() {
        let cfg = ProviderConfig::llamacpp("/tmp/test.gguf");
        let _provider = cfg.build();
    }

    #[test]
    fn build_dashscope_oauth_provider() {
        let cfg = ProviderConfig::dashscope_oauth("at-test", "rt-test", 9999999999_i64, None);
        let _provider = cfg.build();
    }
}
