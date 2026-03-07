#pragma once
/// @file dbus_types.hpp
/// Common D-Bus type mapping helpers and structures shared across
/// the backend adaptor, frontend proxy, and client SDK proxy.

#include <cstdint>
#include <string>
#include <vector>

namespace firebox {

// ── Capability flags (maps to D-Bus struct (bbbbb)) ──────────────
struct ModelCapabilities {
    bool chat         = true;
    bool streaming    = true;
    bool embeddings   = false;
    bool vision       = false;
    bool tool_calling = false;
};

// ── Model metadata ───────────────────────────────────────────────
struct ModelMetadata {
    int32_t context_window = 0;
    std::string pricing_tier;
    std::string description;
    std::vector<std::string> strengths;
};

// ── ServiceModel — exposed to clients ────────────────────────────
struct ServiceModel {
    std::string id;
    std::string display_name;
    ModelCapabilities capabilities;
    ModelMetadata metadata;
};

// ── Provider types ───────────────────────────────────────────────
enum class ProviderType : int32_t {
    ApiKey = 1,
    OAuth  = 2,
    Local  = 3,
};

struct Provider {
    std::string provider_id;
    std::string name;
    ProviderType type;
    std::string subtype;   ///< Provider subtype: "openai", "anthropic", "gemini", etc.
    std::string base_url;
    std::string local_path;
    bool enabled = true;
};

// ── Model (admin view) ──────────────────────────────────────────
struct Model {
    std::string model_id;
    std::string provider_id;
    bool enabled = true;
    bool capability_chat = true;
    bool capability_streaming = true;
};

// ── Metrics ──────────────────────────────────────────────────────
struct MetricsSnapshot {
    int64_t window_start_ms  = 0;
    int64_t window_end_ms    = 0;
    int64_t requests_total   = 0;
    int64_t requests_failed  = 0;
    int64_t prompt_tokens    = 0;
    int64_t completion_tokens = 0;
    double  cost_total       = 0.0;
};

// ── Route types ──────────────────────────────────────────────────
struct RouteTarget {
    std::string provider_id;
    std::string model_id;
};

enum class RouteStrategy : int32_t {
    Failover = 1,
    Random   = 2,
};

struct RouteRule {
    std::string virtual_model_id;
    std::string display_name;
    ModelCapabilities capabilities;
    ModelMetadata metadata;
    std::vector<RouteTarget> targets;
    RouteStrategy strategy = RouteStrategy::Failover;
};

// ── Connection info ──────────────────────────────────────────────
struct ConnectionInfo {
    std::string connection_id;
    std::string client_name;
    int32_t pid = 0;
    int64_t connected_at_ms = 0;
    int64_t requests_count  = 0;
};

// ── Allowlist ────────────────────────────────────────────────────
struct AllowedApp {
    std::string app_path;
    std::string display_name;
    int64_t first_seen_ms = 0;
    int64_t last_used_ms  = 0;
};

// ── Token usage ──────────────────────────────────────────────────
struct Usage {
    int32_t prompt_tokens     = 0;
    int32_t completion_tokens = 0;
    int32_t total_tokens      = 0;
};

// ── OAuth types ──────────────────────────────────────────────────
struct OAuthChallenge {
    std::string device_code;
    std::string user_code;
    std::string verification_uri;
    int32_t expires_in = 0;
    int32_t interval   = 0;
};

enum class OAuthStatus : int32_t {
    Pending      = 1,
    Success      = 2,
    Expired      = 3,
    Denied       = 4,
    NetworkError = 5,
};

} // namespace firebox
