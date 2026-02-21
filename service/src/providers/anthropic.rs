use anyhow::{Result, bail};
use futures_util::stream;
use serde::{Deserialize, Serialize};

use crate::middleware::keyring as secure_keyring;
use crate::providers::{
    BoxStream, ChatMessage, Choice, CompletionRequest, CompletionResponse, EmbeddingRequest,
    EmbeddingResponse, Provider, RetryConfig, StreamEvent, Usage, with_retry,
};

/// Adapter for Anthropic's Claude API.
pub struct AnthropicProvider {
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider with the given API key.
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.anthropic.com/v1".to_string(),
        }
    }

    /// Create a new Anthropic provider with a custom base URL.
    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self { api_key, base_url }
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
    // Keyring helpers
    // -----------------------------------------------------------------------

    /// Persist the API key in the OS keyring.
    pub fn save_to_keyring(&self) -> Result<()> {
        secure_keyring::set_password("fire-box-anthropic", "api-key", &self.api_key)
            .map_err(|e| anyhow::anyhow!("failed to save Anthropic key: {e}"))
    }

    /// Load the API key from the OS keyring and construct a provider.
    pub fn from_keyring() -> Result<Self> {
        let key = secure_keyring::get_password("fire-box-anthropic", "api-key")
            .map_err(|e| anyhow::anyhow!("failed to load Anthropic key: {e}"))?;
        Ok(Self::new(key))
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn build_client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_default()
    }

    /// Convert messages to Anthropic format.
    /// Anthropic separates system messages from conversation messages.
    fn prepare_messages(request: &CompletionRequest) -> (Option<String>, Vec<AnthropicMessage>) {
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

        (system, messages)
    }

    fn prepare_request(&self, request: &CompletionRequest, stream: bool) -> serde_json::Value {
        let (system, messages) = Self::prepare_messages(request);

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages.iter().map(|m| serde_json::json!({
                "role": m.role,
                "content": m.content
            })).collect::<Vec<_>>(),
            "stream": stream,
        });

        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        } else {
            // Anthropic requires max_tokens
            body["max_tokens"] = serde_json::json!(4096);
        }

        if let Some(temperature) = request.temperature {
            body["temperature"] = serde_json::json!(temperature);
        }

        if let Some(sys) = system {
            body["system"] = serde_json::json!(sys);
        }

        body
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

        // Anthropic returns content as an array of content blocks
        let content = json
            .get("content")
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                arr.iter()
                    .find(|block| block.get("type").and_then(|t| t.as_str()) == Some("text"))
                    .and_then(|block| block.get("text").and_then(|t| t.as_str()))
            })
            .unwrap_or("");
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
                    content: content.to_string(),
                },
                finish_reason: stop_reason,
            }],
            usage,
        })
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
        let api_key = self.api_key.clone();
        let base_url = self.base_url.clone();
        let body = self.prepare_request(request, false);

        with_retry(&retry_config, || async {
            let client = Self::build_client();
            let url = format!("{}/messages", base_url);

            let response = client
                .post(&url)
                .header("Content-Type", "application/json")
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                bail!("Anthropic API error: HTTP {} - {}", status, error_text);
            }

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
        let client = Self::build_client();
        let url = format!("{}/messages", self.base_url);

        let body = self.prepare_request(request, true);

        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("Anthropic API error: HTTP {} - {}", status, error_text);
        }

        let event_stream = response.bytes_stream();

        let stream = stream::unfold(event_stream, |mut stream| async move {
            use futures_util::stream::StreamExt;

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        let text = String::from_utf8_lossy(&chunk);
                        for line in text.lines() {
                            let line = line.trim();
                            if line.is_empty() {
                                continue;
                            }

                            // Anthropic uses "data: " prefix for SSE
                            if !line.starts_with("data: ") {
                                continue;
                            }

                            let data = &line[6..];
                            if data == "[DONE]" {
                                return Some((Ok(StreamEvent::Done), stream));
                            }

                            match serde_json::from_str::<serde_json::Value>(data) {
                                Ok(json) => {
                                    let event_type = json.get("type").and_then(|v| v.as_str());
                                    match event_type {
                                        Some("content_block_delta") => {
                                            if let Some(delta) = json.get("delta")
                                                && let Some(text) = delta.get("text")
                                                && let Some(text_str) = text.as_str()
                                                && !text_str.is_empty()
                                            {
                                                return Some((
                                                    Ok(StreamEvent::Delta {
                                                        content: text_str.to_string(),
                                                    }),
                                                    stream,
                                                ));
                                            }
                                        }
                                        Some("message_stop") => {
                                            return Some((Ok(StreamEvent::Done), stream));
                                        }
                                        Some("error") => {
                                            let msg = json
                                                .get("error")
                                                .and_then(|e| e.get("message"))
                                                .and_then(|m| m.as_str())
                                                .unwrap_or("Unknown error");
                                            return Some((
                                                Err(anyhow::anyhow!("Anthropic error: {}", msg)),
                                                stream,
                                            ));
                                        }
                                        _ => {}
                                    }
                                }
                                Err(e) => {
                                    return Some((
                                        Err(anyhow::anyhow!("Failed to parse SSE: {}", e)),
                                        stream,
                                    ));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        return Some((Err(anyhow::anyhow!("Stream error: {}", e)), stream));
                    }
                }
            }

            Some((Ok(StreamEvent::Done), stream))
        });

        Ok(Box::pin(stream))
    }

    async fn embed(
        &self,
        _session_id: &str,
        _request: &EmbeddingRequest,
    ) -> anyhow::Result<EmbeddingResponse> {
        bail!("Anthropic provider: embeddings are not supported by the Anthropic API")
    }

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        // Anthropic doesn't have a public models endpoint
        // Return known Claude models
        Ok(vec![
            "claude-opus-4-5-20251001".to_string(),
            "claude-sonnet-4-5-20251001".to_string(),
            "claude-3-5-sonnet-20241022".to_string(),
            "claude-3-5-haiku-20241022".to_string(),
            "claude-3-opus-20240229".to_string(),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_with_default_url() {
        let p = AnthropicProvider::new("ant-key".to_string());
        assert_eq!(p.base_url(), "https://api.anthropic.com/v1");
        assert_eq!(p.api_key(), "ant-key");
    }

    #[test]
    fn create_with_custom_url() {
        let p = AnthropicProvider::with_base_url(
            "ant-key".to_string(),
            "http://localhost:9090".to_string(),
        );
        assert_eq!(p.base_url(), "http://localhost:9090");
    }
}
