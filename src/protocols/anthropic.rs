/// Codec for the Anthropic messages protocol.
/// Converts between Anthropic JSON and our unified internal types.
use crate::protocol::*;
use bytes::Bytes;
use serde_json::Value;

// ─── Conversion: Anthropic inbound → Unified ──────────────────────────────

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

    // Anthropic uses a top-level "system" field.
    if let Some(sys) = v.get("system").and_then(|s| s.as_str()) {
        messages.push(UnifiedMessage {
            role: "system".into(),
            content: MessageContent::Text(sys.to_string()),
        });
    }

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
                            "image" => {
                                if let Some(src) = block.get("source")
                                    && let (Some(data), Some(media)) = (
                                        src.get("data").and_then(|d| d.as_str()),
                                        src.get("media_type").and_then(|m| m.as_str()),
                                    )
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
                            "document" => {
                                if let Some(src) = block.get("source")
                                    && let (Some(data), Some(media)) = (
                                        src.get("data").and_then(|d| d.as_str()),
                                        src.get("media_type").and_then(|m| m.as_str()),
                                    )
                                {
                                    let filename = media
                                        .split('/')
                                        .next_back()
                                        .unwrap_or("document")
                                        .to_string();
                                    let file_id = filesystem::store_file(
                                        filename.clone(),
                                        data.to_string(),
                                        media.to_string(),
                                    )
                                    .await;
                                    files.push(crate::protocol::FileAttachment {
                                        file_id,
                                        filename,
                                        content_base64: data.to_string(),
                                        media_type: media.to_string(),
                                        origin_provider: origin_provider.to_string(),
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            let content = parse_anthropic_content(msg.get("content"));
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

fn parse_anthropic_content(val: Option<&Value>) -> MessageContent {
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
                        "image" => {
                            let src = block.get("source")?;
                            let data = src.get("data")?.as_str()?.to_string();
                            let media = src
                                .get("media_type")
                                .and_then(|m| m.as_str())
                                .unwrap_or("image/png")
                                .to_string();
                            Some(ContentPart::ImageUrl {
                                image_url: ImageUrl {
                                    url: format!("data:{media};base64,{data}"),
                                },
                            })
                        }
                        "document" => {
                            let src = block.get("source")?;
                            Some(ContentPart::Document {
                                source: DocumentSource {
                                    source_type: src
                                        .get("type")
                                        .and_then(|t| t.as_str())
                                        .unwrap_or("base64")
                                        .to_string(),
                                    media_type: src
                                        .get("media_type")
                                        .and_then(|m| m.as_str())
                                        .unwrap_or("application/pdf")
                                        .to_string(),
                                    data: src
                                        .get("data")
                                        .and_then(|d| d.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                },
                            })
                        }
                        _ => None,
                    }
                })
                .collect();
            MessageContent::Parts(parts)
        }
        _ => MessageContent::Text(String::new()),
    }
}

// ─── Conversion: Unified → Anthropic outbound request JSON ─────────────────

pub async fn encode_request(req: &UnifiedRequest, model: &str) -> anyhow::Result<Value> {
    use crate::filesystem;

    let mut body = serde_json::json!({
        "model": model,
        "stream": req.stream,
    });

    // Extract system message.
    let system_msgs: Vec<&UnifiedMessage> =
        req.messages.iter().filter(|m| m.role == "system").collect();
    if let Some(sys) = system_msgs.first() {
        body["system"] = Value::String(sys.content.text_string());
    }

    // Messages (non-system).
    let mut messages: Vec<Value> = Vec::new();
    for m in req.messages.iter().filter(|m| m.role != "system") {
        let content = encode_anthropic_content(&m.content);
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
                // Determine content type
                if stored.media_type.starts_with("image/") {
                    content_arr.push(serde_json::json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": stored.media_type,
                            "data": stored.content_base64,
                        }
                    }));
                } else {
                    // Document type
                    content_arr.push(serde_json::json!({
                        "type": "document",
                        "source": {
                            "type": "base64",
                            "media_type": stored.media_type,
                            "data": stored.content_base64,
                        }
                    }));
                }
            }
        }
    }

    body["messages"] = Value::Array(messages);

    // max_tokens is required by Anthropic.
    let max = req.max_tokens.unwrap_or(4096);
    body["max_tokens"] = Value::Number(max.into());

    if let Some(temp) = req.temperature {
        body["temperature"] = serde_json::json!(temp);
    }

    Ok(body)
}

fn encode_anthropic_content(content: &MessageContent) -> Value {
    match content {
        MessageContent::Text(t) => Value::String(t.clone()),
        MessageContent::Parts(parts) => {
            let blocks: Vec<Value> = parts
                .iter()
                .map(|p| match p {
                    ContentPart::Text { text } => serde_json::json!({
                        "type": "text",
                        "text": text,
                    }),
                    ContentPart::ImageUrl { image_url } => {
                        // Try to convert data URI back to Anthropic format.
                        if let Some(rest) = image_url.url.strip_prefix("data:")
                            && let Some((media, data)) = rest.split_once(";base64,")
                        {
                            return serde_json::json!({
                                "type": "image",
                                "source": {
                                    "type": "base64",
                                    "media_type": media,
                                    "data": data,
                                }
                            });
                        }
                        // Fallback: URL-based image (Anthropic also supports url source).
                        serde_json::json!({
                            "type": "image",
                            "source": {
                                "type": "url",
                                "url": image_url.url,
                            }
                        })
                    }
                    ContentPart::Document { source } => serde_json::json!({
                        "type": "document",
                        "source": {
                            "type": source.source_type,
                            "media_type": source.media_type,
                            "data": source.data,
                        }
                    }),
                })
                .collect();
            Value::Array(blocks)
        }
    }
}

// ─── Build streaming SSE lines (Anthropic format) ──────────────────────────

pub fn format_stream_start(model: &str, request_id: &str) -> String {
    let msg = serde_json::json!({
        "type": "message_start",
        "message": {
            "id": request_id,
            "type": "message",
            "role": "assistant",
            "content": [],
            "model": model,
            "stop_reason": null,
            "usage": { "input_tokens": 0, "output_tokens": 0 }
        }
    });
    format!(
        "event: message_start\ndata: {}\n\nevent: content_block_start\ndata: {}\n\n",
        serde_json::to_string(&msg).unwrap_or_default(),
        serde_json::to_string(&serde_json::json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": { "type": "text", "text": "" }
        }))
        .unwrap_or_default(),
    )
}

pub fn format_stream_event(event: &StreamEvent, _model: &str, _request_id: &str) -> String {
    if event.event_type == "done" {
        let stop = serde_json::json!({
            "type": "content_block_stop",
            "index": 0,
        });
        let delta = serde_json::json!({
            "type": "message_delta",
            "delta": {
                "stop_reason": event.finish_reason.as_deref().unwrap_or("end_turn"),
            },
            "usage": { "output_tokens": 0 }
        });
        let stop_msg = serde_json::json!({ "type": "message_stop" });
        format!(
            "event: content_block_stop\ndata: {}\n\nevent: message_delta\ndata: {}\n\nevent: message_stop\ndata: {}\n\n",
            serde_json::to_string(&stop).unwrap_or_default(),
            serde_json::to_string(&delta).unwrap_or_default(),
            serde_json::to_string(&stop_msg).unwrap_or_default(),
        )
    } else {
        let d = serde_json::json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {
                "type": "text_delta",
                "text": event.delta_text.as_deref().unwrap_or(""),
            }
        });
        format!(
            "event: content_block_delta\ndata: {}\n\n",
            serde_json::to_string(&d).unwrap_or_default()
        )
    }
}

/// Build a non-streaming full response in Anthropic format.
pub fn format_full_response(text: &str, model: &str, request_id: &str) -> Value {
    serde_json::json!({
        "id": request_id,
        "type": "message",
        "role": "assistant",
        "content": [{ "type": "text", "text": text }],
        "model": model,
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 0,
            "output_tokens": 0,
        }
    })
}

// ─── Parse SSE stream from an Anthropic upstream ───────────────────────────

/// Parse a single SSE event pair (event: ...\ndata: ...) from Anthropic stream.
pub fn parse_stream_event(event_type: &str, data: &str) -> Option<StreamEvent> {
    match event_type {
        "content_block_delta" => {
            let v: Value = serde_json::from_str(data).ok()?;
            let delta = v.get("delta")?;
            let text = delta.get("text")?.as_str()?.to_string();
            Some(StreamEvent {
                event_type: "delta".into(),
                delta_text: Some(text),
                finish_reason: None,
            })
        }
        "message_delta" => {
            let v: Value = serde_json::from_str(data).ok()?;
            let stop = v
                .get("delta")
                .and_then(|d| d.get("stop_reason"))
                .and_then(|s| s.as_str())
                .map(String::from);
            Some(StreamEvent {
                event_type: "done".into(),
                delta_text: None,
                finish_reason: stop,
            })
        }
        "message_stop" => Some(StreamEvent {
            event_type: "done".into(),
            delta_text: None,
            finish_reason: Some("end_turn".into()),
        }),
        _ => None,
    }
}

// ─── Protocol constants ───────────────────────────────────────────────────

/// API endpoint path suffix for Anthropic providers.
pub fn endpoint_path() -> &'static str {
    "/messages"
}

/// Build all protocol-specific request headers for Anthropic.
pub fn request_headers(api_key: &str) -> Vec<(&'static str, String)> {
    vec![
        ("x-api-key", api_key.to_string()),
        ("anthropic-version", "2023-06-01".into()),
        ("Content-Type", "application/json".into()),
    ]
}

/// Parse non-streaming Anthropic response.
pub fn parse_full_response(body: &[u8]) -> anyhow::Result<String> {
    let v: Value = serde_json::from_slice(body)?;
    let text = v
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|b| b.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    Ok(text)
}
