pub mod anthropic;
pub mod config;
pub mod copilot;
pub mod dashscope;
pub mod llamacpp;
pub mod openai;
pub mod retry;

// Re-export for convenience
pub use openai::OpenAiProvider;
pub use retry::{RetryConfig, with_retry};

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_stream::Stream;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Provider-level errors that can be mapped into [`anyhow::Error`].
#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("rate limit exceeded, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("model not found: {0}")]
    ModelNotFound(String),
    #[error("request failed: {0}")]
    RequestFailed(String),
    #[error("streaming error: {0}")]
    Stream(String),
}

// ---------------------------------------------------------------------------
// Common types
// ---------------------------------------------------------------------------

/// A boxed, pinned, sendable stream.
pub type BoxStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>;

/// A chat message with a role and content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Request for a chat completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub stream: bool,
}

/// A single completion choice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

/// Token usage information.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Response from a chat completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

/// Request for embeddings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: Vec<String>,
}

/// A single embedding vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    pub index: u32,
    pub embedding: Vec<f64>,
}

/// Response from an embeddings request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub model: String,
    pub data: Vec<Embedding>,
    pub usage: Option<Usage>,
}

/// A streaming event from a completion.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A content delta.
    Delta { content: String },
    /// The stream has finished.
    Done,
    /// An error occurred.
    Error { message: String },
}

// ---------------------------------------------------------------------------
// Provider trait
// ---------------------------------------------------------------------------

/// Common interface for all model providers.
///
/// Each provider adapts a specific backend API to the unified internal types.
/// Implementations should be kept thin: translate requests, handle auth/quotas
/// via middleware, and return responses in the unified format.
pub trait Provider: Send + Sync {
    /// Perform a chat completion.
    ///
    /// `session_id` identifies the virtual session.  Stateless providers
    /// (OpenAI, Anthropic, …) may ignore this parameter.
    fn complete(
        &self,
        session_id: &str,
        request: &CompletionRequest,
    ) -> impl Future<Output = anyhow::Result<CompletionResponse>> + Send;

    /// Perform a streaming chat completion.
    ///
    /// `session_id` identifies the virtual session.
    fn complete_stream(
        &self,
        session_id: &str,
        request: &CompletionRequest,
    ) -> impl Future<Output = anyhow::Result<BoxStream<anyhow::Result<StreamEvent>>>> + Send;

    /// Generate embeddings for the given input texts.
    ///
    /// `session_id` identifies the virtual session.
    fn embed(
        &self,
        session_id: &str,
        request: &EmbeddingRequest,
    ) -> impl Future<Output = anyhow::Result<EmbeddingResponse>> + Send;

    /// List available models from this provider.
    ///
    /// Returns a list of model IDs that can be used for completions.
    /// Implementations should fetch from the provider's API when possible.
    fn list_models(&self) -> impl Future<Output = anyhow::Result<Vec<String>>> + Send {
        // Default implementation returns empty list
        async { Ok(Vec::new()) }
    }
}

// ---------------------------------------------------------------------------
// Dyn-compatible wrapper for Provider
// ---------------------------------------------------------------------------

/// Boxed future alias for a streaming completion result.
type StreamFuture<'a> = Pin<
    Box<dyn Future<Output = anyhow::Result<BoxStream<anyhow::Result<StreamEvent>>>> + Send + 'a>,
>;

/// Object-safe version of [`Provider`], using boxed futures.
///
/// Auto-implemented for every `T: Provider + Send + Sync + 'static`.
/// Use `Arc<dyn ProviderDyn>` wherever you need to erase the concrete type.
pub trait ProviderDyn: Send + Sync {
    fn complete_dyn<'a>(
        &'a self,
        session_id: &'a str,
        request: &'a CompletionRequest,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<CompletionResponse>> + Send + 'a>>;

    fn complete_stream_dyn<'a>(
        &'a self,
        session_id: &'a str,
        request: &'a CompletionRequest,
    ) -> StreamFuture<'a>;

    fn embed_dyn<'a>(
        &'a self,
        session_id: &'a str,
        request: &'a EmbeddingRequest,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<EmbeddingResponse>> + Send + 'a>>;

    fn list_models_dyn<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<String>>> + Send + 'a>>;
}

impl<T: Provider + Send + Sync + 'static> ProviderDyn for T {
    fn complete_dyn<'a>(
        &'a self,
        session_id: &'a str,
        request: &'a CompletionRequest,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<CompletionResponse>> + Send + 'a>> {
        Box::pin(self.complete(session_id, request))
    }

    fn complete_stream_dyn<'a>(
        &'a self,
        session_id: &'a str,
        request: &'a CompletionRequest,
    ) -> StreamFuture<'a> {
        Box::pin(self.complete_stream(session_id, request))
    }

    fn embed_dyn<'a>(
        &'a self,
        session_id: &'a str,
        request: &'a EmbeddingRequest,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<EmbeddingResponse>> + Send + 'a>> {
        Box::pin(self.embed(session_id, request))
    }

    fn list_models_dyn<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<String>>> + Send + 'a>> {
        Box::pin(self.list_models())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_completion_request() {
        let request = CompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            max_tokens: Some(100),
            temperature: None,
            stream: false,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("gpt-4"));
        assert!(json.contains("Hello"));
        // temperature is None, should be skipped
        assert!(!json.contains("temperature"));
    }

    #[test]
    fn deserialize_completion_response() {
        let json = r#"{
            "id": "test-id",
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hi"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 5, "completion_tokens": 1, "total_tokens": 6}
        }"#;
        let response: CompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "test-id");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].message.content, "Hi");
        assert_eq!(response.usage.unwrap().total_tokens, 6);
    }

    #[test]
    fn serialize_embedding_request() {
        let request = EmbeddingRequest {
            model: "text-embedding-ada-002".to_string(),
            input: vec!["hello world".to_string()],
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("text-embedding-ada-002"));
        assert!(json.contains("hello world"));
    }

    #[test]
    fn provider_error_display() {
        let err = ProviderError::RateLimited {
            retry_after_secs: 30,
        };
        assert_eq!(err.to_string(), "rate limit exceeded, retry after 30s");

        let err = ProviderError::ModelNotFound("unknown".to_string());
        assert_eq!(err.to_string(), "model not found: unknown");
    }

    // -----------------------------------------------------------------------
    // Mock provider & trait tests
    // -----------------------------------------------------------------------

    /// A mock provider that returns deterministic responses.
    struct MockProvider;

    impl Provider for MockProvider {
        async fn complete(
            &self,
            _session_id: &str,
            request: &CompletionRequest,
        ) -> anyhow::Result<CompletionResponse> {
            Ok(CompletionResponse {
                id: "mock-id".to_string(),
                model: request.model.clone(),
                choices: vec![Choice {
                    index: 0,
                    message: ChatMessage {
                        role: "assistant".to_string(),
                        content: "mock response".to_string(),
                    },
                    finish_reason: Some("stop".to_string()),
                }],
                usage: Some(Usage {
                    prompt_tokens: 5,
                    completion_tokens: 2,
                    total_tokens: 7,
                }),
            })
        }

        async fn complete_stream(
            &self,
            _session_id: &str,
            _request: &CompletionRequest,
        ) -> anyhow::Result<BoxStream<anyhow::Result<StreamEvent>>> {
            anyhow::bail!("mock: streaming not implemented")
        }

        async fn embed(
            &self,
            _session_id: &str,
            _request: &EmbeddingRequest,
        ) -> anyhow::Result<EmbeddingResponse> {
            anyhow::bail!("mock: embedding not implemented")
        }
    }

    #[tokio::test]
    async fn mock_provider_complete() {
        let provider = MockProvider;
        let req = CompletionRequest {
            model: "test-model".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            max_tokens: None,
            temperature: None,
            stream: false,
        };

        let resp = provider.complete("session-1", &req).await.unwrap();
        assert_eq!(resp.choices[0].message.content, "mock response");
        assert_eq!(resp.model, "test-model");
    }

    #[tokio::test]
    async fn mock_provider_session_id_is_accepted() {
        let provider = MockProvider;
        let req = CompletionRequest {
            model: "m".to_string(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stream: false,
        };
        // Different session IDs should all work – the mock ignores them.
        assert!(provider.complete("s1", &req).await.is_ok());
        assert!(provider.complete("s2", &req).await.is_ok());
    }
}
