//! Tests for client types.

use firebox_client::*;

#[test]
fn test_chat_message_creation() {
    let msg = ChatMessage {
        role: "user".to_string(),
        content: "Hello".to_string(),
        name: None,
        tool_calls: None,
        tool_call_id: None,
    };

    assert_eq!(msg.role, "user");
    assert_eq!(msg.content, "Hello");
    assert!(msg.name.is_none());
}

#[test]
fn test_tool_creation() {
    let tool = Tool {
        tool_type: "function".to_string(),
        function: ToolFunction {
            name: "get_weather".to_string(),
            description: Some("Get weather info".to_string()),
            parameters: None,
        },
    };

    assert_eq!(tool.tool_type, "function");
    assert_eq!(tool.function.name, "get_weather");
}

#[test]
fn test_usage_creation() {
    let usage = Usage {
        prompt_tokens: 10,
        completion_tokens: 20,
        total_tokens: 30,
    };

    assert_eq!(usage.prompt_tokens, 10);
    assert_eq!(usage.completion_tokens, 20);
    assert_eq!(usage.total_tokens, 30);
}

#[test]
fn test_completion_request_creation() {
    let request = CompletionRequest {
        model_id: "gpt-4".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Test".to_string(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        tools: vec![],
        temperature: Some(0.7),
        max_tokens: Some(100),
    };

    assert_eq!(request.model_id, "gpt-4");
    assert_eq!(request.messages.len(), 1);
    assert_eq!(request.temperature, Some(0.7));
}

#[test]
fn test_stream_chunk_variants() {
    let delta = StreamChunk::Delta("text".to_string());
    let done = StreamChunk::Done {
        usage: None,
        finish_reason: Some("stop".to_string()),
    };
    let error = StreamChunk::Error("error".to_string());

    match delta {
        StreamChunk::Delta(text) => assert_eq!(text, "text"),
        _ => panic!("Expected Delta variant"),
    }

    match done {
        StreamChunk::Done { finish_reason, .. } => {
            assert_eq!(finish_reason, Some("stop".to_string()))
        }
        _ => panic!("Expected Done variant"),
    }

    match error {
        StreamChunk::Error(msg) => assert_eq!(msg, "error"),
        _ => panic!("Expected Error variant"),
    }
}

#[test]
fn test_provider_info_creation() {
    let provider = ProviderInfo {
        profile_id: "openai".to_string(),
        display_name: "OpenAI".to_string(),
        provider_type: "openai".to_string(),
        enabled: true,
        oauth_status: None,
    };

    assert_eq!(provider.profile_id, "openai");
    assert!(provider.enabled);
}

#[test]
fn test_model_info_creation() {
    let model = ModelInfo {
        model_id: "gpt-4".to_string(),
        provider_id: "openai".to_string(),
        display_name: "GPT-4".to_string(),
        enabled: true,
        capabilities: vec!["chat".to_string(), "completion".to_string()],
    };

    assert_eq!(model.model_id, "gpt-4");
    assert_eq!(model.capabilities.len(), 2);
}

#[test]
fn test_routing_rule_creation() {
    let rule = RoutingRule {
        pattern: "gpt-*".to_string(),
        target_provider: "openai".to_string(),
    };

    assert_eq!(rule.pattern, "gpt-*");
    assert_eq!(rule.target_provider, "openai");
}

#[test]
fn test_provider_metrics_creation() {
    let metrics = ProviderMetrics {
        provider_id: "openai".to_string(),
        requests_count: 100,
        errors_count: 5,
        total_prompt_tokens: 1000,
        total_completion_tokens: 2000,
    };

    assert_eq!(metrics.requests_count, 100);
    assert_eq!(metrics.errors_count, 5);
}

#[test]
fn test_embedding_request_creation() {
    let request = EmbeddingRequest {
        model_id: "text-embedding-ada-002".to_string(),
        input: vec!["Hello".to_string(), "World".to_string()],
    };

    assert_eq!(request.model_id, "text-embedding-ada-002");
    assert_eq!(request.input.len(), 2);
}

#[test]
fn test_embedding_response_creation() {
    let response = EmbeddingResponse {
        embeddings: vec![vec![0.1, 0.2, 0.3], vec![0.4, 0.5, 0.6]],
        usage: Some(Usage {
            prompt_tokens: 10,
            completion_tokens: 0,
            total_tokens: 10,
        }),
    };

    assert_eq!(response.embeddings.len(), 2);
    assert_eq!(response.embeddings[0].len(), 3);
}
