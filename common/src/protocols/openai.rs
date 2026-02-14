/// Codec for the OpenAI chat-completions protocol.
/// Converts between OpenAI JSON and our unified internal types.
use crate::protocol::*;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ─── OpenAI request/response types (subset) ────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct OpenAIRequest {
    pub model: String,
    pub messages: Vec<OpenAIMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Preserve all other fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenAIMessage {
    pub role: String,
    pub content: Option<OpenAIContent>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum OpenAIContent {
    Text(String),
    Parts(Vec<Value>),
}

// ─── Streaming chunk format ────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct OpenAIStreamChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAIStreamChoice>,
}

#[derive(Debug, Serialize)]
pub struct OpenAIStreamChoice {
    pub index: u32,
    pub delta: OpenAIDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OpenAIDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

// ─── Conversion: OpenAI → Unified ──────────────────────────────────────────

pub async fn decode_request(
    body: &Bytes,
    origin_provider: &str,
) -> anyhow::Result<(UnifiedRequest, OpenAIRequest)> {
    use crate::filesystem;

    let oai: OpenAIRequest = serde_json::from_slice(body)?;
    let mut files = Vec::new();

    let messages = oai
        .messages
        .iter()
        .map(|m| {
            let content = match &m.content {
                Some(OpenAIContent::Text(t)) => MessageContent::Text(t.clone()),
                Some(OpenAIContent::Parts(parts)) => {
                    let converted: Vec<ContentPart> = parts
                        .iter()
                        .filter_map(|p| {
                            let tp = p.get("type")?.as_str()?;
                            match tp {
                                "text" => Some(ContentPart::Text {
                                    text: p.get("text")?.as_str()?.to_string(),
                                }),
                                "image_url" => {
                                    let url_obj = p.get("image_url")?;
                                    let url = url_obj.get("url")?.as_str()?.to_string();
                                    Some(ContentPart::ImageUrl {
                                        image_url: ImageUrl { url },
                                    })
                                }
                                _ => None,
                            }
                        })
                        .collect();
                    MessageContent::Parts(converted)
                }
                None => MessageContent::Text(String::new()),
            };
            UnifiedMessage {
                role: m.role.clone(),
                content,
            }
        })
        .collect();

    // Extract files from image_url blocks with data URIs
    for msg in &oai.messages {
        if let Some(OpenAIContent::Parts(parts)) = &msg.content {
            for part in parts {
                if let Some("image_url") = part.get("type").and_then(|t| t.as_str())
                    && let Some(url_obj) = part.get("image_url")
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
        }
    }

    let unified = UnifiedRequest {
        model: oai.model.clone(),
        messages,
        stream: oai.stream,
        max_tokens: oai.max_tokens,
        temperature: oai.temperature,
        files,
    };
    Ok((unified, oai))
}

// ─── Conversion: Unified → OpenAI outbound request JSON ────────────────────

pub async fn encode_request(req: &UnifiedRequest, model: &str) -> anyhow::Result<Value> {
    use crate::filesystem;

    // Build messages, injecting files as needed.
    let mut messages: Vec<Value> = Vec::new();

    for m in &req.messages {
        let content = match &m.content {
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
                        ContentPart::Document { source } => serde_json::json!({
                            "type": "text",
                            "text": format!("[document: {}]", source.media_type),
                        }),
                    })
                    .collect();
                Value::Array(arr)
            }
        };
        messages.push(serde_json::json!({
            "role": m.role,
            "content": content,
        }));
    }

    // Lazily inject files into the last user message (or create a new one).
    if !req.files.is_empty() {
        // Find the last message or create a new user message.
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
                content_arr.push(serde_json::json!({
                    "type": "image_url",
                    "image_url": { "url": data_uri },
                }));
            }
        }
    }

    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": req.stream,
    });

    if let Some(max) = req.max_tokens {
        body["max_tokens"] = Value::Number(max.into());
    }
    if let Some(temp) = req.temperature {
        body["temperature"] = serde_json::json!(temp);
    }

    Ok(body)
}

// ─── Build streaming SSE lines (OpenAI format) ────────────────────────────

pub fn format_stream_event(event: &StreamEvent, model: &str, request_id: &str) -> String {
    if event.event_type == "done" {
        let chunk = OpenAIStreamChunk {
            id: request_id.to_string(),
            object: "chat.completion.chunk".into(),
            created: now_epoch(),
            model: model.to_string(),
            choices: vec![OpenAIStreamChoice {
                index: 0,
                delta: OpenAIDelta {
                    role: None,
                    content: None,
                },
                finish_reason: event.finish_reason.clone().or(Some("stop".into())),
            }],
        };
        let json = serde_json::to_string(&chunk).unwrap_or_default();
        format!("data: {json}\n\ndata: [DONE]\n\n")
    } else {
        let chunk = OpenAIStreamChunk {
            id: request_id.to_string(),
            object: "chat.completion.chunk".into(),
            created: now_epoch(),
            model: model.to_string(),
            choices: vec![OpenAIStreamChoice {
                index: 0,
                delta: OpenAIDelta {
                    role: None,
                    content: event.delta_text.clone(),
                },
                finish_reason: None,
            }],
        };
        let json = serde_json::to_string(&chunk).unwrap_or_default();
        format!("data: {json}\n\n")
    }
}

/// Build a non-streaming full response in OpenAI format.
pub fn format_full_response(text: &str, model: &str, request_id: &str) -> Value {
    serde_json::json!({
        "id": request_id,
        "object": "chat.completion",
        "created": now_epoch(),
        "model": model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": text,
            },
            "finish_reason": "stop",
        }],
        "usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0,
        }
    })
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ─── Parse SSE stream from an OpenAI-compatible upstream ────────────────────

/// Extract delta text from one SSE `data:` JSON line from an OpenAI stream.
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
    let choice = v.get("choices")?.as_array()?.first()?;
    let delta = choice.get("delta")?;
    let finish = choice
        .get("finish_reason")
        .and_then(|f| f.as_str())
        .map(String::from);

    if finish.is_some() && delta.get("content").is_none() {
        return Some(StreamEvent {
            event_type: "done".into(),
            delta_text: None,
            finish_reason: finish,
        });
    }

    let text = delta.get("content")?.as_str()?.to_string();
    Some(StreamEvent {
        event_type: "delta".into(),
        delta_text: Some(text),
        finish_reason: None,
    })
}

// ─── Protocol constants ────────────────────────────────────────────────────

/// API endpoint path suffix for OpenAI-compatible providers.
pub fn endpoint_path() -> &'static str {
    "/chat/completions"
}

/// Build all protocol-specific request headers for OpenAI.
pub fn request_headers(api_key: &str) -> Vec<(&'static str, String)> {
    vec![
        ("Authorization", format!("Bearer {}", api_key)),
        ("Content-Type", "application/json".into()),
    ]
}

/// Parse a non-streaming OpenAI response body into the assistant text.
pub fn parse_full_response(body: &[u8]) -> anyhow::Result<String> {
    let v: Value = serde_json::from_slice(body)?;
    let text = v
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    Ok(text)
}
