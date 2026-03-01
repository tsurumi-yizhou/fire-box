//! Error types for FireBox client SDK.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IPC communication error: {0}")]
    Ipc(String),

    #[error("Service returned error: {0}")]
    Service(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid response format: {0}")]
    InvalidResponse(String),

    #[error("Stream error: {0}")]
    Stream(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Not supported on this platform")]
    PlatformNotSupported,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
