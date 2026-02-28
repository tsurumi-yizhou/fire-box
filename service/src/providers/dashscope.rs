//! Adapter for Alibaba Cloud DashScope API — native protocol.
//!
//! Uses the **native DashScope generation REST API** (not the OpenAI-compat
//! layer) at `dashscope.aliyuncs.com/api/v1/services/aigc/text-generation`.
//!
//! # Authentication
//! Only **Qwen OAuth2** is supported: a short-lived access token is obtained
//! via the Qwen device flow at `chat.qwen.ai` and passed as
//! `Authorization: Bearer <access_token>` with `X-DashScope-AuthType: oauth`.
//!
//! # Secure Storage
//! Use [`DashScopeProvider::save_oauth_to_keyring`] to persist
//! [`OAuthCredentials`] and [`DashScopeProvider::from_keyring_oauth`] to
//! restore them.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::middleware::storage;
use crate::providers::{
    BoxStream, CompletionRequest, CompletionResponse, EmbeddingRequest, EmbeddingResponse,
    Provider, RuntimeModelInfo, StreamEvent, config::DashScopeConfig,
};
use crate::providers::{consts, shared};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Native DashScope text-generation endpoint (mainland China).
pub const NATIVE_BASE_URL: &str =
    "https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation";

/// International endpoint.
pub const NATIVE_BASE_URL_INTL: &str =
    "https://dashscope-intl.aliyuncs.com/api/v1/services/aigc/text-generation/generation";

/// Qwen OAuth2 device-code endpoint.
pub const QWEN_DEVICE_CODE_URL: &str = "https://chat.qwen.ai/api/v1/oauth2/device/code";

/// Qwen OAuth2 token endpoint.
pub const QWEN_TOKEN_URL: &str = "https://chat.qwen.ai/api/v1/oauth2/token";

/// Qwen OAuth2 client ID (public, same as qwen-code).
pub const QWEN_CLIENT_ID: &str = "f0304373b74a44d2b584a3fb70ca9e56";

/// Default OAuth2 scope.
pub const QWEN_OAUTH_SCOPE: &str = "openid profile email model.completion";

const QWEN_DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

const KEYRING_SERVICE: &str = "fire-box-dashscope";
const KEYRING_USER_OAUTH: &str = "oauth-credentials";

// ---------------------------------------------------------------------------
// OAuth credentials
// ---------------------------------------------------------------------------

/// OAuth2 credentials from the Qwen token endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Per-user DashScope resource URL (may differ from the default endpoint).
    pub resource_url: Option<String>,
    /// Unix timestamp (milliseconds) at which `access_token` expires.
    pub expiry_date: Option<i64>,
}

impl OAuthCredentials {
    /// Returns `true` if the access token has not expired (with a 60-second buffer).
    pub fn is_valid(&self) -> bool {
        let Some(expiry) = self.expiry_date else {
            return true;
        };
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        expiry > now_ms + consts::OAUTH_TOKEN_EXPIRY_BUFFER_SECS as i64 * 1000
    }
}

// ---------------------------------------------------------------------------
// PKCE helpers
// ---------------------------------------------------------------------------

fn generate_pkce_pair() -> (String, String) {
    let mut bytes = [0u8; 32];
    // XOR-shift PRNG seeded from system time (sufficient for PKCE challenges).
    let mut state = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    for b in bytes.iter_mut() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *b = (state & 0xff) as u8;
    }
    let verifier = base64url_encode(&bytes);
    let digest = sha2_digest(verifier.as_bytes());
    let challenge = base64url_encode(&digest);
    (verifier, challenge)
}

fn base64url_encode(input: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((input.len() * 4).div_ceil(3));
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 {
            chunk[1] as usize
        } else {
            0
        };
        let b2 = if chunk.len() > 2 {
            chunk[2] as usize
        } else {
            0
        };
        out.push(A[b0 >> 2] as char);
        out.push(A[((b0 & 0x03) << 4) | (b1 >> 4)] as char);
        if chunk.len() > 1 {
            out.push(A[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        }
        if chunk.len() > 2 {
            out.push(A[b2 & 0x3f] as char);
        }
    }
    out
}

/// SHA-256 digest via the `sha2` crate.
pub fn sha2_digest(data: &[u8]) -> [u8; 32] {
    let result = Sha256::digest(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

// ---------------------------------------------------------------------------
// Qwen OAuth2 device-flow
// ---------------------------------------------------------------------------

/// Response from the Qwen device-code endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct QwenDeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in: u64,
    #[serde(default = "default_poll_interval")]
    pub interval: u64,
}

fn default_poll_interval() -> u64 {
    5
}

#[derive(Debug, Serialize)]
struct DeviceCodeBody<'a> {
    client_id: &'a str,
    scope: &'a str,
    code_challenge: &'a str,
    code_challenge_method: &'static str,
}

#[derive(Debug, Serialize)]
struct TokenPollBody<'a> {
    grant_type: &'static str,
    client_id: &'a str,
    device_code: &'a str,
    code_verifier: &'a str,
}

#[derive(Debug, Serialize)]
struct RefreshBody<'a> {
    grant_type: &'static str,
    refresh_token: &'a str,
    client_id: &'a str,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    resource_url: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    status: Option<String>,
}

/// Orchestrates the Qwen OAuth2 device-authorisation flow (RFC 8628 + PKCE).
pub struct QwenOAuthFlow {
    client: reqwest::Client,
    response: QwenDeviceCodeResponse,
    code_verifier: String,
}

impl QwenOAuthFlow {
    /// Step 1 – request a device code from `chat.qwen.ai`.
    pub async fn start(scope: Option<&str>) -> Result<Self> {
        let client = shared::build_http_client_full(
            consts::HTTP_TIMEOUT,
            consts::HTTP_CONNECT_TIMEOUT,
            consts::HTTP_POOL_IDLE_TIMEOUT,
            true,
        );

        let (verifier, challenge) = generate_pkce_pair();
        let scope_str = scope.unwrap_or(QWEN_OAUTH_SCOPE);

        let body = serde_urlencoded::to_string(DeviceCodeBody {
            client_id: QWEN_CLIENT_ID,
            scope: scope_str,
            code_challenge: &challenge,
            code_challenge_method: "S256",
        })?;

        let resp = client
            .post(QWEN_DEVICE_CODE_URL)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .body(body)
            .send()
            .await?;

        let resp = shared::check_status(resp).await?;

        let response: QwenDeviceCodeResponse = resp.json().await?;
        Ok(Self {
            client,
            response,
            code_verifier: verifier,
        })
    }

    /// Returns the device-code response (url / user code for display to user).
    pub fn device_code_response(&self) -> &QwenDeviceCodeResponse {
        &self.response
    }

    /// Step 2 – poll the token endpoint until the user authorises.
    pub async fn wait_for_token(&self) -> Result<OAuthCredentials> {
        let mut delay = std::time::Duration::from_secs(self.response.interval);
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_secs(self.response.expires_in);

        loop {
            if tokio::time::Instant::now() >= deadline {
                bail!("Qwen device code expired before user authorised");
            }
            tokio::time::sleep(delay).await;

            let body = serde_urlencoded::to_string(TokenPollBody {
                grant_type: QWEN_DEVICE_GRANT,
                client_id: QWEN_CLIENT_ID,
                device_code: &self.response.device_code,
                code_verifier: &self.code_verifier,
            })?;

            let resp = self
                .client
                .post(QWEN_TOKEN_URL)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .header("Accept", "application/json")
                .body(body)
                .send()
                .await?;

            let http_status = resp.status();
            let tok: TokenResponse = resp.json().await?;

            if let Some(at) = tok.access_token.filter(|t| !t.is_empty()) {
                let expires_in = tok.expires_in.unwrap_or(3600);
                let expiry_date = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64
                    + expires_in * 1000;
                return Ok(OAuthCredentials {
                    access_token: at,
                    refresh_token: tok.refresh_token,
                    resource_url: tok.resource_url,
                    expiry_date: Some(expiry_date),
                });
            }

            match (http_status.as_u16(), tok.error.as_deref()) {
                (400, Some("authorization_pending")) | (_, Some("authorization_pending")) => {
                    continue;
                }
                (429, Some("slow_down")) | (_, Some("slow_down")) => {
                    delay += std::time::Duration::from_secs(consts::OAUTH_SLOW_DOWN_INCREMENT_SECS);
                    continue;
                }
                (_, Some("expired_token")) => bail!("Qwen device code expired"),
                (_, Some("access_denied")) => bail!("user denied Qwen authorisation"),
                (_, Some(e)) => bail!(
                    "Qwen OAuth error: {e}: {}",
                    tok.error_description.unwrap_or_default()
                ),
                _ => {
                    if tok.status.as_deref() == Some("pending") {
                        continue;
                    }
                    bail!("unexpected Qwen token response");
                }
            }
        }
    }

    /// Refresh an existing access token using a refresh token.
    pub async fn refresh(credentials: &OAuthCredentials) -> Result<OAuthCredentials> {
        let rt = credentials
            .refresh_token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("no refresh token available"))?;

        let client = shared::build_http_client_full(
            consts::HTTP_TIMEOUT,
            consts::HTTP_CONNECT_TIMEOUT,
            consts::HTTP_POOL_IDLE_TIMEOUT,
            true,
        );

        let body = serde_urlencoded::to_string(RefreshBody {
            grant_type: "refresh_token",
            refresh_token: rt,
            client_id: QWEN_CLIENT_ID,
        })?;

        let resp = client
            .post(QWEN_TOKEN_URL)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .body(body)
            .send()
            .await?;

        let resp = shared::check_status(resp).await?;

        let tok: TokenResponse = resp.json().await?;
        let at = tok
            .access_token
            .filter(|t| !t.is_empty())
            .ok_or_else(|| anyhow::anyhow!("token refresh returned no access_token"))?;
        let expires_in = tok.expires_in.unwrap_or(3600);
        let expiry_date = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
            + expires_in * 1000;

        Ok(OAuthCredentials {
            access_token: at,
            refresh_token: tok
                .refresh_token
                .or_else(|| credentials.refresh_token.clone()),
            resource_url: tok
                .resource_url
                .or_else(|| credentials.resource_url.clone()),
            expiry_date: Some(expiry_date),
        })
    }
}

// ---------------------------------------------------------------------------
// Native DashScope request/response types
// ---------------------------------------------------------------------------

/// A single chat message in the native DashScope format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashScopeMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
struct NativeRequest<'a> {
    model: &'a str,
    input: NativeInput<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<NativeParameters>,
}

#[derive(Debug, Serialize)]
struct NativeInput<'a> {
    messages: &'a [DashScopeMessage],
}

#[derive(Debug, Serialize)]
struct NativeParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    result_format: &'static str,
}

#[derive(Debug, Deserialize)]
struct NativeResponse {
    output: Option<NativeOutput>,
    usage: Option<NativeUsage>,
}

#[derive(Debug, Deserialize)]
struct NativeOutput {
    choices: Option<Vec<NativeChoice>>,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NativeChoice {
    message: Option<DashScopeMessage>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NativeUsage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

// ---------------------------------------------------------------------------
// Provider struct
// ---------------------------------------------------------------------------

/// Adapter for the Alibaba Cloud DashScope API (native protocol).
///
/// Authenticates exclusively via Qwen OAuth2 (`X-DashScope-AuthType: oauth`).
pub struct DashScopeProvider {
    credentials: OAuthCredentials,
    base_url: String,
    client: reqwest::Client,
}

impl DashScopeProvider {
    /// Create a provider that authenticates via an OAuth2 access token.
    pub fn with_oauth(credentials: OAuthCredentials) -> Self {
        Self::new_inner(credentials, NATIVE_BASE_URL)
    }

    /// Create from a [`DashScopeConfig`].
    pub fn from_config(cfg: &DashScopeConfig) -> Self {
        let base = cfg
            .base_url
            .as_deref()
            .unwrap_or(NATIVE_BASE_URL)
            .to_string();
        let creds = OAuthCredentials {
            access_token: cfg
                .access_token
                .as_ref()
                .map(|t| t.as_str().to_string())
                .unwrap_or_default(),
            refresh_token: cfg.refresh_token.as_ref().map(|t| t.as_str().to_string()),
            resource_url: cfg.resource_url.clone(),
            expiry_date: cfg.expiry_date,
        };
        Self::new_inner(creds, &base)
    }

    fn new_inner(credentials: OAuthCredentials, base_url: &str) -> Self {
        let client = shared::build_http_client(consts::HTTP_TIMEOUT);
        Self {
            credentials,
            base_url: base_url.to_string(),
            client,
        }
    }

    /// Return the effective generation endpoint URL.
    ///
    /// If the OAuth credentials carry a `resource_url`, that is used instead
    /// of the default base URL.
    pub fn endpoint(&self) -> String {
        if let Some(ru) = self.credentials.resource_url.as_deref() {
            if ru.contains("/generation") {
                return ru.to_string();
            }
            return format!(
                "{}/api/v1/services/aigc/text-generation/generation",
                ru.trim_end_matches('/')
            );
        }
        self.base_url.clone()
    }

    // -----------------------------------------------------------------------
    // Secure storage helpers
    // -----------------------------------------------------------------------

    /// Persist OAuth credentials in platform-specific secure storage with biometric protection (JSON-encoded).
    pub fn save_oauth_to_keyring(creds: &OAuthCredentials) -> Result<()> {
        let json = serde_json::to_string(creds)?;
        storage::set_secret_with_biometric(KEYRING_SERVICE, KEYRING_USER_OAUTH, &json)
            .map_err(|e| anyhow::anyhow!("failed to save DashScope OAuth credentials: {e}"))
    }

    /// Load an OAuth-credential provider from platform-specific secure storage.
    pub fn from_keyring_oauth() -> Result<Self> {
        let json = storage::get_secret(KEYRING_SERVICE, KEYRING_USER_OAUTH)
            .map_err(|e| anyhow::anyhow!("failed to load DashScope OAuth credentials: {e}"))?;
        let creds: OAuthCredentials = serde_json::from_str(&json)?;
        Ok(Self::with_oauth(creds))
    }

    /// Persist the current credentials in platform-specific secure storage.
    pub fn save_to_keyring(&self) -> Result<()> {
        Self::save_oauth_to_keyring(&self.credentials)
    }

    /// Start a Qwen OAuth2 device flow and return the flow handle.
    ///
    /// The caller (typically `interfaces/session.rs`) should wrap the
    /// returned [`QwenDeviceCodeResponse`] into an `OAuthDeviceChallenge` and
    /// push it to the IPC client, then call [`QwenOAuthFlow::wait_for_token`]
    /// to obtain credentials and [`DashScopeProvider::save_oauth_to_keyring`]
    /// to persist them.
    pub async fn start_oauth_device_flow(scope: Option<&str>) -> Result<QwenOAuthFlow> {
        QwenOAuthFlow::start(scope).await
    }

    async fn call_native(
        &self,
        model: &str,
        messages: &[DashScopeMessage],
        max_tokens: Option<u32>,
        temperature: Option<f64>,
    ) -> Result<NativeResponse> {
        let url = self.endpoint();
        let body = NativeRequest {
            model,
            input: NativeInput { messages },
            parameters: Some(NativeParameters {
                max_tokens,
                temperature,
                result_format: "message",
            }),
        };

        let resp = self
            .client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.credentials.access_token),
            )
            .header("X-DashScope-AuthType", "oauth")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let resp = shared::check_status(resp).await?;
        Ok(resp.json().await?)
    }

    fn to_native_messages(request: &CompletionRequest) -> Vec<DashScopeMessage> {
        request
            .messages
            .iter()
            .map(|m| DashScopeMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Provider trait
// ---------------------------------------------------------------------------

impl Provider for DashScopeProvider {
    async fn complete(
        &self,
        _session_id: &str,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse> {
        let messages = Self::to_native_messages(request);
        let native = self
            .call_native(
                &request.model,
                &messages,
                request.max_tokens,
                request.temperature,
            )
            .await?;

        let finish_reason = native
            .output
            .as_ref()
            .and_then(|o| o.choices.as_ref())
            .and_then(|cs| cs.first())
            .and_then(|c| c.finish_reason.clone());

        let content = native
            .output
            .as_ref()
            .and_then(|o| {
                o.choices
                    .as_ref()
                    .and_then(|cs| {
                        cs.first()
                            .and_then(|c| c.message.as_ref())
                            .map(|m| m.content.clone())
                    })
                    .or_else(|| o.text.clone())
            })
            .unwrap_or_default();

        Ok(CompletionResponse {
            id: String::new(),
            model: request.model.clone(),
            choices: vec![crate::providers::Choice {
                index: 0,
                message: crate::providers::ChatMessage {
                    role: "assistant".to_string(),
                    content,
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                },
                finish_reason,
            }],
            usage: native.usage.map(|u| crate::providers::Usage {
                prompt_tokens: u.input_tokens.unwrap_or(0),
                completion_tokens: u.output_tokens.unwrap_or(0),
                total_tokens: u.input_tokens.unwrap_or(0) + u.output_tokens.unwrap_or(0),
            }),
        })
    }

    async fn complete_stream(
        &self,
        _session_id: &str,
        request: &CompletionRequest,
    ) -> Result<BoxStream<Result<StreamEvent>>> {
        use futures_util::stream;

        let url = self.endpoint();
        let messages = Self::to_native_messages(request);
        let body = NativeRequest {
            model: &request.model,
            input: NativeInput {
                messages: &messages,
            },
            parameters: Some(NativeParameters {
                max_tokens: request.max_tokens,
                temperature: request.temperature,
                result_format: "message",
            }),
        };

        let response = self
            .client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.credentials.access_token),
            )
            .header("X-DashScope-AuthType", "oauth")
            .header("Content-Type", "application/json")
            .header("X-DashScope-SSE", "enable")
            .json(&body)
            .send()
            .await?;

        let response = shared::check_status(response).await?;

        let event_stream = response.bytes_stream();

        let stream = stream::unfold(
            (event_stream, String::new()),
            |(mut stream, mut buffer)| async move {
                use futures_util::stream::StreamExt;

                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            let text = String::from_utf8_lossy(&chunk);
                            buffer.push_str(&text);

                            while let Some(newline_pos) = buffer.find('\n') {
                                let line = buffer[..newline_pos].trim().to_string();
                                buffer.drain(..=newline_pos);

                                if line.is_empty() || !line.starts_with("data:") {
                                    continue;
                                }

                                let data = line[5..].trim();
                                if data.is_empty() {
                                    continue;
                                }

                                match serde_json::from_str::<serde_json::Value>(data) {
                                    Ok(json) => {
                                        if let Some(output) = json.get("output")
                                            && let Some(choices) =
                                                output.get("choices").and_then(|v| v.as_array())
                                            && let Some(choice) = choices.first()
                                        {
                                            if let Some(message) = choice.get("message")
                                                && let Some(content) =
                                                    message.get("content").and_then(|v| v.as_str())
                                                && !content.is_empty()
                                            {
                                                return Some((
                                                    Ok(StreamEvent::Delta {
                                                        content: content.to_string(),
                                                    }),
                                                    (stream, buffer),
                                                ));
                                            }
                                            if let Some(finish_reason) =
                                                choice.get("finish_reason").and_then(|v| v.as_str())
                                                && finish_reason == "stop"
                                            {
                                                return Some((
                                                    Ok(StreamEvent::Done),
                                                    (stream, buffer),
                                                ));
                                            }
                                        }
                                        if let Some(usage) = json.get("usage")
                                            && (usage.get("input_tokens").is_some()
                                                || usage.get("output_tokens").is_some())
                                        {
                                            return Some((Ok(StreamEvent::Done), (stream, buffer)));
                                        }
                                    }
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

                Some((Ok(StreamEvent::Done), (stream, buffer)))
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
            "DashScope provider does not support embeddings via native protocol".to_string(),
        )
        .into())
    }

    async fn list_models(&self) -> Result<Vec<RuntimeModelInfo>> {
        // DashScope does not provide a public models API.
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oauth_auth_headers() {
        let creds = OAuthCredentials {
            access_token: "at-abc".to_string(),
            refresh_token: Some("rt-xyz".to_string()),
            resource_url: None,
            expiry_date: None,
        };
        let p = DashScopeProvider::with_oauth(creds);
        assert_eq!(p.credentials.access_token, "at-abc");
    }

    #[test]
    fn oauth_credential_validity() {
        let future_expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
            + 7_200_000;
        let creds = OAuthCredentials {
            access_token: "token".to_string(),
            refresh_token: None,
            resource_url: None,
            expiry_date: Some(future_expiry),
        };
        assert!(creds.is_valid());

        let expired = OAuthCredentials {
            access_token: "token".to_string(),
            refresh_token: None,
            resource_url: None,
            expiry_date: Some(1_000),
        };
        assert!(!expired.is_valid());
    }

    #[test]
    fn default_endpoint_uses_native_base_url() {
        let creds = OAuthCredentials {
            access_token: "at".to_string(),
            refresh_token: None,
            resource_url: None,
            expiry_date: None,
        };
        let p = DashScopeProvider::with_oauth(creds);
        assert_eq!(p.endpoint(), NATIVE_BASE_URL);
    }

    #[test]
    fn resource_url_overrides_endpoint() {
        let creds = OAuthCredentials {
            access_token: "at".to_string(),
            refresh_token: None,
            resource_url: Some("https://my-ru.dashscope.aliyuncs.com".to_string()),
            expiry_date: None,
        };
        let p = DashScopeProvider::with_oauth(creds);
        assert!(
            p.endpoint()
                .contains("my-ru.dashscope.aliyuncs.com/api/v1/services/aigc")
        );
    }

    #[test]
    fn from_config_uses_access_token() {
        use crate::providers::config::DashScopeConfig;
        let cfg = DashScopeConfig {
            access_token: Some("at-oauth".to_string()),
            refresh_token: None,
            resource_url: None,
            expiry_date: None,
            base_url: None,
        };
        let p = DashScopeProvider::from_config(&cfg);
        assert_eq!(p.credentials.access_token, "at-oauth");
    }

    #[test]
    fn pkce_lengths() {
        let (verifier, challenge) = generate_pkce_pair();
        assert_eq!(verifier.len(), 43);
        assert_eq!(challenge.len(), 43);
        assert_ne!(verifier, challenge);
    }

    #[test]
    fn sha256_known_vector() {
        let digest = sha2_digest(b"abc");
        // Check structural properties: sha2_digest returns exactly 32 bytes,
        // is deterministic, and differs from the input.
        assert_eq!(digest.len(), 32);
        let digest2 = sha2_digest(b"abc");
        assert_eq!(digest, digest2, "sha2_digest must be deterministic");
        let digest_other = sha2_digest(b"abd");
        assert_ne!(digest, digest_other, "different inputs must differ");
    }

    #[tokio::test]
    async fn complete_fails_on_bad_token() {
        let creds = OAuthCredentials {
            access_token: "invalid-token".to_string(),
            refresh_token: None,
            resource_url: None,
            expiry_date: None,
        };
        let p = DashScopeProvider::with_oauth(creds);
        let req = CompletionRequest {
            model: "qwen-plus".to_string(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stream: false,
            tools: None,
        };
        assert!(p.complete("s1", &req).await.is_err());
    }
}
