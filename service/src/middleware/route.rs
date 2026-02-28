//! Model routing and failover logic.
//!
//! Routes model aliases to concrete provider+model pairs,
//! supporting ordered failover targets.

use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::{OnceCell, RwLock};

use crate::middleware::config;
use crate::middleware::metadata::MetadataManager;

/// A route target: provider + model pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteTarget {
    pub provider_id: String,
    pub model_id: String,
}

/// Required capabilities for a virtual model route contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteCapabilities {
    #[serde(default = "default_true")]
    pub chat: bool,
    #[serde(default = "default_true")]
    pub streaming: bool,
    #[serde(default)]
    pub embeddings: bool,
    #[serde(default)]
    pub vision: bool,
    #[serde(default)]
    pub tool_calling: bool,
}

fn default_true() -> bool {
    true
}

impl Default for RouteCapabilities {
    fn default() -> Self {
        Self {
            chat: true,
            streaming: true,
            embeddings: false,
            vision: false,
            tool_calling: false,
        }
    }
}

/// Additional metadata and constraints for a virtual model route.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RouteMetadata {
    pub context_window: Option<u32>,
    pub pricing_tier: Option<String>,
    #[serde(default)]
    pub strengths: Vec<String>,
    pub description: Option<String>,
}

/// Routing strategy for a virtual model.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RouteStrategy {
    /// Try targets in order; fall over to the next on failure.
    #[default]
    Failover,
    /// Pick a random target for each request.
    Random,
}

/// A route rule: defines a virtual model and its required capability contract, mapping to targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRule {
    #[serde(default)]
    pub alias: String,
    pub virtual_model_id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub capabilities: RouteCapabilities,
    #[serde(default)]
    pub metadata: RouteMetadata,
    pub targets: Vec<RouteTarget>,
    #[serde(default)]
    pub strategy: RouteStrategy,
}

/// Model enabled state per provider.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelEnabledState {
    /// Map of provider_id -> list of enabled model IDs
    pub enabled_models: HashMap<String, Vec<String>>,
}

// Store data in memory with file persistence
// Uses OnceCell for thread-safe lazy async initialization
static ROUTE_DATA: OnceCell<RwLock<RouteData>> = OnceCell::const_new();

/// Get the route data lock, returning an error if not yet initialized.
fn route_data() -> Result<&'static RwLock<RouteData>> {
    ROUTE_DATA
        .get()
        .ok_or_else(|| anyhow::anyhow!("ROUTE_DATA not initialized"))
}

/// Ensure route data is initialized, loading from store if necessary.
/// This is called automatically by all public functions.
async fn ensure_initialized() {
    ROUTE_DATA
        .get_or_init(|| async {
            let data = match config::load_config().await {
                Ok(store_data) => {
                    let mut rules = HashMap::new();
                    for (alias, json) in store_data.route_rules {
                        match serde_json::from_str::<serde_json::Value>(&json) {
                            Ok(mut val) => {
                                // Migration: handle older format with 'alias' instead of 'virtual_model_id'
                                if let Some(obj) = val.as_object_mut()
                                    && !obj.contains_key("virtual_model_id")
                                    && obj.contains_key("alias")
                                    && let Some(alias_val) = obj.remove("alias")
                                {
                                    obj.insert("virtual_model_id".to_string(), alias_val.clone());
                                    if !obj.contains_key("display_name") {
                                        obj.insert("display_name".to_string(), alias_val);
                                    }
                                }

                                match serde_json::from_value::<RouteRule>(val) {
                                    Ok(mut rule) => {
                                        if rule.alias.is_empty() {
                                            rule.alias = rule.virtual_model_id.clone();
                                        }
                                        rules.insert(alias, rule);
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            "Failed to map route rule for alias '{}': {}",
                                            alias,
                                            e
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to parse route rule JSON for alias '{}': {}",
                                    alias,
                                    e
                                );
                            }
                        }
                    }

                    RouteData {
                        rules,
                        enabled_models: store_data.enabled_models,
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to load route data from store: {}", e);
                    RouteData::default()
                }
            };
            RwLock::new(data)
        })
        .await;
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RouteData {
    /// Map of alias -> route rule
    pub rules: HashMap<String, RouteRule>,
    /// Map of provider_id -> list of enabled model IDs
    pub enabled_models: HashMap<String, Vec<String>>,
}

// ---------------------------------------------------------------------------
// Route rules management
// ---------------------------------------------------------------------------

/// Set route rules for a specific virtual model using default metadata and strategy.
///
/// This is a compatibility helper retained for older call sites.
pub async fn set_route_rules(virtual_model_id: &str, targets: Vec<RouteTarget>) -> Result<()> {
    set_route_rules_with_options(
        virtual_model_id,
        virtual_model_id,
        RouteCapabilities::default(),
        RouteMetadata::default(),
        targets,
        RouteStrategy::default(),
    )
    .await
}

/// Set route rules for a specific virtual model.
pub async fn set_route_rules_with_options(
    virtual_model_id: &str,
    display_name: &str,
    capabilities: RouteCapabilities,
    metadata: RouteMetadata,
    targets: Vec<RouteTarget>,
    strategy: RouteStrategy,
) -> Result<()> {
    // Validate capabilities against models.dev data
    let mut manager = MetadataManager::new();
    // Pre-load metadata; failure is non-fatal (allows offline operation).
    if let Err(e) = manager.get().await {
        tracing::debug!("Metadata pre-load skipped (offline?): {e}");
    }

    for target in &targets {
        manager
            .check_capabilities(
                &target.provider_id,
                &target.model_id,
                capabilities.vision,
                capabilities.tool_calling,
            )
            .await
            .with_context(|| {
                format!(
                    "Target {}:{} does not meet capability contract",
                    target.provider_id, target.model_id
                )
            })?;
    }

    ensure_initialized().await;
    {
        let lock = route_data()?;
        let mut data = lock.write().await;

        let rule = RouteRule {
            alias: virtual_model_id.to_string(),
            virtual_model_id: virtual_model_id.to_string(),
            display_name: display_name.to_string(),
            capabilities,
            metadata,
            targets,
            strategy,
        };
        data.rules.insert(virtual_model_id.to_string(), rule);
    } // Drop lock before persisting

    // Persist to store
    persist().await?;
    Ok(())
}

/// Get route rules for a specific alias.
pub async fn get_route_rules(alias: &str) -> Result<Option<RouteRule>> {
    ensure_initialized().await;
    let lock = ROUTE_DATA.get().expect("ROUTE_DATA should be initialized");
    let data = lock.read().await;
    Ok(data.rules.get(alias).cloned())
}

/// Get all route rules.
pub async fn get_all_rules() -> Result<Vec<RouteRule>> {
    ensure_initialized().await;
    let lock = ROUTE_DATA.get().expect("ROUTE_DATA should be initialized");
    let data = lock.read().await;
    Ok(data.rules.values().cloned().collect())
}

/// Delete route rules for an alias.
pub async fn delete_route_rules(alias: &str) -> Result<()> {
    ensure_initialized().await;
    {
        let lock = route_data()?;
        let mut data = lock.write().await;
        data.rules.remove(alias);
    } // Drop lock

    persist().await?;
    Ok(())
}

/// Resolve an alias to a target.
/// Returns the first available target (no health checking).
/// Returns (provider_id, model_id) on success.
pub async fn resolve_alias(alias: &str) -> Result<(String, String)> {
    let rules = get_route_rules(alias).await?;
    match rules {
        Some(rule) => {
            if rule.targets.is_empty() {
                anyhow::bail!("No targets for alias: {}", alias);
            }
            // Return first target (failover is handled by caller on error)
            Ok((
                rule.targets[0].provider_id.clone(),
                rule.targets[0].model_id.clone(),
            ))
        }
        None => {
            // If no rule exists, treat alias as direct model reference
            Ok(("default".to_string(), alias.to_string()))
        }
    }
}

/// Get the next target for failover.
/// Returns None if no more targets.
pub async fn get_next_target(
    alias: &str,
    current_provider_id: &str,
) -> Result<Option<(String, String)>> {
    let rules = get_route_rules(alias).await?;
    match rules {
        Some(rule) => {
            let current_idx = rule
                .targets
                .iter()
                .position(|t| t.provider_id == current_provider_id);

            match current_idx {
                Some(idx) if idx + 1 < rule.targets.len() => {
                    let next = &rule.targets[idx + 1];
                    Ok(Some((next.provider_id.clone(), next.model_id.clone())))
                }
                _ => Ok(None),
            }
        }
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Model enabled state management
// ---------------------------------------------------------------------------

/// Load the set of enabled models for a provider.
pub async fn load_enabled_models(provider_id: &str) -> Option<Vec<String>> {
    ensure_initialized().await;
    let lock = ROUTE_DATA.get()?;
    let data = lock.read().await;
    data.enabled_models.get(provider_id).cloned()
}

/// Save the set of enabled models for a provider.
pub async fn save_enabled_models(provider_id: &str, models: &[String]) -> Result<()> {
    ensure_initialized().await;
    {
        let lock = route_data()?;
        let mut data = lock.write().await;
        data.enabled_models
            .insert(provider_id.to_string(), models.to_vec());
    }
    persist().await?;
    Ok(())
}

/// Check if a model is enabled for a provider.
pub async fn is_model_enabled(provider_id: &str, model_id: &str) -> bool {
    match load_enabled_models(provider_id).await {
        Some(enabled) => enabled.iter().any(|m| m == model_id),
        None => true, // None means all enabled
    }
}

/// Toggle a model's enabled state.
pub async fn toggle_model(
    provider_id: &str,
    model_id: &str,
    enabled: bool,
    all_models: &[String],
) -> Result<()> {
    let mut current = load_enabled_models(provider_id)
        .await
        .unwrap_or_else(|| all_models.to_vec());
    if enabled {
        if !current.iter().any(|m| m == model_id) {
            current.push(model_id.to_string());
        }
    } else {
        current.retain(|m| m != model_id);
    }
    save_enabled_models(provider_id, &current).await
}

/// List all enabled models for a provider.
pub async fn list_enabled_models(provider_id: &str, all_models: &[String]) -> Vec<String> {
    match load_enabled_models(provider_id).await {
        Some(enabled) => enabled,
        None => all_models.to_vec(),
    }
}

/// Persist route data to store.
async fn persist() -> Result<()> {
    let (rules_map, enabled_models) = {
        let lock = route_data()?;
        let data = lock.read().await;

        // Serialize rules to JSON strings
        let rules_map: HashMap<String, String> = data
            .rules
            .iter()
            .map(|(id, rule)| (id.clone(), serde_json::to_string(rule).unwrap_or_default()))
            .collect();

        (rules_map, data.enabled_models.clone())
    }; // Drop lock

    config::update_config(move |store| {
        store.route_rules = rules_map;
        store.enabled_models = enabled_models;
    })
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to reset route data between tests
    async fn reset_route_data() {
        if let Some(lock) = ROUTE_DATA.get() {
            let mut data = lock.write().await;
            data.rules.clear();
            data.enabled_models.clear();
        }
    }

    #[tokio::test]
    async fn test_resolve_alias_without_rule() {
        reset_route_data().await;
        let result = resolve_alias("gpt-4").await.unwrap();
        assert_eq!(result, ("default".to_string(), "gpt-4".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_alias_with_rule() {
        reset_route_data().await;
        let targets = vec![
            RouteTarget {
                provider_id: "openai".to_string(),
                model_id: "gpt-4".to_string(),
            },
            RouteTarget {
                provider_id: "anthropic".to_string(),
                model_id: "claude-3".to_string(),
            },
        ];
        set_route_rules_with_options(
            "my-model",
            "My Model",
            RouteCapabilities::default(),
            RouteMetadata::default(),
            targets,
            RouteStrategy::default(),
        )
        .await
        .unwrap();

        let result = resolve_alias("my-model").await.unwrap();
        assert_eq!(result, ("openai".to_string(), "gpt-4".to_string()));
    }

    #[tokio::test]
    async fn test_get_next_target() {
        reset_route_data().await;
        let targets = vec![
            RouteTarget {
                provider_id: "openai".to_string(),
                model_id: "gpt-4".to_string(),
            },
            RouteTarget {
                provider_id: "anthropic".to_string(),
                model_id: "claude-3".to_string(),
            },
        ];
        set_route_rules_with_options(
            "my-model",
            "My Model",
            RouteCapabilities::default(),
            RouteMetadata::default(),
            targets,
            RouteStrategy::default(),
        )
        .await
        .unwrap();

        let next = get_next_target("my-model", "openai").await.unwrap();
        assert_eq!(
            next,
            Some(("anthropic".to_string(), "claude-3".to_string()))
        );

        let next = get_next_target("my-model", "anthropic").await.unwrap();
        assert_eq!(next, None);
    }

    #[tokio::test]
    async fn test_model_enabled() {
        reset_route_data().await;
        let _all_models = ["gpt-4".to_string(), "gpt-3.5".to_string()];

        // All enabled by default
        assert!(is_model_enabled("openai", "gpt-4").await);

        // Enable specific models
        save_enabled_models("openai", &["gpt-4".to_string()])
            .await
            .unwrap();
        assert!(is_model_enabled("openai", "gpt-4").await);
        assert!(!is_model_enabled("openai", "gpt-3.5").await);
    }
}
