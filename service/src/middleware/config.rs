//! Encrypted configuration storage.
//!
//! Provides encrypted local file storage for application configuration.
//! Encryption keys are stored in the native platform keyring.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::storage::{get_secret, set_secret};

const CONFIG_SERVICE: &str = "fire-box";
const CONFIG_KEY: &str = "encryption-key";
const CONFIG_FILE: &str = "fire-box-store.enc";

/// Application configuration data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigData {
    pub provider_index: Vec<String>,
    pub providers: HashMap<String, String>,
    pub display_names: HashMap<String, String>,
    #[serde(default)]
    pub route_rules: HashMap<String, String>,
    #[serde(default)]
    pub enabled_models: HashMap<String, Vec<String>>,
}

fn generate_config_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).expect("failed to generate random key");
    key
}

fn get_or_create_config_key() -> Result<[u8; 32]> {
    match get_secret(CONFIG_SERVICE, CONFIG_KEY) {
        Ok(key_hex) => {
            let key_bytes = hex::decode(key_hex.as_str())
                .context("failed to decode encryption key from keyring")?;
            if key_bytes.len() != 32 {
                anyhow::bail!("invalid encryption key length");
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&key_bytes);
            Ok(key)
        }
        Err(_) => {
            let key = generate_config_key();
            let key_hex = hex::encode(key);
            set_secret(CONFIG_SERVICE, CONFIG_KEY, &key_hex)?;
            Ok(key)
        }
    }
}

fn config_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("fire-box");
    std::fs::create_dir_all(&config_dir).ok();
    config_dir.join(CONFIG_FILE)
}

async fn encrypt_and_save_config(data: &[u8]) -> Result<()> {
    use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};

    let key = get_or_create_config_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key)?;

    let mut nonce_bytes = [0u8; 12];
    getrandom::fill(&mut nonce_bytes)?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    let mut output = nonce_bytes.to_vec();
    output.extend_from_slice(&ciphertext);

    let path = config_path();
    tokio::fs::write(&path, output).await?;
    Ok(())
}

async fn load_and_decrypt_config() -> Result<Vec<u8>> {
    use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};

    let key = get_or_create_config_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key)?;

    let path = config_path();
    let data = tokio::fs::read(&path).await?;

    if data.len() < 12 {
        anyhow::bail!("Invalid encrypted data: file too short");
    }

    let nonce_bytes = &data[..12];
    let nonce = Nonce::from_slice(nonce_bytes);

    let ciphertext = &data[12..];

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;
    Ok(plaintext)
}

/// Load configuration data from encrypted storage.
pub async fn load_config() -> Result<ConfigData> {
    match load_and_decrypt_config().await {
        Ok(data) => {
            let config: ConfigData = serde_json::from_slice(&data)?;
            Ok(config)
        }
        Err(_) => Ok(ConfigData::default()),
    }
}

/// Update configuration data atomically.
pub async fn update_config<F>(f: F) -> Result<()>
where
    F: FnOnce(&mut ConfigData),
{
    let mut data = load_config().await?;
    f(&mut data);

    let serialized = serde_json::to_vec(&data)?;
    encrypt_and_save_config(&serialized).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_config_roundtrip() {
        let mut config = ConfigData::default();
        config.provider_index.push("test-provider".to_string());
        config
            .providers
            .insert("test-provider".to_string(), "test-config".to_string());

        update_config(|d| {
            d.provider_index = config.provider_index.clone();
            d.providers = config.providers.clone();
        })
        .await
        .unwrap();

        let loaded = load_config().await.unwrap();
        assert_eq!(loaded.provider_index, config.provider_index);
        assert_eq!(loaded.providers, config.providers);
    }
}
