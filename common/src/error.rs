//! Fire Box error types using thiserror

use thiserror::Error;

/// Fire Box error type
#[derive(Debug, Error)]
pub enum FireBoxError {
    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("OAuth error: {0}")]
    OAuthError(String),

    #[error("Authentication error: {0}")]
    AuthError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Keyring error: {0}")]
    KeyringError(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Provider not found: {0}")]
    ProviderNotFound(String),
}

impl From<anyhow::Error> for FireBoxError {
    fn from(e: anyhow::Error) -> Self {
        FireBoxError::InternalError(e.to_string())
    }
}
