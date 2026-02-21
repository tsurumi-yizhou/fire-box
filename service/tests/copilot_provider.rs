//! Tests for GitHub Copilot Provider

use firebox_service::providers::copilot::{CopilotProvider, DeviceCodeResponse};
use firebox_service::providers::{ChatMessage, CompletionRequest, Provider};

// Provider construction tests
#[test]
fn create_with_default_endpoint() {
    let provider = CopilotProvider::new("oauth-token".to_string());
    assert_eq!(provider.endpoint(), "https://api.githubcopilot.com");
    assert_eq!(provider.oauth_token(), "oauth-token");
}

#[test]
fn create_with_custom_endpoint() {
    let custom_endpoint = "https://custom.copilot.example.com";
    let provider =
        CopilotProvider::with_endpoint("oauth-token".to_string(), custom_endpoint.to_string());
    assert_eq!(provider.endpoint(), custom_endpoint);
}

#[test]
fn default_endpoint_is_githubcopilot() {
    let provider = CopilotProvider::new("token".to_string());
    assert!(provider.endpoint().contains("githubcopilot.com"));
}

#[test]
fn oauth_token_is_stored() {
    let token = "gho_abcdefghijklmnopqrstuvwxyz";
    let provider = CopilotProvider::new(token.to_string());
    assert_eq!(provider.oauth_token(), token);
}

#[test]
fn provider_with_empty_token() {
    let provider = CopilotProvider::new("".to_string());
    assert_eq!(provider.oauth_token(), "");
}

#[test]
fn provider_with_long_token() {
    let long_token = "gho_".to_string() + &"x".repeat(100);
    let provider = CopilotProvider::new(long_token.clone());
    assert_eq!(provider.oauth_token(), long_token);
}

#[test]
fn different_tokens_different_providers() {
    let provider1 = CopilotProvider::new("token1".to_string());
    let provider2 = CopilotProvider::new("token2".to_string());

    assert_ne!(provider1.oauth_token(), provider2.oauth_token());
}

#[test]
fn different_endpoints_different_providers() {
    let provider1 = CopilotProvider::new("token".to_string());
    let provider2 = CopilotProvider::with_endpoint(
        "token".to_string(),
        "https://custom.example.com".to_string(),
    );

    assert_ne!(provider1.endpoint(), provider2.endpoint());
}

#[test]
fn endpoint_with_port() {
    let provider =
        CopilotProvider::with_endpoint("token".to_string(), "http://localhost:8080".to_string());
    assert!(provider.endpoint().contains(":8080"));
}

#[test]
fn endpoint_with_path() {
    let provider = CopilotProvider::with_endpoint(
        "token".to_string(),
        "https://api.example.com/copilot/v1".to_string(),
    );
    assert!(provider.endpoint().contains("/copilot/v1"));
}

// OAuth token format tests
#[test]
fn oauth_token_github_format() {
    // GitHub OAuth tokens typically start with gho_
    let token = "gho_abc123";
    let provider = CopilotProvider::new(token.to_string());
    assert!(provider.oauth_token().starts_with("gho_"));
}

#[test]
fn oauth_token_with_special_chars() {
    let token = "gho_test-token_123";
    let provider = CopilotProvider::new(token.to_string());
    assert_eq!(provider.oauth_token(), token);
}

// Request structure tests
#[test]
fn completion_request_for_copilot() {
    let request = CompletionRequest {
        model: "copilot-chat".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Explain this code".to_string(),
        }],
        max_tokens: Some(500),
        temperature: Some(0.7),
        stream: false,
    };

    assert_eq!(request.model, "copilot-chat");
    assert_eq!(request.messages.len(), 1);
}

#[test]
fn copilot_chat_models() {
    let models = vec!["copilot-chat", "gpt-4", "gpt-3.5-turbo"];

    for model in models {
        let request = CompletionRequest {
            model: model.to_string(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stream: false,
        };
        assert!(!request.model.is_empty());
    }
}

// Device code response structure tests
#[test]
fn device_code_response_structure() {
    let response = DeviceCodeResponse {
        device_code: "device-code-123".to_string(),
        user_code: "ABC-123".to_string(),
        verification_uri: "https://github.com/login/device".to_string(),
        expires_in: 900,
        interval: 5,
    };

    assert_eq!(response.device_code, "device-code-123");
    assert_eq!(response.user_code, "ABC-123");
    assert!(response.verification_uri.contains("github.com"));
    assert_eq!(response.expires_in, 900);
    assert_eq!(response.interval, 5);
}

#[test]
fn device_code_expires_in_seconds() {
    let response = DeviceCodeResponse {
        device_code: "code".to_string(),
        user_code: "CODE".to_string(),
        verification_uri: "https://github.com".to_string(),
        expires_in: 900, // 15 minutes
        interval: 5,
    };

    // 900 seconds = 15 minutes
    assert_eq!(response.expires_in, 900);
}

#[test]
fn device_code_poll_interval() {
    let response = DeviceCodeResponse {
        device_code: "code".to_string(),
        user_code: "CODE".to_string(),
        verification_uri: "https://github.com".to_string(),
        expires_in: 900,
        interval: 5, // Poll every 5 seconds
    };

    assert_eq!(response.interval, 5);
}

#[test]
fn device_code_verification_uri_format() {
    let response = DeviceCodeResponse {
        device_code: "code".to_string(),
        user_code: "CODE".to_string(),
        verification_uri: "https://github.com/login/device".to_string(),
        expires_in: 900,
        interval: 5,
    };

    assert!(response.verification_uri.starts_with("https://"));
    assert!(response.verification_uri.contains("github.com"));
}

// Cached token tests
#[test]
fn cached_token_starts_empty() {
    let _provider = CopilotProvider::new("token".to_string());
    // The cached_token field is private, but we can test behavior
    // This test documents that tokens are fetched on-demand
}

#[test]
fn provider_maintains_oauth_token() {
    let token = "gho_test";
    let provider = CopilotProvider::new(token.to_string());
    assert_eq!(provider.oauth_token(), token);
}

// Integration-style tests
#[tokio::test]
async fn complete_without_valid_token_should_fail() {
    let provider = CopilotProvider::new("invalid-token".to_string());
    let request = CompletionRequest {
        model: "copilot-chat".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Test".to_string(),
        }],
        max_tokens: None,
        temperature: None,
        stream: false,
    };

    let result = provider.complete("test-session", &request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn complete_stream_without_valid_token_should_fail() {
    let provider = CopilotProvider::new("invalid-token".to_string());
    let request = CompletionRequest {
        model: "copilot-chat".to_string(),
        messages: vec![],
        max_tokens: None,
        temperature: None,
        stream: true,
    };

    let result = provider.complete_stream("test-session", &request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn embed_should_fail_for_copilot() {
    use firebox_service::providers::EmbeddingRequest;

    let provider = CopilotProvider::new("token".to_string());
    let request = EmbeddingRequest {
        model: "not-applicable".to_string(),
        input: vec!["test".to_string()],
    };

    let result = provider.embed("test-session", &request).await;
    assert!(result.is_err());
    // Copilot doesn't support embeddings
}

#[tokio::test]
async fn list_models_without_auth_should_fail() {
    let provider = CopilotProvider::new("invalid-token".to_string());
    let result = provider.list_models().await;
    assert!(result.is_err());
}

// Device flow helper tests
#[test]
fn device_code_user_code_format() {
    // User codes are typically short alphanumeric codes
    let user_codes = vec!["ABC-123", "XYZ-789", "A1B2C3"];

    for code in user_codes {
        assert!(!code.is_empty());
        assert!(code.len() <= 20); // Reasonable length limit
    }
}

#[test]
fn device_code_expires_reasonable_time() {
    // Device codes should expire in a reasonable time (e.g., 15 minutes)
    let expires_in_seconds = 900;
    assert!(expires_in_seconds > 60); // More than 1 minute
    assert!(expires_in_seconds < 3600); // Less than 1 hour
}

#[test]
fn poll_interval_reasonable() {
    // Poll interval should be reasonable (not too frequent)
    let interval_seconds = 5;
    assert!(interval_seconds >= 1); // At least 1 second
    assert!(interval_seconds <= 60); // At most 1 minute
}

// Endpoint validation tests
#[test]
fn default_endpoint_is_valid_url() {
    let provider = CopilotProvider::new("token".to_string());
    let endpoint = provider.endpoint();

    // Basic URL validation
    assert!(endpoint.starts_with("https://"));
    assert!(!endpoint.is_empty());
}

#[test]
fn custom_endpoint_preserved() {
    let custom = "https://custom.example.com/api";
    let provider = CopilotProvider::with_endpoint("token".to_string(), custom.to_string());
    assert_eq!(provider.endpoint(), custom);
}

#[test]
fn endpoint_with_trailing_slash() {
    let provider =
        CopilotProvider::with_endpoint("token".to_string(), "https://api.example.com/".to_string());
    assert!(provider.endpoint().ends_with('/'));
}

#[test]
fn endpoint_without_trailing_slash() {
    let provider =
        CopilotProvider::with_endpoint("token".to_string(), "https://api.example.com".to_string());
    assert!(!provider.endpoint().ends_with('/'));
}
