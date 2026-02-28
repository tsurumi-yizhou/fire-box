#pragma once

// ---------------------------------------------------------------------------
// dbus_client.hpp — D-Bus client for the FireBox Linux GUI
//
// Connects to the FireBox Rust service over the session bus using sdbus-c++.
// All control-plane commands go through the single generic `Invoke(cmd, json)`
// D-Bus method.  Responses are parsed with json-glib.
// ---------------------------------------------------------------------------

#include <sdbus-c++/sdbus-c++.h>
#include <json-glib/json-glib.h>

#include <cstdint>
#include <memory>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

struct ProviderInfo {
    std::string id;
    std::string name;
    std::string base_url;
    int         type;
};

struct MetricsSnapshot {
    int64_t requests_total;
    int64_t requests_failed;
    int64_t prompt_tokens;
    int64_t completion_tokens;
    int64_t latency_avg_ms;
    double  cost_total;
};

struct ConnectionInfo {
    std::string connection_id;
    std::string client_name;
    std::string app_path;
    int64_t     requests_count;
    int64_t     connected_at_ms;
};

struct RouteRuleInfo {
    std::string virtual_model_id;
    std::string display_name;
    std::string strategy;
    // Each pair: (provider_id, model_id)
    std::vector<std::pair<std::string, std::string>> targets;
};

struct AllowlistEntry {
    std::string app_path;
    std::string display_name;
};

struct OAuthChallenge {
    std::string device_code;
    std::string user_code;
    std::string verification_uri;
    int64_t     expires_in;
    int64_t     interval;
};

// ---------------------------------------------------------------------------
// RAII wrapper for GLib-based JsonParser
// ---------------------------------------------------------------------------

namespace detail {

class ScopedJsonParser {
public:
    ScopedJsonParser() : parser_(json_parser_new()) {}

    ~ScopedJsonParser() {
        if (parser_) {
            g_object_unref(parser_);
        }
    }

    ScopedJsonParser(const ScopedJsonParser&)            = delete;
    ScopedJsonParser& operator=(const ScopedJsonParser&) = delete;

    ScopedJsonParser(ScopedJsonParser&& o) noexcept : parser_(o.parser_) {
        o.parser_ = nullptr;
    }

    ScopedJsonParser& operator=(ScopedJsonParser&& o) noexcept {
        if (this != &o) {
            if (parser_) g_object_unref(parser_);
            parser_   = o.parser_;
            o.parser_ = nullptr;
        }
        return *this;
    }

    /// Parse a JSON string.  Throws std::runtime_error on failure.
    void load(const std::string& json) {
        GError* err = nullptr;
        if (!json_parser_load_from_data(parser_, json.c_str(),
                                        static_cast<gssize>(json.size()), &err)) {
            std::string msg = err ? err->message : "unknown JSON parse error";
            if (err) g_error_free(err);
            throw std::runtime_error("JSON parse error: " + msg);
        }
    }

    /// Return the root JsonNode* (owned by the parser — do NOT free).
    [[nodiscard]] JsonNode* root() const {
        return json_parser_get_root(parser_);
    }

private:
    JsonParser* parser_;
};

// ---------------------------------------------------------------------------
// json-glib convenience helpers
// ---------------------------------------------------------------------------

inline const char* obj_get_string(JsonObject* obj, const char* key) {
    if (!json_object_has_member(obj, key)) return "";
    JsonNode* node = json_object_get_member(obj, key);
    if (json_node_is_null(node)) return "";
    return json_node_get_string(node);
}

inline int64_t obj_get_int(JsonObject* obj, const char* key) {
    if (!json_object_has_member(obj, key)) return 0;
    return json_object_get_int_member(obj, key);
}

inline double obj_get_double(JsonObject* obj, const char* key) {
    if (!json_object_has_member(obj, key)) return 0.0;
    return json_object_get_double_member(obj, key);
}

inline bool obj_get_bool(JsonObject* obj, const char* key) {
    if (!json_object_has_member(obj, key)) return false;
    return json_object_get_boolean_member(obj, key) != FALSE;
}

inline JsonObject* obj_get_object(JsonObject* obj, const char* key) {
    if (!json_object_has_member(obj, key)) return nullptr;
    JsonNode* node = json_object_get_member(obj, key);
    if (!JSON_NODE_HOLDS_OBJECT(node)) return nullptr;
    return json_node_get_object(node);
}

inline JsonArray* obj_get_array(JsonObject* obj, const char* key) {
    if (!json_object_has_member(obj, key)) return nullptr;
    JsonNode* node = json_object_get_member(obj, key);
    if (!JSON_NODE_HOLDS_ARRAY(node)) return nullptr;
    return json_node_get_array(node);
}

} // namespace detail

// ---------------------------------------------------------------------------
// FireBoxDbusClient
// ---------------------------------------------------------------------------

class FireBoxDbusClient {
public:
    static constexpr const char* BUS_NAME    = "com.firebox.Service";
    static constexpr const char* OBJECT_PATH = "/com/firebox/Service";
    static constexpr const char* INTERFACE   = "com.firebox.Service";

    FireBoxDbusClient()
        : connection_(sdbus::createSessionBusConnection())
        , proxy_(sdbus::createProxy(*connection_, BUS_NAME, OBJECT_PATH))
    {
        connection_->enterEventLoopAsync();
    }

    ~FireBoxDbusClient() {
        // Stop the async event loop when the client is destroyed, preventing
        // the background thread from accessing freed proxy/connection objects.
        if (connection_) {
            connection_->leaveEventLoop();
        }
    }

    FireBoxDbusClient(const FireBoxDbusClient&)            = delete;
    FireBoxDbusClient& operator=(const FireBoxDbusClient&) = delete;

    // -----------------------------------------------------------------------
    // Generic invoke — calls the D-Bus Invoke(cmd, payload_json) method.
    //
    // The Rust service returns (bool success, string response_json).
    // On error (D-Bus failure or success == false) throws std::runtime_error.
    // On success returns a JsonNode* for the "body" field.  The caller must
    // g_object_unref() the returned parser via the out-parameter, or — more
    // conveniently — use the typed helpers below which handle lifetime.
    // -----------------------------------------------------------------------

    JsonNode* invoke(const std::string& cmd,
                     const std::string& payload_json = "{}") {
        bool        dbus_ok{};
        std::string response_json;

        proxy_->callMethod("Invoke")
              .onInterface(INTERFACE)
              .withArguments(cmd, payload_json)
              .storeResultsTo(dbus_ok, response_json);

        // Parse the full JSON envelope
        auto& parser = ensure_parser();
        parser.load(response_json);

        JsonNode* root = parser.root();
        if (!root || !JSON_NODE_HOLDS_OBJECT(root)) {
            throw std::runtime_error("invoke(" + cmd + "): response is not a JSON object");
        }

        JsonObject* envelope = json_node_get_object(root);

        bool success = detail::obj_get_bool(envelope, "success");
        if (!success) {
            const char* msg = detail::obj_get_string(envelope, "message");
            throw std::runtime_error(
                std::string("invoke(") + cmd + ") failed: " +
                (msg && msg[0] ? msg : "unknown error"));
        }

        // Return the "body" node (may be null for void-returning commands)
        if (!json_object_has_member(envelope, "body")) {
            return nullptr;
        }
        return json_object_get_member(envelope, "body");
    }

    // -----------------------------------------------------------------------
    // Typed convenience methods
    // -----------------------------------------------------------------------

    /// Ping the service.  Returns true if the service responds.
    bool ping() {
        invoke("ping");
        return true;
    }

    /// List all configured providers.
    std::vector<ProviderInfo> list_providers() {
        JsonNode* body = invoke("list_providers");
        std::vector<ProviderInfo> result;

        if (!body || !JSON_NODE_HOLDS_OBJECT(body)) return result;
        JsonObject* obj = json_node_get_object(body);
        JsonArray* arr  = detail::obj_get_array(obj, "providers");
        if (!arr) return result;

        guint len = json_array_get_length(arr);
        result.reserve(len);
        for (guint i = 0; i < len; ++i) {
            JsonObject* entry = json_array_get_object_element(arr, i);
            ProviderInfo info;
            info.id       = detail::obj_get_string(entry, "provider_id");
            info.name     = detail::obj_get_string(entry, "name");
            info.type     = static_cast<int>(detail::obj_get_int(entry, "type"));
            info.base_url = detail::obj_get_string(entry, "base_url");
            result.push_back(std::move(info));
        }
        return result;
    }

    /// Get a snapshot of current metrics.
    MetricsSnapshot get_metrics_snapshot() {
        JsonNode* body = invoke("get_metrics_snapshot");

        MetricsSnapshot snap{};
        if (!body || !JSON_NODE_HOLDS_OBJECT(body)) return snap;
        JsonObject* obj      = json_node_get_object(body);
        JsonObject* snap_obj = detail::obj_get_object(obj, "snapshot");
        if (!snap_obj) return snap;

        snap.requests_total   = detail::obj_get_int(snap_obj, "requests_total");
        snap.requests_failed  = detail::obj_get_int(snap_obj, "requests_failed");
        snap.prompt_tokens    = detail::obj_get_int(snap_obj, "prompt_tokens_total");
        snap.completion_tokens = detail::obj_get_int(snap_obj, "completion_tokens_total");
        snap.latency_avg_ms   = detail::obj_get_int(snap_obj, "latency_avg_ms");
        snap.cost_total       = detail::obj_get_double(snap_obj, "cost_total");
        return snap;
    }

    /// List active client connections.
    std::vector<ConnectionInfo> list_connections() {
        JsonNode* body = invoke("list_connections");
        std::vector<ConnectionInfo> result;

        if (!body || !JSON_NODE_HOLDS_OBJECT(body)) return result;
        JsonObject* obj = json_node_get_object(body);
        JsonArray* arr  = detail::obj_get_array(obj, "connections");
        if (!arr) return result;

        guint len = json_array_get_length(arr);
        result.reserve(len);
        for (guint i = 0; i < len; ++i) {
            JsonObject* entry = json_array_get_object_element(arr, i);
            ConnectionInfo ci;
            ci.connection_id  = detail::obj_get_string(entry, "connection_id");
            ci.client_name    = detail::obj_get_string(entry, "client_name");
            ci.app_path       = detail::obj_get_string(entry, "app_path");
            ci.requests_count = detail::obj_get_int(entry, "requests_count");
            ci.connected_at_ms = detail::obj_get_int(entry, "connected_at_ms");
            result.push_back(std::move(ci));
        }
        return result;
    }

    /// Get all route rules.
    std::vector<RouteRuleInfo> get_route_rules() {
        JsonNode* body = invoke("get_route_rules");
        std::vector<RouteRuleInfo> result;

        if (!body || !JSON_NODE_HOLDS_OBJECT(body)) return result;
        JsonObject* obj = json_node_get_object(body);
        JsonArray* arr  = detail::obj_get_array(obj, "rules");
        if (!arr) return result;

        guint len = json_array_get_length(arr);
        result.reserve(len);
        for (guint i = 0; i < len; ++i) {
            JsonObject* entry = json_array_get_object_element(arr, i);
            result.push_back(parse_route_rule(entry));
        }
        return result;
    }

    /// Get the allowlist of approved applications.
    std::vector<AllowlistEntry> get_allowlist() {
        JsonNode* body = invoke("get_allowlist");
        std::vector<AllowlistEntry> result;

        if (!body || !JSON_NODE_HOLDS_OBJECT(body)) return result;
        JsonObject* obj = json_node_get_object(body);
        JsonArray* arr  = detail::obj_get_array(obj, "apps");
        if (!arr) return result;

        guint len = json_array_get_length(arr);
        result.reserve(len);
        for (guint i = 0; i < len; ++i) {
            JsonObject* entry = json_array_get_object_element(arr, i);
            AllowlistEntry ae;
            ae.app_path     = detail::obj_get_string(entry, "app_path");
            ae.display_name = detail::obj_get_string(entry, "display_name");
            result.push_back(std::move(ae));
        }
        return result;
    }

    // -----------------------------------------------------------------------
    // Provider management
    // -----------------------------------------------------------------------

    /// Add an API-key-based provider.  Returns the provider_id.
    std::string add_api_key_provider(const std::string& name,
                                     const std::string& provider_type,
                                     const std::string& api_key,
                                     const std::string& base_url = "") {
        std::string payload = build_json({
            {"name",          quote(name)},
            {"provider_type", quote(provider_type)},
            {"api_key",       quote(api_key)},
            {"base_url",      quote(base_url)},
        });

        JsonNode* body = invoke("add_api_key_provider", payload);
        if (!body || !JSON_NODE_HOLDS_OBJECT(body)) {
            throw std::runtime_error("add_api_key_provider: empty body");
        }
        return detail::obj_get_string(json_node_get_object(body), "provider_id");
    }

    /// Start an OAuth device flow.  Returns challenge info and the provider_id
    /// is embedded in the body (caller can read it from the OAuthChallenge or
    /// the returned pair).
    std::pair<std::string, OAuthChallenge>
    add_oauth_provider(const std::string& name,
                       const std::string& provider_type) {
        std::string payload = build_json({
            {"name",          quote(name)},
            {"provider_type", quote(provider_type)},
        });

        JsonNode* body = invoke("add_oauth_provider", payload);
        if (!body || !JSON_NODE_HOLDS_OBJECT(body)) {
            throw std::runtime_error("add_oauth_provider: empty body");
        }
        JsonObject* obj = json_node_get_object(body);
        std::string provider_id = detail::obj_get_string(obj, "provider_id");

        OAuthChallenge ch{};
        JsonObject* challenge = detail::obj_get_object(obj, "challenge");
        if (challenge) {
            ch.device_code      = detail::obj_get_string(challenge, "device_code");
            ch.user_code        = detail::obj_get_string(challenge, "user_code");
            ch.verification_uri = detail::obj_get_string(challenge, "verification_uri");
            ch.expires_in       = detail::obj_get_int(challenge, "expires_in");
            ch.interval         = detail::obj_get_int(challenge, "interval");
        }
        return {std::move(provider_id), std::move(ch)};
    }

    /// Complete an ongoing OAuth device flow (blocks until the user authorises
    /// or the flow times out on the service side).
    void complete_oauth(const std::string& provider_id) {
        std::string payload = build_json({
            {"provider_id", quote(provider_id)},
        });
        invoke("complete_oauth", payload);
    }

    /// Add a local (llama.cpp) provider.  Returns the provider_id.
    std::string add_local_provider(const std::string& name,
                                   const std::string& model_path) {
        std::string payload = build_json({
            {"name",       quote(name)},
            {"model_path", quote(model_path)},
        });

        JsonNode* body = invoke("add_local_provider", payload);
        if (!body || !JSON_NODE_HOLDS_OBJECT(body)) {
            throw std::runtime_error("add_local_provider: empty body");
        }
        return detail::obj_get_string(json_node_get_object(body), "provider_id");
    }

    /// Delete a provider by its id.
    void delete_provider(const std::string& provider_id) {
        std::string payload = build_json({
            {"provider_id", quote(provider_id)},
        });
        invoke("delete_provider", payload);
    }

    // -----------------------------------------------------------------------
    // Routing
    // -----------------------------------------------------------------------

    /// Set (or create) a route rule.
    void set_route_rules(const std::string& virtual_model_id,
                         const std::string& display_name,
                         const std::string& strategy,
                         const std::string& provider_id,
                         const std::string& model_id) {
        // Build the targets array with a single entry
        std::string targets_json =
            "[{\"provider_id\":" + quote(provider_id) +
            ",\"model_id\":"    + quote(model_id) + "}]";

        std::string payload =
            "{\"virtual_model_id\":" + quote(virtual_model_id) +
            ",\"display_name\":"     + quote(display_name) +
            ",\"strategy\":"         + quote(strategy) +
            ",\"targets\":"          + targets_json + "}";

        invoke("set_route_rules", payload);
    }

    /// Delete a route by its virtual model id.
    void delete_route(const std::string& virtual_model_id) {
        std::string payload = build_json({
            {"virtual_model_id", quote(virtual_model_id)},
        });
        invoke("delete_route", payload);
    }

    // -----------------------------------------------------------------------
    // Allowlist
    // -----------------------------------------------------------------------

    /// Remove an application from the allowlist.
    void remove_from_allowlist(const std::string& app_path) {
        std::string payload = build_json({
            {"app_path", quote(app_path)},
        });
        invoke("remove_from_allowlist", payload);
    }

private:
    std::unique_ptr<sdbus::IConnection> connection_;
    std::unique_ptr<sdbus::IProxy>      proxy_;

    // The parser is kept alive so that JsonNode* pointers returned from
    // invoke() remain valid until the next invoke() call.
    detail::ScopedJsonParser parser_;

    detail::ScopedJsonParser& ensure_parser() {
        // Re-create for each call so previous data is released
        parser_ = detail::ScopedJsonParser();
        return parser_;
    }

    // -----------------------------------------------------------------------
    // JSON construction helpers (no external dependency needed)
    // -----------------------------------------------------------------------

    /// Escape a string for embedding in JSON.
    static std::string escape(const std::string& s) {
        std::string out;
        out.reserve(s.size() + 8);
        for (char c : s) {
            switch (c) {
                case '\"': out += "\\\""; break;
                case '\\': out += "\\\\"; break;
                case '\b': out += "\\b";  break;
                case '\f': out += "\\f";  break;
                case '\n': out += "\\n";  break;
                case '\r': out += "\\r";  break;
                case '\t': out += "\\t";  break;
                default:
                    if (static_cast<unsigned char>(c) < 0x20) {
                        char buf[8];
                        std::snprintf(buf, sizeof(buf), "\\u%04x",
                                      static_cast<unsigned>(c));
                        out += buf;
                    } else {
                        out += c;
                    }
                    break;
            }
        }
        return out;
    }

    /// Quote and escape a string value for JSON.
    static std::string quote(const std::string& s) {
        return "\"" + escape(s) + "\"";
    }

    /// Build a flat JSON object from key-value pairs where values are already
    /// JSON-encoded (e.g. quoted strings, numbers, arrays).
    static std::string build_json(
        std::initializer_list<std::pair<const char*, std::string>> fields) {
        std::string out = "{";
        bool first = true;
        for (auto& [key, val] : fields) {
            if (!first) out += ',';
            first = false;
            out += "\"";
            out += key;
            out += "\":";
            out += val;
        }
        out += '}';
        return out;
    }

    // -----------------------------------------------------------------------
    // Route rule parser
    // -----------------------------------------------------------------------

    static RouteRuleInfo parse_route_rule(JsonObject* entry) {
        RouteRuleInfo ri;
        ri.virtual_model_id = detail::obj_get_string(entry, "virtual_model_id");
        ri.display_name     = detail::obj_get_string(entry, "display_name");
        ri.strategy         = detail::obj_get_string(entry, "strategy");

        JsonArray* targets = detail::obj_get_array(entry, "targets");
        if (targets) {
            guint tlen = json_array_get_length(targets);
            ri.targets.reserve(tlen);
            for (guint j = 0; j < tlen; ++j) {
                JsonObject* te = json_array_get_object_element(targets, j);
                std::string pid = detail::obj_get_string(te, "provider_id");
                std::string mid = detail::obj_get_string(te, "model_id");
                ri.targets.emplace_back(std::move(pid), std::move(mid));
            }
        }
        return ri;
    }
};
