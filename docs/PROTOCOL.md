# Local AI Capability Protocol
- Provide a cross-platform IPC contract between the GUI app and the backend service.
- Use each platform's native IPC type system (COM / XPC / D-Bus), not a JSON-encoded envelope.
- The schema below uses a proto2-like style for readability.

## Providers
### Common Types
```proto
enum ProviderType {
  PROVIDER_TYPE_API_KEY = 1;
  PROVIDER_TYPE_OAUTH = 2;
  PROVIDER_TYPE_LOCAL = 3;
}

enum MessageRole {
  MESSAGE_ROLE_SYSTEM = 1;
  MESSAGE_ROLE_USER = 2;
  MESSAGE_ROLE_ASSISTANT = 3;
  MESSAGE_ROLE_TOOL = 4;
}

message OAuthInfo {
  required string auth_url;
  optional string state;
  optional string pkce_challenge;
  optional int64 expires_at_ms;
}

message Provider {
  required string provider_id;
  required string name;
  required ProviderType type;
  optional string base_url;
  optional string local_path;
}

message Model {
  required string model_id;
  // Intentionally optional. Unset means global/unspecified ownership.
  optional string provider_id;
  optional int32 context_window;
  required bool enabled;
  optional bool capability_chat;
  optional bool capability_tools;
  optional bool capability_vision;
  optional bool capability_embeddings;
  optional bool capability_streaming;
  optional double cost_input_per_million_tokens;
  optional double cost_output_per_million_tokens;
  optional double cost_cache_read_per_million_tokens;
  optional double cost_cache_write_per_million_tokens;
}

message RouteTarget {
  required string provider_id;
  required string model_id;
}

message RouteRule {
  // Exposed to local programs as a stable model alias.
  required string alias;
  // Backend tries targets in order for alignment and failover.
  repeated RouteTarget targets;
}

message Tool {
  required string name;
  optional string description;
  required string json_schema;
}

message ToolCall {
  required string id;
  required string name;
  required string arguments_json;
}

message Message {
  required MessageRole role;
  optional string content;
  optional string name;
  optional string tool_call_id;
  repeated ToolCall tool_calls;
}

message Usage {
  optional int32 prompt_tokens;
  optional int32 completion_tokens;
  optional int32 total_tokens;
}

message Embedding {
  optional string chunk_id;
  optional string text;
  repeated float vector;
}

message MetricsSnapshot {
  optional int64 window_start_ms;
  optional int64 window_end_ms;
  optional int64 requests_total;
  optional int64 requests_failed;
  optional int64 prompt_tokens_total;
  optional int64 completion_tokens_total;
  optional double cost_total;
}

message Result {
  required bool success;
  optional string message;
}

```

### List Providers
Returns the provider list, optionally filtered by provider name.
```proto
message ListProvidersRequest {
  optional string name_filter;
}

message ListProvidersResponse {
  required Result result;
  repeated Provider providers;
}
```

### Get Provider
Returns one provider by `provider_id`.
```proto
message GetProviderRequest {
  required string provider_id;
}

message GetProviderResponse {
  required Result result;
  optional Provider provider;
}
```

### Add Provider
Creates a provider and returns its id. OAuth providers may return extra auth info.
```proto
message AddProviderRequest {
  required string name;
  required ProviderType type;
  optional string api_key;
  optional string base_url;
  optional string local_path;
}

message AddProviderResponse {
  required Result result;
  optional string provider_id;
  optional OAuthInfo oauth_info;
}
```

### Delete Provider
Deletes one provider by `provider_id`.
```proto
message DeleteProviderRequest {
  required string provider_id;
}

message DeleteProviderResponse {
  required Result result;
}
```

### Get Models
Returns the full model list, optionally filtered by model name.
```proto
message GetModelsRequest {
  optional string name_filter;
}

message GetModelsResponse {
  required Result result;
  repeated Model models;
}
```

### Set Models
Fully replaces the model list with the provided items.
```proto
// Full replace: this request overwrites the entire model list.
message SetModelsRequest {
  repeated Model models;
}

message SetModelsResponse {
  required Result result;
}
```

## Route
Local programs call a user-defined `alias`. The backend forwards
requests to `RouteRule.targets` in order until one succeeds. This supports
cross-provider model id alignment and fallback/downgrade.
`SetRouteRules` only applies to aliases that exist in alias list. Removing an
alias from alias list also removes its route rules.

### Set Alias List
Fully replaces the alias list used by local programs.
```proto
// Full replace: this request overwrites the entire alias list.
message SetAliasListRequest {
  repeated string aliases;
}

message SetAliasListResponse {
  required Result result;
}
```

### Get Alias List
Returns all aliases currently available for routing.
```proto
message GetAliasListRequest {
}

message GetAliasListResponse {
  required Result result;
  repeated string aliases;
}
```

### Set Route Rules
Sets the ordered routing targets for one alias.
```proto
message SetRouteRulesRequest {
  required string alias;
  // Must contain at least 1 target.
  repeated RouteTarget targets;
}

message SetRouteRulesResponse {
  required Result result;
}
```

### Get Route Rules
Returns routing targets for one alias.
```proto
message GetRouteRulesRequest {
  required string alias;
}

message GetRouteRulesResponse {
  required Result result;
  optional RouteRule route_rule;
}
```

## Sessions
### Complete Chat
Runs one chat completion using an alias as `model_id`.
```proto
message CompleteChatRequest {
  // Must be an alias defined by route rules.
  required string model_id;
  repeated Message messages;
  repeated Tool tools;
}

message CompleteChatResponse {
  required Result result;
  optional Message completion;
  optional Usage usage;
}
```

### Create Stream
Creates a chat message stream channel.
```proto
message CreateStreamRequest {
  // Must be an alias defined by route rules.
  required string model_id;
}

message CreateStreamResponse {
  required Result result;
  optional string stream_id;
}
```

### Ask Stream
Appends one message into an existing stream.
Client must always send full `tools` state in every call (can be empty).
Server must not reuse tools from previous turns when omitted.
```proto
message AskStreamRequest {
  required string stream_id;
  required Message message;
  // Full tools state for this turn, not a delta.
  // Empty list means clear all tools.
  repeated Tool tools;
}

message AskStreamResponse {
  required Result result;
}
```

### Reply Stream
Fetches one new reply chunk from a stream.
If both `chunk` and `tool_call` exist, process `chunk` first.
```proto
message ReplyStreamRequest {
  required string stream_id;
  // Block for at most wait_ms when no new chunk is available.
  optional int32 wait_ms;
}

message ReplyStreamResponse {
  required Result result;
  // Assistant text/message delta chunk.
  optional Message chunk;
  // Assistant tool call chunk.
  optional ToolCall tool_call;
  optional bool done;
  optional Usage usage;
}
```

### Cancel Stream
Cancels an existing stream and releases server-side resources.
```proto
message CancelStreamRequest {
  required string stream_id;
}

message CancelStreamResponse {
  required Result result;
}
```

### Embed Content
Embeds a local file and returns generated vectors.
```proto
message EmbedContentRequest {
  required string file_path;
}

message EmbedContentResponse {
  required Result result;
  repeated Embedding embeddings;
}
```

## Metrics
### Get Metrics Snapshot
Returns the latest aggregated metrics snapshot for dashboard display.
```proto
message GetMetricsSnapshotRequest {
}

message GetMetricsSnapshotResponse {
  required Result result;
  optional MetricsSnapshot snapshot;
}
```

### Get Metrics Range
Returns aggregated metrics in a time range for charts and trend analysis.
```proto
message GetMetricsRangeRequest {
  required int64 start_ms;
  required int64 end_ms;
}

message GetMetricsRangeResponse {
  required Result result;
  repeated MetricsSnapshot snapshots;
}
```
