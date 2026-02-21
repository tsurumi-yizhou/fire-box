//! Tests for Provider Configuration Module

use firebox_service::providers::config::{
    AnthropicConfig, ApiKeyConfig, CopilotConfig, DashScopeConfig, LlamaCppProviderConfig,
    ProviderConfig,
};
use std::path::PathBuf;

// ApiKeyConfig tests
#[test]
fn api_key_config_basic() {
    let config = ApiKeyConfig {
        api_key: "sk-test-key".to_string(),
        base_url: None,
    };

    assert_eq!(config.api_key, "sk-test-key");
    assert!(config.base_url.is_none());
}

#[test]
fn api_key_config_with_custom_url() {
    let config = ApiKeyConfig {
        api_key: "sk-custom".to_string(),
        base_url: Some("http://localhost:8080/v1".to_string()),
    };

    assert_eq!(
        config.base_url,
        Some("http://localhost:8080/v1".to_string())
    );
}

#[test]
fn api_key_config_empty_key() {
    let config = ApiKeyConfig {
        api_key: "".to_string(),
        base_url: None,
    };

    assert_eq!(config.api_key, "");
}

#[test]
fn api_key_config_clone() {
    let config = ApiKeyConfig {
        api_key: "sk-clone".to_string(),
        base_url: Some("https://example.com".to_string()),
    };

    let cloned = config.clone();
    assert_eq!(config.api_key, cloned.api_key);
}

#[test]
fn api_key_config_debug() {
    let config = ApiKeyConfig {
        api_key: "sk-secret".to_string(),
        base_url: None,
    };

    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("ApiKeyConfig"));
}

// AnthropicConfig tests
#[test]
fn anthropic_config_basic() {
    let config = AnthropicConfig {
        api_key: "sk-ant-test".to_string(),
        base_url: None,
    };

    assert_eq!(config.api_key, "sk-ant-test");
}

#[test]
fn anthropic_config_with_custom_url() {
    let config = AnthropicConfig {
        api_key: "sk-ant-custom".to_string(),
        base_url: Some("http://localhost:9090/v1".to_string()),
    };

    assert_eq!(
        config.base_url,
        Some("http://localhost:9090/v1".to_string())
    );
}

#[test]
fn anthropic_config_clone() {
    let config = AnthropicConfig {
        api_key: "sk-ant-clone".to_string(),
        base_url: None,
    };

    let cloned = config.clone();
    assert_eq!(config.api_key, cloned.api_key);
}

// CopilotConfig tests
#[test]
fn copilot_config_with_token() {
    let config = CopilotConfig {
        oauth_token: Some("gho-token".to_string()),
        endpoint: None,
    };

    assert_eq!(config.oauth_token, Some("gho-token".to_string()));
}

#[test]
fn copilot_config_without_token() {
    let config = CopilotConfig {
        oauth_token: None,
        endpoint: None,
    };

    assert!(config.oauth_token.is_none());
}

#[test]
fn copilot_config_with_custom_endpoint() {
    let config = CopilotConfig {
        oauth_token: Some("gho-token".to_string()),
        endpoint: Some("https://custom.copilot.com".to_string()),
    };

    assert_eq!(
        config.endpoint,
        Some("https://custom.copilot.com".to_string())
    );
}

#[test]
fn copilot_config_clone() {
    let config = CopilotConfig {
        oauth_token: Some("gho-clone".to_string()),
        endpoint: None,
    };

    let cloned = config.clone();
    assert_eq!(config.oauth_token, cloned.oauth_token);
}

// DashScopeConfig tests
#[test]
fn dashscope_config_basic() {
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
fn dashscope_config_full_oauth() {
    let config = DashScopeConfig {
        access_token: Some("at-access".to_string()),
        refresh_token: Some("rt-refresh".to_string()),
        resource_url: Some("https://dashscope.aliyuncs.com".to_string()),
        expiry_date: Some(1234567890),
        base_url: None,
    };

    assert_eq!(config.access_token, Some("at-access".to_string()));
    assert_eq!(config.refresh_token, Some("rt-refresh".to_string()));
    assert_eq!(config.expiry_date, Some(1234567890));
}

#[test]
fn dashscope_config_clone() {
    let config = DashScopeConfig {
        access_token: Some("at-clone".to_string()),
        refresh_token: Some("rt-clone".to_string()),
        resource_url: None,
        expiry_date: None,
        base_url: None,
    };

    let cloned = config.clone();
    assert_eq!(config.access_token, cloned.access_token);
}

// LlamaCppProviderConfig tests
#[test]
fn llamacpp_config_basic() {
    let config = LlamaCppProviderConfig {
        model_path: PathBuf::from("/models/llama.gguf"),
        context_size: 4096,
        gpu_layers: None,
        threads: None,
    };

    assert_eq!(config.model_path, PathBuf::from("/models/llama.gguf"));
    assert_eq!(config.context_size, 4096);
}

#[test]
fn llamacpp_config_with_gpu_layers() {
    let config = LlamaCppProviderConfig {
        model_path: PathBuf::from("/models/mistral.gguf"),
        context_size: 8192,
        gpu_layers: Some(32),
        threads: None,
    };

    assert_eq!(config.gpu_layers, Some(32));
}

#[test]
fn llamacpp_config_with_threads() {
    let config = LlamaCppProviderConfig {
        model_path: PathBuf::from("/models/qwen.gguf"),
        context_size: 4096,
        gpu_layers: None,
        threads: Some(8),
    };

    assert_eq!(config.threads, Some(8));
}

#[test]
fn llamacpp_config_clone() {
    let config = LlamaCppProviderConfig {
        model_path: PathBuf::from("/models/clone.gguf"),
        context_size: 4096,
        gpu_layers: None,
        threads: None,
    };

    let cloned = config.clone();
    assert_eq!(config.model_path, cloned.model_path);
}

// ProviderConfig enum tests
#[test]
fn provider_config_openai() {
    let config = ProviderConfig::openai("sk-test", None);

    match config {
        ProviderConfig::OpenAi(api_config) => {
            assert_eq!(api_config.api_key, "sk-test");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn provider_config_openai_with_url() {
    let config = ProviderConfig::openai("sk-test", Some("http://localhost:8080/v1".to_string()));

    match config {
        ProviderConfig::OpenAi(api_config) => {
            assert_eq!(
                api_config.base_url,
                Some("http://localhost:8080/v1".to_string())
            );
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn provider_config_ollama() {
    let config = ProviderConfig::ollama(None);

    match config {
        ProviderConfig::OpenAi(api_config) => {
            assert_eq!(api_config.api_key, ""); // Ollama has no API key
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn provider_config_ollama_with_url() {
    let config = ProviderConfig::ollama(Some("http://localhost:11434/v1".to_string()));

    match config {
        ProviderConfig::OpenAi(api_config) => {
            assert_eq!(
                api_config.base_url,
                Some("http://localhost:11434/v1".to_string())
            );
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn provider_config_vllm_with_key() {
    let config = ProviderConfig::vllm(
        Some("vllm-key".to_string()),
        Some("http://localhost:8000/v1".to_string()),
    );

    match config {
        ProviderConfig::OpenAi(api_config) => {
            assert_eq!(api_config.api_key, "vllm-key");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn provider_config_vllm_without_key() {
    let config = ProviderConfig::vllm(None, None);

    match config {
        ProviderConfig::OpenAi(api_config) => {
            assert_eq!(api_config.api_key, "");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn provider_config_anthropic() {
    let config = ProviderConfig::anthropic("sk-ant-test", None);

    match config {
        ProviderConfig::Anthropic(ant_config) => {
            assert_eq!(ant_config.api_key, "sk-ant-test");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn provider_config_anthropic_with_url() {
    let config =
        ProviderConfig::anthropic("sk-ant-test", Some("http://localhost:9090/v1".to_string()));

    match config {
        ProviderConfig::Anthropic(ant_config) => {
            assert_eq!(
                ant_config.base_url,
                Some("http://localhost:9090/v1".to_string())
            );
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn provider_config_copilot() {
    let config = ProviderConfig::copilot("gho-token", None);

    match config {
        ProviderConfig::Copilot(copilot_config) => {
            assert_eq!(copilot_config.oauth_token, Some("gho-token".to_string()));
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn provider_config_copilot_pending() {
    let config = ProviderConfig::copilot_pending(None);

    match config {
        ProviderConfig::Copilot(copilot_config) => {
            assert!(copilot_config.oauth_token.is_none());
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn provider_config_dashscope_oauth() {
    let config = ProviderConfig::dashscope_oauth("at-access", "rt-refresh", 1234567890, None);

    match config {
        ProviderConfig::DashScope(ds_config) => {
            assert_eq!(ds_config.access_token, Some("at-access".to_string()));
            assert_eq!(ds_config.refresh_token, Some("rt-refresh".to_string()));
            assert_eq!(ds_config.expiry_date, Some(1234567890));
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn provider_config_llamacpp() {
    let config = ProviderConfig::llamacpp("/models/llama.gguf");

    match config {
        ProviderConfig::LlamaCpp(llama_config) => {
            assert_eq!(llama_config.model_path, PathBuf::from("/models/llama.gguf"));
            assert_eq!(llama_config.context_size, 4096);
        }
        _ => panic!("Wrong variant"),
    }
}

// type_slug tests
#[test]
fn provider_config_type_slug_openai() {
    let config = ProviderConfig::openai("sk-test", None);
    assert_eq!(config.type_slug(), "openai");
}

#[test]
fn provider_config_type_slug_anthropic() {
    let config = ProviderConfig::anthropic("sk-ant", None);
    assert_eq!(config.type_slug(), "anthropic");
}

#[test]
fn provider_config_type_slug_copilot() {
    let config = ProviderConfig::copilot("gho-token", None);
    assert_eq!(config.type_slug(), "copilot");
}

#[test]
fn provider_config_type_slug_dashscope() {
    let config = ProviderConfig::dashscope_oauth("at", "rt", 0, None);
    assert_eq!(config.type_slug(), "dashscope");
}

#[test]
fn provider_config_type_slug_llamacpp() {
    let config = ProviderConfig::llamacpp("/models/test.gguf");
    assert_eq!(config.type_slug(), "llamacpp");
}

// base_url tests
#[test]
fn provider_config_base_url_openai() {
    let config = ProviderConfig::openai("sk-test", Some("http://custom:8080/v1".to_string()));
    assert_eq!(config.base_url(), Some("http://custom:8080/v1".to_string()));
}

#[test]
fn provider_config_base_url_anthropic() {
    let config = ProviderConfig::anthropic("sk-ant", Some("http://custom:9090/v1".to_string()));
    assert_eq!(config.base_url(), Some("http://custom:9090/v1".to_string()));
}

#[test]
fn provider_config_base_url_copilot() {
    let config =
        ProviderConfig::copilot("gho-token", Some("https://custom.copilot.com".to_string()));
    assert_eq!(
        config.base_url(),
        Some("https://custom.copilot.com".to_string())
    );
}

#[test]
fn provider_config_base_url_llamacpp() {
    let config = ProviderConfig::llamacpp("/models/test.gguf");
    assert_eq!(config.base_url(), None);
}

// display_name tests
#[test]
fn provider_config_display_name_openai() {
    let name = ProviderConfig::display_name("openai", "openai");
    assert_eq!(name, "OpenAI");
}

#[test]
fn provider_config_display_name_anthropic() {
    let name = ProviderConfig::display_name("anthropic", "anthropic");
    assert_eq!(name, "Anthropic");
}

#[test]
fn provider_config_display_name_copilot() {
    let name = ProviderConfig::display_name("copilot", "copilot");
    assert_eq!(name, "GitHub Copilot");
}

#[test]
fn provider_config_display_name_dashscope() {
    let name = ProviderConfig::display_name("dashscope", "dashscope");
    assert_eq!(name, "DashScope (Qwen)");
}

#[test]
fn provider_config_display_name_llamacpp() {
    let name = ProviderConfig::display_name("llamacpp", "llamacpp");
    assert_eq!(name, "llama.cpp");
}

#[test]
fn provider_config_display_name_custom() {
    let name = ProviderConfig::display_name("my-custom-profile", "openai");
    assert!(name.contains("my-custom-profile"));
}

// JSON serialization tests
#[test]
fn provider_config_to_json_openai() {
    let config = ProviderConfig::openai("sk-test", None);
    let json = config.to_json().unwrap();

    assert!(json.contains("open_ai") || json.contains("OpenAi"));
    assert!(json.contains("sk-test"));
}

#[test]
fn provider_config_from_json_openai() {
    let json = r#"{"OpenAi":{"api_key":"sk-test","base_url":null}}"#;
    let config = ProviderConfig::from_json(json).unwrap();

    match config {
        ProviderConfig::OpenAi(api_config) => {
            assert_eq!(api_config.api_key, "sk-test");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn provider_config_json_roundtrip() {
    let original = ProviderConfig::openai("sk-roundtrip", None);
    let json = original.to_json().unwrap();
    let restored = ProviderConfig::from_json(&json).unwrap();

    assert_eq!(original.type_slug(), restored.type_slug());
}

// openai_compatible tests
#[test]
fn provider_config_openai_compatible_with_key() {
    let config = ProviderConfig::openai_compatible(
        Some("custom-key".to_string()),
        Some("https://custom.api.com/v1".to_string()),
    );

    match config {
        ProviderConfig::OpenAi(api_config) => {
            assert_eq!(api_config.api_key, "custom-key");
            assert_eq!(
                api_config.base_url,
                Some("https://custom.api.com/v1".to_string())
            );
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn provider_config_openai_compatible_without_key() {
    let config = ProviderConfig::openai_compatible(None, None);

    match config {
        ProviderConfig::OpenAi(api_config) => {
            assert_eq!(api_config.api_key, "");
        }
        _ => panic!("Wrong variant"),
    }
}

// Clone tests for ProviderConfig
#[test]
fn provider_config_clone_openai() {
    let config = ProviderConfig::openai("sk-clone", None);
    let cloned = config.clone();
    assert_eq!(config.type_slug(), cloned.type_slug());
}

#[test]
fn provider_config_clone_anthropic() {
    let config = ProviderConfig::anthropic("sk-ant-clone", None);
    let cloned = config.clone();
    assert_eq!(config.type_slug(), cloned.type_slug());
}

#[test]
fn provider_config_clone_llamacpp() {
    let config = ProviderConfig::llamacpp("/models/clone.gguf");
    let cloned = config.clone();
    assert_eq!(config.type_slug(), cloned.type_slug());
}

// Debug tests for ProviderConfig
#[test]
fn provider_config_debug() {
    let config = ProviderConfig::openai("sk-test", None);
    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("OpenAi") || debug_str.contains("open_ai"));
}

// Edge cases
#[test]
fn provider_config_empty_api_key() {
    let config = ProviderConfig::openai("", None);

    match config {
        ProviderConfig::OpenAi(api_config) => {
            assert_eq!(api_config.api_key, "");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn provider_config_long_api_key() {
    let long_key = "sk-".to_string() + &"x".repeat(200);
    let config = ProviderConfig::openai(long_key.clone(), None);

    match config {
        ProviderConfig::OpenAi(api_config) => {
            assert_eq!(api_config.api_key, long_key);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn provider_config_unicode_in_key() {
    let config = ProviderConfig::openai("sk-测试 -🔑", None);

    match config {
        ProviderConfig::OpenAi(api_config) => {
            assert_eq!(api_config.api_key, "sk-测试 -🔑");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn provider_config_special_chars_in_url() {
    let config = ProviderConfig::openai(
        "sk-test",
        Some("https://api.example.com/v1?key=value&foo=bar".to_string()),
    );

    match config {
        ProviderConfig::OpenAi(api_config) => {
            assert!(api_config.base_url.unwrap().contains("?key=value"));
        }
        _ => panic!("Wrong variant"),
    }
}
