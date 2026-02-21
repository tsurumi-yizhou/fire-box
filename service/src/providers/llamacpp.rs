use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::{Result, bail};
use futures_util::stream;

use crate::middleware::keyring as secure_keyring;
use crate::providers::{
    BoxStream, ChatMessage, Choice, CompletionRequest, CompletionResponse, EmbeddingRequest,
    EmbeddingResponse, Provider, StreamEvent, Usage,
};

/// Configuration for the local llama.cpp inference engine.
#[derive(Debug, Clone)]
pub struct LlamaCppConfig {
    /// Path to the GGUF model file.
    pub model_path: PathBuf,
    /// Context window size in tokens.
    pub context_size: u32,
    /// Number of layers to offload to GPU (if available).
    pub gpu_layers: Option<u32>,
    /// Number of threads for inference.
    pub threads: Option<u32>,
    /// llama.cpp server URL (if running as server).
    pub server_url: Option<String>,
}

/// Local LLM adapter using llama.cpp for on-device inference.
///
/// Supports synchronous and streaming generation, model configuration,
/// and native library loading.
pub struct LlamaCppProvider {
    config: LlamaCppConfig,
    client: reqwest::Client,
}

impl LlamaCppProvider {
    /// Create a new llama.cpp provider with the given configuration.
    pub fn new(config: LlamaCppConfig) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
            config,
        }
    }

    /// Create a provider with just a model path and sensible defaults.
    pub fn from_model_path(path: impl Into<PathBuf>) -> Self {
        Self {
            config: LlamaCppConfig {
                model_path: path.into(),
                context_size: 4096,
                gpu_layers: None,
                threads: None,
                server_url: None,
            },
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Create a provider that connects to a llama.cpp server.
    pub fn from_server_url(url: String) -> Self {
        Self {
            config: LlamaCppConfig {
                model_path: PathBuf::new(),
                context_size: 4096,
                gpu_layers: None,
                threads: None,
                server_url: Some(url.clone()),
            },
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Return a reference to the current configuration.
    pub fn config(&self) -> &LlamaCppConfig {
        &self.config
    }

    /// Return the model file path.
    pub fn model_path(&self) -> &Path {
        &self.config.model_path
    }

    /// Get the server URL.
    fn server_url(&self) -> String {
        self.config
            .server_url
            .clone()
            .unwrap_or_else(|| "http://localhost:8080".to_string())
    }

    // -----------------------------------------------------------------------
    // Process Management (Task 3)
    // -----------------------------------------------------------------------

    /// Spawn a local `llama-server` process using the current configuration.
    ///
    /// Requires `llama-server` to be in the system PATH.
    /// Returns the child process handle. The caller is responsible for managing
    /// the child process lifecycle (e.g., killing it on shutdown).
    pub fn spawn_server(&self) -> Result<Child> {
        let model_path = self.model_path();
        if !model_path.exists() {
            bail!("Model file not found: {:?}", model_path);
        }

        let mut cmd = Command::new("llama-server");

        // Basic arguments
        cmd.arg("-m").arg(model_path);
        cmd.arg("-c").arg(self.config.context_size.to_string());

        if let Some(layers) = self.config.gpu_layers {
            cmd.arg("-ngl").arg(layers.to_string());
        }

        if let Some(threads) = self.config.threads {
            cmd.arg("-t").arg(threads.to_string());
        }

        // Port configuration (extract from server_url if possible, else default)
        // This is a naive extraction, assuming standard http://host:port format
        if let Some(url_str) = &self.config.server_url
            && let Ok(url) = url::Url::parse(url_str)
        {
            if let Some(port) = url.port() {
                cmd.arg("--port").arg(port.to_string());
            }
            if let Some(host) = url.host_str() {
                cmd.arg("--host").arg(host);
            }
        }

        // Run in background, redirecting output to log
        // TODO: Redirect stdout/stderr to a proper log file
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        let child = cmd.spawn().map_err(|e| {
            anyhow::anyhow!(
                "Failed to spawn llama-server. Ensure it is in your PATH. Error: {}",
                e
            )
        })?;

        Ok(child)
    }

    /// Check if the server is responsive.
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.server_url());
        // Try the dedicated health endpoint first (supported by newer llama.cpp)
        match self.client.get(&url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    return Ok(true);
                }
            }
            Err(_) => {
                // Fallback: try listing models
                let models_url = format!("{}/v1/models", self.server_url());
                if let Ok(resp) = self.client.get(&models_url).send().await {
                    return Ok(resp.status().is_success());
                }
            }
        }
        Ok(false)
    }

    // -----------------------------------------------------------------------
    // Keyring helpers
    // -----------------------------------------------------------------------

    /// Persist the model path in the OS keyring under `fire-box-llamacpp`.
    ///
    /// Only the model path is stored; runtime parameters (context size, GPU
    /// layers, threads) are not persisted here — use
    /// [`crate::providers::config::configure_provider`] for full config
    /// persistence.
    pub fn save_model_path_to_keyring(&self) -> Result<()> {
        let path_str = self
            .config
            .model_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("model path is not valid UTF-8"))?;
        secure_keyring::set_password("fire-box-llamacpp", "model-path", path_str)
            .map_err(|e| anyhow::anyhow!("failed to save model path: {e}"))
    }

    /// Load the model path from the OS keyring and construct a provider
    /// with default configuration parameters.
    pub fn from_keyring() -> Result<Self> {
        let path_str = secure_keyring::get_password("fire-box-llamacpp", "model-path")
            .map_err(|e| anyhow::anyhow!("failed to load model path: {e}"))?;
        Ok(Self::from_model_path(path_str))
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn prepare_request(&self, request: &CompletionRequest, stream: bool) -> serde_json::Value {
        let mut body = serde_json::json!({
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

        let choices_json = json
            .get("choices")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Invalid response: missing choices"))?;

        let mut choices = Vec::new();
        for choice_json in choices_json {
            let index = choice_json
                .get("index")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let message_json = choice_json
                .get("message")
                .ok_or_else(|| anyhow::anyhow!("Invalid response: missing message"))?;
            let message = ChatMessage {
                role: message_json
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("assistant")
                    .to_string(),
                content: message_json
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            };
            let finish_reason = choice_json
                .get("finish_reason")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            choices.push(Choice {
                index,
                message,
                finish_reason,
            });
        }

        let usage = json.get("usage").map(|u| Usage {
            prompt_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            completion_tokens: u
                .get("completion_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            total_tokens: u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        });

        Ok(CompletionResponse {
            id,
            model,
            choices,
            usage,
        })
    }
}

impl Provider for LlamaCppProvider {
    async fn complete(
        &self,
        _session_id: &str,
        request: &CompletionRequest,
    ) -> anyhow::Result<CompletionResponse> {
        let url = format!("{}/v1/chat/completions", self.server_url());
        let body = self.prepare_request(request, false);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("llama.cpp API error: HTTP {} - {}", status, error_text);
        }

        let json: serde_json::Value = response.json().await?;
        self.parse_response(json)
    }

    async fn complete_stream(
        &self,
        _session_id: &str,
        request: &CompletionRequest,
    ) -> anyhow::Result<BoxStream<anyhow::Result<StreamEvent>>> {
        let url = format!("{}/v1/chat/completions", self.server_url());
        let body = self.prepare_request(request, true);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("llama.cpp API error: HTTP {} - {}", status, error_text);
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
                                    if let Some(choices) =
                                        json.get("choices").and_then(|v| v.as_array())
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
        _request: &EmbeddingRequest,
    ) -> anyhow::Result<EmbeddingResponse> {
        bail!("llama.cpp provider: embeddings not yet implemented")
    }

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        // Task 4: Try to fetch from server first
        let server_url = self.server_url();
        let models_url = format!("{}/v1/models", server_url);

        if let Ok(response) = self.client.get(&models_url).send().await
            && response.status().is_success()
        {
            #[derive(serde::Deserialize)]
            struct ModelInfo {
                id: String,
            }
            #[derive(serde::Deserialize)]
            struct ModelsResponse {
                data: Vec<ModelInfo>,
            }

            if let Ok(json) = response.json::<ModelsResponse>().await {
                let models: Vec<String> = json.data.into_iter().map(|m| m.id).collect();
                if !models.is_empty() {
                    return Ok(models);
                }
            }
        }

        // Fallback: use configured filename
        // For llama.cpp, the model is the filename from the model path.
        let model_name = self
            .config
            .model_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();

        if model_name.is_empty() {
            bail!("No model file configured")
        }

        Ok(vec![model_name])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_from_model_path() {
        let p = LlamaCppProvider::from_model_path("/models/llama-7b.gguf");
        assert_eq!(p.model_path(), Path::new("/models/llama-7b.gguf"));
        assert_eq!(p.config().context_size, 4096);
        assert!(p.config().gpu_layers.is_none());
    }

    #[test]
    fn create_with_config() {
        let config = LlamaCppConfig {
            model_path: PathBuf::from("/models/mistral.gguf"),
            context_size: 8192,
            gpu_layers: Some(32),
            threads: Some(8),
            server_url: None,
        };
        let p = LlamaCppProvider::new(config);
        assert_eq!(p.config().context_size, 8192);
        assert_eq!(p.config().gpu_layers, Some(32));
        assert_eq!(p.config().threads, Some(8));
    }

    #[tokio::test]
    #[ignore = "Requires network access"]
    async fn complete_returns_not_implemented() {
        let p = LlamaCppProvider::from_model_path("/tmp/model.gguf");
        let req = CompletionRequest {
            model: "local".to_string(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stream: false,
        };
        assert!(p.complete("test-session", &req).await.is_err());
    }
}
