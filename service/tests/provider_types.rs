//! Tests for provider core types: ChatMessage, CompletionRequest, CompletionResponse, etc.

use firebox_service::providers::{
    ChatMessage, Choice, CompletionRequest, CompletionResponse, Embedding, EmbeddingRequest,
    EmbeddingResponse, ProviderError, StreamEvent, Usage,
};

#[test]
fn chat_message_serialization() {
    let msg = ChatMessage {
        role: "user".to_string(),
        content: "Hello, world!".to_string(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("user"));
    assert!(json.contains("Hello, world!"));
}

#[test]
fn chat_message_deserialization() {
    let json = r#"{"role": "assistant", "content": "Hi there"}"#;
    let msg: ChatMessage = serde_json::from_str(json).unwrap();
    assert_eq!(msg.role, "assistant");
    assert_eq!(msg.content, "Hi there");
}

#[test]
fn completion_request_with_all_fields() {
    let req = CompletionRequest {
        model: "gpt-4".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Test".to_string(),
        }],
        max_tokens: Some(100),
        temperature: Some(0.7),
        stream: true,
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("gpt-4"));
    assert!(json.contains("100"));
    assert!(json.contains("0.7"));
    assert!(json.contains("true"));
}

#[test]
fn completion_request_minimal_fields() {
    let req = CompletionRequest {
        model: "gpt-3.5".to_string(),
        messages: vec![],
        max_tokens: None,
        temperature: None,
        stream: false,
    };
    let json = serde_json::to_string(&req).unwrap();
    // Optional fields should be skipped
    assert!(!json.contains("max_tokens"));
    assert!(!json.contains("temperature"));
}

#[test]
fn completion_response_full() {
    let resp = CompletionResponse {
        id: "resp-123".to_string(),
        model: "gpt-4".to_string(),
        choices: vec![Choice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content: "Response".to_string(),
            },
            finish_reason: Some("stop".to_string()),
        }],
        usage: Some(Usage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
        }),
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("resp-123"));
    assert!(json.contains("gpt-4"));
    assert!(json.contains("stop"));
}

#[test]
fn completion_response_no_usage() {
    let resp = CompletionResponse {
        id: "resp-456".to_string(),
        model: "claude-3".to_string(),
        choices: vec![Choice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content: "No usage info".to_string(),
            },
            finish_reason: None,
        }],
        usage: None,
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(!json.contains("usage"));
}

#[test]
fn usage_serialization() {
    let usage = Usage {
        prompt_tokens: 100,
        completion_tokens: 50,
        total_tokens: 150,
    };
    let json = serde_json::to_string(&usage).unwrap();
    assert!(json.contains("100"));
    assert!(json.contains("50"));
    assert!(json.contains("150"));
}

#[test]
fn choice_with_finish_reason() {
    let choice = Choice {
        index: 1,
        message: ChatMessage {
            role: "assistant".to_string(),
            content: "Test".to_string(),
        },
        finish_reason: Some("length".to_string()),
    };
    let json = serde_json::to_string(&choice).unwrap();
    assert!(json.contains("length"));
}

#[test]
fn choice_without_finish_reason() {
    let choice = Choice {
        index: 0,
        message: ChatMessage {
            role: "user".to_string(),
            content: "Test".to_string(),
        },
        finish_reason: None,
    };
    let json = serde_json::to_string(&choice).unwrap();
    // null might be present or field might be skipped depending on serde config
    assert!(json.contains("user"));
}

#[test]
fn embedding_request_single() {
    let req = EmbeddingRequest {
        model: "text-embedding-ada-002".to_string(),
        input: vec!["hello".to_string()],
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("text-embedding-ada-002"));
    assert!(json.contains("hello"));
}

#[test]
fn embedding_request_multiple() {
    let req = EmbeddingRequest {
        model: "embeddings-v3".to_string(),
        input: vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
        ],
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("first"));
    assert!(json.contains("second"));
    assert!(json.contains("third"));
}

#[test]
fn embedding_response_with_vectors() {
    let resp = EmbeddingResponse {
        model: "embeddings-v3".to_string(),
        data: vec![Embedding {
            index: 0,
            embedding: vec![0.1, 0.2, 0.3, 0.4],
        }],
        usage: Some(Usage {
            prompt_tokens: 5,
            completion_tokens: 0,
            total_tokens: 5,
        }),
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("0.1"));
    assert!(json.contains("0.2"));
}

#[test]
fn embedding_with_empty_vector() {
    let embedding = Embedding {
        index: 0,
        embedding: vec![],
    };
    let json = serde_json::to_string(&embedding).unwrap();
    assert!(json.contains("[]"));
}

#[test]
fn provider_error_auth() {
    let err = ProviderError::Auth("invalid key".to_string());
    assert_eq!(err.to_string(), "authentication failed: invalid key");
}

#[test]
fn provider_error_rate_limited() {
    let err = ProviderError::RateLimited {
        retry_after_secs: 60,
    };
    assert_eq!(err.to_string(), "rate limit exceeded, retry after 60s");
}

#[test]
fn provider_error_model_not_found() {
    let err = ProviderError::ModelNotFound("gpt-5".to_string());
    assert_eq!(err.to_string(), "model not found: gpt-5");
}

#[test]
fn provider_error_request_failed() {
    let err = ProviderError::RequestFailed("connection timeout".to_string());
    assert_eq!(err.to_string(), "request failed: connection timeout");
}

#[test]
fn provider_error_stream() {
    let err = ProviderError::Stream("broken connection".to_string());
    assert_eq!(err.to_string(), "streaming error: broken connection");
}

#[test]
fn provider_error_to_anyhow() {
    let err = ProviderError::Auth("test".to_string());
    let anyhow_err: anyhow::Error = err.into();
    assert!(anyhow_err.to_string().contains("authentication failed"));
}

#[test]
fn stream_event_delta() {
    let event = StreamEvent::Delta {
        content: "Hello ".to_string(),
    };
    match event {
        StreamEvent::Delta { content } => assert_eq!(content, "Hello "),
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn stream_event_done() {
    let event = StreamEvent::Done;
    match event {
        StreamEvent::Done => (),
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn stream_event_error() {
    let event = StreamEvent::Error {
        message: "timeout".to_string(),
    };
    match event {
        StreamEvent::Error { message } => assert_eq!(message, "timeout"),
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn completion_request_multiple_messages() {
    let req = CompletionRequest {
        model: "gpt-4".to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: "You are helpful".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: "Hello!".to_string(),
            },
        ],
        max_tokens: None,
        temperature: None,
        stream: false,
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("system"));
    assert!(json.contains("user"));
    assert!(json.contains("assistant"));
}

#[test]
fn completion_response_multiple_choices() {
    let resp = CompletionResponse {
        id: "multi-choice".to_string(),
        model: "gpt-4".to_string(),
        choices: vec![
            Choice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content: "First".to_string(),
                },
                finish_reason: Some("stop".to_string()),
            },
            Choice {
                index: 1,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content: "Second".to_string(),
                },
                finish_reason: Some("length".to_string()),
            },
        ],
        usage: None,
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("First"));
    assert!(json.contains("Second"));
}

#[test]
fn usage_zero_tokens() {
    let usage = Usage {
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
    };
    let json = serde_json::to_string(&usage).unwrap();
    assert!(json.contains("0"));
}

#[test]
fn chat_message_with_special_characters() {
    let msg = ChatMessage {
        role: "user".to_string(),
        content: "Hello\nWorld\t\"quoted\"".to_string(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let decoded: ChatMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.content, "Hello\nWorld\t\"quoted\"");
}

#[test]
fn completion_request_with_unicode() {
    let req = CompletionRequest {
        model: "gpt-4".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "你好 世界 🌍".to_string(),
        }],
        max_tokens: None,
        temperature: None,
        stream: false,
    };
    let json = serde_json::to_string(&req).unwrap();
    let decoded: CompletionRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.messages[0].content, "你好 世界 🌍");
}

#[test]
fn embedding_with_large_vector() {
    let large_vector: Vec<f64> = (0..1000).map(|i| i as f64 / 1000.0).collect();
    let embedding = Embedding {
        index: 0,
        embedding: large_vector,
    };
    let json = serde_json::to_string(&embedding).unwrap();
    assert!(json.contains("0.0"));
    assert!(json.contains("0.999"));
}
