//! Tests for Anthropic Provider

use firebox_service::providers::anthropic::AnthropicProvider;
use firebox_service::providers::{ChatMessage, CompletionRequest, Provider};

#[test]
fn create_with_default_url() {
    let provider = AnthropicProvider::new("ant-test-key".to_string());
    assert_eq!(provider.base_url(), "https://api.anthropic.com/v1");
    assert_eq!(provider.api_key(), "ant-test-key");
}

#[test]
fn create_with_custom_url() {
    let provider = AnthropicProvider::with_base_url(
        "ant-custom-key".to_string(),
        "http://localhost:9090/v1".to_string(),
    );
    assert_eq!(provider.base_url(), "http://localhost:9090/v1");
    assert_eq!(provider.api_key(), "ant-custom-key");
}

#[test]
fn api_key_is_stored() {
    let key = "sk-ant-12345";
    let provider = AnthropicProvider::new(key.to_string());
    assert_eq!(provider.api_key(), key);
}

#[test]
fn base_url_default_is_anthropic() {
    let provider = AnthropicProvider::new("key".to_string());
    assert!(provider.base_url().contains("anthropic.com"));
}

#[test]
fn custom_url_override() {
    let custom_url = "https://proxy.example.com/anthropic/v1";
    let provider = AnthropicProvider::with_base_url("key".to_string(), custom_url.to_string());
    assert_eq!(provider.base_url(), custom_url);
}

#[test]
fn api_key_with_prefix() {
    let key = "sk-ant-api03-abcdefghijklmnopqrstuvwxyz";
    let provider = AnthropicProvider::new(key.to_string());
    assert!(provider.api_key().starts_with("sk-ant"));
}

#[test]
fn provider_with_empty_api_key() {
    let provider = AnthropicProvider::new("".to_string());
    assert_eq!(provider.api_key(), "");
}

#[test]
fn provider_with_special_characters_in_key() {
    let key = "sk-ant_test-key.123!@#";
    let provider = AnthropicProvider::new(key.to_string());
    assert_eq!(provider.api_key(), key);
}

#[test]
fn custom_url_with_different_ports() {
    let ports = vec![8080, 9000, 3000, 5000];
    for port in ports {
        let url = format!("http://localhost:{}/v1", port);
        let provider = AnthropicProvider::with_base_url("key".to_string(), url.clone());
        assert_eq!(provider.base_url(), url);
    }
}

#[test]
fn different_instances_different_keys() {
    let provider1 = AnthropicProvider::new("key1".to_string());
    let provider2 = AnthropicProvider::new("key2".to_string());

    assert_eq!(provider1.api_key(), "key1");
    assert_eq!(provider2.api_key(), "key2");
    assert_ne!(provider1.api_key(), provider2.api_key());
}

#[test]
fn different_instances_different_urls() {
    let provider1 = AnthropicProvider::new("key".to_string());
    let provider2 =
        AnthropicProvider::with_base_url("key".to_string(), "http://custom:8080/v1".to_string());

    assert_ne!(provider1.base_url(), provider2.base_url());
}

#[test]
fn url_https_required_for_production() {
    let provider = AnthropicProvider::new("key".to_string());
    assert!(provider.base_url().starts_with("https://"));
}

#[test]
fn url_http_allowed_for_local() {
    let provider =
        AnthropicProvider::with_base_url("key".to_string(), "http://localhost:8080/v1".to_string());
    assert!(provider.base_url().starts_with("http://"));
}

// Message preparation tests (testing the structure)
#[test]
fn request_with_user_message() {
    let request = CompletionRequest {
        model: "claude-3-sonnet-20240229".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Hello, Claude".to_string(),
        }],
        max_tokens: Some(1024),
        temperature: Some(0.7),
        stream: false,
    };

    assert_eq!(request.model, "claude-3-sonnet-20240229");
    assert_eq!(request.messages[0].role, "user");
}

#[test]
fn request_with_system_message() {
    let request = CompletionRequest {
        model: "claude-3-opus-20240229".to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: "You are a helpful assistant".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            },
        ],
        max_tokens: Some(512),
        temperature: None,
        stream: false,
    };

    assert_eq!(request.messages.len(), 2);
    assert_eq!(request.messages[0].role, "system");
}

#[test]
fn claude_model_names() {
    let claude_models = vec![
        "claude-opus-4-5-20251001",
        "claude-sonnet-4-5-20251001",
        "claude-3-5-sonnet-20241022",
        "claude-3-5-haiku-20241022",
        "claude-3-opus-20240229",
    ];

    for model in claude_models {
        let request = CompletionRequest {
            model: model.to_string(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stream: false,
        };
        assert!(request.model.starts_with("claude-"));
    }
}

// Integration-style tests
#[tokio::test]
async fn complete_with_invalid_key_should_fail() {
    let provider = AnthropicProvider::new("invalid-key".to_string());
    let request = CompletionRequest {
        model: "claude-3-sonnet-20240229".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Test".to_string(),
        }],
        max_tokens: Some(100),
        temperature: None,
        stream: false,
    };

    let result = provider.complete("test-session", &request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn complete_stream_with_invalid_key_should_fail() {
    let provider = AnthropicProvider::new("invalid-key".to_string());
    let request = CompletionRequest {
        model: "claude-3".to_string(),
        messages: vec![],
        max_tokens: None,
        temperature: None,
        stream: true,
    };

    let result = provider.complete_stream("test-session", &request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn embed_should_fail_for_anthropic() {
    use firebox_service::providers::EmbeddingRequest;

    let provider = AnthropicProvider::new("key".to_string());
    let request = EmbeddingRequest {
        model: "not-applicable".to_string(),
        input: vec!["test".to_string()],
    };

    let result = provider.embed("test-session", &request).await;
    assert!(result.is_err());
    // Anthropic doesn't support embeddings
}

#[tokio::test]
async fn list_models_returns_claude_models() {
    let provider = AnthropicProvider::new("key".to_string());
    let models = provider.list_models().await.unwrap();

    assert!(!models.is_empty());
    // All models should be Claude models
    for model in &models {
        assert!(model.starts_with("claude-"));
    }
}
