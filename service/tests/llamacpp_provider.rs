//! Tests for LlamaCpp Provider

use firebox_service::providers::llamacpp::{LlamaCppConfig, LlamaCppProvider};
use firebox_service::providers::{
    BoxStream, ChatMessage, CompletionRequest, CompletionResponse, EmbeddingRequest,
    EmbeddingResponse, Provider, StreamEvent,
};
use std::path::PathBuf;

// LlamaCppConfig tests
#[test]
fn config_with_model_path() {
    let config = LlamaCppConfig {
        model_path: PathBuf::from("/models/llama-7b.gguf"),
        context_size: 4096,
        gpu_layers: None,
        threads: None,
        server_url: None,
    };

    assert_eq!(config.model_path, PathBuf::from("/models/llama-7b.gguf"));
    assert_eq!(config.context_size, 4096);
}

#[test]
fn config_with_gpu_layers() {
    let config = LlamaCppConfig {
        model_path: PathBuf::from("/models/mistral-7b.gguf"),
        context_size: 4096,
        gpu_layers: Some(32),
        threads: None,
        server_url: None,
    };

    assert_eq!(config.gpu_layers, Some(32));
}

#[test]
fn config_with_threads() {
    let config = LlamaCppConfig {
        model_path: PathBuf::from("/models/qwen-7b.gguf"),
        context_size: 8192,
        gpu_layers: None,
        threads: Some(8),
        server_url: None,
    };

    assert_eq!(config.threads, Some(8));
}

#[test]
fn config_with_all_options() {
    let config = LlamaCppConfig {
        model_path: PathBuf::from("/models/llama-2-70b.gguf"),
        context_size: 16384,
        gpu_layers: Some(48),
        threads: Some(16),
        server_url: None,
    };

    assert_eq!(config.context_size, 16384);
    assert_eq!(config.gpu_layers, Some(48));
    assert_eq!(config.threads, Some(16));
}

#[test]
fn config_with_server_url() {
    let config = LlamaCppConfig {
        model_path: PathBuf::new(),
        context_size: 4096,
        gpu_layers: None,
        threads: None,
        server_url: Some("http://localhost:8080".to_string()),
    };

    assert_eq!(config.server_url, Some("http://localhost:8080".to_string()));
}

#[test]
fn config_clone() {
    let config = LlamaCppConfig {
        model_path: PathBuf::from("/models/test.gguf"),
        context_size: 4096,
        gpu_layers: None,
        threads: None,
        server_url: None,
    };

    let cloned = config.clone();
    assert_eq!(config.model_path, cloned.model_path);
}

#[test]
fn config_debug() {
    let config = LlamaCppConfig {
        model_path: PathBuf::from("/models/debug.gguf"),
        context_size: 4096,
        gpu_layers: None,
        threads: None,
        server_url: None,
    };

    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("LlamaCppConfig"));
    assert!(debug_str.contains("debug.gguf"));
}

// LlamaCppProvider construction tests
#[test]
fn provider_from_model_path() {
    let provider = LlamaCppProvider::from_model_path("/models/llama.gguf");
    assert_eq!(
        provider.model_path(),
        std::path::Path::new("/models/llama.gguf")
    );
}

#[test]
fn provider_from_config() {
    let config = LlamaCppConfig {
        model_path: PathBuf::from("/models/mistral.gguf"),
        context_size: 8192,
        gpu_layers: Some(24),
        threads: Some(8),
        server_url: None,
    };

    let provider = LlamaCppProvider::new(config);
    assert_eq!(provider.config().context_size, 8192);
    assert_eq!(provider.config().gpu_layers, Some(24));
}

#[test]
fn provider_from_server_url() {
    let provider = LlamaCppProvider::from_server_url("http://localhost:8080".to_string());
    // Can't test server_url directly as it's private, but we can test list_models fails
    let _ = provider.config();
}

#[test]
fn provider_config_accessor() {
    let config = LlamaCppConfig {
        model_path: PathBuf::from("/models/test.gguf"),
        context_size: 4096,
        gpu_layers: None,
        threads: None,
        server_url: None,
    };

    let provider = LlamaCppProvider::new(config);
    let retrieved_config = provider.config();
    assert_eq!(retrieved_config.context_size, 4096);
}

#[test]
fn provider_model_path_accessor() {
    let provider = LlamaCppProvider::from_model_path("/models/qwen-72b.gguf");
    assert_eq!(
        provider.model_path(),
        std::path::Path::new("/models/qwen-72b.gguf")
    );
}

// Model path tests
#[test]
fn model_path_with_gguf_extension() {
    let provider = LlamaCppProvider::from_model_path("/models/llama-2-7b.gguf");
    let path = provider.model_path();
    assert_eq!(path.extension(), Some(std::ffi::OsStr::new("gguf")));
}

#[test]
fn model_path_various_formats() {
    let paths = vec![
        "/models/llama.gguf",
        "/home/user/models/mistral-7b-instruct.gguf",
        "./local-models/qwen.gguf",
        "/opt/ai/llama-2-70b-chat.gguf",
    ];

    for path_str in paths {
        let provider = LlamaCppProvider::from_model_path(path_str);
        assert!(!provider.model_path().as_os_str().is_empty());
    }
}

#[test]
fn model_path_filename_extraction() {
    let provider = LlamaCppProvider::from_model_path("/models/llama-2-7b.gguf");
    let filename = provider.model_path().file_name();
    assert_eq!(filename, Some(std::ffi::OsStr::new("llama-2-7b.gguf")));
}

#[test]
fn model_path_parent_directory() {
    let provider = LlamaCppProvider::from_model_path("/models/llama.gguf");
    let parent = provider.model_path().parent();
    assert_eq!(parent, Some(std::path::Path::new("/models")));
}

// Context size tests
#[test]
fn context_size_default() {
    let provider = LlamaCppProvider::from_model_path("/models/test.gguf");
    assert_eq!(provider.config().context_size, 4096);
}

#[test]
fn context_size_custom() {
    let config = LlamaCppConfig {
        model_path: PathBuf::from("/models/test.gguf"),
        context_size: 8192,
        gpu_layers: None,
        threads: None,
        server_url: None,
    };

    let provider = LlamaCppProvider::new(config);
    assert_eq!(provider.config().context_size, 8192);
}

#[test]
fn context_size_variations() {
    let sizes = vec![2048, 4096, 8192, 16384, 32768];

    for size in sizes {
        let config = LlamaCppConfig {
            model_path: PathBuf::from("/models/test.gguf"),
            context_size: size,
            gpu_layers: None,
            threads: None,
            server_url: None,
        };

        let provider = LlamaCppProvider::new(config);
        assert_eq!(provider.config().context_size, size);
    }
}

// GPU layers tests
#[test]
fn gpu_layers_none() {
    let provider = LlamaCppProvider::from_model_path("/models/test.gguf");
    assert!(provider.config().gpu_layers.is_none());
}

#[test]
fn gpu_layers_some() {
    let config = LlamaCppConfig {
        model_path: PathBuf::from("/models/test.gguf"),
        context_size: 4096,
        gpu_layers: Some(32),
        threads: None,
        server_url: None,
    };

    let provider = LlamaCppProvider::new(config);
    assert_eq!(provider.config().gpu_layers, Some(32));
}

#[test]
fn gpu_layers_variations() {
    let layers = vec![0, 16, 24, 32, 48, 64];

    for layers_count in layers {
        let config = LlamaCppConfig {
            model_path: PathBuf::from("/models/test.gguf"),
            context_size: 4096,
            gpu_layers: Some(layers_count),
            threads: None,
            server_url: None,
        };

        let provider = LlamaCppProvider::new(config);
        assert_eq!(provider.config().gpu_layers, Some(layers_count));
    }
}

// Threads tests
#[test]
fn threads_none() {
    let provider = LlamaCppProvider::from_model_path("/models/test.gguf");
    assert!(provider.config().threads.is_none());
}

#[test]
fn threads_some() {
    let config = LlamaCppConfig {
        model_path: PathBuf::from("/models/test.gguf"),
        context_size: 4096,
        gpu_layers: None,
        threads: Some(8),
        server_url: None,
    };

    let provider = LlamaCppProvider::new(config);
    assert_eq!(provider.config().threads, Some(8));
}

#[test]
fn threads_variations() {
    let thread_counts = vec![1, 2, 4, 8, 16, 32];

    for threads in thread_counts {
        let config = LlamaCppConfig {
            model_path: PathBuf::from("/models/test.gguf"),
            context_size: 4096,
            gpu_layers: None,
            threads: Some(threads),
            server_url: None,
        };

        let provider = LlamaCppProvider::new(config);
        assert_eq!(provider.config().threads, Some(threads));
    }
}

// Request structure tests
#[test]
fn completion_request_local() {
    let request = CompletionRequest {
        model: "llama-2-7b".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
        }],
        max_tokens: Some(256),
        temperature: Some(0.7),
        stream: false,
    };

    assert_eq!(request.model, "llama-2-7b");
}

#[test]
fn completion_request_with_system_message() {
    let request = CompletionRequest {
        model: "mistral-7b".to_string(),
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
        max_tokens: Some(128),
        temperature: None,
        stream: false,
    };

    assert_eq!(request.messages.len(), 2);
}

// GGUF model tests
#[test]
fn gguf_model_names() {
    let models = vec![
        "llama-2-7b.gguf",
        "llama-2-13b.gguf",
        "llama-2-70b.gguf",
        "mistral-7b-instruct.gguf",
        "qwen-7b-chat.gguf",
    ];

    for model in models {
        let provider = LlamaCppProvider::from_model_path(format!("/models/{}", model));
        assert!(provider.model_path().to_string_lossy().contains(".gguf"));
    }
}

// Integration-style tests
#[tokio::test]
async fn complete_without_server_should_fail() {
    let provider = LlamaCppProvider::from_model_path("/models/test.gguf");
    let request = CompletionRequest {
        model: "local".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Test".to_string(),
        }],
        max_tokens: None,
        temperature: None,
        stream: false,
    };

    // This will fail because no llama.cpp server is running
    let result: anyhow::Result<CompletionResponse> =
        provider.complete("test-session", &request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn complete_stream_without_server_should_fail() {
    let provider = LlamaCppProvider::from_model_path("/models/test.gguf");
    let request = CompletionRequest {
        model: "local".to_string(),
        messages: vec![],
        max_tokens: None,
        temperature: None,
        stream: true,
    };

    let result: anyhow::Result<BoxStream<anyhow::Result<StreamEvent>>> =
        provider.complete_stream("test-session", &request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn embed_not_implemented() {
    let provider = LlamaCppProvider::from_model_path("/models/test.gguf");
    let request = EmbeddingRequest {
        model: "local".to_string(),
        input: vec!["test".to_string()],
    };

    let result: anyhow::Result<EmbeddingResponse> = provider.embed("test-session", &request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn list_models_without_server() {
    let provider = LlamaCppProvider::from_model_path("/models/qwen-7b.gguf");

    // list_models should return the model filename even without server
    let models: Vec<String> = provider.list_models().await.unwrap();
    assert!(!models.is_empty());
    assert!(models.iter().any(|m| m.contains("qwen-7b.gguf")));
}

#[tokio::test]
async fn list_models_with_server_url() {
    let provider = LlamaCppProvider::from_server_url("http://localhost:8080".to_string());

    // Without a running server, this should fail
    let result: anyhow::Result<Vec<String>> = provider.list_models().await;
    assert!(result.is_err());
}

// Provider comparison tests
#[test]
fn different_model_paths_different_providers() {
    let provider1 = LlamaCppProvider::from_model_path("/models/llama.gguf");
    let provider2 = LlamaCppProvider::from_model_path("/models/mistral.gguf");

    assert_ne!(provider1.model_path(), provider2.model_path());
}

#[test]
fn same_model_path_same_provider() {
    let provider1 = LlamaCppProvider::from_model_path("/models/same.gguf");
    let provider2 = LlamaCppProvider::from_model_path("/models/same.gguf");

    assert_eq!(provider1.model_path(), provider2.model_path());
}

// Edge cases
#[test]
fn model_path_empty_string() {
    let provider = LlamaCppProvider::from_model_path("");
    assert_eq!(provider.model_path(), std::path::Path::new(""));
}

#[test]
fn model_path_relative() {
    let provider = LlamaCppProvider::from_model_path("./models/test.gguf");
    assert!(provider.model_path().is_relative());
}

#[test]
fn model_path_absolute() {
    let provider = LlamaCppProvider::from_model_path("/absolute/path/test.gguf");
    assert!(provider.model_path().is_absolute());
}

#[test]
fn context_size_zero() {
    let config = LlamaCppConfig {
        model_path: PathBuf::from("/models/test.gguf"),
        context_size: 0,
        gpu_layers: None,
        threads: None,
        server_url: None,
    };

    let provider = LlamaCppProvider::new(config);
    assert_eq!(provider.config().context_size, 0);
}

#[test]
fn gpu_layers_zero() {
    let config = LlamaCppConfig {
        model_path: PathBuf::from("/models/test.gguf"),
        context_size: 4096,
        gpu_layers: Some(0),
        threads: None,
        server_url: None,
    };

    let provider = LlamaCppProvider::new(config);
    assert_eq!(provider.config().gpu_layers, Some(0));
}

#[test]
fn threads_one() {
    let config = LlamaCppConfig {
        model_path: PathBuf::from("/models/test.gguf"),
        context_size: 4096,
        gpu_layers: None,
        threads: Some(1),
        server_url: None,
    };

    let provider = LlamaCppProvider::new(config);
    assert_eq!(provider.config().threads, Some(1));
}
