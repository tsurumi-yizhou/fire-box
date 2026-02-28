use anyhow::Result;
use futures_util::stream;

use crate::middleware::storage;
use crate::providers::{
    BoxStream, ChatMessage, Choice, CompletionRequest, CompletionResponse, EmbeddingRequest,
    EmbeddingResponse, Provider, RetryConfig, RuntimeModelInfo, StreamEvent, ToolCall,
    ToolCallFunction, Usage, with_retry,
};
use crate::providers::{consts, shared};

/// Adapter for the OpenAI API and OpenAI-compatible endpoints.
///
/// Supports OpenAI, Ollama, vLLM, and any OpenAI-compatible API.
/// The caller **must** supply the base URL; this struct never hard-codes one.
pub struct OpenAiProvider {
    api_key: Option<String>,
    base_url: String,
}

impl OpenAiProvider {
    /// Create a provider with an explicit base URL.
    ///
    /// Use [`consts::OPENAI_BASE_URL`] when targeting the official OpenAI API.
    pub fn with_base_url(api_key: Option<String>, base_url: String) -> Self {
        Self { api_key, base_url }
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    // -----------------------------------------------------------------------
    // Secure storage helpers
    // -----------------------------------------------------------------------

    /// Persist the API key in platform-specific secure storage with biometric protection.
    pub fn save_to_keyring(&self, service: &str) -> Result<()> {
        if let Some(ref key) = self.api_key {
            storage::set_secret_with_biometric(service, "api-key", key)
                .map_err(|e| anyhow::anyhow!("failed to save API key: {e}"))
        } else {
            Ok(())
        }
    }

    /// Load the API key from platform-specific secure storage and construct a provider.
    pub fn from_keyring(service: &str, base_url: &str) -> Result<Self> {
        let key = storage::get_secret(service, "api-key")
            .map_err(|e| anyhow::anyhow!("failed to load API key: {e}"))?;
        Ok(Self {
            api_key: Some(key.to_string()),
            base_url: base_url.to_string(),
        })
    }

    // -----------------------------------------------------------------------
    // Convenience constructors for well-known local endpoints
    // -----------------------------------------------------------------------

    /// Create an Ollama provider pointing at the default local Ollama address.
    pub fn ollama() -> Self {
        Self::with_base_url(None, "http://localhost:11434/v1".to_string())
    }

    /// Create a vLLM provider pointing at the default local vLLM address.
    pub fn vllm(api_key: Option<String>) -> Self {
        Self::with_base_url(api_key, "http://localhost:8000/v1".to_string())
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn prepare_request(&self, request: &CompletionRequest, stream: bool) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(shared::message_to_json).collect::<Vec<_>>(),
            "stream": stream,
        });

        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(temperature) = request.temperature {
            body["temperature"] = serde_json::json!(temperature);
        }
        if let Some(tools) = request.tools.as_ref().filter(|t| !t.is_empty()) {
            body["tools"] =
                serde_json::Value::Array(tools.iter().map(shared::tool_to_json).collect());
        }

        body
    }

    fn parse_tool_calls(v: &serde_json::Value) -> Option<Vec<ToolCall>> {
        let arr = v.as_array()?;
        let mut out = Vec::new();
        for tc in arr {
            let id = tc
                .get("id")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string();
            let call_type = tc
                .get("type")
                .and_then(|x| x.as_str())
                .unwrap_or("function")
                .to_string();
            let fn_obj = tc.get("function").unwrap_or(&serde_json::Value::Null);
            let name = fn_obj
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string();
            let arguments = fn_obj
                .get("arguments")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string();
            if !name.is_empty() {
                out.push(ToolCall {
                    id,
                    call_type,
                    function: ToolCallFunction { name, arguments },
                });
            }
        }
        if out.is_empty() { None } else { Some(out) }
    }

    fn parse_response(&self, json: serde_json::Value) -> Result<CompletionResponse> {
        let id = json["id"].as_str().unwrap_or("").to_string();
        let model = json["model"].as_str().unwrap_or("").to_string();

        let choices_json = json["choices"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Invalid response: missing choices"))?;

        let mut choices = Vec::new();
        for choice_json in choices_json {
            let index = choice_json["index"].as_u64().unwrap_or(0) as u32;
            let message_json = &choice_json["message"];
            let message = ChatMessage {
                role: message_json["role"]
                    .as_str()
                    .unwrap_or("assistant")
                    .to_string(),
                content: message_json["content"].as_str().unwrap_or("").to_string(),
                tool_calls: Self::parse_tool_calls(&message_json["tool_calls"]),
                tool_call_id: message_json["tool_call_id"]
                    .as_str()
                    .map(ToString::to_string),
                name: message_json["name"].as_str().map(ToString::to_string),
            };
            let finish_reason = choice_json["finish_reason"].as_str().map(|s| s.to_string());
            choices.push(Choice {
                index,
                message,
                finish_reason,
            });
        }

        let usage = if json.get("usage").is_some() && !json["usage"].is_null() {
            let u = &json["usage"];
            Some(Usage {
                prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
                total_tokens: u["total_tokens"].as_u64().unwrap_or(0) as u32,
            })
        } else {
            None
        };

        Ok(CompletionResponse {
            id,
            model,
            choices,
            usage,
        })
    }
}

impl Provider for OpenAiProvider {
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
            let client = shared::build_http_client(consts::HTTP_TIMEOUT);
            let url = format!("{}/chat/completions", base_url);

            let mut req_builder = client.post(&url).header("Content-Type", "application/json");
            if let Some(ref key) = api_key {
                req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
            }

            let response = shared::check_status(req_builder.json(&body).send().await?).await?;
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
        let url = format!("{}/chat/completions", self.base_url);
        let body = self.prepare_request(request, true);

        let mut req_builder = client.post(&url).header("Content-Type", "application/json");
        if let Some(ref key) = self.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }

        let response = shared::check_status(req_builder.json(&body).send().await?).await?;
        let event_stream = response.bytes_stream();

        let stream = stream::unfold(
            (event_stream, String::new(), Vec::<ToolCall>::new()),
            |(mut stream, mut buffer, mut pending_tool_calls)| async move {
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
                                    if !pending_tool_calls.is_empty() {
                                        let ready = std::mem::take(&mut pending_tool_calls);
                                        return Some((
                                            Ok(StreamEvent::ToolCalls { tool_calls: ready }),
                                            (stream, buffer, pending_tool_calls),
                                        ));
                                    }
                                    return Some((
                                        Ok(StreamEvent::Done),
                                        (stream, buffer, pending_tool_calls),
                                    ));
                                }

                                match serde_json::from_str::<serde_json::Value>(&data) {
                                    Ok(json) => {
                                        if let Some(choices) = json["choices"].as_array()
                                            && let Some(choice) = choices.first()
                                        {
                                            if let Some(delta) = choice.get("delta")
                                                && let Some(content) = delta.get("content")
                                                && let Some(content_str) = content.as_str()
                                                && !content_str.is_empty()
                                            {
                                                return Some((
                                                    Ok(StreamEvent::Delta {
                                                        content: content_str.to_string(),
                                                    }),
                                                    (stream, buffer, pending_tool_calls),
                                                ));
                                            }
                                            if let Some(delta) = choice.get("delta")
                                                && let Some(tcs) = delta
                                                    .get("tool_calls")
                                                    .and_then(|v| v.as_array())
                                            {
                                                shared::merge_tool_call_deltas(
                                                    &mut pending_tool_calls,
                                                    tcs,
                                                );
                                            }
                                            if choice
                                                .get("finish_reason")
                                                .and_then(|v| v.as_str())
                                                .is_some()
                                            {
                                                if !pending_tool_calls.is_empty() {
                                                    let ready =
                                                        std::mem::take(&mut pending_tool_calls);
                                                    return Some((
                                                        Ok(StreamEvent::ToolCalls {
                                                            tool_calls: ready,
                                                        }),
                                                        (stream, buffer, pending_tool_calls),
                                                    ));
                                                }
                                                return Some((
                                                    Ok(StreamEvent::Done),
                                                    (stream, buffer, pending_tool_calls),
                                                ));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        return Some((
                                            Err(anyhow::anyhow!("Failed to parse SSE: {}", e)),
                                            (stream, buffer, pending_tool_calls),
                                        ));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            return Some((
                                Err(anyhow::anyhow!("Stream error: {}", e)),
                                (stream, buffer, pending_tool_calls),
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
        request: &EmbeddingRequest,
    ) -> anyhow::Result<EmbeddingResponse> {
        let client = shared::build_http_client(consts::HTTP_TIMEOUT);
        let url = format!("{}/embeddings", self.base_url);

        let body = serde_json::json!({
            "model": request.model,
            "input": request.input,
        });

        let mut req_builder = client.post(&url).header("Content-Type", "application/json");
        if let Some(ref key) = self.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }

        let response = shared::check_status(req_builder.json(&body).send().await?).await?;
        let json: serde_json::Value = response.json().await?;

        let model = json
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let data_json = json
            .get("data")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Invalid response: missing data"))?;

        let mut data = Vec::new();
        for embedding_json in data_json {
            let index = embedding_json
                .get("index")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let embedding_vec = embedding_json
                .get("embedding")
                .and_then(|v| v.as_array())
                .ok_or_else(|| anyhow::anyhow!("Invalid response: missing embedding vector"))?;

            let embedding: Vec<f64> = embedding_vec.iter().filter_map(|v| v.as_f64()).collect();
            data.push(crate::providers::Embedding { index, embedding });
        }

        let usage = json.get("usage").map(|u| crate::providers::Usage {
            prompt_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            completion_tokens: 0,
            total_tokens: u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        });

        Ok(EmbeddingResponse { model, data, usage })
    }

    async fn list_models(&self) -> anyhow::Result<Vec<RuntimeModelInfo>> {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct ModelList {
            data: Vec<ModelInfo>,
        }

        #[derive(Deserialize)]
        struct ModelInfo {
            id: String,
        }

        let client = shared::build_http_client(consts::HTTP_TIMEOUT);
        let mut req_builder = client.get(format!("{}/models", self.base_url));
        if let Some(ref key) = self.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }

        let response = shared::check_status(req_builder.send().await?).await?;
        let model_list: ModelList = response.json().await?;

        Ok(model_list
            .data
            .into_iter()
            .map(|m| RuntimeModelInfo {
                id: m.id,
                owner: "openai".to_string(),
                created: None,
                context_window: None,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_with_base_url() {
        let p = OpenAiProvider::with_base_url(
            Some("sk-test".to_string()),
            consts::OPENAI_BASE_URL.to_string(),
        );
        assert_eq!(p.base_url(), consts::OPENAI_BASE_URL);
    }

    #[test]
    fn create_with_custom_url() {
        let p = OpenAiProvider::with_base_url(
            Some("sk-test".to_string()),
            "http://localhost:8080".to_string(),
        );
        assert_eq!(p.base_url(), "http://localhost:8080");
    }

    #[test]
    fn ollama_uses_local_address() {
        let p = OpenAiProvider::ollama();
        assert!(p.base_url().starts_with("http://localhost"));
        assert!(p.api_key.is_none());
    }

    #[tokio::test]
    #[ignore = "Requires network access"]
    async fn complete_returns_error_without_valid_key() {
        let p = OpenAiProvider::with_base_url(
            Some("sk-invalid".to_string()),
            consts::OPENAI_BASE_URL.to_string(),
        );
        let req = CompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stream: false,
            tools: None,
        };
        let result = p.complete("test-session", &req).await;
        assert!(result.is_err());
    }
}
