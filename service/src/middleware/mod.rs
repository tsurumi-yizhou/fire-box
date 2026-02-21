//! Middleware layer for the fire-box service.
//!
//! This layer sits between the IPC interface and the provider implementations,
//! handling:
//! - **keyring**: OS keychain abstraction for secrets
//! - **store**: Encrypted local storage for configurations
//! - **route**: Model routing and failover logic
//! - **metrics**: Request/response metrics collection
//! - **metadata**: AI provider and model metadata management

pub mod keyring;
pub mod metadata;
pub mod metrics;
pub mod route;
pub mod store;

// Re-export commonly used types
pub use keyring::{get_password, set_password};
pub use metadata::{MetadataManager, Model, Vendor};
pub use store::{load, update};
