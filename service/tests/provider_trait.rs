//! Tests for the Provider trait and ProviderDyn wrapper

use firebox_service::providers::{
    BoxStream, ChatMessage, CompletionRequest, CompletionResponse, EmbeddingRequest,
    EmbeddingResponse, Provider, ProviderDyn, ProviderError, StreamEvent, Usage,
};

/// Mock provider for testing the Provider trait
struct MockProvider {
    should_fail: bool,
    stream_supported: bool,
    embed_supported: bool,
}

impl MockProvider {
    fn new() -> Self {
        Self {
            should_fail: false,
            stream_supported: true,
            embed_supported: true,
        }
    }

    fn failing() -> Self {
        Self {
            should_fail: true,
            stream_supported: false,
            embed_supported: false,
        }
    }

    fn no_stream() -> Self {
        Self {
            should_fail: false,
            stream_supported: false,
            embed_supported: true,
        }
    }
}

impl Provider for MockProvider {
    async fn complete(
        &self,
        session_id: &str,
        request: &CompletionRequest,
    ) -> anyhow::Result<CompletionResponse> {
        if self.should_fail {
            anyhow::bail!("Mock provider is configured to fail");
        }

        Ok(CompletionResponse {
            id: format!("mock-{}", session_id),
            model: request.model.clone(),
            choices: vec![firebox_service::providers::Choice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content: format!("Mock response for: {}", request.model),
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: Some(Usage {
                prompt_tokens: request.messages.len() as u32,
                completion_tokens: 10,
                total_tokens: request.messages.len() as u32 + 10,
            }),
        })
    }

    async fn complete_stream(
        &self,
        _session_id: &str,
        _request: &CompletionRequest,
    ) -> anyhow::Result<BoxStream<anyhow::Result<StreamEvent>>> {
        if !self.stream_supported {
            anyhow::bail!("Streaming not supported");
        }

        use futures_util::stream;
        let stream = stream::iter(vec![
            Ok(StreamEvent::Delta {
                content: "Hello ".to_string(),
            }),
            Ok(StreamEvent::Delta {
                content: "World".to_string(),
            }),
            Ok(StreamEvent::Done),
        ]);
        Ok(Box::pin(stream))
    }

    async fn embed(
        &self,
        _session_id: &str,
        request: &EmbeddingRequest,
    ) -> anyhow::Result<EmbeddingResponse> {
        if !self.embed_supported {
            anyhow::bail!("Embeddings not supported");
        }

        Ok(EmbeddingResponse {
            model: request.model.clone(),
            data: request
                .input
                .iter()
                .enumerate()
                .map(|(i, _)| firebox_service::providers::Embedding {
                    index: i as u32,
                    embedding: vec![0.1, 0.2, 0.3],
                })
                .collect(),
            usage: Some(Usage {
                prompt_tokens: request.input.len() as u32,
                completion_tokens: 0,
                total_tokens: request.input.len() as u32,
            }),
        })
    }

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        if self.should_fail {
            anyhow::bail!("Cannot list models");
        }
        Ok(vec![
            "mock-model-v1".to_string(),
            "mock-model-v2".to_string(),
        ])
    }
}

#[tokio::test]
async fn mock_provider_complete_basic() {
    let provider = MockProvider::new();
    let request = CompletionRequest {
        model: "test-model".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
        }],
        max_tokens: None,
        temperature: None,
        stream: false,
    };

    let response = provider.complete("session-1", &request).await.unwrap();
    assert_eq!(response.id, "mock-session-1");
    assert_eq!(response.model, "test-model");
    assert_eq!(response.choices.len(), 1);
}

#[tokio::test]
async fn mock_provider_complete_failing() {
    let provider = MockProvider::failing();
    let request = CompletionRequest {
        model: "test-model".to_string(),
        messages: vec![],
        max_tokens: None,
        temperature: None,
        stream: false,
    };

    let result = provider.complete("session-1", &request).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("configured to fail")
    );
}

#[tokio::test]
async fn mock_provider_complete_stream() {
    let provider = MockProvider::new();
    let request = CompletionRequest {
        model: "test-model".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Stream me".to_string(),
        }],
        max_tokens: None,
        temperature: None,
        stream: true,
    };

    let stream = provider
        .complete_stream("session-1", &request)
        .await
        .unwrap();

    use futures_util::StreamExt;
    let events: Vec<_> = stream.collect().await;
    assert_eq!(events.len(), 3);

    // Check event types
    match &events[0] {
        Ok(StreamEvent::Delta { content }) => assert_eq!(content, "Hello "),
        _ => panic!("Expected Delta"),
    }
    match &events[2] {
        Ok(StreamEvent::Done) => (),
        _ => panic!("Expected Done"),
    }
}

#[tokio::test]
async fn mock_provider_complete_stream_not_supported() {
    let provider = MockProvider::no_stream();
    let request = CompletionRequest {
        model: "test-model".to_string(),
        messages: vec![],
        max_tokens: None,
        temperature: None,
        stream: true,
    };

    let result = provider.complete_stream("session-1", &request).await;
    assert!(result.is_err());

    match result {
        Err(e) => assert!(e.to_string().contains("Streaming not supported")),
        Ok(_) => panic!("Expected error"),
    }
}

#[tokio::test]
async fn mock_provider_embed() {
    let provider = MockProvider::new();
    let request = EmbeddingRequest {
        model: "embed-model".to_string(),
        input: vec!["hello".to_string(), "world".to_string()],
    };

    let response = provider.embed("session-1", &request).await.unwrap();
    assert_eq!(response.model, "embed-model");
    assert_eq!(response.data.len(), 2);
    assert_eq!(response.data[0].index, 0);
    assert_eq!(response.data[1].index, 1);
}

#[tokio::test]
async fn mock_provider_embed_not_supported() {
    let provider = MockProvider::failing();
    let request = EmbeddingRequest {
        model: "embed-model".to_string(),
        input: vec!["hello".to_string()],
    };

    let result = provider.embed("session-1", &request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn mock_provider_list_models() {
    let provider = MockProvider::new();
    let models = provider.list_models().await.unwrap();
    assert_eq!(models.len(), 2);
    assert!(models.contains(&"mock-model-v1".to_string()));
    assert!(models.contains(&"mock-model-v2".to_string()));
}

#[tokio::test]
async fn mock_provider_list_models_failing() {
    let provider = MockProvider::failing();
    let result = provider.list_models().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn provider_dyn_complete() {
    let provider = MockProvider::new();
    let request = CompletionRequest {
        model: "dyn-test".to_string(),
        messages: vec![],
        max_tokens: None,
        temperature: None,
        stream: false,
    };

    // Use the dyn wrapper
    let dyn_provider: &dyn ProviderDyn = &provider;
    let response = dyn_provider
        .complete_dyn("session-1", &request)
        .await
        .unwrap();
    assert_eq!(response.model, "dyn-test");
}

#[tokio::test]
async fn provider_dyn_complete_stream() {
    let provider = MockProvider::new();
    let request = CompletionRequest {
        model: "dyn-stream".to_string(),
        messages: vec![],
        max_tokens: None,
        temperature: None,
        stream: true,
    };

    let dyn_provider: &dyn ProviderDyn = &provider;
    let stream = dyn_provider
        .complete_stream_dyn("session-1", &request)
        .await
        .unwrap();

    use futures_util::StreamExt;
    let events: Vec<_> = stream.collect().await;
    assert!(!events.is_empty());
}

#[tokio::test]
async fn provider_dyn_embed() {
    let provider = MockProvider::new();
    let request = EmbeddingRequest {
        model: "dyn-embed".to_string(),
        input: vec!["test".to_string()],
    };

    let dyn_provider: &dyn ProviderDyn = &provider;
    let response = dyn_provider.embed_dyn("session-1", &request).await.unwrap();
    assert_eq!(response.model, "dyn-embed");
}

#[tokio::test]
async fn provider_dyn_list_models() {
    let provider = MockProvider::new();
    let dyn_provider: &dyn ProviderDyn = &provider;
    let models = dyn_provider.list_models_dyn().await.unwrap();
    assert_eq!(models.len(), 2);
}

#[test]
fn provider_error_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ProviderError>();
}

#[tokio::test]
async fn complete_with_empty_messages() {
    let provider = MockProvider::new();
    let request = CompletionRequest {
        model: "test".to_string(),
        messages: vec![],
        max_tokens: None,
        temperature: None,
        stream: false,
    };

    let response = provider.complete("session-1", &request).await.unwrap();
    assert!(response.choices[0].message.content.contains("test"));
}

#[tokio::test]
async fn complete_with_system_message() {
    let provider = MockProvider::new();
    let request = CompletionRequest {
        model: "test".to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: "Be helpful".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            },
        ],
        max_tokens: None,
        temperature: None,
        stream: false,
    };

    let response = provider.complete("session-1", &request).await.unwrap();
    assert_eq!(response.usage.unwrap().prompt_tokens, 2);
}

#[tokio::test]
async fn embed_with_single_input() {
    let provider = MockProvider::new();
    let request = EmbeddingRequest {
        model: "embed".to_string(),
        input: vec!["single".to_string()],
    };

    let response = provider.embed("session-1", &request).await.unwrap();
    assert_eq!(response.data.len(), 1);
    assert_eq!(response.data[0].embedding.len(), 3);
}

#[tokio::test]
async fn embed_with_empty_input() {
    let provider = MockProvider::new();
    let request = EmbeddingRequest {
        model: "embed".to_string(),
        input: vec![],
    };

    let response = provider.embed("session-1", &request).await.unwrap();
    assert_eq!(response.data.len(), 0);
}

#[tokio::test]
async fn session_id_in_response() {
    let provider = MockProvider::new();
    let request = CompletionRequest {
        model: "test".to_string(),
        messages: vec![],
        max_tokens: None,
        temperature: None,
        stream: false,
    };

    let response1 = provider
        .complete("unique-session-123", &request)
        .await
        .unwrap();
    assert_eq!(response1.id, "mock-unique-session-123");

    let response2 = provider
        .complete("another-session-456", &request)
        .await
        .unwrap();
    assert_eq!(response2.id, "mock-another-session-456");
}
