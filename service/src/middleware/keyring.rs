//! OS keychain abstraction for storing secrets.
//!
//! Uses the `keyring` crate to provide a cross-platform interface to:
//! - macOS: Keychain
//! - Linux: Secret Service (GNOME Keyring / KWallet)
//! - Windows: Credential Manager

use anyhow::Result;

/// Store a password in the OS keychain.
pub fn set_password(service: &str, user: &str, secret: &str) -> Result<()> {
    let entry = keyring::Entry::new(service, user)?;
    entry.set_password(secret)?;
    Ok(())
}

/// Retrieve a password from the OS keychain.
pub fn get_password(service: &str, user: &str) -> Result<String> {
    let entry = keyring::Entry::new(service, user)?;
    let password = entry.get_password()?;
    Ok(password)
}

/// Delete a password from the OS keychain.
pub fn delete_password(service: &str, user: &str) -> Result<()> {
    let entry = keyring::Entry::new(service, user)?;
    // keyring 3.x uses delete_credential instead of delete_password
    entry.delete_credential()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires actual keychain access
    fn test_keyring_roundtrip() {
        let service = "fire-box-test";
        let user = "test-user";
        let secret = "test-secret";

        set_password(service, user, secret).unwrap();
        let retrieved = get_password(service, user).unwrap();
        assert_eq!(retrieved, secret);

        delete_password(service, user).unwrap();
    }
}
