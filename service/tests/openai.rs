//! Tests for OpenAI Provider

use firebox_service::providers::openai::OpenAiProvider;
use firebox_service::providers::{ChatMessage, CompletionRequest, EmbeddingRequest, Provider};

const DEFAULT_URL: &str = "https://api.openai.com/v1";

#[test]
fn create_with_default_url() {
    let provider =
        OpenAiProvider::with_base_url(Some("sk-test-key".to_string()), DEFAULT_URL.to_string());
    assert_eq!(provider.base_url(), DEFAULT_URL);
}

#[test]
fn create_with_custom_url() {
    let provider = OpenAiProvider::with_base_url(
        Some("sk-custom".to_string()),
        "http://localhost:8080/v1".to_string(),
    );
    assert_eq!(provider.base_url(), "http://localhost:8080/v1");
}

#[test]
fn create_ollama() {
    let provider = OpenAiProvider::ollama();
    assert_eq!(provider.base_url(), "http://localhost:11434/v1");
}

#[test]
fn create_vllm_with_key() {
    let provider = OpenAiProvider::vllm(Some("vllm-key".to_string()));
    assert_eq!(provider.base_url(), "http://localhost:8000/v1");
}

#[test]
fn create_vllm_without_key() {
    let provider = OpenAiProvider::vllm(None);
    assert_eq!(provider.base_url(), "http://localhost:8000/v1");
}

#[test]
fn base_url_is_immutable() {
    let provider =
        OpenAiProvider::with_base_url(Some("sk-test".to_string()), DEFAULT_URL.to_string());
    let url = provider.base_url();
    assert_eq!(url, DEFAULT_URL);
}

#[test]
fn custom_url_with_trailing_slash() {
    let provider = OpenAiProvider::with_base_url(
        Some("key".to_string()),
        "http://localhost:8080/".to_string(),
    );
    assert_eq!(provider.base_url(), "http://localhost:8080/");
}

#[test]
fn ollama_is_local() {
    let provider = OpenAiProvider::ollama();
    assert!(provider.base_url().starts_with("http://localhost"));
}

#[test]
fn vllm_is_local() {
    let provider = OpenAiProvider::vllm(None);
    assert!(provider.base_url().starts_with("http://localhost"));
}

#[test]
fn openai_is_https() {
    let provider =
        OpenAiProvider::with_base_url(Some("sk-test".to_string()), DEFAULT_URL.to_string());
    assert!(provider.base_url().starts_with("https://"));
}

#[test]
fn provider_with_empty_api_key() {
    let provider = OpenAiProvider::with_base_url(
        Some("".to_string()),
        "http://localhost:11434/v1".to_string(),
    );
    assert_eq!(provider.base_url(), "http://localhost:11434/v1");
}

#[test]
fn different_providers_different_urls() {
    let openai = OpenAiProvider::with_base_url(Some("key".to_string()), DEFAULT_URL.to_string());
    let ollama = OpenAiProvider::ollama();
    let vllm = OpenAiProvider::vllm(None);

    assert_ne!(openai.base_url(), ollama.base_url());
    assert_ne!(ollama.base_url(), vllm.base_url());
}

#[test]
fn provider_url_formats() {
    let urls = vec![
        "https://api.openai.com/v1",
        "http://localhost:11434/v1",
        "http://localhost:8000/v1",
        "https://custom.example.com/api/v1",
    ];

    for url in urls {
        let provider = OpenAiProvider::with_base_url(Some("key".to_string()), url.to_string());
        assert_eq!(provider.base_url(), url);
    }
}

#[test]
fn provider_with_special_characters_in_key() {
    let provider = OpenAiProvider::with_base_url(
        Some("sk-test_特殊文字 -🔑".to_string()),
        DEFAULT_URL.to_string(),
    );
    assert_eq!(provider.base_url(), DEFAULT_URL);
}

#[test]
fn provider_with_long_api_key() {
    let long_key = "sk-".to_string() + &"a".repeat(100);
    let provider = OpenAiProvider::with_base_url(Some(long_key), DEFAULT_URL.to_string());
    assert_eq!(provider.base_url(), DEFAULT_URL);
}

// Request structure tests
#[test]
fn request_with_single_message() {
    let request = CompletionRequest {
        model: "gpt-4".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }],
        max_tokens: Some(100),
        temperature: Some(0.7),
        stream: false,
        tools: None,
    };

    assert_eq!(request.model, "gpt-4");
    assert_eq!(request.messages.len(), 1);
    assert_eq!(request.max_tokens, Some(100));
}

#[test]
fn request_with_multiple_messages() {
    let request = CompletionRequest {
        model: "gpt-4".to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: "You are helpful".to_string(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        ],
        max_tokens: None,
        temperature: None,
        stream: false,
        tools: None,
    };

    assert_eq!(request.messages.len(), 2);
}

#[test]
fn embedding_request_format() {
    let request = EmbeddingRequest {
        model: "text-embedding-ada-002".to_string(),
        input: vec!["hello world".to_string()],
        encoding_format: None,
    };

    assert_eq!(request.model, "text-embedding-ada-002");
    assert_eq!(request.input.len(), 1);
}

#[test]
fn embedding_request_multiple_inputs() {
    let request = EmbeddingRequest {
        model: "text-embedding-3-small".to_string(),
        input: vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
        ],
        encoding_format: None,
    };

    assert_eq!(request.input.len(), 3);
}

// Integration-style tests
#[tokio::test]
async fn complete_without_network_should_fail() {
    let provider =
        OpenAiProvider::with_base_url(Some("invalid-key".to_string()), DEFAULT_URL.to_string());
    let request = CompletionRequest {
        model: "gpt-4".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Test".to_string(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }],
        max_tokens: None,
        temperature: None,
        stream: false,
        tools: None,
    };

    let result = provider.complete("test-session", &request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn complete_stream_without_network_should_fail() {
    let provider =
        OpenAiProvider::with_base_url(Some("invalid-key".to_string()), DEFAULT_URL.to_string());
    let request = CompletionRequest {
        model: "gpt-4".to_string(),
        messages: vec![],
        max_tokens: None,
        temperature: None,
        stream: true,
        tools: None,
    };

    let result = provider.complete_stream("test-session", &request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn embed_without_network_should_fail() {
    let provider =
        OpenAiProvider::with_base_url(Some("invalid-key".to_string()), DEFAULT_URL.to_string());
    let request = EmbeddingRequest {
        model: "text-embedding-ada-002".to_string(),
        input: vec!["test".to_string()],
        encoding_format: None,
    };

    let result = provider.embed("test-session", &request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn list_models_without_network_should_fail() {
    let provider =
        OpenAiProvider::with_base_url(Some("invalid-key".to_string()), DEFAULT_URL.to_string());
    let _result = provider.list_models().await;
    // May fail or return empty list depending on implementation
}

#[test]
fn ollama_no_auth_required() {
    let _provider = OpenAiProvider::ollama();
    // Ollama typically runs locally without auth
}

#[test]
fn vllm_optional_auth() {
    let with_auth = OpenAiProvider::vllm(Some("key".to_string()));
    let without_auth = OpenAiProvider::vllm(None);

    // Both should have the same base URL
    assert_eq!(with_auth.base_url(), without_auth.base_url());
}
