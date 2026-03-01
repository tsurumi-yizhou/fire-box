//! D-Bus communication layer for Linux.

use std::collections::HashMap;
use std::time::Duration;

use zbus::{Connection, proxy};

use crate::error::{Error, Result};
use crate::types::*;

const SERVICE_NAME: &str = "com.firebox.Service";
const OBJECT_PATH: &str = "/com/firebox/Service";

#[proxy(
    interface = "com.firebox.Service",
    default_service = "com.firebox.Service",
    default_path = "/com/firebox/Service"
)]
trait FireBoxService {
    // Control methods
    fn ping(&self) -> zbus::Result<()>;

    fn add_api_key_provider(
        &self,
        name: &str,
        provider_type: &str,
        api_key: &str,
        base_url: Option<&str>,
    ) -> zbus::Result<()>;

    fn add_oauth_provider(
        &self,
        name: &str,
        provider_type: &str,
    ) -> zbus::Result<(String, String, u64)>;

    fn complete_oauth(&self, profile_id: &str) -> zbus::Result<()>;

    fn add_local_provider(&self, name: &str, base_url: &str) -> zbus::Result<()>;

    fn list_providers(&self) -> zbus::Result<Vec<(String, String, String, bool, Option<String>)>>;

    fn delete_provider(&self, profile_id: &str) -> zbus::Result<()>;

    fn get_all_models(
        &self,
        force_refresh: bool,
    ) -> zbus::Result<Vec<(String, String, String, bool, Vec<String>)>>;

    fn set_model_enabled(&self, model_id: &str, enabled: bool) -> zbus::Result<()>;

    // Capability methods
    fn complete(
        &self,
        model_id: &str,
        messages_json: &str,
        tools_json: &str,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
    ) -> zbus::Result<(String, String, Option<(u32, u32, u32)>, Option<String>)>;

    fn stream_start(
        &self,
        model_id: &str,
        messages_json: &str,
        tools_json: &str,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
    ) -> zbus::Result<String>;

    fn stream_poll(&self, stream_id: &str) -> zbus::Result<Vec<String>>;

    fn stream_cancel(&self, stream_id: &str) -> zbus::Result<()>;

    fn embed(
        &self,
        model_id: &str,
        input_json: &str,
    ) -> zbus::Result<(String, Option<(u32, u32, u32)>)>;

    // Routing and metrics
    fn get_routing_rules(&self) -> zbus::Result<Vec<(String, String)>>;

    fn set_routing_rules(&self, rules_json: &str) -> zbus::Result<()>;

    fn get_metrics(&self) -> zbus::Result<Vec<(String, u64, u64, u64, u64)>>;

    // Access control
    fn get_allowlist(&self) -> zbus::Result<Vec<(String, String)>>;

    fn remove_from_allowlist(&self, app_path: &str) -> zbus::Result<()>;
}

pub struct DbusConnection {
    proxy: FireBoxServiceProxy<'static>,
}

impl DbusConnection {
    pub async fn new() -> Result<Self> {
        let conn = Connection::session()
            .await
            .map_err(|e| Error::Ipc(format!("Failed to connect to session bus: {}", e)))?;

        let proxy = FireBoxServiceProxy::new(&conn)
            .await
            .map_err(|e| Error::Ipc(format!("Failed to create proxy: {}", e)))?;

        Ok(Self { proxy })
    }

    fn map_err<T>(result: zbus::Result<T>) -> Result<T> {
        result.map_err(|e| Error::Service(e.to_string()))
    }

    pub async fn ping(&self) -> Result<()> {
        Self::map_err(self.proxy.ping().await)
    }

    pub async fn add_api_key_provider(
        &self,
        name: &str,
        provider_type: &str,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<()> {
        Self::map_err(
            self.proxy
                .add_api_key_provider(name, provider_type, api_key, base_url)
                .await,
        )
    }

    pub async fn add_oauth_provider(
        &self,
        name: &str,
        provider_type: &str,
    ) -> Result<OAuthInitResponse> {
        let (verification_uri, user_code, expires_in) =
            Self::map_err(self.proxy.add_oauth_provider(name, provider_type).await)?;

        Ok(OAuthInitResponse {
            verification_uri,
            user_code,
            expires_in,
        })
    }

    pub async fn complete_oauth(&self, profile_id: &str) -> Result<()> {
        Self::map_err(self.proxy.complete_oauth(profile_id).await)
    }

    pub async fn add_local_provider(&self, name: &str, base_url: &str) -> Result<()> {
        Self::map_err(self.proxy.add_local_provider(name, base_url).await)
    }

    pub async fn list_providers(&self) -> Result<Vec<ProviderInfo>> {
        let providers = Self::map_err(self.proxy.list_providers().await)?;

        Ok(providers
            .into_iter()
            .map(
                |(profile_id, display_name, provider_type, enabled, oauth_status)| ProviderInfo {
                    profile_id,
                    display_name,
                    provider_type,
                    enabled,
                    oauth_status,
                },
            )
            .collect())
    }

    pub async fn delete_provider(&self, profile_id: &str) -> Result<()> {
        Self::map_err(self.proxy.delete_provider(profile_id).await)
    }

    pub async fn get_all_models(&self, force_refresh: bool) -> Result<Vec<ModelInfo>> {
        let models = Self::map_err(self.proxy.get_all_models(force_refresh).await)?;

        Ok(models
            .into_iter()
            .map(
                |(model_id, provider_id, display_name, enabled, capabilities)| ModelInfo {
                    model_id,
                    provider_id,
                    display_name,
                    enabled,
                    capabilities,
                },
            )
            .collect())
    }

    pub async fn set_model_enabled(&self, model_id: &str, enabled: bool) -> Result<()> {
        Self::map_err(self.proxy.set_model_enabled(model_id, enabled).await)
    }

    pub async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let messages_json = serde_json::to_string(&request.messages)?;
        let tools_json = serde_json::to_string(&request.tools)?;

        let (content, tool_calls_json, usage_tuple, finish_reason) = Self::map_err(
            self.proxy
                .complete(
                    &request.model_id,
                    &messages_json,
                    &tools_json,
                    request.temperature,
                    request.max_tokens,
                )
                .await,
        )?;

        let tool_calls: Vec<ToolCall> = serde_json::from_str(&tool_calls_json)?;
        let usage = usage_tuple.map(|(p, c, t)| Usage {
            prompt_tokens: p,
            completion_tokens: c,
            total_tokens: t,
        });

        Ok(CompletionResponse {
            content,
            tool_calls,
            usage,
            finish_reason,
        })
    }

    pub async fn stream_start(&self, request: &CompletionRequest) -> Result<String> {
        let messages_json = serde_json::to_string(&request.messages)?;
        let tools_json = serde_json::to_string(&request.tools)?;

        Self::map_err(
            self.proxy
                .stream_start(
                    &request.model_id,
                    &messages_json,
                    &tools_json,
                    request.temperature,
                    request.max_tokens,
                )
                .await,
        )
    }

    pub async fn stream_poll(&self, stream_id: &str) -> Result<Vec<StreamChunk>> {
        let chunks_json = Self::map_err(self.proxy.stream_poll(stream_id).await)?;

        let mut chunks = Vec::new();
        for json in chunks_json {
            let chunk: StreamChunk = serde_json::from_str(&json)?;
            chunks.push(chunk);
        }

        Ok(chunks)
    }

    pub async fn stream_cancel(&self, stream_id: &str) -> Result<()> {
        Self::map_err(self.proxy.stream_cancel(stream_id).await)
    }

    pub async fn embed(&self, request: &EmbeddingRequest) -> Result<EmbeddingResponse> {
        let input_json = serde_json::to_string(&request.input)?;

        let (embeddings_json, usage_tuple) =
            Self::map_err(self.proxy.embed(&request.model_id, &input_json).await)?;

        let embeddings: Vec<Vec<f64>> = serde_json::from_str(&embeddings_json)?;
        let usage = usage_tuple.map(|(p, c, t)| Usage {
            prompt_tokens: p,
            completion_tokens: c,
            total_tokens: t,
        });

        Ok(EmbeddingResponse { embeddings, usage })
    }

    pub async fn get_routing_rules(&self) -> Result<Vec<RoutingRule>> {
        let rules = Self::map_err(self.proxy.get_routing_rules().await)?;

        Ok(rules
            .into_iter()
            .map(|(pattern, target_provider)| RoutingRule {
                pattern,
                target_provider,
            })
            .collect())
    }

    pub async fn set_routing_rules(&self, rules: &[RoutingRule]) -> Result<()> {
        let rules_json = serde_json::to_string(rules)?;
        Self::map_err(self.proxy.set_routing_rules(&rules_json).await)
    }

    pub async fn get_metrics(&self) -> Result<Vec<ProviderMetrics>> {
        let metrics = Self::map_err(self.proxy.get_metrics().await)?;

        Ok(metrics
            .into_iter()
            .map(
                |(
                    provider_id,
                    requests_count,
                    errors_count,
                    total_prompt_tokens,
                    total_completion_tokens,
                )| {
                    ProviderMetrics {
                        provider_id,
                        requests_count,
                        errors_count,
                        total_prompt_tokens,
                        total_completion_tokens,
                    }
                },
            )
            .collect())
    }

    pub async fn get_provider_metrics(&self) -> Result<Vec<ProviderMetrics>> {
        self.get_metrics().await
    }

    pub async fn get_allowlist(&self) -> Result<Vec<AllowlistEntry>> {
        let entries = Self::map_err(self.proxy.get_allowlist().await)?;

        Ok(entries
            .into_iter()
            .map(|(app_path, display_name)| AllowlistEntry {
                app_path,
                display_name,
            })
            .collect())
    }

    pub async fn remove_from_allowlist(&self, app_path: &str) -> Result<()> {
        Self::map_err(self.proxy.remove_from_allowlist(app_path).await)
    }
}
