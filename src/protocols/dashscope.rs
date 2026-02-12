/// Codec for the DashScope (Qwen) compatible-mode protocol.
///
/// DashScope uses an OpenAI-compatible API with extra features:
/// - Special headers: `X-DashScope-CacheControl`, `X-DashScope-UserAgent`
/// - `cache_control` fields on system messages and tools
/// - `metadata` field with session/prompt info
/// - File types: `file` (PDF), `video_url`, `input_audio` in addition to `image_url`
/// - Streaming quirk: finish_reason and usage may come in separate chunks
use crate::protocol::*;
use anyhow::Context;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// ─── DashScope OAuth token management ──────────────────────────────────────
// oauth_creds_path points to a JSON file with access_token, refresh_token,
// resource_url, and expiry_date. We use the access_token directly if valid,
// otherwise refresh via the OAuth token endpoint. If the creds file does not
// exist or the refresh_token is invalid, we start a device code OAuth flow.

const OAUTH_BASE_URL: &str = "https://chat.qwen.ai";
const DEVICE_CODE_PATH: &str = "/api/v1/oauth2/device/code";
const TOKEN_PATH: &str = "/api/v1/oauth2/token";
const OAUTH_CLIENT_ID: &str = "f0304373b74a44d2b584a3fb70ca9e56";
const OAUTH_SCOPE: &str = "openid profile email model.completion";
const DEVICE_CODE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
const TOKEN_REFRESH_BUFFER_SECS: u64 = 30;
const DEVICE_CODE_POLL_INTERVAL_SECS: u64 = 2;
const DEVICE_CODE_MAX_POLL_INTERVAL_SECS: u64 = 10;

/// On-disk format of the OAuth credentials file.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OAuthCredsFile {
    access_token: String,
    #[serde(default)]
    token_type: Option<String>,
    refresh_token: String,
    resource_url: String,
    /// Unix timestamp in **milliseconds** when the access_token expires.
    expiry_date: u64,
}

fn resolve_creds_path(raw: &str) -> PathBuf {
    PathBuf::from(shellexpand::tilde(raw).as_ref())
}

fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

/// Build the HTTPS base URL from a resource_url value (e.g. "portal.qwen.ai").
fn base_url_from_resource(resource_url: &str) -> String {
    let normalized = if resource_url.starts_with("http") {
        resource_url.to_string()
    } else {
        format!("https://{}", resource_url)
    };
    if normalized.ends_with("/v1") {
        normalized
    } else {
        format!("{}/v1", normalized.trim_end_matches('/'))
    }
}

/// Current epoch in milliseconds.
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

struct TokenCache {
    access_token: String,
    base_url: String,
    expires_at: Instant,
}

static TOKEN_CACHE: OnceLock<RwLock<std::collections::HashMap<String, TokenCache>>> =
    OnceLock::new();

fn token_cache() -> &'static RwLock<std::collections::HashMap<String, TokenCache>> {
    TOKEN_CACHE.get_or_init(|| RwLock::new(std::collections::HashMap::new()))
}

/// Resolved access token and API base URL obtained from creds file or refresh.
pub struct ResolvedToken {
    pub access_token: String,
    pub base_url: String,
}

/// Ensure a valid access token is available for DashScope.
///
/// Flow:
/// 1. Check in-memory cache
/// 2. Read creds file → if access_token valid, use it
/// 3. If access_token expired → refresh via refresh_token
/// 4. If creds file missing or refresh fails → run device code OAuth flow
pub async fn ensure_access_token(
    http: &reqwest::Client,
    provider_tag: &str,
    oauth_creds_path: &str,
) -> anyhow::Result<ResolvedToken> {
    let creds_path = resolve_creds_path(oauth_creds_path);
    let cache_key = oauth_creds_path.to_string();

    // Fast path: read lock
    {
        let guard = token_cache().read().await;
        if let Some(cached) = guard.get(&cache_key)
            && cached.expires_at > Instant::now()
        {
            return Ok(ResolvedToken {
                access_token: cached.access_token.clone(),
                base_url: cached.base_url.clone(),
            });
        }
    }

    // Slow path: write lock + refresh
    let mut guard = token_cache().write().await;
    // Double-check after lock upgrade
    if let Some(cached) = guard.get(&cache_key)
        && cached.expires_at > Instant::now()
    {
        return Ok(ResolvedToken {
            access_token: cached.access_token.clone(),
            base_url: cached.base_url.clone(),
        });
    }

    // Try reading the creds file
    let creds_result = std::fs::read_to_string(&creds_path)
        .ok()
        .and_then(|content| serde_json::from_str::<OAuthCredsFile>(&content).ok());

    let resolved = match creds_result {
        Some(mut creds) => {
            let now = now_millis();
            let buffer_ms = TOKEN_REFRESH_BUFFER_SECS * 1000;

            if creds.expiry_date > now + buffer_ms {
                // Access token is still valid — use directly
                let base_url = base_url_from_resource(&creds.resource_url);
                info!(
                    provider = provider_tag,
                    base_url = %base_url,
                    expires_in_secs = (creds.expiry_date - now) / 1000,
                    "Using existing DashScope access token"
                );

                let remaining_ms = creds.expiry_date - now - buffer_ms;
                let expires_at = Instant::now() + Duration::from_millis(remaining_ms);

                guard.insert(
                    cache_key,
                    TokenCache {
                        access_token: creds.access_token.clone(),
                        base_url: base_url.clone(),
                        expires_at,
                    },
                );

                return Ok(ResolvedToken {
                    access_token: creds.access_token,
                    base_url,
                });
            }

            // Access token expired — try refresh
            match refresh_access_token(http, provider_tag, &mut creds, &creds_path).await {
                Ok(r) => r,
                Err(e) => {
                    warn!(
                        provider = provider_tag,
                        error = %e,
                        "Token refresh failed, starting device code OAuth flow"
                    );
                    device_code_auth(http, provider_tag, &creds_path).await?
                }
            }
        }
        None => {
            // No creds file → device code flow
            info!(
                provider = provider_tag,
                path = %creds_path.display(),
                "OAuth creds file not found, starting device code OAuth flow"
            );
            device_code_auth(http, provider_tag, &creds_path).await?
        }
    };

    guard.insert(
        cache_key,
        TokenCache {
            access_token: resolved.access_token.clone(),
            base_url: resolved.base_url.clone(),
            expires_at: resolved.expires_at,
        },
    );

    Ok(ResolvedToken {
        access_token: resolved.access_token,
        base_url: resolved.base_url,
    })
}

/// Internal result that also carries the cache expiry.
struct ResolvedTokenWithExpiry {
    access_token: String,
    base_url: String,
    expires_at: Instant,
}

/// Refresh the access_token using the refresh_token.
/// On success, writes updated creds to disk.
async fn refresh_access_token(
    http: &reqwest::Client,
    provider_tag: &str,
    creds: &mut OAuthCredsFile,
    creds_path: &PathBuf,
) -> anyhow::Result<ResolvedTokenWithExpiry> {
    debug!(provider = provider_tag, "Refreshing DashScope access token");

    let token_url = format!("{}{}", OAUTH_BASE_URL, TOKEN_PATH);

    let form_body = format!(
        "grant_type=refresh_token&refresh_token={}&client_id={}",
        urlencoded(&creds.refresh_token),
        urlencoded(OAUTH_CLIENT_ID),
    );

    let resp = http
        .post(&token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(form_body)
        .send()
        .await
        .context("DashScope token refresh request failed")?;

    let status = resp.status();
    let resp_bytes = resp.bytes().await?;
    if !status.is_success() {
        let body_text = String::from_utf8_lossy(&resp_bytes);
        anyhow::bail!("DashScope token refresh failed ({}): {}", status, body_text);
    }

    let data: Value = serde_json::from_slice(&resp_bytes)?;

    let access_token = data
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing access_token in DashScope token response"))?
        .to_string();

    let expires_in = data
        .get("expires_in")
        .and_then(|v| v.as_u64())
        .unwrap_or(3600);

    let new_refresh = data
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| creds.refresh_token.clone());

    let resource_url = data
        .get("resource_url")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| creds.resource_url.clone());

    let base_url = base_url_from_resource(&resource_url);

    let expires_at =
        Instant::now() + Duration::from_secs(expires_in.saturating_sub(TOKEN_REFRESH_BUFFER_SECS));

    info!(
        expires_in = expires_in,
        base_url = %base_url,
        provider = provider_tag,
        "DashScope access token refreshed"
    );

    // Write updated creds back to file
    creds.access_token = access_token.clone();
    creds.refresh_token = new_refresh;
    creds.resource_url = resource_url;
    creds.expiry_date = now_millis() + expires_in * 1000;
    write_creds_file(creds_path, creds);

    Ok(ResolvedTokenWithExpiry {
        access_token,
        base_url,
        expires_at,
    })
}

// ─── Device Code OAuth Flow (RFC 8628) ─────────────────────────────────────

/// Generate a PKCE code_verifier and code_challenge (S256).
fn generate_pkce_pair() -> (String, String) {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use rand::Rng;

    let mut verifier_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut verifier_bytes);
    let code_verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let digest = hasher.finalize();
    let code_challenge = URL_SAFE_NO_PAD.encode(digest);

    (code_verifier, code_challenge)
}

/// Response from the device authorization endpoint.
#[derive(Debug, Deserialize)]
struct DeviceAuthResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    expires_in: u64,
}

/// Run the full device code OAuth flow:
/// 1. Request device code with PKCE
/// 2. Open browser for user authorization
/// 3. Poll for token
/// 4. Write creds to file
async fn device_code_auth(
    http: &reqwest::Client,
    provider_tag: &str,
    creds_path: &PathBuf,
) -> anyhow::Result<ResolvedTokenWithExpiry> {
    let (code_verifier, code_challenge) = generate_pkce_pair();

    // Step 1: Request device authorization
    let device_url = format!("{}{}", OAUTH_BASE_URL, DEVICE_CODE_PATH);
    let form_body = format!(
        "client_id={}&scope={}&code_challenge={}&code_challenge_method=S256",
        urlencoded(OAUTH_CLIENT_ID),
        urlencoded(OAUTH_SCOPE),
        urlencoded(&code_challenge),
    );

    let resp = http
        .post(&device_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(form_body)
        .send()
        .await
        .context("DashScope device code request failed")?;

    let status = resp.status();
    let resp_bytes = resp.bytes().await?;
    if !status.is_success() {
        let body_text = String::from_utf8_lossy(&resp_bytes);
        anyhow::bail!(
            "DashScope device code request failed ({}): {}",
            status,
            body_text
        );
    }

    let device_auth: DeviceAuthResponse =
        serde_json::from_slice(&resp_bytes).context("Failed to parse device code response")?;

    // Step 2: Print URL for user to open manually
    let auth_url = device_auth
        .verification_uri_complete
        .as_deref()
        .unwrap_or(&device_auth.verification_uri);

    info!(
        provider = provider_tag,
        user_code = %device_auth.user_code,
        url = %auth_url,
        "DashScope OAuth: please open the URL in your browser to authorize"
    );

    // Step 3: Poll for token
    let token_url = format!("{}{}", OAUTH_BASE_URL, TOKEN_PATH);
    let deadline = Instant::now() + Duration::from_secs(device_auth.expires_in);
    let mut interval = Duration::from_secs(DEVICE_CODE_POLL_INTERVAL_SECS);

    loop {
        if Instant::now() > deadline {
            anyhow::bail!("DashScope device code authorization timed out");
        }

        tokio::time::sleep(interval).await;

        let poll_body = format!(
            "grant_type={}&client_id={}&device_code={}&code_verifier={}",
            urlencoded(DEVICE_CODE_GRANT_TYPE),
            urlencoded(OAUTH_CLIENT_ID),
            urlencoded(&device_auth.device_code),
            urlencoded(&code_verifier),
        );

        let resp = http
            .post(&token_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .body(poll_body)
            .send()
            .await;

        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "DashScope token poll network error, retrying");
                continue;
            }
        };

        let poll_status = resp.status();
        let poll_bytes = resp.bytes().await.unwrap_or_default();
        let poll_data: Value =
            serde_json::from_slice(&poll_bytes).unwrap_or(Value::Object(Default::default()));

        if poll_status.is_success()
            && let Some(access_token) = poll_data.get("access_token").and_then(|v| v.as_str())
        {
            let expires_in = poll_data
                .get("expires_in")
                .and_then(|v| v.as_u64())
                .unwrap_or(3600);

            let refresh_token = poll_data
                .get("refresh_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let resource_url = poll_data
                .get("resource_url")
                .and_then(|v| v.as_str())
                .unwrap_or("dashscope.aliyuncs.com/compatible-mode")
                .to_string();

            let token_type = poll_data
                .get("token_type")
                .and_then(|v| v.as_str())
                .unwrap_or("Bearer")
                .to_string();

            let base_url = base_url_from_resource(&resource_url);
            let expiry_date = now_millis() + expires_in * 1000;

            let creds = OAuthCredsFile {
                access_token: access_token.to_string(),
                token_type: Some(token_type),
                refresh_token,
                resource_url,
                expiry_date,
            };

            // Ensure parent directory exists
            if let Some(parent) = creds_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            write_creds_file(creds_path, &creds);

            info!(
                provider = provider_tag,
                base_url = %base_url,
                expires_in = expires_in,
                "DashScope OAuth device flow completed successfully"
            );

            let expires_at = Instant::now()
                + Duration::from_secs(expires_in.saturating_sub(TOKEN_REFRESH_BUFFER_SECS));

            return Ok(ResolvedTokenWithExpiry {
                access_token: access_token.to_string(),
                base_url,
                expires_at,
            });
        }

        // Check error type for retry logic (RFC 8628)
        let error_code = poll_data
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match error_code {
            "authorization_pending" => {
                debug!("DashScope device code: authorization pending, continuing to poll");
            }
            "slow_down" => {
                let new_secs = (interval.as_millis() as u64 * 3 / 2 / 1000)
                    .min(DEVICE_CODE_MAX_POLL_INTERVAL_SECS);
                interval = Duration::from_secs(new_secs.max(1));
                debug!(
                    interval_secs = interval.as_secs(),
                    "DashScope device code: slow_down, increased poll interval"
                );
            }
            "expired_token" | "access_denied" => {
                anyhow::bail!("DashScope device code authorization failed: {}", error_code);
            }
            _ => {
                let desc = poll_data
                    .get("error_description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error");
                warn!(
                    error = error_code,
                    description = desc,
                    status = %poll_status,
                    "DashScope device code poll unexpected response, retrying"
                );
            }
        }
    }
}

/// Pre-flight check for DashScope OAuth credentials.
///
/// Called at startup. Reads the creds file, attempts a token refresh if the
/// access_token is expired, and triggers the device code flow if the creds
/// file is missing or the refresh_token is invalid. The result is cached so
/// subsequent `ensure_access_token` calls are instant.
pub async fn preflight_check(
    http: &reqwest::Client,
    provider_tag: &str,
    oauth_creds_path: &str,
) -> anyhow::Result<()> {
    ensure_access_token(http, provider_tag, oauth_creds_path).await?;
    Ok(())
}

/// Write the OAuth credentials to disk.
fn write_creds_file(path: &PathBuf, creds: &OAuthCredsFile) {
    if let Ok(json) = serde_json::to_string_pretty(creds)
        && let Err(e) = std::fs::write(path, json)
    {
        warn!(error = %e, path = %path.display(), "Failed to write OAuth creds file");
    }
}

// ─── Conversion: DashScope inbound → Unified ──────────────────────────────
// DashScope channel accepts OpenAI-compatible format, so decoding is similar
// to OpenAI but we also handle DashScope-specific content types.

#[allow(dead_code)]
pub async fn decode_request(body: &Bytes, origin_provider: &str) -> anyhow::Result<UnifiedRequest> {
    use crate::filesystem;

    let v: Value = serde_json::from_slice(body)?;

    let model = v
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("unknown")
        .to_string();
    let stream = v.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    let max_tokens = v.get("max_tokens").and_then(|m| m.as_u64());
    let temperature = v.get("temperature").and_then(|t| t.as_f64());

    let mut messages = Vec::new();
    let mut files = Vec::new();

    if let Some(msgs) = v.get("messages").and_then(|m| m.as_array()) {
        for msg in msgs {
            let role = msg
                .get("role")
                .and_then(|r| r.as_str())
                .unwrap_or("user")
                .to_string();

            // Extract files from content blocks
            if let Some(Value::Array(arr)) = msg.get("content") {
                for block in arr {
                    if let Some(tp) = block.get("type").and_then(|t| t.as_str()) {
                        match tp {
                            "image_url" => {
                                if let Some(url_obj) = block.get("image_url")
                                    && let Some(url) = url_obj.get("url").and_then(|u| u.as_str())
                                    && let Some(rest) = url.strip_prefix("data:")
                                    && let Some((media, data)) = rest.split_once(";base64,")
                                {
                                    let file_id = filesystem::store_file(
                                        "image".to_string(),
                                        data.to_string(),
                                        media.to_string(),
                                    )
                                    .await;
                                    files.push(crate::protocol::FileAttachment {
                                        file_id,
                                        filename: "image".to_string(),
                                        content_base64: data.to_string(),
                                        media_type: media.to_string(),
                                        origin_provider: origin_provider.to_string(),
                                    });
                                }
                            }
                            "file" => {
                                if let Some(file_obj) = block.get("file")
                                    && let Some(file_data) =
                                        file_obj.get("file_data").and_then(|f| f.as_str())
                                {
                                    let filename = file_obj
                                        .get("filename")
                                        .and_then(|f| f.as_str())
                                        .unwrap_or("file")
                                        .to_string();
                                    let (media_type, data) = if let Some(rest) =
                                        file_data.strip_prefix("data:")
                                        && let Some((media, d)) = rest.split_once(";base64,")
                                    {
                                        (media.to_string(), d.to_string())
                                    } else {
                                        ("application/octet-stream".to_string(), String::new())
                                    };
                                    let file_id = filesystem::store_file(
                                        filename.clone(),
                                        data.clone(),
                                        media_type.clone(),
                                    )
                                    .await;
                                    files.push(crate::protocol::FileAttachment {
                                        file_id,
                                        filename,
                                        content_base64: data,
                                        media_type,
                                        origin_provider: origin_provider.to_string(),
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            let content = parse_dashscope_content(msg.get("content"));
            messages.push(UnifiedMessage { role, content });
        }
    }

    Ok(UnifiedRequest {
        model,
        messages,
        stream,
        max_tokens,
        temperature,
        files,
    })
}

#[allow(dead_code)]
fn parse_dashscope_content(val: Option<&Value>) -> MessageContent {
    match val {
        Some(Value::String(s)) => MessageContent::Text(s.clone()),
        Some(Value::Array(arr)) => {
            let parts: Vec<ContentPart> = arr
                .iter()
                .filter_map(|block| {
                    let tp = block.get("type")?.as_str()?;
                    match tp {
                        "text" => Some(ContentPart::Text {
                            text: block.get("text")?.as_str()?.to_string(),
                        }),
                        "image_url" => {
                            let url = block.get("image_url")?.get("url")?.as_str()?.to_string();
                            Some(ContentPart::ImageUrl {
                                image_url: ImageUrl { url },
                            })
                        }
                        // DashScope-specific: file blocks (PDF, etc.)
                        "file" => {
                            let file_obj = block.get("file")?;
                            let file_data = file_obj.get("file_data")?.as_str()?.to_string();
                            let _filename = file_obj
                                .get("filename")
                                .and_then(|f| f.as_str())
                                .unwrap_or("file")
                                .to_string();
                            // Extract media type from data URI or default
                            let media_type = if let Some(rest) = file_data.strip_prefix("data:") {
                                rest.split_once(';')
                                    .map(|(m, _)| m.to_string())
                                    .unwrap_or_else(|| "application/octet-stream".into())
                            } else {
                                "application/octet-stream".into()
                            };
                            let data = file_data
                                .split_once(";base64,")
                                .map(|(_, d)| d.to_string())
                                .unwrap_or_default();
                            Some(ContentPart::Document {
                                source: DocumentSource {
                                    source_type: "base64".into(),
                                    media_type,
                                    data,
                                },
                            })
                        }
                        _ => None,
                    }
                })
                .collect();
            if parts.is_empty() {
                MessageContent::Text(String::new())
            } else {
                MessageContent::Parts(parts)
            }
        }
        _ => MessageContent::Text(String::new()),
    }
}

// ─── Conversion: Unified → DashScope outbound request JSON ─────────────────

pub async fn encode_request(req: &UnifiedRequest, model: &str) -> anyhow::Result<Value> {
    use crate::filesystem;

    let mut messages: Vec<Value> = Vec::new();
    for m in &req.messages {
        let content = encode_dashscope_content(&m.content);
        messages.push(serde_json::json!({
            "role": m.role,
            "content": content,
        }));
    }

    // Lazily inject files into the last user message (or create a new one).
    if !req.files.is_empty() {
        let last_idx = messages.len().saturating_sub(1);
        let last_msg = if messages.is_empty() {
            messages.push(serde_json::json!({
                "role": "user",
                "content": [],
            }));
            &mut messages[0]
        } else {
            &mut messages[last_idx]
        };

        // Ensure content is an array.
        if last_msg.get("content").and_then(|c| c.as_str()).is_some() {
            let text = last_msg["content"].as_str().unwrap_or("").to_string();
            last_msg["content"] = serde_json::json!([
                { "type": "text", "text": text }
            ]);
        } else if last_msg.get("content").and_then(|c| c.as_array()).is_none() {
            last_msg["content"] = serde_json::json!([]);
        }

        let content_arr = last_msg["content"].as_array_mut().unwrap();
        for file in &req.files {
            let file_data = filesystem::get_file(&file.file_id).await;
            if let Some(stored) = file_data {
                let data_uri = format!(
                    "data:{};base64,{}",
                    stored.media_type, stored.content_base64
                );
                if stored.media_type.starts_with("image/") {
                    content_arr.push(serde_json::json!({
                        "type": "image_url",
                        "image_url": { "url": data_uri },
                    }));
                } else {
                    // File/document type
                    content_arr.push(serde_json::json!({
                        "type": "file",
                        "file": {
                            "file_data": data_uri,
                            "filename": stored.filename,
                        }
                    }));
                }
            }
        }
    }

    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": req.stream,
    });

    if req.stream {
        body["stream_options"] = serde_json::json!({ "include_usage": true });
    }

    if let Some(max) = req.max_tokens {
        body["max_tokens"] = Value::Number(max.into());
    }
    if let Some(temp) = req.temperature {
        body["temperature"] = serde_json::json!(temp);
    }

    Ok(body)
}

fn encode_dashscope_content(content: &MessageContent) -> Value {
    match content {
        MessageContent::Text(t) => Value::String(t.clone()),
        MessageContent::Parts(parts) => {
            let arr: Vec<Value> = parts
                .iter()
                .map(|p| match p {
                    ContentPart::Text { text } => serde_json::json!({
                        "type": "text",
                        "text": text,
                    }),
                    ContentPart::ImageUrl { image_url } => serde_json::json!({
                        "type": "image_url",
                        "image_url": { "url": image_url.url },
                    }),
                    ContentPart::Document { source } => {
                        // Encode as DashScope `file` block
                        let data_uri = format!("data:{};base64,{}", source.media_type, source.data);
                        serde_json::json!({
                            "type": "file",
                            "file": {
                                "file_data": data_uri,
                            }
                        })
                    }
                })
                .collect();
            Value::Array(arr)
        }
    }
}

// ─── DashScope-specific HTTP headers ───────────────────────────────────────

/// API endpoint path suffix for DashScope compatible-mode providers.
pub fn endpoint_path() -> &'static str {
    "/chat/completions"
}

/// Build all protocol-specific request headers for DashScope.
pub fn request_headers(api_key: &str) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        ("Authorization", format!("Bearer {}", api_key)),
        ("Content-Type", "application/json".into()),
    ];
    headers.extend(extra_headers());
    headers
}

/// Returns extra headers specific to DashScope.
fn extra_headers() -> Vec<(&'static str, String)> {
    vec![
        ("X-DashScope-CacheControl", "enable".into()),
        (
            "X-DashScope-UserAgent",
            format!(
                "fire-box/{} ({}/{})",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH,
            ),
        ),
    ]
}

// ─── Build streaming SSE lines (DashScope → client, OpenAI format) ─────────
// When used as a channel, DashScope format is OpenAI-compatible for output.
// We reuse the OpenAI streaming format. The functions below handle the
// DashScope-specific quirks when *parsing* from upstream.

#[allow(dead_code)]
pub fn format_stream_event(event: &StreamEvent, model: &str, request_id: &str) -> String {
    // DashScope channel output is OpenAI-compatible
    crate::protocols::openai::format_stream_event(event, model, request_id)
}

#[allow(dead_code)]
pub fn format_full_response(text: &str, model: &str, request_id: &str) -> Value {
    crate::protocols::openai::format_full_response(text, model, request_id)
}

// ─── Parse SSE stream from a DashScope upstream ────────────────────────────
// DashScope uses OpenAI SSE format but with a quirk: finish_reason and usage
// may come in separate final chunks. We handle chunk merging here.

/// Parse a single SSE `data:` line from a DashScope stream.
/// Returns None for lines that should be skipped (e.g. pure usage chunks).
pub fn parse_stream_line(line: &str) -> Option<StreamEvent> {
    let data = line.strip_prefix("data: ")?.trim();
    if data == "[DONE]" {
        return Some(StreamEvent {
            event_type: "done".into(),
            delta_text: None,
            finish_reason: Some("stop".into()),
        });
    }
    let v: Value = serde_json::from_str(data).ok()?;

    // DashScope quirk: a chunk with empty choices but usage info → skip
    let choices = v.get("choices")?.as_array()?;
    if choices.is_empty() {
        // This is a usage-only chunk, not a content chunk
        return None;
    }

    let choice = choices.first()?;
    let delta = choice.get("delta")?;
    let finish = choice
        .get("finish_reason")
        .and_then(|f| f.as_str())
        .map(String::from);

    // Check for reasoning_content (Qwen thinking models)
    let reasoning = delta
        .get("reasoning_content")
        .and_then(|r| r.as_str())
        .map(String::from);

    let content = delta
        .get("content")
        .and_then(|c| c.as_str())
        .map(String::from);

    // If this chunk has finish_reason but no content, it's the end signal
    if finish.is_some() && content.is_none() && reasoning.is_none() {
        return Some(StreamEvent {
            event_type: "done".into(),
            delta_text: None,
            finish_reason: finish,
        });
    }

    // Prefer content, fall back to reasoning_content
    let text = content.or(reasoning);
    text.map(|t| StreamEvent {
        event_type: "delta".into(),
        delta_text: Some(t),
        finish_reason: None,
    })
}

/// Parse a non-streaming DashScope response (OpenAI-compatible format).
pub fn parse_full_response(body: &[u8]) -> anyhow::Result<String> {
    crate::protocols::openai::parse_full_response(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_dashscope_config() -> Option<crate::config::ProviderConfig> {
        let config = crate::config::Config::load("config.json").ok()?;
        config
            .providers
            .into_iter()
            .find(|p| p.provider_type == crate::config::ProtocolType::DashScope)
    }

    #[tokio::test]
    async fn test_token_refresh() {
        let ds = match load_dashscope_config() {
            Some(p) => p,
            None => {
                eprintln!("Skipping: no DashScope provider in config.json");
                return;
            }
        };

        let http = reqwest::Client::new();
        let creds_path = ds
            .oauth_creds_path
            .as_deref()
            .expect("DashScope provider missing oauth_creds_path");
        let resolved = ensure_access_token(&http, &ds.tag, creds_path)
            .await
            .expect("token refresh should succeed");

        assert!(
            !resolved.access_token.is_empty(),
            "access_token must not be empty"
        );
        assert!(!resolved.base_url.is_empty(), "base_url must not be empty");
        eprintln!("token refresh OK: base_url={}", resolved.base_url);
    }
}
