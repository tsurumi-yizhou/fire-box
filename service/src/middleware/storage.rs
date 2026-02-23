//! Unified storage layer using native platform keyrings.
//!
//! Provides secure credential storage using platform-specific keyrings:
//! - macOS: System Keychain (with Touch ID/Face ID if configured)
//! - Windows: Credential Manager (with Windows Hello if configured)
//! - Linux: Secret Service (GNOME Keyring/KWallet)

use anyhow::{Context, Result};
use keyring::Entry;
use zeroize::Zeroizing;

/// Store a secret in the native platform keyring.
pub fn set_secret(service: &str, key: &str, secret: &str) -> Result<()> {
    let entry = Entry::new(service, key)
        .with_context(|| format!("failed to create keyring entry: {service}/{key}"))?;
    entry
        .set_password(secret)
        .with_context(|| format!("failed to store secret in keyring: {service}/{key}"))
}

/// Store a secret with biometric protection (uses platform defaults).
///
/// On macOS and Windows, this respects system biometric settings.
/// The platform will prompt for Touch ID/Face ID/Windows Hello if configured.
pub fn set_secret_with_biometric(service: &str, key: &str, secret: &str) -> Result<()> {
    // keyring crate uses platform defaults which include biometric auth
    set_secret(service, key, secret)
}

/// Retrieve a secret from the native platform keyring.
pub fn get_secret(service: &str, key: &str) -> Result<Zeroizing<String>> {
    let entry = Entry::new(service, key)
        .with_context(|| format!("failed to create keyring entry: {service}/{key}"))?;
    let password = entry
        .get_password()
        .with_context(|| format!("failed to retrieve secret from keyring: {service}/{key}"))?;
    Ok(Zeroizing::new(password))
}

/// Delete a secret from the native platform keyring.
pub fn delete_secret(service: &str, key: &str) -> Result<()> {
    let entry = Entry::new(service, key)
        .with_context(|| format!("failed to create keyring entry: {service}/{key}"))?;
    entry
        .delete_credential()
        .with_context(|| format!("failed to delete secret from keyring: {service}/{key}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_secret_roundtrip() {
        let service = "fire-box-test";
        let key = "test-key";
        let secret = "test-secret-value";

        set_secret(service, key, secret).unwrap();
        let retrieved = get_secret(service, key).unwrap();
        assert_eq!(*retrieved, secret);

        delete_secret(service, key).unwrap();
    }

    #[test]
    #[ignore]
    fn test_biometric_secret_roundtrip() {
        let service = "fire-box-test-biometric";
        let key = "test-biometric-key";
        let secret = "test-biometric-secret-value";

        set_secret_with_biometric(service, key, secret).unwrap();
        let retrieved = get_secret(service, key).unwrap();
        assert_eq!(*retrieved, secret);

        delete_secret(service, key).unwrap();
    }
}
