use anyhow::{Result, bail};
use futures_util::stream;

use crate::middleware::keyring as secure_keyring;
use crate::providers::{
    BoxStream, ChatMessage, Choice, CompletionRequest, CompletionResponse, EmbeddingRequest,
    EmbeddingResponse, Provider, RetryConfig, StreamEvent, Usage, with_retry,
};

/// Adapter for the OpenAI API and OpenAI-compatible endpoints.
///
/// Supports OpenAI, Ollama, vLLM, and any OpenAI-compatible API.
pub struct OpenAiProvider {
    api_key: Option<String>,
    base_url: String,
}

impl OpenAiProvider {
    /// Create a new OpenAI provider with the given API key.
    pub fn new(api_key: String) -> Self {
        Self {
            api_key: Some(api_key),
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }

    /// Create a new provider with a custom base URL.
    pub fn with_base_url(api_key: Option<String>, base_url: String) -> Self {
        Self { api_key, base_url }
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    // -----------------------------------------------------------------------
    // Keyring helpers
    // -----------------------------------------------------------------------

    /// Persist the API key in the OS keyring.
    pub fn save_to_keyring(&self, service: &str) -> Result<()> {
        if let Some(ref key) = self.api_key {
            secure_keyring::set_password(service, "api-key", key)
                .map_err(|e| anyhow::anyhow!("failed to save API key: {e}"))
        } else {
            Ok(())
        }
    }

    /// Load the API key from the OS keyring and construct a provider.
    pub fn from_keyring(service: &str, base_url: &str) -> Result<Self> {
        let key = secure_keyring::get_password(service, "api-key")
            .map_err(|e| anyhow::anyhow!("failed to load API key: {e}"))?;
        Ok(Self {
            api_key: Some(key),
            base_url: base_url.to_string(),
        })
    }

    // -----------------------------------------------------------------------
    // Convenience constructors for OpenAI-compatible providers
    // -----------------------------------------------------------------------

    /// Create an Ollama provider (no authentication).
    pub fn ollama() -> Self {
        Self {
            api_key: None,
            base_url: "http://localhost:11434/v1".to_string(),
        }
    }

    /// Create a vLLM provider with optional API key.
    pub fn vllm(api_key: Option<String>) -> Self {
        Self {
            api_key,
            base_url: "http://localhost:8000/v1".to_string(),
        }
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

    fn prepare_request(&self, request: &CompletionRequest, stream: bool) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(|m| serde_json::json!({
                "role": m.role,
                "content": m.content
            })).collect::<Vec<_>>(),
            "stream": stream,
        });

        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(temperature) = request.temperature {
            body["temperature"] = serde_json::json!(temperature);
        }

        body
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
            let client = Self::build_client();
            let url = format!("{}/chat/completions", base_url);

            let mut req_builder = client.post(&url).header("Content-Type", "application/json");

            // Add Authorization header if API key is present
            if let Some(ref key) = api_key {
                req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
            }

            let response = req_builder.json(&body).send().await?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                bail!("OpenAI API error: HTTP {} - {}", status, error_text);
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
        let url = format!("{}/chat/completions", self.base_url);

        let body = self.prepare_request(request, true);

        let mut req_builder = client.post(&url).header("Content-Type", "application/json");

        if let Some(ref key) = self.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }

        let response = req_builder.json(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("OpenAI API error: HTTP {} - {}", status, error_text);
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
                            if line.is_empty() || !line.starts_with("data: ") {
                                continue;
                            }

                            let data = &line[6..]; // Skip "data: "
                            if data == "[DONE]" {
                                return Some((Ok(StreamEvent::Done), stream));
                            }

                            match serde_json::from_str::<serde_json::Value>(data) {
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
                                                stream,
                                            ));
                                        }
                                        if let Some(finish_reason) = choice.get("finish_reason")
                                            && finish_reason.is_string()
                                        {
                                            return Some((Ok(StreamEvent::Done), stream));
                                        }
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
        request: &EmbeddingRequest,
    ) -> anyhow::Result<EmbeddingResponse> {
        let client = Self::build_client();
        let url = format!("{}/embeddings", self.base_url);

        let body = serde_json::json!({
            "model": request.model,
            "input": request.input,
        });

        let mut req_builder = client.post(&url).header("Content-Type", "application/json");

        if let Some(ref key) = self.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }

        let response = req_builder.json(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("OpenAI API error: HTTP {} - {}", status, error_text);
        }

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

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct ModelList {
            data: Vec<ModelInfo>,
        }

        #[derive(Deserialize)]
        struct ModelInfo {
            id: String,
        }

        let client = Self::build_client();

        let mut req_builder = client.get(format!("{}/models", self.base_url));

        if let Some(ref key) = self.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }

        let response = req_builder.send().await?;

        if !response.status().is_success() {
            bail!("Failed to fetch models: HTTP {}", response.status());
        }

        let model_list: ModelList = response.json().await?;
        Ok(model_list.data.into_iter().map(|m| m.id).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_with_default_url() {
        let p = OpenAiProvider::new("sk-test".to_string());
        assert_eq!(p.base_url(), "https://api.openai.com/v1");
    }

    #[test]
    fn create_with_custom_url() {
        let p = OpenAiProvider::with_base_url(
            Some("sk-test".to_string()),
            "http://localhost:8080".to_string(),
        );
        assert_eq!(p.base_url(), "http://localhost:8080");
    }

    #[tokio::test]
    #[ignore = "Requires network access"]
    async fn complete_returns_not_implemented() {
        let p = OpenAiProvider::new("sk-test".to_string());
        let req = CompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stream: false,
        };
        let result = p.complete("test-session", &req).await;
        assert!(result.is_err());
    }
}
