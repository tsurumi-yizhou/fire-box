use crate::config::{ProtocolType, ProviderConfig};
use crate::protocol::{StreamEvent, UnifiedRequest};
/// Upstream provider client.
/// Sends requests to LLM providers and returns responses (streaming or full).
use crate::protocols::{anthropic, copilot, dashscope, openai};
use anyhow::Context;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// Fixed URL for Copilot chat completions (not provider-configurable).
const COPILOT_CHAT_URL: &str = "https://api.githubcopilot.com/chat/completions";

// ─── Protocol dispatch helpers ──────────────────────────────────────────────

fn build_url(provider: &ProviderConfig) -> String {
    let base = provider
        .base_url
        .as_deref()
        .unwrap_or("")
        .trim_end_matches('/');
    let path = match provider.provider_type {
        ProtocolType::OpenAI => openai::endpoint_path(),
        ProtocolType::Anthropic => anthropic::endpoint_path(),
        ProtocolType::DashScope => dashscope::endpoint_path(),
        ProtocolType::Copilot => copilot::endpoint_path(),
    };
    format!("{}{}", base, path)
}

/// Resolve the actual URL and auth headers for a provider.
/// For DashScope this reads the OAuth creds file and refreshes if needed.
async fn resolve_request(
    http: &reqwest::Client,
    provider: &ProviderConfig,
) -> anyhow::Result<(String, Vec<(&'static str, String)>)> {
    match provider.provider_type {
        ProtocolType::DashScope => {
            let creds_path = provider.oauth_creds_path.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "DashScope provider '{}' missing oauth_creds_path",
                    provider.tag
                )
            })?;
            let resolved =
                dashscope::ensure_access_token(http, &provider.tag, creds_path).await?;
            let url = format!(
                "{}{}",
                resolved.base_url.trim_end_matches('/'),
                dashscope::endpoint_path()
            );
            let headers = dashscope::request_headers(&resolved.access_token);
            Ok((url, headers))
        }
        ProtocolType::Copilot => {
            let session = copilot::ensure_session(http, &provider.tag).await?;
            let url = COPILOT_CHAT_URL.to_string();
            let headers = copilot::request_headers(&session);
            Ok((url, headers))
        }
        _ => {
            let url = build_url(provider);
            let headers = match provider.provider_type {
                ProtocolType::OpenAI => {
                    let key = crate::keystore::get_provider_key(&provider.tag).unwrap_or_default();
                    openai::request_headers(&key)
                }
                ProtocolType::Anthropic => {
                    let token = crate::keystore::get_auth_token(&provider.tag).unwrap_or_default();
                    anthropic::request_headers(&token)
                }
                ProtocolType::DashScope | ProtocolType::Copilot => unreachable!(),
            };
            Ok((url, headers))
        }
    }
}

fn apply_resolved_headers(
    mut builder: reqwest::RequestBuilder,
    headers: &[(&str, String)],
) -> reqwest::RequestBuilder {
    for (name, value) in headers {
        builder = builder.header(*name, value);
    }
    builder
}

async fn encode_body(
    req: &UnifiedRequest,
    model: &str,
    protocol: ProtocolType,
) -> anyhow::Result<Value> {
    match protocol {
        ProtocolType::OpenAI => openai::encode_request(req, model).await,
        ProtocolType::Anthropic => anthropic::encode_request(req, model).await,
        ProtocolType::DashScope => dashscope::encode_request(req, model).await,
        ProtocolType::Copilot => copilot::encode_request(req, model).await,
    }
}

fn parse_response(body: &[u8], protocol: ProtocolType) -> anyhow::Result<String> {
    match protocol {
        ProtocolType::OpenAI => openai::parse_full_response(body),
        ProtocolType::Anthropic => anthropic::parse_full_response(body),
        ProtocolType::DashScope => dashscope::parse_full_response(body),
        ProtocolType::Copilot => copilot::parse_full_response(body),
    }
}

// ─── Non-streaming request ──────────────────────────────────────────────────

/// Send a non-streaming request to a provider and return the assistant text.
pub async fn send_request(
    http: &reqwest::Client,
    provider: &ProviderConfig,
    model: &str,
    request: &UnifiedRequest,
) -> anyhow::Result<String> {
    let mut req = request.clone();
    req.stream = false;

    let body = encode_body(&req, model, provider.provider_type).await?;
    let (url, headers) = resolve_request(http, provider).await?;
    debug!(url = %url, provider = %provider.tag, model = %model, "Sending request");

    let builder = apply_resolved_headers(http.post(&url), &headers);
    let resp = builder
        .json(&body)
        .send()
        .await
        .with_context(|| format!("Failed to send request to {} provider", provider.tag))?;

    let status = resp.status();
    let bytes = resp.bytes().await?;
    if !status.is_success() {
        let body_text = String::from_utf8_lossy(&bytes);
        anyhow::bail!(
            "{} provider returned {}: {}",
            provider.tag,
            status,
            body_text
        );
    }
    parse_response(&bytes, provider.provider_type)
}

/// Send a streaming request and return a channel of StreamEvents.
pub async fn send_stream_request(
    http: &reqwest::Client,
    provider: &ProviderConfig,
    model: &str,
    request: &UnifiedRequest,
) -> anyhow::Result<mpsc::Receiver<StreamEvent>> {
    let mut req = request.clone();
    req.stream = true;

    let body = encode_body(&req, model, provider.provider_type).await?;
    let (url, headers) = resolve_request(http, provider).await?;
    debug!(url = %url, provider = %provider.tag, model = %model, "Sending streaming request");

    let builder = apply_resolved_headers(http.post(&url), &headers);
    let resp = builder.json(&body).send().await.with_context(|| {
        format!(
            "Failed to send streaming request to {} provider",
            provider.tag
        )
    })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "{} provider returned {}: {}",
            provider.tag,
            status,
            body_text
        );
    }

    let (tx, rx) = mpsc::channel::<StreamEvent>(256);
    let protocol = provider.provider_type;

    tokio::spawn(async move {
        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));
                    if !process_sse_buffer(&mut buffer, &tx, protocol).await {
                        return;
                    }
                }
                Err(e) => {
                    warn!("Stream read error: {e}");
                    let _ = tx
                        .send(StreamEvent {
                            event_type: "done".into(),
                            delta_text: None,
                            finish_reason: Some("error".into()),
                        })
                        .await;
                    return;
                }
            }
        }
        // Stream ended naturally.
        let _ = tx
            .send(StreamEvent {
                event_type: "done".into(),
                delta_text: None,
                finish_reason: Some("stop".into()),
            })
            .await;
    });

    Ok(rx)
}

// ─── SSE buffer processing ──────────────────────────────────────────────────

/// Process accumulated SSE data in the buffer. Returns false if stream is done.
async fn process_sse_buffer(
    buffer: &mut String,
    tx: &mpsc::Sender<StreamEvent>,
    protocol: ProtocolType,
) -> bool {
    match protocol {
        ProtocolType::OpenAI => process_line_sse(buffer, tx, openai::parse_stream_line).await,
        ProtocolType::DashScope => process_line_sse(buffer, tx, dashscope::parse_stream_line).await,
        ProtocolType::Copilot => process_line_sse(buffer, tx, copilot::parse_stream_line).await,
        ProtocolType::Anthropic => process_event_sse(buffer, tx).await,
    }
}

/// Process line-based SSE (OpenAI / DashScope format).
async fn process_line_sse(
    buffer: &mut String,
    tx: &mpsc::Sender<StreamEvent>,
    parser: fn(&str) -> Option<StreamEvent>,
) -> bool {
    while let Some(pos) = buffer.find('\n') {
        let line = buffer[..pos].trim().to_string();
        *buffer = buffer[pos + 1..].to_string();
        if line.is_empty() {
            continue;
        }
        if let Some(event) = parser(&line) {
            let is_done = event.event_type == "done";
            if tx.send(event).await.is_err() {
                return false;
            }
            if is_done {
                return false;
            }
        }
    }
    true
}

/// Process event-based SSE (Anthropic format: event: type\ndata: json\n\n).
async fn process_event_sse(buffer: &mut String, tx: &mpsc::Sender<StreamEvent>) -> bool {
    while let Some((evt_type, data)) = parse_next_anthropic_sse(buffer) {
        if let Some(event) = anthropic::parse_stream_event(&evt_type, &data) {
            let is_done = event.event_type == "done";
            if tx.send(event).await.is_err() {
                return false;
            }
            if is_done {
                return false;
            }
        }
    }
    true
}

/// Parse the next complete SSE event from the buffer.
/// Returns (event_type, data) and removes the consumed part from buffer.
fn parse_next_anthropic_sse(buffer: &mut String) -> Option<(String, String)> {
    // Look for a blank-line-terminated block.
    let block_end = buffer.find("\n\n")?;
    let block = buffer[..block_end].to_string();
    *buffer = buffer[block_end + 2..].to_string();

    let mut event_type = String::new();
    let mut data = String::new();

    for line in block.lines() {
        if let Some(rest) = line.strip_prefix("event: ") {
            event_type = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data: ") {
            data = rest.trim().to_string();
        }
    }

    if event_type.is_empty() && data.is_empty() {
        return None;
    }
    Some((event_type, data))
}

// previously provided `collect_stream` helper was removed as it was unused.

#[cfg(test)]
mod dashscope_tests {
    use super::*;
    use crate::config::Config;
    use crate::protocol::{MessageContent, UnifiedMessage};

    fn load_dashscope_provider() -> Option<ProviderConfig> {
        let config = Config::load_from_keyring();
        config
            .providers
            .into_iter()
            .find(|p| p.provider_type == ProtocolType::DashScope)
    }

    fn simple_request() -> UnifiedRequest {
        UnifiedRequest {
            model: String::new(),
            messages: vec![UnifiedMessage {
                role: "user".into(),
                content: MessageContent::Text(
                    "\u{8BF4}\u{201C}\u{4F60}\u{597D}\u{201D}\u{4E24}\u{4E2A}\u{5B57}\u{FF0C}\u{4E0D}\u{8981}\u{8BF4}\u{522B}\u{7684}\u{3002}".into(),
                ),
            }],
            stream: false,
            max_tokens: Some(32),
            temperature: Some(0.0),
            files: Vec::new(),
        }
    }

    #[tokio::test]
    async fn test_dashscope_non_streaming() {
        let ds = match load_dashscope_provider() {
            Some(p) => p,
            None => {
                eprintln!("Skipping: no DashScope provider in config.json");
                return;
            }
        };

        let http = reqwest::Client::new();
        let result = send_request(&http, &ds, "coder-model", &simple_request())
            .await
            .expect("non-streaming request should succeed");

        assert!(!result.is_empty(), "response must not be empty");
        eprintln!("non-streaming response: {}", result);
    }

    #[tokio::test]
    async fn test_dashscope_streaming() {
        let ds = match load_dashscope_provider() {
            Some(p) => p,
            None => {
                eprintln!("Skipping: no DashScope provider in config.json");
                return;
            }
        };

        let http = reqwest::Client::new();
        let mut rx = send_stream_request(&http, &ds, "coder-model", &simple_request())
            .await
            .expect("streaming request should succeed");

        let mut full_text = String::new();
        let mut got_done = false;

        while let Some(event) = rx.recv().await {
            if let Some(text) = &event.delta_text {
                full_text.push_str(text);
            }
            if event.event_type == "done" {
                got_done = true;
                break;
            }
        }

        assert!(got_done, "should receive done event");
        assert!(!full_text.is_empty(), "streamed text must not be empty");
        eprintln!("streaming response: {}", full_text);
    }
}
