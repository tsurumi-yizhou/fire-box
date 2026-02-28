//! Middleware layer for the fire-box service.
//!
//! This layer sits between the IPC interface and the provider implementations,
//! handling:
//! - **storage**: Secure credential storage in native platform keyrings
//! - **config**: Encrypted configuration file management
//! - **route**: Model routing and failover logic
//! - **metrics**: Request/response metrics collection
//! - **metadata**: AI provider and model metadata management
//! - **access**: TOFU allowlist and connection access control

pub mod access;
pub mod config;
pub mod metadata;
pub mod metrics;
pub mod route;
pub mod storage;

// Re-export commonly used types
pub use config::{AccessEntry, AccessStatus, ConfigData, load_config, update_config};
pub use metadata::{MetadataManager, Model, Vendor};
pub use storage::{delete_secret, get_secret, set_secret, set_secret_with_biometric};
