//! Data types for FireBox client SDK.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Chat & Completion
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model_id: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<Tool>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<Usage>,
    pub finish_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum StreamChunk {
    Delta(String),
    ToolCalls(Vec<ToolCall>),
    Done {
        usage: Option<Usage>,
        finish_reason: Option<String>,
    },
    Error(String),
}

// ---------------------------------------------------------------------------
// Provider Management
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub profile_id: String,
    pub display_name: String,
    pub provider_type: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthInitResponse {
    pub verification_uri: String,
    pub user_code: String,
    pub expires_in: u64,
}

// ---------------------------------------------------------------------------
// Model Management
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub model_id: String,
    pub provider_id: String,
    pub display_name: String,
    pub enabled: bool,
    pub capabilities: Vec<String>,
}

// ---------------------------------------------------------------------------
// Routing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub pattern: String,
    pub target_provider: String,
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetrics {
    pub provider_id: String,
    pub requests_count: u64,
    pub errors_count: u64,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
}

// ---------------------------------------------------------------------------
// Access Control
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowlistEntry {
    pub app_path: String,
    pub display_name: String,
}

// ---------------------------------------------------------------------------
// Embeddings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct EmbeddingRequest {
    pub model_id: String,
    pub input: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub embeddings: Vec<Vec<f64>>,
    pub usage: Option<Usage>,
}
