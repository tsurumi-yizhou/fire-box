//! Model routing and failover logic.
//!
//! Routes model aliases to concrete provider+model pairs,
//! supporting ordered failover targets.

use std::collections::HashMap;
use std::sync::RwLock;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::middleware::store;

/// A route target: provider + model pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteTarget {
    pub provider_id: String,
    pub model_id: String,
}

/// A route rule: maps an alias to ordered targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRule {
    pub alias: String,
    pub targets: Vec<RouteTarget>,
}

/// Model enabled state per provider.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelEnabledState {
    /// Map of provider_id -> list of enabled model IDs
    pub enabled_models: HashMap<String, Vec<String>>,
}

// Store data in memory with file persistence
static ROUTE_DATA: RwLock<Option<RouteData>> = RwLock::new(None);

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

/// Initialize route storage from file.
pub fn init() -> Result<()> {
    let store_data = store::load()?;

    let mut rules = HashMap::new();
    for (alias, json) in store_data.route_rules {
        match serde_json::from_str::<RouteRule>(&json) {
            Ok(rule) => {
                rules.insert(alias, rule);
            }
            Err(e) => {
                log::warn!("Failed to parse route rule for alias '{}': {}", alias, e);
            }
        }
    }

    let data = RouteData {
        rules,
        enabled_models: store_data.enabled_models,
    };

    *ROUTE_DATA.write().unwrap() = Some(data);
    Ok(())
}

/// Set route rules for a specific alias.
pub fn set_route_rules(alias: &str, targets: Vec<RouteTarget>) -> Result<()> {
    let mut data = ROUTE_DATA.write().unwrap();
    let data = data.as_mut().unwrap();

    let rule = RouteRule {
        alias: alias.to_string(),
        targets,
    };
    data.rules.insert(alias.to_string(), rule);

    // Persist to store
    persist()?;
    Ok(())
}

/// Get route rules for a specific alias.
pub fn get_route_rules(alias: &str) -> Result<Option<RouteRule>> {
    let data = ROUTE_DATA.read().unwrap();
    let data = data.as_ref().unwrap();
    Ok(data.rules.get(alias).cloned())
}

/// Get all route rules.
pub fn get_all_rules() -> Result<Vec<RouteRule>> {
    let data = ROUTE_DATA.read().unwrap();
    let data = data.as_ref().unwrap();
    Ok(data.rules.values().cloned().collect())
}

/// Delete route rules for an alias.
pub fn delete_route_rules(alias: &str) -> Result<()> {
    let mut data = ROUTE_DATA.write().unwrap();
    let data = data.as_mut().unwrap();
    data.rules.remove(alias);
    persist()?;
    Ok(())
}

/// Resolve an alias to a target.
/// Returns the first available target (no health checking).
/// Returns (provider_id, model_id) on success.
pub fn resolve_alias(alias: &str) -> Result<(String, String)> {
    let rules = get_route_rules(alias)?;
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
pub fn get_next_target(alias: &str, current_provider_id: &str) -> Result<Option<(String, String)>> {
    let rules = get_route_rules(alias)?;
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
pub fn load_enabled_models(provider_id: &str) -> Option<Vec<String>> {
    let data = ROUTE_DATA.read().unwrap();
    data.as_ref()
        .unwrap()
        .enabled_models
        .get(provider_id)
        .cloned()
}

/// Save the set of enabled models for a provider.
pub fn save_enabled_models(provider_id: &str, models: &[String]) -> Result<()> {
    let mut data = ROUTE_DATA.write().unwrap();
    let data = data.as_mut().unwrap();
    data.enabled_models
        .insert(provider_id.to_string(), models.to_vec());
    persist()?;
    Ok(())
}

/// Check if a model is enabled for a provider.
pub fn is_model_enabled(provider_id: &str, model_id: &str) -> bool {
    match load_enabled_models(provider_id) {
        Some(enabled) => enabled.iter().any(|m| m == model_id),
        None => true, // None means all enabled
    }
}

/// Toggle a model's enabled state.
pub fn toggle_model(
    provider_id: &str,
    model_id: &str,
    enabled: bool,
    all_models: &[String],
) -> Result<()> {
    let mut current = load_enabled_models(provider_id).unwrap_or_else(|| all_models.to_vec());
    if enabled {
        if !current.iter().any(|m| m == model_id) {
            current.push(model_id.to_string());
        }
    } else {
        current.retain(|m| m != model_id);
    }
    save_enabled_models(provider_id, &current)
}

/// List all enabled models for a provider.
pub fn list_enabled_models(provider_id: &str, all_models: &[String]) -> Vec<String> {
    match load_enabled_models(provider_id) {
        Some(enabled) => enabled,
        None => all_models.to_vec(),
    }
}

/// Persist route data to store.
fn persist() -> Result<()> {
    let data_guard = ROUTE_DATA.read().unwrap();
    if let Some(data) = data_guard.as_ref() {
        // Serialize rules to JSON strings
        let rules_map: HashMap<String, String> = data
            .rules
            .iter()
            .map(|(alias, rule)| {
                (
                    alias.clone(),
                    serde_json::to_string(rule).unwrap_or_default(),
                )
            })
            .collect();

        let enabled_models = data.enabled_models.clone();

        store::update(|store| {
            store.route_rules = rules_map;
            store.enabled_models = enabled_models;
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_alias_without_rule() {
        let _ = init();
        let result = resolve_alias("gpt-4").unwrap();
        assert_eq!(result, ("default".to_string(), "gpt-4".to_string()));
    }

    #[test]
    fn test_resolve_alias_with_rule() {
        let _ = init();
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
        set_route_rules("my-model", targets).unwrap();

        let result = resolve_alias("my-model").unwrap();
        assert_eq!(result, ("openai".to_string(), "gpt-4".to_string()));
    }

    #[test]
    fn test_get_next_target() {
        let _ = init();
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
        set_route_rules("my-model", targets).unwrap();

        let next = get_next_target("my-model", "openai").unwrap();
        assert_eq!(
            next,
            Some(("anthropic".to_string(), "claude-3".to_string()))
        );

        let next = get_next_target("my-model", "anthropic").unwrap();
        assert_eq!(next, None);
    }

    #[test]
    fn test_model_enabled() {
        let _ = init();
        let _all_models = ["gpt-4".to_string(), "gpt-3.5".to_string()];

        // All enabled by default
        assert!(is_model_enabled("openai", "gpt-4"));

        // Enable specific models
        save_enabled_models("openai", &["gpt-4".to_string()]).unwrap();
        assert!(is_model_enabled("openai", "gpt-4"));
        assert!(!is_model_enabled("openai", "gpt-3.5"));
    }
}
