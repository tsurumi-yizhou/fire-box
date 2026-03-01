//! FireBox Client SDK
//!
//! A Rust SDK for interacting with the FireBox AI service.
//!
//! # Example
//!
//! ```no_run
//! use firebox_client::{FireBoxClient, CompletionRequest, ChatMessage};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let client = FireBoxClient::new()?;
//!
//! // Check if service is running
//! client.ping()?;
//!
//! // Add a provider
//! client.add_api_key_provider(
//!     "OpenAI",
//!     "openai",
//!     "sk-...",
//!     None,
//! )?;
//!
//! // List models
//! let models = client.get_all_models(false)?;
//!
//! // Send a completion request
//! let request = CompletionRequest {
//!     model_id: "gpt-4".to_string(),
//!     messages: vec![
//!         ChatMessage {
//!             role: "user".to_string(),
//!             content: "Hello!".to_string(),
//!             name: None,
//!             tool_calls: None,
//!             tool_call_id: None,
//!         },
//!     ],
//!     tools: vec![],
//!     temperature: Some(0.7),
//!     max_tokens: None,
//! };
//!
//! let response = client.complete(&request)?;
//! println!("Response: {}", response.content);
//! # Ok(())
//! # }
//! ```

mod client;
mod error;
mod stream;
mod types;

#[cfg(target_os = "macos")]
mod xpc;

#[cfg(target_os = "linux")]
mod dbus;

#[cfg(target_os = "windows")]
mod com;

pub use client::FireBoxClient;
pub use error::{Error, Result};
pub use stream::StreamReader;
pub use types::*;
