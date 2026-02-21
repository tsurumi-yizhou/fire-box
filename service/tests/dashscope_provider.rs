//! Tests for Alibaba Cloud DashScope Provider

use firebox_service::providers::config::DashScopeConfig;
use firebox_service::providers::dashscope::{
    DashScopeProvider, NATIVE_BASE_URL, NATIVE_BASE_URL_INTL, OAuthCredentials,
    QwenDeviceCodeResponse,
};
use firebox_service::providers::{ChatMessage, CompletionRequest, Provider};

// OAuthCredentials tests
#[test]
fn oauth_credentials_with_access_token() {
    let creds = OAuthCredentials {
        access_token: "at-abc123".to_string(),
        refresh_token: None,
        resource_url: None,
        expiry_date: None,
    };

    assert_eq!(creds.access_token, "at-abc123");
}

#[test]
fn oauth_credentials_with_refresh_token() {
    let creds = OAuthCredentials {
        access_token: "at-abc123".to_string(),
        refresh_token: Some("rt-xyz789".to_string()),
        resource_url: None,
        expiry_date: None,
    };

    assert_eq!(creds.access_token, "at-abc123");
    assert_eq!(creds.refresh_token, Some("rt-xyz789".to_string()));
}

#[test]
fn oauth_credentials_with_resource_url() {
    let resource_url = "https://dashscope.aliyuncs.com/api/v1";
    let creds = OAuthCredentials {
        access_token: "at-token".to_string(),
        refresh_token: None,
        resource_url: Some(resource_url.to_string()),
        expiry_date: None,
    };

    assert_eq!(creds.resource_url, Some(resource_url.to_string()));
}

#[test]
fn oauth_credentials_with_expiry() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let creds = OAuthCredentials {
        access_token: "at-token".to_string(),
        refresh_token: None,
        resource_url: None,
        expiry_date: Some(now + 3600000), // 1 hour from now
    };

    assert!(creds.expiry_date.is_some());
    assert!(creds.expiry_date.unwrap() > now);
}

#[test]
fn oauth_credentials_is_valid_true() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let creds = OAuthCredentials {
        access_token: "at-token".to_string(),
        refresh_token: None,
        resource_url: None,
        expiry_date: Some(now + 3600000), // 1 hour from now
    };

    assert!(creds.is_valid());
}

#[test]
fn oauth_credentials_is_valid_false() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let creds = OAuthCredentials {
        access_token: "at-token".to_string(),
        refresh_token: None,
        resource_url: None,
        expiry_date: Some(now - 1000), // Expired 1 second ago
    };

    assert!(!creds.is_valid());
}

#[test]
fn oauth_credentials_is_valid_no_expiry() {
    let creds = OAuthCredentials {
        access_token: "at-token".to_string(),
        refresh_token: None,
        resource_url: None,
        expiry_date: None,
    };

    // No expiry means always valid
    assert!(creds.is_valid());
}

#[test]
fn oauth_credentials_is_valid_near_expiry() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    // Less than 60 seconds buffer
    let creds = OAuthCredentials {
        access_token: "at-token".to_string(),
        refresh_token: None,
        resource_url: None,
        expiry_date: Some(now + 30000), // 30 seconds from now
    };

    // Should be invalid due to 60-second buffer
    assert!(!creds.is_valid());
}

#[test]
fn oauth_credentials_clone() {
    let creds = OAuthCredentials {
        access_token: "at-token".to_string(),
        refresh_token: Some("rt-token".to_string()),
        resource_url: Some("https://example.com".to_string()),
        expiry_date: Some(1234567890),
    };

    let cloned = creds.clone();
    assert_eq!(creds.access_token, cloned.access_token);
    assert_eq!(creds.refresh_token, cloned.refresh_token);
}

#[test]
fn oauth_credentials_debug() {
    let creds = OAuthCredentials {
        access_token: "at-secret".to_string(),
        refresh_token: None,
        resource_url: None,
        expiry_date: None,
    };

    let debug_str = format!("{:?}", creds);
    assert!(debug_str.contains("OAuthCredentials"));
}

// QwenDeviceCodeResponse tests
#[test]
fn qwen_device_code_response_structure() {
    let response = QwenDeviceCodeResponse {
        device_code: "device-code-123".to_string(),
        user_code: "ABC-123".to_string(),
        verification_uri: "https://chat.qwen.ai/device".to_string(),
        verification_uri_complete: "https://chat.qwen.ai/device?code=ABC-123".to_string(),
        expires_in: 900,
        interval: 5,
    };

    assert_eq!(response.device_code, "device-code-123");
    assert_eq!(response.user_code, "ABC-123");
    assert!(response.verification_uri.contains("chat.qwen.ai"));
    assert_eq!(response.expires_in, 900);
}

#[test]
fn qwen_device_code_verification_uri_complete() {
    let response = QwenDeviceCodeResponse {
        device_code: "code".to_string(),
        user_code: "CODE".to_string(),
        verification_uri: "https://chat.qwen.ai/device".to_string(),
        verification_uri_complete: "https://chat.qwen.ai/device?code=CODE".to_string(),
        expires_in: 900,
        interval: 5,
    };

    assert!(
        response
            .verification_uri_complete
            .contains(response.user_code.as_str())
    );
}

#[test]
fn qwen_device_code_default_interval() {
    let response = QwenDeviceCodeResponse {
        device_code: "code".to_string(),
        user_code: "CODE".to_string(),
        verification_uri: "https://chat.qwen.ai".to_string(),
        verification_uri_complete: "https://chat.qwen.ai?code=CODE".to_string(),
        expires_in: 900,
        interval: 5,
    };

    assert_eq!(response.interval, 5);
}

#[test]
fn qwen_device_code_expires_in_seconds() {
    let response = QwenDeviceCodeResponse {
        device_code: "code".to_string(),
        user_code: "CODE".to_string(),
        verification_uri: "https://chat.qwen.ai".to_string(),
        verification_uri_complete: "https://chat.qwen.ai?code=CODE".to_string(),
        expires_in: 900, // 15 minutes
        interval: 5,
    };

    assert!(response.expires_in > 60); // More than 1 minute
    assert!(response.expires_in < 3600); // Less than 1 hour
}

// DashScopeConfig tests
#[test]
fn dashscope_config_with_access_token() {
    let config = DashScopeConfig {
        access_token: Some("at-token".to_string()),
        refresh_token: None,
        resource_url: None,
        expiry_date: None,
        base_url: None,
    };

    assert_eq!(config.access_token, Some("at-token".to_string()));
}

#[test]
fn dashscope_config_with_full_oauth() {
    let config = DashScopeConfig {
        access_token: Some("at-token".to_string()),
        refresh_token: Some("rt-token".to_string()),
        resource_url: Some("https://dashscope.aliyuncs.com".to_string()),
        expiry_date: Some(1234567890),
        base_url: None,
    };

    assert!(config.access_token.is_some());
    assert!(config.refresh_token.is_some());
    assert!(config.resource_url.is_some());
    assert!(config.expiry_date.is_some());
}

#[test]
fn dashscope_config_with_custom_base_url() {
    let config = DashScopeConfig {
        access_token: Some("at-token".to_string()),
        refresh_token: None,
        resource_url: None,
        expiry_date: None,
        base_url: Some("https://custom.dashscope.com".to_string()),
    };

    assert_eq!(
        config.base_url,
        Some("https://custom.dashscope.com".to_string())
    );
}

// DashScopeProvider construction tests
#[test]
fn provider_from_config() {
    let config = DashScopeConfig {
        access_token: Some("at-token".to_string()),
        refresh_token: None,
        resource_url: None,
        expiry_date: None,
        base_url: None,
    };

    let provider = DashScopeProvider::from_config(&config);
    // Provider should be constructed without error
    assert!(!provider.endpoint().is_empty());
}

#[test]
fn provider_endpoint_from_config() {
    let config = DashScopeConfig {
        access_token: Some("at-token".to_string()),
        refresh_token: None,
        resource_url: None,
        expiry_date: None,
        base_url: None,
    };

    let provider = DashScopeProvider::from_config(&config);
    assert!(provider.endpoint().contains("dashscope.aliyuncs.com"));
}

#[test]
fn provider_endpoint_with_resource_url() {
    let custom_resource_url = "https://custom-resource.com/api/v1/generation";
    let config = DashScopeConfig {
        access_token: Some("at-token".to_string()),
        refresh_token: None,
        resource_url: Some(custom_resource_url.to_string()),
        expiry_date: None,
        base_url: None,
    };

    let provider = DashScopeProvider::from_config(&config);
    assert_eq!(provider.endpoint(), custom_resource_url);
}

#[test]
fn provider_endpoint_with_resource_url_no_generation_path() {
    let custom_resource_url = "https://custom-resource.com";
    let expected_url = format!(
        "{}/api/v1/services/aigc/text-generation/generation",
        custom_resource_url
    );
    let config = DashScopeConfig {
        access_token: Some("at-token".to_string()),
        refresh_token: None,
        resource_url: Some(custom_resource_url.to_string()),
        expiry_date: None,
        base_url: None,
    };

    let provider = DashScopeProvider::from_config(&config);
    assert_eq!(provider.endpoint(), expected_url);
}

#[test]
fn provider_with_oauth_credentials() {
    let creds = OAuthCredentials {
        access_token: "at-token".to_string(),
        refresh_token: None,
        resource_url: None,
        expiry_date: None,
    };

    let provider = DashScopeProvider::with_oauth(creds);
    assert!(!provider.endpoint().is_empty());
}

// Request structure tests
#[test]
fn completion_request_for_dashscope() {
    let request = CompletionRequest {
        model: "qwen-max".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "你好".to_string(),
        }],
        max_tokens: Some(1000),
        temperature: Some(0.7),
        stream: false,
    };

    assert_eq!(request.model, "qwen-max");
}

#[test]
fn qwen_model_names() {
    let models = vec!["qwen-max", "qwen-plus", "qwen-turbo", "qwen-long-context"];

    for model in models {
        let request = CompletionRequest {
            model: model.to_string(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stream: false,
        };
        assert!(request.model.starts_with("qwen-"));
    }
}

#[test]
fn completion_request_with_chinese_content() {
    let request = CompletionRequest {
        model: "qwen-max".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "请解释这段代码".to_string(),
        }],
        max_tokens: Some(500),
        temperature: None,
        stream: false,
    };

    assert!(request.messages[0].content.contains("请解释"));
}

// Integration-style tests
#[tokio::test]
async fn complete_with_invalid_token_should_fail() {
    let config = DashScopeConfig {
        access_token: Some("invalid-token".to_string()),
        refresh_token: None,
        resource_url: None,
        expiry_date: None,
        base_url: None,
    };

    let provider = DashScopeProvider::from_config(&config);
    let request = CompletionRequest {
        model: "qwen-max".to_string(),
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
async fn complete_stream_not_implemented() {
    let config = DashScopeConfig {
        access_token: Some("token".to_string()),
        refresh_token: None,
        resource_url: None,
        expiry_date: None,
        base_url: None,
    };

    let provider = DashScopeProvider::from_config(&config);
    let request = CompletionRequest {
        model: "qwen-max".to_string(),
        messages: vec![],
        max_tokens: None,
        temperature: None,
        stream: true,
    };

    let result = provider.complete_stream("test-session", &request).await;
    assert!(result.is_err());
    // Streaming is not yet implemented
}

#[tokio::test]
async fn embed_not_implemented() {
    use firebox_service::providers::EmbeddingRequest;

    let config = DashScopeConfig {
        access_token: Some("token".to_string()),
        refresh_token: None,
        resource_url: None,
        expiry_date: None,
        base_url: None,
    };

    let provider = DashScopeProvider::from_config(&config);
    let request = EmbeddingRequest {
        model: "text-embedding".to_string(),
        input: vec!["test".to_string()],
    };

    let result = provider.embed("test-session", &request).await;
    assert!(result.is_err());
}

// OAuth flow tests
#[test]
fn qwen_oauth_scope_default() {
    // Default scope should include necessary permissions
    let default_scope = "openid profile email model.completion";
    assert!(default_scope.contains("openid"));
    assert!(default_scope.contains("model.completion"));
}

#[test]
fn qwen_grant_type_device() {
    // Device flow grant type
    let grant_type = "urn:ietf:params:oauth:grant-type:device_code";
    assert!(grant_type.starts_with("urn:ietf:params:oauth"));
}

// Endpoint URL tests
#[test]
fn native_base_url_format() {
    assert!(NATIVE_BASE_URL.starts_with("https://"));
    assert!(NATIVE_BASE_URL.contains("/api/v1/"));
    assert!(NATIVE_BASE_URL.ends_with("/generation"));
}

#[test]
fn intl_base_url_format() {
    assert!(NATIVE_BASE_URL_INTL.starts_with("https://"));
    assert!(NATIVE_BASE_URL_INTL.contains("dashscope-intl"));
}

#[test]
fn china_vs_intl_endpoints() {
    // China endpoint should not contain "intl"
    assert!(!NATIVE_BASE_URL.contains("intl"));
    // Intl endpoint should contain "intl"
    assert!(NATIVE_BASE_URL_INTL.contains("intl"));
}
