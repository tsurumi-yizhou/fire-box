use anyhow::Result;
use futures_util::stream;
use serde::{Deserialize, Serialize};

use crate::middleware::storage;
use crate::providers::{
    BoxStream, ChatMessage, Choice, CompletionRequest, CompletionResponse, EmbeddingRequest,
    EmbeddingResponse, Provider, RetryConfig, RuntimeModelInfo, StreamEvent, ToolCall,
    ToolCallFunction, Usage, with_retry,
};
use crate::providers::{consts, shared};

/// Adapter for Anthropic's Claude API.
///
/// The caller **must** supply the base URL; this struct never hard-codes one.
/// Use [`consts::ANTHROPIC_BASE_URL`] when targeting the official Anthropic API.
pub struct AnthropicProvider {
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
    /// Create an Anthropic provider with an explicit base URL.
    pub fn new(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
        }
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Return the configured API key.
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    // -----------------------------------------------------------------------
    // Secure storage helpers
    // -----------------------------------------------------------------------

    /// Persist the API key in platform-specific secure storage with biometric protection.
    pub fn save_to_keyring(&self) -> Result<()> {
        storage::set_secret_with_biometric("fire-box-anthropic", "api-key", &self.api_key)
            .map_err(|e| anyhow::anyhow!("failed to save Anthropic key: {e}"))
    }

    /// Load the API key from platform-specific secure storage and construct a provider
    /// pointing at the official Anthropic API endpoint.
    pub fn from_keyring() -> Result<Self> {
        let key = storage::get_secret("fire-box-anthropic", "api-key")
            .map_err(|e| anyhow::anyhow!("failed to load Anthropic key: {e}"))?;
        Ok(Self::new(key.as_str(), consts::ANTHROPIC_BASE_URL))
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn prepare_request(&self, request: &CompletionRequest, stream: bool) -> serde_json::Value {
        let mut system: Option<String> = None;
        let mut messages = Vec::new();

        for msg in &request.messages {
            if msg.role == "system" {
                system = Some(msg.content.clone());
            } else if msg.role == "user" || msg.role == "assistant" {
                messages.push(AnthropicMessage {
                    role: msg.role.clone(),
                    content: msg.content.clone(),
                });
            }
        }

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages.iter().map(|m| serde_json::json!({
                "role": m.role,
                "content": m.content
            })).collect::<Vec<_>>(),
            "stream": stream,
            "max_tokens": request.max_tokens.unwrap_or(consts::ANTHROPIC_DEFAULT_MAX_TOKENS),
        });

        if let Some(temperature) = request.temperature {
            body["temperature"] = serde_json::json!(temperature);
        }
        if let Some(sys) = system {
            body["system"] = serde_json::json!(sys);
        }
        if let Some(tools) = request.tools.as_ref().filter(|t| !t.is_empty()) {
            body["tools"] = serde_json::Value::Array(
                tools
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "name": t.function.name,
                            "description": t.function.description,
                            "input_schema": t.function.parameters,
                        })
                    })
                    .collect(),
            );
        }

        body
    }

    fn anthropic_headers(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder
            .header("Content-Type", "application/json")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", consts::ANTHROPIC_API_VERSION)
    }

    fn parse_response(&self, json: serde_json::Value) -> Result<CompletionResponse> {
        let id = json
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let model = json
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let content_blocks = json
            .get("content")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let content = content_blocks
            .iter()
            .filter(|block| block.get("type").and_then(|t| t.as_str()) == Some("text"))
            .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
            .collect::<String>();
        let tool_calls = Self::parse_tool_calls(&content_blocks);
        let stop_reason = json
            .get("stop_reason")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let usage = json.get("usage").map(|u| Usage {
            prompt_tokens: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            completion_tokens: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            total_tokens: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32
                + u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        });

        Ok(CompletionResponse {
            id,
            model,
            choices: vec![Choice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content,
                    tool_calls,
                    tool_call_id: None,
                    name: None,
                },
                finish_reason: stop_reason,
            }],
            usage,
        })
    }

    fn parse_tool_calls(content_blocks: &[serde_json::Value]) -> Option<Vec<ToolCall>> {
        let mut out = Vec::new();
        for block in content_blocks {
            if block.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                continue;
            }
            let name = block
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                continue;
            }
            let id = block
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let arguments = block
                .get("input")
                .map(serde_json::to_string)
                .transpose()
                .ok()
                .flatten()
                .unwrap_or_else(|| "{}".to_string());
            out.push(ToolCall {
                id,
                call_type: "function".to_string(),
                function: ToolCallFunction { name, arguments },
            });
        }
        if out.is_empty() { None } else { Some(out) }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

impl Provider for AnthropicProvider {
    async fn complete(
        &self,
        _session_id: &str,
        request: &CompletionRequest,
    ) -> anyhow::Result<CompletionResponse> {
        let retry_config = RetryConfig::default();
        let base_url = self.base_url.clone();
        let body = self.prepare_request(request, false);

        with_retry(&retry_config, || async {
            let client = shared::build_http_client(consts::HTTP_TIMEOUT);
            let url = format!("{}/messages", base_url);
            let builder = self.anthropic_headers(client.post(&url));
            let response = shared::check_status(builder.json(&body).send().await?).await?;
            let json: serde_json::Value = response.json().await?;
            self.parse_response(json)
        })
        .await
    }

    async fn complete_stream(
        &self,
        _session_id: &str,
        request: &CompletionRequest,
    ) -> anyhow::Result<BoxStream<anyhow::Result<StreamEvent>>> {
        let client = shared::build_http_client(consts::HTTP_TIMEOUT);
        let url = format!("{}/messages", self.base_url);
        let body = self.prepare_request(request, true);
        let builder = self.anthropic_headers(client.post(&url));
        let response = shared::check_status(builder.json(&body).send().await?).await?;
        let event_stream = response.bytes_stream();

        let stream = stream::unfold(
            (event_stream, String::new()),
            |(mut stream, mut buffer)| async move {
                use futures_util::stream::StreamExt;

                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            buffer.push_str(&String::from_utf8_lossy(&chunk));

                            while let Some(newline_pos) = buffer.find('\n') {
                                let line = buffer[..newline_pos].trim().to_string();
                                buffer.drain(..=newline_pos);

                                let data = match shared::sse_data(&line) {
                                    Some(d) => d.to_string(),
                                    None => continue,
                                };

                                if data == "[DONE]" {
                                    return Some((Ok(StreamEvent::Done), (stream, buffer)));
                                }

                                match serde_json::from_str::<serde_json::Value>(&data) {
                                    Ok(json) => match json.get("type").and_then(|v| v.as_str()) {
                                        Some("content_block_delta") => {
                                            if let Some(delta) = json.get("delta") {
                                                if let Some(partial_json) = delta
                                                    .get("partial_json")
                                                    .and_then(|v| v.as_str())
                                                    && !partial_json.is_empty()
                                                {
                                                    let id = json
                                                        .get("index")
                                                        .and_then(|v| v.as_i64())
                                                        .map(|i| format!("tool_{i}"))
                                                        .unwrap_or_else(|| "tool_0".to_string());
                                                    return Some((
                                                        Ok(StreamEvent::ToolCalls {
                                                            tool_calls: vec![ToolCall {
                                                                id,
                                                                call_type: "function".to_string(),
                                                                function: ToolCallFunction {
                                                                    name: String::new(),
                                                                    arguments: partial_json
                                                                        .to_string(),
                                                                },
                                                            }],
                                                        }),
                                                        (stream, buffer),
                                                    ));
                                                }
                                                if let Some(text_str) =
                                                    delta.get("text").and_then(|v| v.as_str())
                                                    && !text_str.is_empty()
                                                {
                                                    return Some((
                                                        Ok(StreamEvent::Delta {
                                                            content: text_str.to_string(),
                                                        }),
                                                        (stream, buffer),
                                                    ));
                                                }
                                            }
                                        }
                                        Some("message_stop") => {
                                            return Some((Ok(StreamEvent::Done), (stream, buffer)));
                                        }
                                        Some("content_block_start") => {
                                            if let Some(block) = json.get("content_block")
                                                && block.get("type").and_then(|v| v.as_str())
                                                    == Some("tool_use")
                                            {
                                                let id = block
                                                    .get("id")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or_default()
                                                    .to_string();
                                                let name = block
                                                    .get("name")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or_default()
                                                    .to_string();
                                                if !name.is_empty() {
                                                    let arguments = block
                                                        .get("input")
                                                        .map(serde_json::to_string)
                                                        .transpose()
                                                        .ok()
                                                        .flatten()
                                                        .unwrap_or_else(|| "{}".to_string());
                                                    return Some((
                                                        Ok(StreamEvent::ToolCalls {
                                                            tool_calls: vec![ToolCall {
                                                                id,
                                                                call_type: "function".to_string(),
                                                                function: ToolCallFunction {
                                                                    name,
                                                                    arguments,
                                                                },
                                                            }],
                                                        }),
                                                        (stream, buffer),
                                                    ));
                                                }
                                            }
                                        }
                                        Some("error") => {
                                            let msg = json
                                                .get("error")
                                                .and_then(|e| e.get("message"))
                                                .and_then(|m| m.as_str())
                                                .unwrap_or("Unknown error");
                                            return Some((
                                                Err(anyhow::anyhow!("Anthropic error: {}", msg)),
                                                (stream, buffer),
                                            ));
                                        }
                                        _ => {}
                                    },
                                    Err(e) => {
                                        return Some((
                                            Err(anyhow::anyhow!("Failed to parse SSE: {}", e)),
                                            (stream, buffer),
                                        ));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            return Some((
                                Err(anyhow::anyhow!("Stream error: {}", e)),
                                (stream, buffer),
                            ));
                        }
                    }
                }

                None
            },
        );

        Ok(Box::pin(stream))
    }

    async fn embed(
        &self,
        _session_id: &str,
        _request: &EmbeddingRequest,
    ) -> anyhow::Result<EmbeddingResponse> {
        Err(crate::providers::ProviderError::RequestFailed(
            "Anthropic provider does not support embeddings".to_string(),
        )
        .into())
    }

    /// List models available via the Anthropic `/v1/models` endpoint.
    async fn list_models(&self) -> anyhow::Result<Vec<RuntimeModelInfo>> {
        let client = shared::build_http_client(consts::HTTP_TIMEOUT);
        let url = format!("{}/models", self.base_url);
        let builder = self.anthropic_headers(client.get(&url));
        let response = shared::check_status(builder.send().await?).await?;
        let json: serde_json::Value = response.json().await?;

        let models = json["data"].as_array().cloned().unwrap_or_default();
        Ok(models
            .iter()
            .filter_map(|m| {
                let id = m.get("id").and_then(|v| v.as_str())?;
                Some(RuntimeModelInfo {
                    id: id.to_string(),
                    owner: "anthropic".to_string(),
                    created: None,
                    context_window: None,
                })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_with_base_url() {
        let p = AnthropicProvider::new("ant-key", consts::ANTHROPIC_BASE_URL);
        assert_eq!(p.base_url(), consts::ANTHROPIC_BASE_URL);
        assert_eq!(p.api_key(), "ant-key");
    }

    #[test]
    fn create_with_custom_url() {
        let p = AnthropicProvider::new("ant-key", "http://localhost:9090");
        assert_eq!(p.base_url(), "http://localhost:9090");
    }
}
