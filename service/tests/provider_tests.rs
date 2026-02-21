//! Integration tests for provider implementations.
//!
//! These tests verify that providers correctly implement the Provider trait
//! and handle various scenarios including errors, retries, and edge cases.

use firebox_service::providers::{
    ChatMessage, CompletionRequest, Provider, anthropic::AnthropicProvider,
    copilot::CopilotProvider, openai::OpenAiProvider,
};

/// Test helper to create a basic completion request
fn create_test_request() -> CompletionRequest {
    CompletionRequest {
        model: "test-model".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Hello, world!".to_string(),
        }],
        max_tokens: Some(100),
        temperature: Some(0.7),
        stream: false,
    }
}

#[tokio::test]
#[ignore] // Requires valid API key
async fn test_openai_provider_complete() {
    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
    let provider = OpenAiProvider::new(api_key);

    let request = create_test_request();
    let response = provider.complete("test-session", &request).await;

    assert!(response.is_ok(), "OpenAI completion should succeed");
    let response = response.unwrap();
    assert!(
        !response.choices.is_empty(),
        "Should have at least one choice"
    );
    assert!(
        !response.choices[0].message.content.is_empty(),
        "Should have content"
    );
}

#[tokio::test]
#[ignore] // Requires valid API key
async fn test_anthropic_provider_complete() {
    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY not set");
    let provider = AnthropicProvider::new(api_key);

    let request = create_test_request();
    let response = provider.complete("test-session", &request).await;

    assert!(response.is_ok(), "Anthropic completion should succeed");
    let response = response.unwrap();
    assert!(
        !response.choices.is_empty(),
        "Should have at least one choice"
    );
    assert!(
        !response.choices[0].message.content.is_empty(),
        "Should have content"
    );
}

#[tokio::test]
#[ignore] // Requires GitHub OAuth token
async fn test_copilot_provider_complete() {
    let oauth_token = std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN not set");
    let provider = CopilotProvider::new(oauth_token);

    let request = create_test_request();
    let response = provider.complete("test-session", &request).await;

    assert!(response.is_ok(), "Copilot completion should succeed");
    let response = response.unwrap();
    assert!(
        !response.choices.is_empty(),
        "Should have at least one choice"
    );
}

#[tokio::test]
async fn test_openai_provider_invalid_key() {
    let provider = OpenAiProvider::new("invalid-key".to_string());

    let request = create_test_request();
    let response = provider.complete("test-session", &request).await;

    assert!(response.is_err(), "Should fail with invalid API key");
    let error = response.unwrap_err().to_string();
    assert!(
        error.contains("401") || error.contains("403"),
        "Should be authentication error"
    );
}

#[tokio::test]
async fn test_anthropic_provider_invalid_key() {
    let provider = AnthropicProvider::new("invalid-key".to_string());

    let request = create_test_request();
    let response = provider.complete("test-session", &request).await;

    assert!(response.is_err(), "Should fail with invalid API key");
    let error = response.unwrap_err().to_string();
    assert!(
        error.contains("401") || error.contains("403"),
        "Should be authentication error"
    );
}

#[tokio::test]
async fn test_openai_provider_with_custom_endpoint() {
    // Test with Ollama-style endpoint (no auth required)
    let provider = OpenAiProvider::with_base_url(None, "http://localhost:11434/v1".to_string());

    assert_eq!(provider.base_url(), "http://localhost:11434/v1");
}

#[tokio::test]
async fn test_anthropic_provider_with_custom_endpoint() {
    let provider = AnthropicProvider::with_base_url(
        "test-key".to_string(),
        "http://localhost:8080".to_string(),
    );

    assert_eq!(provider.base_url(), "http://localhost:8080");
    assert_eq!(provider.api_key(), "test-key");
}

#[tokio::test]
#[ignore] // Requires valid API key
async fn test_openai_provider_list_models() {
    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
    let provider = OpenAiProvider::new(api_key);

    let models = provider.list_models().await;
    assert!(models.is_ok(), "Should list models successfully");
    let models = models.unwrap();
    assert!(!models.is_empty(), "Should have at least one model");
}

#[tokio::test]
async fn test_anthropic_provider_list_models() {
    let provider = AnthropicProvider::new("test-key".to_string());

    let models = provider.list_models().await;
    assert!(models.is_ok(), "Should list models successfully");
    let models = models.unwrap();
    assert!(!models.is_empty(), "Should have known Claude models");
    assert!(
        models.iter().any(|m| m.contains("claude")),
        "Should include Claude models"
    );
}

#[tokio::test]
async fn test_completion_request_serialization() {
    let request = create_test_request();
    let json = serde_json::to_string(&request).unwrap();

    assert!(json.contains("test-model"));
    assert!(json.contains("Hello, world!"));
    assert!(json.contains("\"max_tokens\":100"));
    assert!(json.contains("\"temperature\":0.7"));
}

#[tokio::test]
#[ignore] // Requires valid API key and streaming support
async fn test_openai_provider_stream() {
    use futures_util::StreamExt;

    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
    let provider = OpenAiProvider::new(api_key);

    let request = create_test_request();
    let stream = provider.complete_stream("test-session", &request).await;

    assert!(stream.is_ok(), "Should create stream successfully");
    let mut stream = stream.unwrap();

    let mut event_count = 0;
    while let Some(event) = stream.next().await {
        assert!(event.is_ok(), "Stream event should be ok");
        event_count += 1;
        if event_count > 10 {
            break; // Don't consume entire stream
        }
    }

    assert!(event_count > 0, "Should receive at least one event");
}
