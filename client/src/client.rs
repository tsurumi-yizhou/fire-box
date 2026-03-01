//! FireBox client SDK.

use crate::error::Result;
use crate::types::*;

#[cfg(target_os = "macos")]
use crate::xpc::XpcConnection;

#[cfg(target_os = "linux")]
use crate::dbus::DbusConnection;

#[cfg(target_os = "windows")]
use crate::com::ComConnection;

/// FireBox client for interacting with the FireBox service.
pub struct FireBoxClient {
    #[cfg(target_os = "macos")]
    conn: XpcConnection,

    #[cfg(target_os = "linux")]
    conn: DbusConnection,

    #[cfg(target_os = "windows")]
    conn: ComConnection,
}

// Platform-specific implementations
#[cfg(target_os = "macos")]
impl FireBoxClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            conn: XpcConnection::new()?,
        })
    }

    pub fn ping(&self) -> Result<()> {
        self.conn.ping()
    }

    pub fn add_api_key_provider(
        &self,
        name: &str,
        provider_type: &str,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<()> {
        self.conn
            .add_api_key_provider(name, provider_type, api_key, base_url)
    }

    pub fn add_oauth_provider(&self, name: &str, provider_type: &str) -> Result<OAuthInitResponse> {
        self.conn.add_oauth_provider(name, provider_type)
    }

    pub fn complete_oauth(&self, profile_id: &str) -> Result<()> {
        self.conn.complete_oauth(profile_id)
    }

    pub fn add_local_provider(&self, name: &str, base_url: &str) -> Result<()> {
        self.conn.add_local_provider(name, base_url)
    }

    pub fn list_providers(&self) -> Result<Vec<ProviderInfo>> {
        self.conn.list_providers()
    }

    pub fn delete_provider(&self, profile_id: &str) -> Result<()> {
        self.conn.delete_provider(profile_id)
    }

    pub fn get_all_models(&self, force_refresh: bool) -> Result<Vec<ModelInfo>> {
        self.conn.get_all_models(force_refresh)
    }

    pub fn set_model_enabled(&self, model_id: &str, enabled: bool) -> Result<()> {
        self.conn.set_model_enabled(model_id, enabled)
    }

    pub fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.conn.complete(request)
    }

    pub fn stream_start(&self, request: &CompletionRequest) -> Result<String> {
        self.conn.stream_start(request)
    }

    pub fn stream_poll(&self, stream_id: &str) -> Result<Vec<StreamChunk>> {
        self.conn.stream_poll(stream_id)
    }

    pub fn stream_cancel(&self, stream_id: &str) -> Result<()> {
        self.conn.stream_cancel(stream_id)
    }

    pub fn embed(&self, request: &EmbeddingRequest) -> Result<EmbeddingResponse> {
        self.conn.embed(request)
    }

    pub fn get_routing_rules(&self) -> Result<Vec<RoutingRule>> {
        self.conn.get_routing_rules()
    }

    pub fn set_routing_rules(&self, rules: &[RoutingRule]) -> Result<()> {
        self.conn.set_routing_rules(rules)
    }

    pub fn get_provider_metrics(&self) -> Result<Vec<ProviderMetrics>> {
        self.conn.get_provider_metrics()
    }

    pub fn get_allowlist(&self) -> Result<Vec<AllowlistEntry>> {
        self.conn.get_allowlist()
    }

    pub fn remove_from_allowlist(&self, app_path: &str) -> Result<()> {
        self.conn.remove_from_allowlist(app_path)
    }
}

#[cfg(target_os = "linux")]
impl FireBoxClient {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            conn: DbusConnection::new().await?,
        })
    }

    pub async fn ping(&self) -> Result<()> {
        self.conn.ping().await
    }

    pub async fn add_api_key_provider(
        &self,
        name: &str,
        provider_type: &str,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<()> {
        self.conn
            .add_api_key_provider(name, provider_type, api_key, base_url)
            .await
    }

    pub async fn add_oauth_provider(
        &self,
        name: &str,
        provider_type: &str,
    ) -> Result<OAuthInitResponse> {
        self.conn.add_oauth_provider(name, provider_type).await
    }

    pub async fn complete_oauth(&self, profile_id: &str) -> Result<()> {
        self.conn.complete_oauth(profile_id).await
    }

    pub async fn add_local_provider(&self, name: &str, base_url: &str) -> Result<()> {
        self.conn.add_local_provider(name, base_url).await
    }

    pub async fn list_providers(&self) -> Result<Vec<ProviderInfo>> {
        self.conn.list_providers().await
    }

    pub async fn delete_provider(&self, profile_id: &str) -> Result<()> {
        self.conn.delete_provider(profile_id).await
    }

    pub async fn get_all_models(&self, force_refresh: bool) -> Result<Vec<ModelInfo>> {
        self.conn.get_all_models(force_refresh).await
    }

    pub async fn set_model_enabled(&self, model_id: &str, enabled: bool) -> Result<()> {
        self.conn.set_model_enabled(model_id, enabled).await
    }

    pub async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.conn.complete(request).await
    }

    pub async fn stream_start(&self, request: &CompletionRequest) -> Result<String> {
        self.conn.stream_start(request).await
    }

    pub async fn stream_poll(&self, stream_id: &str) -> Result<Vec<StreamChunk>> {
        self.conn.stream_poll(stream_id).await
    }

    pub async fn stream_cancel(&self, stream_id: &str) -> Result<()> {
        self.conn.stream_cancel(stream_id).await
    }

    pub async fn embed(&self, request: &EmbeddingRequest) -> Result<EmbeddingResponse> {
        self.conn.embed(request).await
    }

    pub async fn get_routing_rules(&self) -> Result<Vec<RoutingRule>> {
        self.conn.get_routing_rules().await
    }

    pub async fn set_routing_rules(&self, rules: &[RoutingRule]) -> Result<()> {
        self.conn.set_routing_rules(rules).await
    }

    pub async fn get_provider_metrics(&self) -> Result<Vec<ProviderMetrics>> {
        self.conn.get_provider_metrics().await
    }

    pub async fn get_allowlist(&self) -> Result<Vec<AllowlistEntry>> {
        self.conn.get_allowlist().await
    }

    pub async fn remove_from_allowlist(&self, app_path: &str) -> Result<()> {
        self.conn.remove_from_allowlist(app_path).await
    }
}

#[cfg(target_os = "windows")]
impl FireBoxClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            conn: ComConnection::new()?,
        })
    }

    pub fn ping(&self) -> Result<()> {
        self.conn.ping()
    }

    pub fn add_api_key_provider(
        &self,
        name: &str,
        provider_type: &str,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<()> {
        self.conn
            .add_api_key_provider(name, provider_type, api_key, base_url)
    }

    pub fn add_oauth_provider(&self, name: &str, provider_type: &str) -> Result<OAuthInitResponse> {
        self.conn.add_oauth_provider(name, provider_type)
    }

    pub fn complete_oauth(&self, profile_id: &str) -> Result<()> {
        self.conn.complete_oauth(profile_id)
    }

    pub fn add_local_provider(&self, name: &str, base_url: &str) -> Result<()> {
        self.conn.add_local_provider(name, base_url)
    }

    pub fn list_providers(&self) -> Result<Vec<ProviderInfo>> {
        self.conn.list_providers()
    }

    pub fn delete_provider(&self, profile_id: &str) -> Result<()> {
        self.conn.delete_provider(profile_id)
    }

    pub fn get_all_models(&self, force_refresh: bool) -> Result<Vec<ModelInfo>> {
        self.conn.get_all_models(force_refresh)
    }

    pub fn set_model_enabled(&self, model_id: &str, enabled: bool) -> Result<()> {
        self.conn.set_model_enabled(model_id, enabled)
    }

    pub fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.conn.complete(request)
    }

    pub fn stream_start(&self, request: &CompletionRequest) -> Result<String> {
        self.conn.stream_start(request)
    }

    pub fn stream_poll(&self, stream_id: &str) -> Result<Vec<StreamChunk>> {
        self.conn.stream_poll(stream_id)
    }

    pub fn stream_cancel(&self, stream_id: &str) -> Result<()> {
        self.conn.stream_cancel(stream_id)
    }

    pub fn embed(&self, request: &EmbeddingRequest) -> Result<EmbeddingResponse> {
        self.conn.embed(request)
    }

    pub fn get_routing_rules(&self) -> Result<Vec<RoutingRule>> {
        self.conn.get_routing_rules()
    }

    pub fn set_routing_rules(&self, rules: &[RoutingRule]) -> Result<()> {
        self.conn.set_routing_rules(rules)
    }

    pub fn get_provider_metrics(&self) -> Result<Vec<ProviderMetrics>> {
        self.conn.get_provider_metrics()
    }

    pub fn get_allowlist(&self) -> Result<Vec<AllowlistEntry>> {
        self.conn.get_allowlist()
    }

    pub fn remove_from_allowlist(&self, app_path: &str) -> Result<()> {
        self.conn.remove_from_allowlist(app_path)
    }
}
