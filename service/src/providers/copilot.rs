//! Adapter for the GitHub Copilot API.
//!
//! Uses the OpenAI-compatible protocol with a Copilot-specific endpoint.
//! Authentication is handled through the GitHub OAuth device flow:
//!
//! 1. **Device code** – request a one-time code from GitHub.
//! 2. **User authorisation** – the user enters the code in the browser.
//! 3. **Token exchange** – poll GitHub until the user authorises, then
//!    exchange the GitHub token for a short-lived Copilot API token.
//!
//! Tokens are cached in-memory and persisted via the system keychain.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::middleware::storage;
use crate::providers::{
    BoxStream, CompletionRequest, CompletionResponse, EmbeddingRequest, EmbeddingResponse,
    Provider, RuntimeModelInfo, StreamEvent, ToolCall,
};
use crate::providers::{consts, shared};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
const COPILOT_CHAT_ENDPOINT: &str = "https://api.githubcopilot.com";
const DEFAULT_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const KEYRING_SERVICE: &str = "fire-box-copilot";
const KEYRING_GITHUB_USER: &str = "github-oauth";

/// Version string used in API requests, derived from package version at compile time.
const FIREBOX_VERSION: &str = concat!("fire-box/", env!("CARGO_PKG_VERSION"));

/// Public helper used by the session layer to persist a GitHub OAuth token
fn store_credential(service: &str, user: &str, secret: &str) -> Result<()> {
    storage::set_secret_with_biometric(service, user, secret)
        .map_err(|e| anyhow::anyhow!("failed to store credential: {e}"))
}

fn retrieve_credential(service: &str, user: &str) -> Result<String> {
    let secret = storage::get_secret(service, user)
        .map_err(|e| anyhow::anyhow!("failed to retrieve credential: {e}"))?;
    Ok(secret.to_string())
}

/// Public helper used by the session layer to persist a GitHub OAuth token
/// after the IPC-driven device flow completes.
pub fn store_github_token(token: &str) -> Result<()> {
    store_credential(KEYRING_SERVICE, KEYRING_GITHUB_USER, token)
}

/// Returns `true` if a GitHub OAuth token is present in the keyring.
pub fn has_github_token() -> bool {
    retrieve_credential(KEYRING_SERVICE, KEYRING_GITHUB_USER).is_ok()
}

// ---------------------------------------------------------------------------
// OAuth types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct DeviceCodeRequestBody {
    client_id: String,
    scope: String,
}

/// Response from the GitHub device-code endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCodeResponse {
    /// One-time device code used when polling for a token.
    pub device_code: String,
    /// Short code the user enters in the browser.
    pub user_code: String,
    /// URL the user visits to authorise.
    pub verification_uri: String,
    /// Lifetime of the device code in seconds.
    pub expires_in: u64,
    /// Minimum poll interval in seconds.
    pub interval: u64,
}

#[derive(Debug, Serialize)]
struct TokenPollBody {
    client_id: String,
    device_code: String,
    grant_type: String,
}

#[derive(Debug, Deserialize)]
struct TokenPollResponse {
    access_token: Option<String>,
    #[allow(dead_code)]
    token_type: Option<String>,
    #[allow(dead_code)]
    scope: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct CopilotTokenBody {
    token: String,
    expires_at: i64,
}

/// Cached Copilot API token with its expiry timestamp.
struct CachedToken {
    token: String,
    expires_at: i64,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Adapter for the Copilot API.
///
/// Uses the OpenAI-compatible protocol with a different endpoint.
/// OAuth authentication is required.
pub struct CopilotProvider {
    /// GitHub OAuth access token.
    oauth_token: String,
    /// Copilot chat completions endpoint.
    endpoint: String,
    /// Reusable HTTP client.
    client: reqwest::Client,
    /// Cached Copilot API token (automatically refreshed).
    cached_token: Mutex<Option<CachedToken>>,
}

impl CopilotProvider {
    /// Create a new Copilot provider with an existing OAuth token.
    pub fn new(oauth_token: impl Into<String>) -> Self {
        Self {
            oauth_token: oauth_token.into(),
            endpoint: COPILOT_CHAT_ENDPOINT.to_string(),
            client: shared::build_http_client_full(
                consts::HTTP_TIMEOUT,
                consts::HTTP_CONNECT_TIMEOUT,
                consts::HTTP_POOL_IDLE_TIMEOUT,
                true,
            ),
            cached_token: Mutex::new(None),
        }
    }

    /// Create a new Copilot provider with a custom endpoint.
    pub fn with_endpoint(oauth_token: impl Into<String>, endpoint: String) -> Self {
        Self {
            oauth_token: oauth_token.into(),
            endpoint,
            client: shared::build_http_client_full(
                consts::HTTP_TIMEOUT,
                consts::HTTP_CONNECT_TIMEOUT,
                consts::HTTP_POOL_IDLE_TIMEOUT,
                true,
            ),
            cached_token: Mutex::new(None),
        }
    }

    /// Return the configured endpoint.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Return the configured OAuth token.
    pub fn oauth_token(&self) -> &str {
        &self.oauth_token
    }

    // -----------------------------------------------------------------------
    // OAuth device flow
    // -----------------------------------------------------------------------

    /// Step 1: Request a device code from GitHub.
    ///
    /// Returns the device code, user code, and verification URI.  The caller
    /// should display `user_code` and direct the user to `verification_uri`.
    pub async fn start_device_flow(client_id: Option<&str>) -> Result<DeviceCodeResponse> {
        let client = shared::build_http_client_full(
            consts::HTTP_TIMEOUT,
            consts::HTTP_CONNECT_TIMEOUT,
            consts::HTTP_POOL_IDLE_TIMEOUT,
            true,
        );
        let resp = client
            .post(GITHUB_DEVICE_CODE_URL)
            .header("Accept", "application/json")
            .json(&DeviceCodeRequestBody {
                client_id: client_id.unwrap_or(DEFAULT_CLIENT_ID).to_string(),
                scope: "read:user".to_string(),
            })
            .send()
            .await?;

        let resp = shared::check_status(resp).await?;

        Ok(resp.json().await?)
    }

    /// Step 2: Poll GitHub until the user authorises (or the code expires).
    ///
    /// Returns the GitHub OAuth access token on success.
    pub async fn poll_for_token(
        client_id: Option<&str>,
        device_code: &str,
        interval: u64,
        expires_in: u64,
    ) -> Result<String> {
        let client = shared::build_http_client_full(
            consts::HTTP_TIMEOUT,
            consts::HTTP_CONNECT_TIMEOUT,
            consts::HTTP_POOL_IDLE_TIMEOUT,
            true,
        );
        let cid = client_id.unwrap_or(DEFAULT_CLIENT_ID);
        let mut delay = std::time::Duration::from_secs(interval);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(expires_in);

        loop {
            if tokio::time::Instant::now() >= deadline {
                bail!("device code expired before user authorised");
            }

            tokio::time::sleep(delay).await;

            let resp = client
                .post(GITHUB_TOKEN_URL)
                .header("Accept", "application/json")
                .json(&TokenPollBody {
                    client_id: cid.to_string(),
                    device_code: device_code.to_string(),
                    grant_type: "urn:ietf:params:oauth:grant-type:device_code".to_string(),
                })
                .send()
                .await?;

            let poll: TokenPollResponse = resp.json().await?;

            if let Some(token) = poll.access_token {
                return Ok(token);
            }

            match poll.error.as_deref() {
                Some("authorization_pending") => continue,
                Some("slow_down") => {
                    delay += std::time::Duration::from_secs(
                        poll.interval
                            .unwrap_or(consts::OAUTH_SLOW_DOWN_INCREMENT_SECS),
                    );
                    continue;
                }
                Some("expired_token") => bail!("device code expired"),
                Some("access_denied") => bail!("user denied authorisation"),
                Some(other) => bail!(
                    "OAuth error: {other}: {}",
                    poll.error_description.unwrap_or_default()
                ),
                None => bail!("unexpected OAuth response: no token and no error"),
            }
        }
    }

    /// Step 3: Exchange a GitHub OAuth token for a short-lived Copilot API token.
    pub async fn exchange_copilot_token(github_token: &str) -> Result<(String, i64)> {
        let client = shared::build_http_client_full(
            consts::HTTP_TIMEOUT,
            consts::HTTP_CONNECT_TIMEOUT,
            consts::HTTP_POOL_IDLE_TIMEOUT,
            true,
        );
        let resp = shared::check_status(
            client
                .get(COPILOT_TOKEN_URL)
                .header("Authorization", format!("token {github_token}"))
                .header("Accept", "application/json")
                .header("Editor-Version", FIREBOX_VERSION)
                .header("Editor-Plugin-Version", FIREBOX_VERSION)
                .send()
                .await?,
        )
        .await?;

        let body: CopilotTokenBody = resp.json().await?;
        Ok((body.token, body.expires_at))
    }

    /// Start a GitHub device OAuth flow and return the device code response.
    ///
    /// The caller (typically `interfaces/session.rs`) should wrap the
    /// returned [`DeviceCodeResponse`] into an `OAuthDeviceChallenge` and
    /// push it to the IPC client, then call [`Self::poll_for_token`] +
    /// [`Self::exchange_copilot_token`] to complete authentication.
    pub async fn start_oauth_device_flow(client_id: Option<&str>) -> Result<DeviceCodeResponse> {
        Self::start_device_flow(client_id).await
    }

    /// Run the full device-flow authentication and return an authenticated
    /// provider.
    ///
    /// Performs all three steps (device code → user authorisation → token
    /// exchange) and stores the GitHub token in the system keychain.
    pub async fn authenticate(client_id: Option<&str>) -> Result<Self> {
        let device = Self::start_device_flow(client_id).await?;

        tracing::info!("Go to:  {}", device.verification_uri);
        tracing::info!("Enter:  {}", device.user_code);

        let github_token = Self::poll_for_token(
            client_id,
            &device.device_code,
            device.interval,
            device.expires_in,
        )
        .await?;

        if let Err(e) = store_credential(KEYRING_SERVICE, KEYRING_GITHUB_USER, &github_token) {
            tracing::warn!(
                "Failed to store GitHub token in keyring: {}",
                e.root_cause()
            );
        }

        let (copilot_token, expires_at) = Self::exchange_copilot_token(&github_token).await?;

        let provider = Self::new(github_token);
        *provider.cached_token.lock().await = Some(CachedToken {
            token: copilot_token,
            expires_at,
        });
        Ok(provider)
    }

    /// Persist the OAuth token in the OS keyring.
    ///
    /// Equivalent to the internal `store_credential` call made by
    /// [`Self::authenticate`], but callable on an already-constructed provider.
    pub fn save_to_keyring(&self) -> Result<()> {
        store_credential(KEYRING_SERVICE, KEYRING_GITHUB_USER, &self.oauth_token)
    }

    /// Try to restore a previously-stored OAuth token from the system keychain.
    pub fn from_keyring() -> Result<Self> {
        let token = retrieve_credential(KEYRING_SERVICE, KEYRING_GITHUB_USER)?;
        Ok(Self::new(token))
    }

    /// Ensure a valid Copilot API token is cached, refreshing if necessary.
    /// Uses async lock to prevent race conditions during token refresh.
    async fn ensure_copilot_token(&self) -> Result<String> {
        let mut guard = self.cached_token.lock().await;

        // Check if we have a valid cached token
        if let Some(cached) = guard.as_ref() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            if cached.expires_at > now + consts::OAUTH_TOKEN_EXPIRY_BUFFER_SECS as i64 {
                return Ok(cached.token.clone());
            }
        }

        // Token missing or about to expire — refresh while holding the lock
        let (token, expires_at) = Self::exchange_copilot_token(&self.oauth_token).await?;
        let cloned = token.clone();
        *guard = Some(CachedToken { token, expires_at });
        Ok(cloned)
    }
}

// ---------------------------------------------------------------------------
// Provider trait
// ---------------------------------------------------------------------------

impl Provider for CopilotProvider {
    async fn complete(
        &self,
        _session_id: &str,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse> {
        let token = self.ensure_copilot_token().await?;

        let url = format!("{}/chat/completions", self.endpoint);
        let resp = shared::check_status(
            self.client
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .header("Editor-Version", FIREBOX_VERSION)
                .header("Copilot-Integration-Id", "fire-box")
                .json(request)
                .send()
                .await?,
        )
        .await?;

        Ok(resp.json().await?)
    }

    async fn complete_stream(
        &self,
        _session_id: &str,
        request: &CompletionRequest,
    ) -> Result<BoxStream<Result<StreamEvent>>> {
        use futures_util::stream;

        let token = self.ensure_copilot_token().await?;

        let url = format!("{}/chat/completions", self.endpoint);
        let body = serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(shared::message_to_json).collect::<Vec<_>>(),
            "stream": true,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "tools": request
                .tools
                .as_ref()
                .map(|ts| ts.iter().map(shared::tool_to_json).collect::<Vec<_>>()),
        });

        let response = shared::check_status(
            self.client
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .header("Editor-Version", FIREBOX_VERSION)
                .header("Copilot-Integration-Id", "fire-box")
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
        _request: &EmbeddingRequest,
    ) -> Result<EmbeddingResponse> {
        Err(crate::providers::ProviderError::RequestFailed(
            "Copilot provider does not support embeddings".to_string(),
        )
        .into())
    }

    async fn list_models(&self) -> Result<Vec<RuntimeModelInfo>> {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct ModelList {
            data: Vec<ModelInfo>,
        }

        #[derive(Deserialize)]
        struct ModelInfo {
            id: String,
            created: Option<u64>,
            owned_by: Option<String>,
        }

        // Copilot uses the OpenAI-compatible API at githubcopilot.com
        let token = self.ensure_copilot_token().await?;

        let response = shared::check_status(
            self.client
                .get(format!("{}/v1/models", self.endpoint))
                .header("Authorization", format!("Bearer {token}"))
                .header("Editor-Version", FIREBOX_VERSION)
                .header("Copilot-Integration-Id", "fire-box")
                .send()
                .await?,
        )
        .await?;

        let model_list: ModelList = response.json().await?;
        Ok(model_list
            .data
            .into_iter()
            .map(|m| RuntimeModelInfo {
                id: m.id,
                owner: m.owned_by.unwrap_or_else(|| "github".to_string()),
                created: m.created,
                context_window: None,
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_with_default_endpoint() {
        let p = CopilotProvider::new("oauth-token".to_string());
        assert_eq!(p.endpoint(), COPILOT_CHAT_ENDPOINT);
        assert_eq!(p.oauth_token(), "oauth-token");
    }

    #[test]
    fn create_with_custom_endpoint() {
        let p = CopilotProvider::with_endpoint(
            "oauth-token".to_string(),
            "http://localhost:7070".to_string(),
        );
        assert_eq!(p.endpoint(), "http://localhost:7070");
    }

    #[tokio::test]
    async fn complete_without_valid_token_fails() {
        let p = CopilotProvider::new("invalid-token".to_string());
        let req = CompletionRequest {
            model: "copilot-chat".to_string(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stream: false,
            tools: None,
        };
        assert!(p.complete("test-session", &req).await.is_err());
    }

    #[test]
    fn constants_are_sensible() {
        assert!(GITHUB_DEVICE_CODE_URL.starts_with("https://"));
        assert!(GITHUB_TOKEN_URL.starts_with("https://"));
        assert!(COPILOT_TOKEN_URL.starts_with("https://"));
        assert!(!DEFAULT_CLIENT_ID.is_empty());
    }

    #[tokio::test]
    async fn cached_token_starts_empty() {
        let p = CopilotProvider::new("tok".to_string());
        let guard = p.cached_token.lock().await;
        assert!(guard.is_none());
    }
}
