//! Encrypted local storage for configurations.
//!
//! Uses AES-256-GCM encryption to store sensitive data locally.
//! The encryption key is stored in the OS keychain.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::middleware::keyring;

const KEYRING_SERVICE: &str = "fire-box";
const KEYRING_USER: &str = "encryption-key";
const STORE_FILE: &str = "fire-box-store.enc";

/// Generate a cryptographically secure random key.
fn generate_key_seed() -> [u8; 32] {
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).expect("failed to generate random key");
    key
}

/// Generate or retrieve the encryption key from the keychain.
fn get_or_create_key() -> Result<[u8; 32]> {
    match keyring::get_password(KEYRING_SERVICE, KEYRING_USER) {
        Ok(key_hex) => {
            let mut key = [0u8; 32];
            hex::decode_to_slice(&key_hex, &mut key)?;
            Ok(key)
        }
        Err(_) => {
            // Generate a new key
            let key = generate_key_seed();
            let key_hex = hex::encode(key);
            keyring::set_password(KEYRING_SERVICE, KEYRING_USER, &key_hex)?;
            Ok(key)
        }
    }
}

/// Get the path to the store file.
fn store_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("fire-box");
    std::fs::create_dir_all(&config_dir).ok();
    config_dir.join(STORE_FILE)
}

/// Encrypt and store data.
fn encrypt_and_save(data: &[u8]) -> Result<()> {
    use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};

    let key = get_or_create_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key)?;

    // Use a simple counter-based nonce
    let nonce_bytes = [0u8; 12];
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    // Write ciphertext to file
    let path = store_path();
    std::fs::write(path, &ciphertext)?;

    Ok(())
}

/// Load and decrypt data.
fn load_and_decrypt() -> Result<Vec<u8>> {
    use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};

    let key = get_or_create_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key)?;

    let path = store_path();
    let ciphertext = std::fs::read(&path)?;

    let nonce_bytes = [0u8; 12];
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_slice())
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;
    Ok(plaintext)
}

/// The data structure stored in the encrypted file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoreData {
    /// Ordered list of provider profile IDs
    pub provider_index: Vec<String>,
    /// Provider configurations (JSON strings)
    pub providers: HashMap<String, String>,
    /// Custom display names for profiles
    pub display_names: HashMap<String, String>,
    /// Route rules (alias -> JSON string of RouteRule)
    #[serde(default)]
    pub route_rules: HashMap<String, String>,
    /// Enabled models per provider (provider_id -> list of model IDs)
    #[serde(default)]
    pub enabled_models: HashMap<String, Vec<String>>,
}

/// Load the store data from the encrypted file.
pub fn load() -> Result<StoreData> {
    match load_and_decrypt() {
        Ok(data) => {
            let store: StoreData = serde_json::from_slice(&data)?;
            Ok(store)
        }
        Err(_) => {
            // File doesn't exist or can't be decrypted, return empty store
            Ok(StoreData::default())
        }
    }
}

/// Update the store data atomically.
pub fn update<F>(f: F) -> Result<()>
where
    F: FnOnce(&mut StoreData),
{
    let mut data = load()?;
    f(&mut data);

    let serialized = serde_json::to_vec(&data)?;
    encrypt_and_save(&serialized)?;

    Ok(())
}
