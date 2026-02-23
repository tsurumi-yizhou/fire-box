# FireBox Management Protocol

This document provides a comprehensive specification of the Inter-Process Communication (IPC) protocol interface designed to enable the FireBox frontend application to configure, monitor, and administer the backend service. It is crucial to distinguish this management protocol from the Service Protocol, which serves a fundamentally different purpose by enabling client applications to consume artificial intelligence capabilities. The management protocol, in contrast, focuses exclusively on administrative and configuration functions.

## Common Type Definitions

The protocol employs a set of common data structures that facilitate consistent communication between the frontend application and the backend service. These type definitions establish a shared vocabulary for representing providers, models, metrics, and authentication credentials.

```proto
enum ProviderType {
  PROVIDER_TYPE_API_KEY = 1;    // OpenAI, Anthropic, Ollama, vLLM
  PROVIDER_TYPE_OAUTH = 2;      // GitHub Copilot, DashScope
  PROVIDER_TYPE_LOCAL = 3;      // llama.cpp
}

message Result {
  required bool success;
  optional string message;
}

message Provider {
  required string provider_id;
  required string name;
  required ProviderType type;
  optional string base_url;
  optional string local_path;
  required bool enabled;
}

message Model {
  required string model_id;
  required string provider_id;
  required bool enabled;
  optional bool capability_chat;
  optional bool capability_streaming;
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

message OAuthChallenge {
  required string device_code;
  required string user_code;
  required string verification_uri;
  required int32 expires_in;
  required int32 interval;
}

message OAuthCredentials {
  required string access_token;
  optional string refresh_token;
  required int64 expiry_date_ms;
  optional string resource_url;
}
```

## Provider Management Operations

### Add API Key Provider

This operation facilitates the integration of API key-based providers into the FireBox service. Such providers, which include prominent services such as OpenAI, Anthropic, Ollama, and vLLM, authenticate through the presentation of static API keys. It is noteworthy that certain providers, such as Ollama, may operate without requiring API key authentication, in which case the API key parameter may be omitted.

```proto
message AddApiKeyProviderRequest {
  required string name;
  required string provider_type;    // "openai", "anthropic", "ollama", "vllm"
  optional string api_key;          // Empty for Ollama
  optional string base_url;
}

message AddApiKeyProviderResponse {
  required Result result;
  optional string provider_id;
}
```

### Add OAuth Provider

This operation facilitates the integration of OAuth-based providers, such as GitHub Copilot and DashScope, into the FireBox service. Unlike API key-based providers, OAuth providers require a multi-step authentication flow to obtain access tokens. The process comprises two distinct steps: initiating the OAuth flow and completing the authentication.

**Step 1: Start OAuth**

```proto
message AddOAuthProviderRequest {
  required string name;
  required string provider_type;    // "copilot", "dashscope"
}

message AddOAuthProviderResponse {
  required Result result;
  optional string provider_id;
  optional OAuthChallenge challenge;
}
```

**Step 2: Complete OAuth**

```proto
message CompleteOAuthRequest {
  required string provider_id;
}

message CompleteOAuthResponse {
  required Result result;
  optional OAuthCredentials credentials;
}
```

### Add Local Provider

This operation enables the integration of local model providers that execute entirely on the user's system without requiring external network connectivity. The llama.cpp provider exemplifies this category, operating by managing a local server process that serves models from the local filesystem.

```proto
message AddLocalProviderRequest {
  required string name;
  required string model_path;
}

message AddLocalProviderResponse {
  required Result result;
  optional string provider_id;
}
```

### List Providers

This operation retrieves a comprehensive enumeration of all providers currently configured within the FireBox service, regardless of their operational status. The returned information includes provider identifiers, names, types, and enablement status.

```proto
message ListProvidersRequest {
}

message ListProvidersResponse {
  required Result result;
  repeated Provider providers;
}
```

### Delete Provider

This operation facilitates the removal of a provider from the service configuration. Upon removal, all associated models and routing rules referencing the provider are also eliminated, ensuring consistency within the system configuration.

```proto
message DeleteProviderRequest {
  required string provider_id;
}

message DeleteProviderResponse {
  required Result result;
}
```

## Model Configuration Operations

### Get All Models (Admin)

This operation retrieves a comprehensive enumeration of all models available across all configured providers, irrespective of any routing rules that may be in place. This administrative view enables complete visibility into the model inventory.

```proto
message GetAllModelsRequest {
  optional string provider_id;
}

message GetAllModelsResponse {
  required Result result;
  repeated Model models;
}
```

### Set Model Enabled

This operation enables administrators to selectively activate or deactivate individual models, providing granular control over which models are available for client applications to utilize. Disabling a model prevents it from being selected during request routing whilst preserving its configuration.

```proto
message SetModelEnabledRequest {
  required string provider_id;
  required string model_id;
  required bool enabled;
}

message SetModelEnabledResponse {
  required Result result;
}
```

## Routing Configuration Operations

### Set Route Rules

This operation establishes routing rules that map virtual model identifiers (aliases) to one or more physical provider-model combinations. The routing system supports multiple strategies, including failover (attempting targets sequentially) and random selection (distributing load across targets). This abstraction layer enables client applications to reference models by stable aliases whilst the underlying provider configuration may change.

```proto
message RouteTarget {
  required string provider_id;
  required string model_id;
}

enum RouteStrategy {
  ROUTE_STRATEGY_FAILOVER = 1; // Try targets in order
  ROUTE_STRATEGY_RANDOM = 2;   // Randomly select target
}

message SetRouteRulesRequest {
  required string alias; // e.g., "gpt-4", "claude-3"
  repeated RouteTarget targets;
  optional RouteStrategy strategy; // Defaults to FAILOVER
}

message SetRouteRulesResponse {
  required Result result;
}
```

### Get Route Rules

This operation retrieves the routing configuration for a specified virtual model identifier (alias), returning the ordered list of target providers and models along with the routing strategy employed.

```proto
message GetRouteRulesRequest {
  required string alias;
}

message GetRouteRulesResponse {
  required Result result;
  repeated RouteTarget targets;
  optional RouteStrategy strategy;
}
```

## Metrics and Monitoring Operations

### Get Metrics Snapshot

This operation retrieves the current aggregated performance and usage metrics, providing a real-time view of system activity. The metrics encompass request volumes, token consumption statistics, cost calculations, and error rates.

```proto
message GetMetricsSnapshotRequest {
}

message GetMetricsSnapshotResponse {
  required Result result;
  optional MetricsSnapshot snapshot;
}
```

### Get Metrics Range

This operation retrieves historical metrics data for a specified time range, enabling administrators to analyze trends and patterns in system usage over time. The returned data comprises a series of metric snapshots covering the requested temporal interval.

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

## Connection Management

### List Connections

```proto
message Connection {
  required string connection_id;
  required string client_name;      // e.g. "VS Code", "curl"
  optional int64 requests_count;
}

message ListConnectionsRequest {
}

message ListConnectionsResponse {
  required Result result;
  repeated Connection connections;
}
```

## Access Control (Revocation)

### Get Allowlist

Retrieve the list of applications currently authorized to use the service.

```proto
message AllowedApp {
  required string app_path;
  required string display_name;
  required int64 first_seen_ms;
  required int64 last_used_ms;
}

message GetAllowlistRequest {
}

message GetAllowlistResponse {
  required Result result;
  repeated AllowedApp apps;
}
```

### Remove From Allowlist

This operation revokes access privileges for a specified application by removing its entry from the persistent allowlist. Following this revocation, should the application subsequently attempt to establish a connection, the system will initiate the user approval workflow, thereby requiring explicit re-authorization.

```proto
message RemoveFromAllowlistRequest {
  required string app_path;
}

message RemoveFromAllowlistResponse {
  required Result result;
}
```

