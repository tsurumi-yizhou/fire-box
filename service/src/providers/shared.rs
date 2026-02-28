//! Shared utilities used by multiple provider implementations.
//!
//! Centralises:
//! - HTTP client construction
//! - HTTP error-status checking
//! - OpenAI-protocol message / tool JSON serialisation
//! - OpenAI-protocol streaming tool-call delta merging
//! - SSE line parsing

use anyhow::{Result, bail};
use std::time::Duration;

use crate::providers::{ChatMessage, Tool, ToolCall, ToolCallFunction};

// ---------------------------------------------------------------------------
// HTTP client construction
// ---------------------------------------------------------------------------

/// Build a `reqwest::Client` with a single overall timeout and TLS enabled.
pub fn build_http_client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .unwrap_or_default()
}

/// Build a `reqwest::Client` with explicit per-phase timeouts.
///
/// - `timeout`: end-to-end request timeout (including response body).
/// - `connect_timeout`: TCP + TLS handshake timeout.
/// - `pool_idle_timeout`: how long an idle keep-alive socket is retained.
/// - `https_only`: reject plain-HTTP connections when `true`.
pub fn build_http_client_full(
    timeout: Duration,
    connect_timeout: Duration,
    pool_idle_timeout: Duration,
    https_only: bool,
) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(connect_timeout)
        .pool_idle_timeout(pool_idle_timeout)
        .https_only(https_only)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

// ---------------------------------------------------------------------------
// HTTP error-status checking
// ---------------------------------------------------------------------------

/// Consume a `Response`, returning it unchanged when the status is 2xx,
/// or bailing with a human-readable error that includes the raw body text.
pub async fn check_status(response: reqwest::Response) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    bail!("HTTP {}: {}", status, body)
}

// ---------------------------------------------------------------------------
// SSE line parsing
// ---------------------------------------------------------------------------

/// Extract the payload from a Server-Sent Events `data: <payload>` line.
///
/// Returns `None` for empty lines, comment lines (`:`), or any line that does
/// not begin with the `data: ` prefix (six characters including the space).
pub fn sse_data(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.starts_with("data: ") {
        return None;
    }
    Some(&trimmed[6..])
}

// ---------------------------------------------------------------------------
// OpenAI-protocol JSON serialisation
// ---------------------------------------------------------------------------

/// Serialise a [`ChatMessage`] to the OpenAI messages-array JSON format.
pub fn message_to_json(m: &ChatMessage) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "role": m.role,
        "content": m.content,
    });
    if let Some(name) = &m.name {
        obj["name"] = serde_json::json!(name);
    }
    if let Some(tool_call_id) = &m.tool_call_id {
        obj["tool_call_id"] = serde_json::json!(tool_call_id);
    }
    if let Some(tool_calls) = &m.tool_calls {
        obj["tool_calls"] = serde_json::Value::Array(
            tool_calls
                .iter()
                .map(|tc| {
                    serde_json::json!({
                        "id": tc.id,
                        "type": tc.call_type,
                        "function": {
                            "name": tc.function.name,
                            "arguments": tc.function.arguments,
                        }
                    })
                })
                .collect(),
        );
    }
    obj
}

/// Serialise a [`Tool`] to the OpenAI tools-array JSON format.
pub fn tool_to_json(t: &Tool) -> serde_json::Value {
    serde_json::json!({
        "type": t.tool_type,
        "function": {
            "name": t.function.name,
            "description": t.function.description,
            "parameters": t.function.parameters,
        }
    })
}

/// Accumulate OpenAI-style streaming tool-call delta objects into `pending`.
///
/// Each delta may carry an `index`, an `id`, a `type`, or a `function`
/// sub-object with `name` / `arguments` fragments.  Deltas for the same index
/// are merged in arrival order.
pub fn merge_tool_call_deltas(pending: &mut Vec<ToolCall>, deltas: &[serde_json::Value]) {
    for item in deltas {
        let index = item.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        while pending.len() <= index {
            let i = pending.len();
            pending.push(ToolCall {
                id: format!("call_{i}"),
                call_type: "function".to_string(),
                function: ToolCallFunction {
                    name: String::new(),
                    arguments: String::new(),
                },
            });
        }
        if let Some(id) = item.get("id").and_then(|v| v.as_str())
            && !id.is_empty()
        {
            pending[index].id = id.to_string();
        }
        if let Some(tp) = item.get("type").and_then(|v| v.as_str())
            && !tp.is_empty()
        {
            pending[index].call_type = tp.to_string();
        }
        if let Some(func) = item.get("function") {
            if let Some(name) = func.get("name").and_then(|v| v.as_str())
                && !name.is_empty()
            {
                pending[index].function.name = name.to_string();
            }
            if let Some(args_part) = func.get("arguments").and_then(|v| v.as_str()) {
                pending[index].function.arguments.push_str(args_part);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SSE stream parser — reusable across providers
// ---------------------------------------------------------------------------

use crate::providers::StreamEvent;

/// Stateful SSE stream parser that accumulates tool-call deltas and
/// converts raw SSE chunks into `StreamEvent` values.
///
/// Used by OpenAI-compatible providers to avoid duplicated SSE parsing logic.
pub struct SseStreamParser {
    pub buffer: String,
    pub pending_tool_calls: Vec<ToolCall>,
}

impl Default for SseStreamParser {
    fn default() -> Self {
        Self::new()
    }
}

impl SseStreamParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            pending_tool_calls: Vec::new(),
        }
    }

    /// Feed raw bytes from the HTTP stream and return any parsed events.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<Result<StreamEvent>> {
        self.buffer.push_str(&String::from_utf8_lossy(chunk));
        let mut events = Vec::new();

        while let Some(newline_pos) = self.buffer.find('\n') {
            let line = self.buffer[..newline_pos].trim().to_string();
            self.buffer.drain(..=newline_pos);

            let data = match sse_data(&line) {
                Some(d) => d.to_string(),
                None => continue,
            };

            if data == "[DONE]" {
                if !self.pending_tool_calls.is_empty() {
                    let ready = std::mem::take(&mut self.pending_tool_calls);
                    events.push(Ok(StreamEvent::ToolCalls { tool_calls: ready }));
                }
                events.push(Ok(StreamEvent::Done));
                return events;
            }

            match serde_json::from_str::<serde_json::Value>(&data) {
                Ok(json) => {
                    if let Some(choices) = json["choices"].as_array()
                        && let Some(choice) = choices.first()
                    {
                        if let Some(delta) = choice.get("delta")
                            && let Some(content) = delta.get("content")
                            && let Some(content_str) = content.as_str()
                            && !content_str.is_empty()
                        {
                            events.push(Ok(StreamEvent::Delta {
                                content: content_str.to_string(),
                            }));
                        }
                        if let Some(delta) = choice.get("delta")
                            && let Some(tcs) = delta.get("tool_calls").and_then(|v| v.as_array())
                        {
                            merge_tool_call_deltas(&mut self.pending_tool_calls, tcs);
                        }
                        if choice
                            .get("finish_reason")
                            .and_then(|v| v.as_str())
                            .is_some()
                        {
                            if !self.pending_tool_calls.is_empty() {
                                let ready = std::mem::take(&mut self.pending_tool_calls);
                                events.push(Ok(StreamEvent::ToolCalls { tool_calls: ready }));
                            }
                            events.push(Ok(StreamEvent::Done));
                            return events;
                        }
                    }
                }
                Err(e) => {
                    events.push(Err(anyhow::anyhow!("Failed to parse SSE: {}", e)));
                }
            }
        }

        events
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ChatMessage, Tool, ToolFunction};

    #[test]
    fn sse_data_extracts_payload() {
        assert_eq!(sse_data("data: hello"), Some("hello"));
        assert_eq!(sse_data("data: [DONE]"), Some("[DONE]"));
        assert_eq!(sse_data("  data: trimmed  "), Some("trimmed")); // function trims input first
        assert_eq!(sse_data("event: ping"), None);
        assert_eq!(sse_data(""), None);
        assert_eq!(sse_data(": comment"), None);
    }

    #[test]
    fn message_to_json_plain() {
        let m = ChatMessage::text("user", "Hello");
        let j = message_to_json(&m);
        assert_eq!(j["role"], "user");
        assert_eq!(j["content"], "Hello");
        assert!(j.get("name").is_none() || j["name"].is_null());
    }

    #[test]
    fn tool_to_json_roundtrip() {
        let t = Tool {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "my_fn".to_string(),
                description: Some("desc".to_string()),
                parameters: None,
            },
        };
        let j = tool_to_json(&t);
        assert_eq!(j["type"], "function");
        assert_eq!(j["function"]["name"], "my_fn");
    }

    #[test]
    fn merge_deltas_accumulates_arguments() {
        let mut pending: Vec<ToolCall> = Vec::new();
        merge_tool_call_deltas(
            &mut pending,
            &[
                serde_json::json!({"index": 0, "id": "c1", "type": "function",
              "function": {"name": "f", "arguments": "{\"a\":"}}),
            ],
        );
        merge_tool_call_deltas(
            &mut pending,
            &[serde_json::json!({"index": 0, "function": {"arguments": "1}"}})],
        );
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "c1");
        assert_eq!(pending[0].function.arguments, r#"{"a":1}"#);
    }
}
