//! Integration Tests for FireBox Service

use firebox_service::middleware::metrics::{MetricsCollector, RequestTimer};
use firebox_service::middleware::route::{
    RouteTarget, init as route_init, resolve_alias, set_route_rules,
};
use firebox_service::providers::config::ProviderConfig;
use firebox_service::providers::{
    ChatMessage, Choice, CompletionRequest, CompletionResponse, EmbeddingRequest, ProviderError,
    StreamEvent, Usage,
};
use std::time::Duration;

// ============================================================================
// Provider Integration Tests
// ============================================================================

/// Test that all provider types can be constructed and have consistent interfaces
#[test]
fn test_all_provider_types_constructible() {
    // OpenAI
    let _openai = ProviderConfig::openai("sk-test", None);

    // Anthropic
    let _anthropic = ProviderConfig::anthropic("sk-ant", None);

    // Copilot
    let _copilot = ProviderConfig::copilot("gho-token", None);

    // DashScope
    let _dashscope = ProviderConfig::dashscope_oauth("at", "rt", 0, None);

    // LlamaCpp
    let _llamacpp = ProviderConfig::llamacpp("/models/test.gguf");
}

/// Test provider config JSON roundtrip
#[test]
fn test_provider_config_json_roundtrip() {
    let configs = vec![
        ProviderConfig::openai("sk-test", Some("https://custom.openai.com/v1".to_string())),
        ProviderConfig::anthropic("sk-ant", None),
        ProviderConfig::copilot("gho-token", None),
        ProviderConfig::llamacpp("/models/qwen.gguf"),
    ];

    for config in configs {
        let json = config.to_json().expect("Failed to serialize");
        let restored = ProviderConfig::from_json(&json).expect("Failed to deserialize");
        assert_eq!(config.type_slug(), restored.type_slug());
    }
}

// ============================================================================
// Route Integration Tests
// ============================================================================

/// Test complete routing workflow
#[test]
fn test_complete_routing_workflow() {
    let _ = route_init();

    // Set up failover chain
    let targets = vec![
        RouteTarget {
            provider_id: "openai".to_string(),
            model_id: "gpt-4".to_string(),
        },
        RouteTarget {
            provider_id: "anthropic".to_string(),
            model_id: "claude-3".to_string(),
        },
        RouteTarget {
            provider_id: "dashscope".to_string(),
            model_id: "qwen-max".to_string(),
        },
    ];

    set_route_rules("production-model", targets).unwrap();

    // Resolve alias
    let (provider, model) = resolve_alias("production-model").unwrap();
    assert_eq!(provider, "openai");
    assert_eq!(model, "gpt-4");

    // Test failover
    use firebox_service::middleware::route::get_next_target;
    let next = get_next_target("production-model", "openai").unwrap();
    assert_eq!(next.unwrap().0, "anthropic");
}

// ============================================================================
// Metrics Integration Tests
// ============================================================================

/// Test metrics collection across multiple requests
#[test]
fn test_metrics_collection_workflow() {
    let collector = MetricsCollector::new();

    // Simulate multiple requests
    for i in 0..10 {
        let timer = RequestTimer::new(&collector);
        if i % 5 == 0 {
            // Every 5th request fails
            timer.failure();
        } else {
            timer.success(100 + i * 10, 50 + i * 5, 0.01 * (i as f64));
        }
    }

    let snapshot = collector.snapshot(0, 10000);

    assert_eq!(snapshot.requests_total, 10);
    assert_eq!(snapshot.requests_failed, 2); // 0 and 5
    assert!(snapshot.prompt_tokens_total > 0);
    assert!(snapshot.completion_tokens_total > 0);
}

/// Test provider-specific metrics
#[test]
fn test_provider_specific_metrics() {
    let collector = MetricsCollector::new();

    // Record metrics for different providers
    collector.record_success_with_breakdown(
        "openai",
        Some("gpt-4"),
        100,
        50,
        Duration::from_millis(200),
        0.05,
    );

    collector.record_success_with_breakdown(
        "anthropic",
        Some("claude-3"),
        150,
        75,
        Duration::from_millis(250),
        0.075,
    );

    collector.record_success_with_breakdown(
        "dashscope",
        Some("qwen-max"),
        200,
        100,
        Duration::from_millis(300),
        0.02,
    );

    let provider_metrics = collector.get_provider_metrics();
    assert_eq!(provider_metrics.len(), 3);

    // Verify each provider has correct data
    for metrics in provider_metrics {
        match metrics.provider_id.as_str() {
            "openai" => {
                assert_eq!(metrics.model_id, Some("gpt-4".to_string()));
                assert_eq!(metrics.requests_total, 1);
            }
            "anthropic" => {
                assert_eq!(metrics.model_id, Some("claude-3".to_string()));
                assert_eq!(metrics.requests_total, 1);
            }
            "dashscope" => {
                assert_eq!(metrics.model_id, Some("qwen-max".to_string()));
                assert_eq!(metrics.requests_total, 1);
            }
            _ => panic!("Unexpected provider"),
        }
    }
}

// ============================================================================
// Message and Request Integration Tests
// ============================================================================

/// Test conversation flow with multiple messages
#[test]
fn test_conversation_flow() {
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: "You are a helpful coding assistant.".to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: "Write a function to add two numbers.".to_string(),
        },
        ChatMessage {
            role: "assistant".to_string(),
            content: "```python\ndef add(a, b):\n    return a + b\n```".to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: "Now make it handle strings.".to_string(),
        },
    ];

    let request = CompletionRequest {
        model: "gpt-4".to_string(),
        messages,
        max_tokens: Some(500),
        temperature: Some(0.7),
        stream: false,
    };

    assert_eq!(request.messages.len(), 4);
    assert_eq!(request.messages[0].role, "system");
    assert_eq!(request.messages[3].role, "user");
}

/// Test request with various configurations
#[test]
fn test_request_configurations() {
    let base_messages = vec![ChatMessage {
        role: "user".to_string(),
        content: "Test".to_string(),
    }];

    // High temperature, low tokens
    let _req1 = CompletionRequest {
        model: "test".to_string(),
        messages: base_messages.clone(),
        max_tokens: Some(50),
        temperature: Some(2.0),
        stream: false,
    };

    // Low temperature, high tokens
    let _req2 = CompletionRequest {
        model: "test".to_string(),
        messages: base_messages.clone(),
        max_tokens: Some(4096),
        temperature: Some(0.1),
        stream: false,
    };

    // No temperature, no max tokens
    let _req3 = CompletionRequest {
        model: "test".to_string(),
        messages: base_messages.clone(),
        max_tokens: None,
        temperature: None,
        stream: false,
    };

    // Streaming request
    let _req4 = CompletionRequest {
        model: "test".to_string(),
        messages: base_messages,
        max_tokens: None,
        temperature: None,
        stream: true,
    };
}

// ============================================================================
// Response Handling Integration Tests
// ============================================================================

/// Test response parsing
#[test]
fn test_response_structure() {
    let response = CompletionResponse {
        id: "test-response-123".to_string(),
        model: "gpt-4".to_string(),
        choices: vec![
            Choice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content: "First choice".to_string(),
                },
                finish_reason: Some("stop".to_string()),
            },
            Choice {
                index: 1,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content: "Second choice".to_string(),
                },
                finish_reason: Some("length".to_string()),
            },
        ],
        usage: Some(Usage {
            prompt_tokens: 100,
            completion_tokens: 200,
            total_tokens: 300,
        }),
    };

    assert_eq!(response.choices.len(), 2);
    assert_eq!(response.usage.unwrap().total_tokens, 300);
}

/// Test response with no usage info
#[test]
fn test_response_without_usage() {
    let response = CompletionResponse {
        id: "no-usage".to_string(),
        model: "local-model".to_string(),
        choices: vec![Choice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content: "Response".to_string(),
            },
            finish_reason: None,
        }],
        usage: None,
    };

    assert!(response.usage.is_none());
}

// ============================================================================
// Error Handling Integration Tests
// ============================================================================

/// Test error conversion to anyhow
#[test]
fn test_provider_error_to_anyhow() {
    let errors = vec![
        ProviderError::Auth("Invalid API key".to_string()),
        ProviderError::RateLimited {
            retry_after_secs: 60,
        },
        ProviderError::ModelNotFound("gpt-5".to_string()),
        ProviderError::RequestFailed("Connection timeout".to_string()),
        ProviderError::Stream("Broken connection".to_string()),
    ];

    for err in errors {
        let anyhow_err: anyhow::Error = err.into();
        assert!(!anyhow_err.to_string().is_empty());
    }
}

/// Test error messages are informative
#[test]
fn test_error_messages_informative() {
    let auth_err = ProviderError::Auth("sk-invalid".to_string());
    assert!(auth_err.to_string().contains("authentication failed"));

    let rate_err = ProviderError::RateLimited {
        retry_after_secs: 120,
    };
    assert!(rate_err.to_string().contains("120"));

    let not_found_err = ProviderError::ModelNotFound("nonexistent".to_string());
    assert!(not_found_err.to_string().contains("nonexistent"));
}

// ============================================================================
// Stream Event Integration Tests
// ============================================================================

/// Test stream event handling
#[test]
fn test_stream_event_sequence() {
    let events = vec![
        StreamEvent::Delta {
            content: "Hello ".to_string(),
        },
        StreamEvent::Delta {
            content: "world".to_string(),
        },
        StreamEvent::Delta {
            content: "!".to_string(),
        },
        StreamEvent::Done,
    ];

    let mut content = String::new();
    for event in events {
        match event {
            StreamEvent::Delta { content: delta } => content.push_str(&delta),
            StreamEvent::Done => break,
            StreamEvent::Error { message } => panic!("Unexpected error: {}", message),
        }
    }

    assert_eq!(content, "Hello world!");
}

/// Test stream error handling
#[test]
fn test_stream_error_handling() {
    let events = vec![
        StreamEvent::Delta {
            content: "Partial ".to_string(),
        },
        StreamEvent::Error {
            message: "Network error".to_string(),
        },
    ];

    let mut content = String::new();
    let mut error_occurred = false;

    for event in events {
        match event {
            StreamEvent::Delta { content: delta } => content.push_str(&delta),
            StreamEvent::Error { message } => {
                error_occurred = true;
                assert_eq!(message, "Network error");
            }
            StreamEvent::Done => (),
        }
    }

    assert!(error_occurred);
    assert_eq!(content, "Partial ");
}

// ============================================================================
// Embedding Integration Tests
// ============================================================================

/// Test embedding request and response
#[test]
fn test_embedding_workflow() {
    let request = EmbeddingRequest {
        model: "text-embedding-3-small".to_string(),
        input: vec![
            "The quick brown fox".to_string(),
            "jumps over the lazy dog".to_string(),
        ],
    };

    assert_eq!(request.input.len(), 2);
    assert!(request.model.contains("embedding"));
}

// ============================================================================
// Multi-Provider Integration Tests
// ============================================================================

/// Test configuration for multiple providers
#[test]
fn test_multi_provider_setup() {
    let providers = vec![
        ("openai-primary", ProviderConfig::openai("sk-primary", None)),
        ("openai-backup", ProviderConfig::openai("sk-backup", None)),
        ("anthropic-main", ProviderConfig::anthropic("sk-ant", None)),
        (
            "local-llama",
            ProviderConfig::llamacpp("/models/llama.gguf"),
        ),
    ];

    for (name, config) in providers {
        let json = config.to_json().unwrap();
        let restored = ProviderConfig::from_json(&json).unwrap();
        assert_eq!(
            config.type_slug(),
            restored.type_slug(),
            "Provider {} failed",
            name
        );
    }
}

/// Test provider failover configuration
#[test]
fn test_failover_configuration() {
    let _ = route_init();

    // Set up a complete failover chain
    let targets = vec![
        RouteTarget {
            provider_id: "openai".to_string(),
            model_id: "gpt-4-turbo".to_string(),
        },
        RouteTarget {
            provider_id: "anthropic".to_string(),
            model_id: "claude-3-opus".to_string(),
        },
        RouteTarget {
            provider_id: "dashscope".to_string(),
            model_id: "qwen-max".to_string(),
        },
        RouteTarget {
            provider_id: "llamacpp".to_string(),
            model_id: "llama-2-70b.gguf".to_string(),
        },
    ];

    set_route_rules("critical-app", targets.clone()).unwrap();

    // Verify complete chain
    let mut current_provider = "openai";
    for expected_next in ["anthropic", "dashscope", "llamacpp"] {
        let next = get_next_target("critical-app", current_provider).unwrap();
        assert!(next.is_some());
        assert_eq!(next.unwrap().0, expected_next);
        current_provider = expected_next;
    }

    // Last provider should have no next
    let next = get_next_target("critical-app", "llamacpp").unwrap();
    assert!(next.is_none());
}

use firebox_service::middleware::route::get_next_target;

// ============================================================================
// Performance Integration Tests
// ============================================================================

/// Test metrics under load
#[test]
fn test_metrics_under_load() {
    let collector = MetricsCollector::new();

    // Simulate 100 requests
    for i in 0..100 {
        let timer = RequestTimer::new(&collector);
        timer.success(100 + i, 50 + i / 2, 0.001 * (i as f64));
    }

    let snapshot = collector.snapshot(0, 100000);

    assert_eq!(snapshot.requests_total, 100);
    assert_eq!(snapshot.requests_failed, 0);
    assert!(snapshot.latency_avg_ms > 0);

    // Verify token counts
    let expected_prompt: u64 = (100..200).sum();
    let expected_completion: u64 = (50..100).sum();
    assert_eq!(snapshot.prompt_tokens_total, expected_prompt);
    assert_eq!(snapshot.completion_tokens_total, expected_completion);
}

// ============================================================================
// Serialization Integration Tests
// ============================================================================

/// Test complete request/response serialization
#[test]
fn test_request_response_serialization() {
    let request = CompletionRequest {
        model: "gpt-4".to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: "You are helpful.".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: "Hello!".to_string(),
            },
        ],
        max_tokens: Some(1000),
        temperature: Some(0.7),
        stream: false,
    };

    // Serialize request
    let request_json = serde_json::to_string(&request).unwrap();

    // Deserialize request
    let restored_request: CompletionRequest = serde_json::from_str(&request_json).unwrap();
    assert_eq!(request.model, restored_request.model);
    assert_eq!(request.messages.len(), restored_request.messages.len());
}

/// Test embedding serialization
#[test]
fn test_embedding_serialization() {
    use firebox_service::providers::{Embedding, EmbeddingResponse};

    let response = EmbeddingResponse {
        model: "text-embedding-3-small".to_string(),
        data: vec![
            Embedding {
                index: 0,
                embedding: vec![0.1, 0.2, 0.3, 0.4, 0.5],
            },
            Embedding {
                index: 1,
                embedding: vec![0.5, 0.4, 0.3, 0.2, 0.1],
            },
        ],
        usage: Some(Usage {
            prompt_tokens: 10,
            completion_tokens: 0,
            total_tokens: 10,
        }),
    };

    let json = serde_json::to_string(&response).unwrap();
    let restored: EmbeddingResponse = serde_json::from_str(&json).unwrap();

    assert_eq!(response.data.len(), restored.data.len());
    assert_eq!(
        response.data[0].embedding.len(),
        restored.data[0].embedding.len()
    );
}
