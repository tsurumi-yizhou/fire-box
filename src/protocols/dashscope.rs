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
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// ─── DashScope OAuth token management ──────────────────────────────────────
// config.api_key is actually a refresh_token. We exchange it for a short-lived
// access_token via {base_url}/api/v1/oauth2/token.

const TOKEN_PATH: &str = "/api/v1/oauth2/token";
const OAUTH_CLIENT_ID: &str = "f0304373b74a44d2b584a3fb70ca9e56";
const TOKEN_REFRESH_BUFFER_SECS: u64 = 30;
/// Default refresh_token validity period: 90 days
const REFRESH_TOKEN_DEFAULT_VALIDITY_SECS: u64 = 90 * 24 * 3600;

#[derive(Debug, Serialize, Deserialize)]
struct PersistedTokenData {
    refresh_token: String,
    /// Unix timestamp (seconds) when the refresh_token expires.
    expiry_date: u64,
}

fn get_token_file_path(provider_tag: &str) -> String {
    format!(".dashscope_refresh_token_{}", provider_tag)
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

/// Load the latest persisted refresh_token with expiry check.
/// Returns None if file doesn't exist, is invalid JSON, or token is expired.
fn load_persisted_token(provider_tag: &str) -> Option<String> {
    let file_path = get_token_file_path(provider_tag);
    let content = std::fs::read_to_string(&file_path).ok()?;
    let data: PersistedTokenData = serde_json::from_str(&content).ok()?;

    // Check if token is expired (with some buffer)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();

    if data.expiry_date > now + TOKEN_REFRESH_BUFFER_SECS {
        Some(data.refresh_token)
    } else {
        debug!(
            provider = provider_tag,
            "Persisted refresh_token expired, will use config value"
        );
        None
    }
}

/// Persist the rotated refresh_token with expiry so it survives process restarts.
/// Note: refresh_token typically has a much longer validity than access_token.
/// We use a 90-day default validity period.
fn save_persisted_token(provider_tag: &str, token: &str) {
    let file_path = get_token_file_path(provider_tag);
    let expiry_date = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() + REFRESH_TOKEN_DEFAULT_VALIDITY_SECS)
        .unwrap_or(0);

    let data = PersistedTokenData {
        refresh_token: token.to_string(),
        expiry_date,
    };

    if let Ok(json) = serde_json::to_string_pretty(&data)
        && let Err(e) = std::fs::write(&file_path, json)
    {
        warn!(error = %e, provider = provider_tag, "Failed to persist DashScope refresh token");
    }
}

struct TokenCache {
    access_token: String,
    base_url: String,
    expires_at: Instant,
    refresh_token: String,
}

static TOKEN_CACHE: OnceLock<RwLock<Option<TokenCache>>> = OnceLock::new();

fn token_cache() -> &'static RwLock<Option<TokenCache>> {
    TOKEN_CACHE.get_or_init(|| RwLock::new(None))
}

/// Resolved access token and API base URL obtained via token refresh.
pub struct ResolvedToken {
    pub access_token: String,
    pub base_url: String,
}

/// Ensure a valid access token is available for DashScope.
/// `provider_tag` is used to create per-provider token persistence files.
/// `refresh_token` is the value from config's `api_key` field.
/// `config_base_url` is the `base_url` from config, used as fallback when the
/// token response does not include a `resource_url`.
pub async fn ensure_access_token(
    http: &reqwest::Client,
    provider_tag: &str,
    refresh_token: &str,
    config_base_url: &str,
) -> anyhow::Result<ResolvedToken> {
    // Fast path: read lock
    {
        let guard = token_cache().read().await;
        if let Some(cached) = guard.as_ref()
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
    if let Some(cached) = guard.as_ref()
        && cached.expires_at > Instant::now()
    {
        return Ok(ResolvedToken {
            access_token: cached.access_token.clone(),
            base_url: cached.base_url.clone(),
        });
    }

    // Use latest refresh_token: in-memory cache > persisted file > config value
    let current_refresh = guard
        .as_ref()
        .map(|c| c.refresh_token.clone())
        .or_else(|| load_persisted_token(provider_tag))
        .unwrap_or_else(|| refresh_token.to_string());

    debug!("Refreshing DashScope access token");

    let token_url = format!("{}{}", config_base_url.trim_end_matches('/'), TOKEN_PATH);

    let form_body = format!(
        "grant_type=refresh_token&refresh_token={}&client_id={}",
        urlencoded(&current_refresh),
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
        .unwrap_or(current_refresh);

    let resource_url = data
        .get("resource_url")
        .and_then(|v| v.as_str())
        .map(String::from);

    let base_url = match resource_url {
        Some(url) => {
            let normalized = if url.starts_with("http") {
                url
            } else {
                format!("https://{}", url)
            };
            if normalized.ends_with("/v1") {
                normalized
            } else {
                format!("{}/v1", normalized)
            }
        }
        None => config_base_url.trim_end_matches('/').to_string(),
    };

    let expires_at =
        Instant::now() + Duration::from_secs(expires_in.saturating_sub(TOKEN_REFRESH_BUFFER_SECS));

    info!(
        expires_in = expires_in,
        base_url = %base_url,
        provider = provider_tag,
        "DashScope access token refreshed"
    );

    // Persist the rotated refresh_token so it survives restarts.
    save_persisted_token(provider_tag, &new_refresh);

    let result = ResolvedToken {
        access_token: access_token.clone(),
        base_url: base_url.clone(),
    };

    *guard = Some(TokenCache {
        access_token,
        base_url,
        expires_at,
        refresh_token: new_refresh,
    });

    Ok(result)
}

// ─── Conversion: DashScope inbound → Unified ──────────────────────────────
// DashScope channel accepts OpenAI-compatible format, so decoding is similar
// to OpenAI but we also handle DashScope-specific content types.

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

pub fn format_stream_event(event: &StreamEvent, model: &str, request_id: &str) -> String {
    // DashScope channel output is OpenAI-compatible
    crate::protocols::openai::format_stream_event(event, model, request_id)
}

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
        let resolved = ensure_access_token(&http, &ds.tag, &ds.api_key, &ds.base_url)
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
