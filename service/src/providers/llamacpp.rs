use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::{Result, bail};
use futures_util::stream;

use crate::middleware::storage;
use crate::providers::{
    BoxStream, ChatMessage, Choice, CompletionRequest, CompletionResponse, EmbeddingRequest,
    EmbeddingResponse, Provider, RuntimeModelInfo, StreamEvent, ToolCall, ToolCallFunction, Usage,
};
use crate::providers::{consts, shared};

/// Configuration for the local llama.cpp inference engine.
#[derive(Debug, Clone)]
pub struct LlamaCppConfig {
    /// Path to the GGUF model file.
    pub model_path: PathBuf,
    /// Path to the llama-server binary.
    pub server_path: Option<PathBuf>,
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
            client: shared::build_http_client(consts::HTTP_TIMEOUT),
            config,
        }
    }

    /// Create a provider with just a model path and sensible defaults.
    pub fn from_model_path(path: impl Into<PathBuf>) -> Self {
        Self {
            config: LlamaCppConfig {
                model_path: path.into(),
                server_path: None,
                context_size: consts::LLAMACPP_DEFAULT_CONTEXT_SIZE,
                gpu_layers: None,
                threads: None,
                server_url: None,
            },
            client: shared::build_http_client(consts::HTTP_TIMEOUT),
        }
    }

    /// Create a provider that connects to a llama.cpp server.
    pub fn from_server_url(url: String) -> Self {
        Self {
            config: LlamaCppConfig {
                model_path: PathBuf::new(),
                server_path: None,
                context_size: consts::LLAMACPP_DEFAULT_CONTEXT_SIZE,
                gpu_layers: None,
                threads: None,
                server_url: Some(url.clone()),
            },
            client: shared::build_http_client(consts::HTTP_TIMEOUT),
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
    // Process Management
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

        let binary = self
            .config
            .server_path
            .as_deref()
            .unwrap_or_else(|| Path::new("llama-server"));

        let mut cmd = Command::new(binary);

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
        let log_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("fire-box")
            .join("logs");
        std::fs::create_dir_all(&log_dir)?;
        let log_path = log_dir.join("llama-server.log");

        let stdout_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| anyhow::anyhow!("Failed to open log file: {}", e))?;
        let stderr_file = stdout_file.try_clone()?;

        cmd.stdout(Stdio::from(stdout_file));
        cmd.stderr(Stdio::from(stderr_file));

        let child = cmd.spawn().map_err(|e| {
            anyhow::anyhow!(
                "Failed to spawn {:?}. Ensure it is in your PATH or configured in 'server_path'. Error: {}",
                binary,
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
    // Secure storage helpers
    // -----------------------------------------------------------------------

    /// Persist the model path in platform-specific secure storage under `fire-box-llamacpp`.
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
        storage::set_secret_with_biometric("fire-box-llamacpp", "model-path", path_str)
            .map_err(|e| anyhow::anyhow!("failed to save model path: {e}"))
    }

    /// Load the model path from platform-specific secure storage and construct a provider
    /// with default configuration parameters.
    pub fn from_keyring() -> Result<Self> {
        let path_str = storage::get_secret("fire-box-llamacpp", "model-path")
            .map_err(|e| anyhow::anyhow!("failed to load model path: {e}"))?;
        Ok(Self::from_model_path(path_str.as_str()))
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn prepare_request(&self, request: &CompletionRequest, stream: bool) -> serde_json::Value {
        let mut body = serde_json::json!({
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
            let name = tc
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                continue;
            }
            let id = tc
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let call_type = tc
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("function")
                .to_string();
            let arguments = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            out.push(ToolCall {
                id,
                call_type,
                function: ToolCallFunction { name, arguments },
            });
        }
        if out.is_empty() { None } else { Some(out) }
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
                tool_calls: Self::parse_tool_calls(&message_json["tool_calls"]),
                tool_call_id: message_json
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string),
                name: message_json
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string),
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

        let response = shared::check_status(
            self.client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await?,
        )
        .await?;

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

        let response = shared::check_status(
            self.client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await?,
        )
        .await?;

        let event_stream = response.bytes_stream();

        let stream = stream::unfold(
            (event_stream, Vec::<ToolCall>::new()),
            |(mut stream, mut pending_tool_calls)| async move {
                use futures_util::stream::StreamExt;

                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            let text = String::from_utf8_lossy(&chunk);
                            for line in text.lines() {
                                let data = match shared::sse_data(line) {
                                    Some(d) => d,
                                    None => continue,
                                };
                                if data == "[DONE]" {
                                    if !pending_tool_calls.is_empty() {
                                        let ready = std::mem::take(&mut pending_tool_calls);
                                        return Some((
                                            Ok(StreamEvent::ToolCalls { tool_calls: ready }),
                                            (stream, pending_tool_calls),
                                        ));
                                    }
                                    return Some((
                                        Ok(StreamEvent::Done),
                                        (stream, pending_tool_calls),
                                    ));
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
                                                    (stream, pending_tool_calls),
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
                                            if let Some(finish_reason) = choice.get("finish_reason")
                                                && finish_reason.as_str().is_some()
                                            {
                                                if !pending_tool_calls.is_empty() {
                                                    let ready =
                                                        std::mem::take(&mut pending_tool_calls);
                                                    return Some((
                                                        Ok(StreamEvent::ToolCalls {
                                                            tool_calls: ready,
                                                        }),
                                                        (stream, pending_tool_calls),
                                                    ));
                                                }
                                                return Some((
                                                    Ok(StreamEvent::Done),
                                                    (stream, pending_tool_calls),
                                                ));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        return Some((
                                            Err(anyhow::anyhow!("Failed to parse SSE: {}", e)),
                                            (stream, pending_tool_calls),
                                        ));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            return Some((
                                Err(anyhow::anyhow!("Stream error: {}", e)),
                                (stream, pending_tool_calls),
                            ));
                        }
                    }
                }

                Some((Ok(StreamEvent::Done), (stream, pending_tool_calls)))
            },
        );

        Ok(Box::pin(stream))
    }

    async fn embed(
        &self,
        _session_id: &str,
        request: &EmbeddingRequest,
    ) -> anyhow::Result<EmbeddingResponse> {
        let url = format!("{}/embedding", self.server_url());

        let body = serde_json::json!({
            "content": request.input.join("\n"),
        });

        let response = shared::check_status(
            self.client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await?,
        )
        .await?;

        let json: serde_json::Value = response.json().await?;

        let embedding_vec = json
            .get("embedding")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Invalid response: missing embedding vector"))?;

        let embedding: Vec<f64> = embedding_vec.iter().filter_map(|v| v.as_f64()).collect();

        let usage = json.get("usage").map(|u| crate::providers::Usage {
            prompt_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            completion_tokens: 0,
            total_tokens: u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        });

        Ok(EmbeddingResponse {
            model: request.model.clone(),
            data: vec![crate::providers::Embedding {
                index: 0,
                embedding,
            }],
            usage,
        })
    }

    async fn list_models(&self) -> anyhow::Result<Vec<RuntimeModelInfo>> {
        // Try to fetch from server first
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
                let models: Vec<RuntimeModelInfo> = json
                    .data
                    .into_iter()
                    .map(|m| RuntimeModelInfo {
                        id: m.id,
                        owner: "llamacpp".to_string(),
                        created: None,
                        context_window: None,
                    })
                    .collect();
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

        Ok(vec![RuntimeModelInfo {
            id: model_name,
            owner: "llamacpp".to_string(),
            created: None,
            context_window: None,
        }])
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
            server_path: None,
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
            tools: None,
        };
        assert!(p.complete("test-session", &req).await.is_err());
    }
}
