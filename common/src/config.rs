use crate::keystore;
use std::collections::HashMap;

// Re-export keystore types for backward compatibility.
pub use keystore::{ProviderInfo as ProviderConfig, ProviderMapping, ProviderType as ProtocolType};

/// Runtime configuration loaded from OS keyring.
#[derive(Debug, Clone)]
pub struct Config {
    pub settings: keystore::ServiceSettings,
    pub providers: Vec<keystore::ProviderInfo>,
    pub models: HashMap<String, Vec<keystore::ProviderMapping>>,
}

impl Config {
    /// Load configuration from OS keyring.
    pub fn load_from_keyring() -> Self {
        Self {
            settings: keystore::load_settings(),
            providers: keystore::load_providers(),
            models: keystore::load_models(),
        }
    }

    /// Save current configuration to OS keyring.
    pub fn save_to_keyring(&self) -> anyhow::Result<()> {
        keystore::save_settings(&self.settings)?;
        keystore::save_providers(&self.providers)?;
        keystore::save_models(&self.models)?;
        Ok(())
    }
}
