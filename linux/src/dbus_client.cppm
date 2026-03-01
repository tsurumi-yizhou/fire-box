module;

#include <sdbus-c++/sdbus-c++.h>
#include <json-glib/json-glib.h>
#include <gtk/gtk.h>

#include <coroutine>
#include <cstdint>
#include <memory>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>
#include <tuple>

export module dbus_client;

// ---------------------------------------------------------------------------
// Basic Task type for coroutines
// ---------------------------------------------------------------------------

export struct Task {
    struct promise_type {
        Task get_return_object() { return {}; }
        std::suspend_never initial_suspend() { return {}; }
        std::suspend_never final_suspend() noexcept { return {}; }
        void return_void() {}
        void unhandled_exception() {
            // Ideally we'd log this, but GTK will typically catch it
            // if we are on the main loop.
        }
    };
};

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

export struct ProviderInfo {
    std::string id;
    std::string name;
    std::string base_url;
    int         type;
};

export struct MetricsSnapshot {
    int64_t requests_total;
    int64_t requests_failed;
    int64_t prompt_tokens;
    int64_t completion_tokens;
    int64_t latency_avg_ms;
    double  cost_total;
};

export struct ConnectionInfo {
    std::string connection_id;
    std::string client_name;
    std::string app_path;
    int64_t     requests_count;
    int64_t     connected_at_ms;
};

export struct RouteRuleInfo {
    std::string virtual_model_id;
    std::string display_name;
    std::string strategy;
    std::vector<std::pair<std::string, std::string>> targets;
};

export struct AllowlistEntry {
    std::string app_path;
    std::string display_name;
};

export struct OAuthChallenge {
    std::string device_code;
    std::string user_code;
    std::string verification_uri;
    int64_t     expires_in;
    int64_t     interval;
};

// ---------------------------------------------------------------------------
// Helpers
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

    void load(const std::string& json) {
        GError* err = nullptr;
        if (!json_parser_load_from_data(parser_, json.c_str(),
                                        static_cast<gssize>(json.size()), &err)) {
            std::string msg = err ? err->message : "unknown JSON parse error";
            if (err) g_error_free(err);
            throw std::runtime_error("JSON parse error: " + msg);
        }
    }

    [[nodiscard]] JsonNode* root() const {
        return json_parser_get_root(parser_);
    }

private:
    JsonParser* parser_;
};

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

struct PendingCall {
    std::coroutine_handle<> handle;
    std::string result_json;
    bool success;
    std::exception_ptr error;
};

// Dispatches the coroutine resumption back to the GTK main thread
gboolean resume_coroutine_on_main_thread(gpointer user_data) {
    auto handle = std::coroutine_handle<>::from_address(user_data);
    if (handle && !handle.done()) {
        handle.resume();
    }
    return G_SOURCE_REMOVE;
}

} // namespace detail

// ---------------------------------------------------------------------------
// DBus Awaiter
// ---------------------------------------------------------------------------

struct DBusAwaiter {
    sdbus::IProxy* proxy;
    std::string cmd;
    std::string payload;
    detail::PendingCall* state;

    bool await_ready() const noexcept { return false; }

    void await_suspend(std::coroutine_handle<> h) {
        state->handle = h;
        auto req = proxy->createMethodCall(
            "com.firebox.Service",
            "Invoke"
        );
        req << cmd << payload;

        proxy->callMethodAsync(req)
            .uponReplyInvoke([state = this->state](const sdbus::Error* error, sdbus::MethodReply& reply) {
                if (error) {
                    state->error = std::make_exception_ptr(std::runtime_error("D-Bus error: " + error->getMessage()));
                } else {
                    try {
                        reply >> state->success >> state->result_json;
                    } catch (const std::exception& e) {
                        state->error = std::current_exception();
                    }
                }
                // Schedule resumption on GTK thread
                g_idle_add(detail::resume_coroutine_on_main_thread, state->handle.address());
            });
    }

    std::string await_resume() {
        if (state->error) {
            std::rethrow_exception(state->error);
        }
        return state->result_json;
    }
};

// ---------------------------------------------------------------------------
// FireBoxDbusClient
// ---------------------------------------------------------------------------

export class FireBoxDbusClient {
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
        if (connection_) {
            connection_->leaveEventLoop();
        }
    }

    FireBoxDbusClient(const FireBoxDbusClient&)            = delete;
    FireBoxDbusClient& operator=(const FireBoxDbusClient&) = delete;

    // -----------------------------------------------------------------------
    // Core Invoker implementation with coroutine
    // -----------------------------------------------------------------------

    struct AwaitableJsonNode {
        FireBoxDbusClient* client;
        std::string cmd;
        std::string payload;

        bool await_ready() const noexcept { return false; }
        
        void await_suspend(std::coroutine_handle<> h) {
            awaiter = std::make_unique<DBusAwaiter>(
                DBusAwaiter{client->proxy_.get(), cmd, payload, &state}
            );
            awaiter->await_suspend(h);
        }

        JsonNode* await_resume() {
            std::string response_json = awaiter->await_resume();

            auto& parser = client->ensure_parser();
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

            if (!json_object_has_member(envelope, "body")) {
                return nullptr;
            }
            return json_object_get_member(envelope, "body");
        }

        detail::PendingCall state;
        std::unique_ptr<DBusAwaiter> awaiter;
    };

    AwaitableJsonNode invoke_async(const std::string& cmd,
                                   const std::string& payload_json = "{}") {
        return AwaitableJsonNode{this, cmd, payload_json};
    }

    // -----------------------------------------------------------------------
    // Typed convenience methods (Now Async)
    // -----------------------------------------------------------------------

    // Used to test connectivity during startup, can be synchronous.
    bool ping_sync() {
        bool success;
        std::string response;
        try {
            proxy_->callMethod("Invoke").onInterface(INTERFACE).withArguments("ping", "{}").storeResultsTo(success, response);
            return success;
        } catch(...) {
            return false;
        }
    }

    struct ListProvidersAwaiter {
        AwaitableJsonNode inner;
        bool await_ready() { return inner.await_ready(); }
        void await_suspend(std::coroutine_handle<> h) { inner.await_suspend(h); }
        std::vector<ProviderInfo> await_resume() {
            JsonNode* body = inner.await_resume();
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
    };
    ListProvidersAwaiter list_providers() {
        return {invoke_async("list_providers")};
    }

    struct GetMetricsAwaiter {
        AwaitableJsonNode inner;
        bool await_ready() { return inner.await_ready(); }
        void await_suspend(std::coroutine_handle<> h) { inner.await_suspend(h); }
        MetricsSnapshot await_resume() {
            JsonNode* body = inner.await_resume();
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
    };
    GetMetricsAwaiter get_metrics_snapshot() {
        return {invoke_async("get_metrics_snapshot")};
    }

    struct ListConnectionsAwaiter {
        AwaitableJsonNode inner;
        bool await_ready() { return inner.await_ready(); }
        void await_suspend(std::coroutine_handle<> h) { inner.await_suspend(h); }
        std::vector<ConnectionInfo> await_resume() {
            JsonNode* body = inner.await_resume();
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
    };
    ListConnectionsAwaiter list_connections() {
        return {invoke_async("list_connections")};
    }

    struct GetRouteRulesAwaiter {
        AwaitableJsonNode inner;
        bool await_ready() { return inner.await_ready(); }
        void await_suspend(std::coroutine_handle<> h) { inner.await_suspend(h); }
        std::vector<RouteRuleInfo> await_resume() {
            JsonNode* body = inner.await_resume();
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
    };
    GetRouteRulesAwaiter get_route_rules() {
        return {invoke_async("get_route_rules")};
    }

    struct GetAllowlistAwaiter {
        AwaitableJsonNode inner;
        bool await_ready() { return inner.await_ready(); }
        void await_suspend(std::coroutine_handle<> h) { inner.await_suspend(h); }
        std::vector<AllowlistEntry> await_resume() {
            JsonNode* body = inner.await_resume();
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
    };
    GetAllowlistAwaiter get_allowlist() {
        return {invoke_async("get_allowlist")};
    }

    // -----------------------------------------------------------------------
    // Action methods
    // -----------------------------------------------------------------------

    struct VoidAwaiter {
        AwaitableJsonNode inner;
        bool await_ready() { return inner.await_ready(); }
        void await_suspend(std::coroutine_handle<> h) { inner.await_suspend(h); }
        void await_resume() { inner.await_resume(); }
    };
    
    struct StringAwaiter {
        AwaitableJsonNode inner;
        std::string key;
        bool await_ready() { return inner.await_ready(); }
        void await_suspend(std::coroutine_handle<> h) { inner.await_suspend(h); }
        std::string await_resume() { 
            JsonNode* body = inner.await_resume();
            if (!body || !JSON_NODE_HOLDS_OBJECT(body)) {
                throw std::runtime_error("expected body");
            }
            return detail::obj_get_string(json_node_get_object(body), key.c_str());
        }
    };

    StringAwaiter add_api_key_provider(const std::string& name,
                                       const std::string& provider_type,
                                       const std::string& api_key,
                                       const std::string& base_url = "") {
        std::string payload = build_json({
            {"name",          quote(name)},
            {"provider_type", quote(provider_type)},
            {"api_key",       quote(api_key)},
            {"base_url",      quote(base_url)},
        });
        return {invoke_async("add_api_key_provider", payload), "provider_id"};
    }

    struct OAuthStartAwaiter {
        AwaitableJsonNode inner;
        bool await_ready() { return inner.await_ready(); }
        void await_suspend(std::coroutine_handle<> h) { inner.await_suspend(h); }
        std::pair<std::string, OAuthChallenge> await_resume() {
            JsonNode* body = inner.await_resume();
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
    };
    OAuthStartAwaiter add_oauth_provider(const std::string& name,
                                         const std::string& provider_type) {
        std::string payload = build_json({
            {"name",          quote(name)},
            {"provider_type", quote(provider_type)},
        });
        return {invoke_async("add_oauth_provider", payload)};
    }

    VoidAwaiter complete_oauth(const std::string& provider_id) {
        std::string payload = build_json({
            {"provider_id", quote(provider_id)},
        });
        return {invoke_async("complete_oauth", payload)};
    }

    StringAwaiter add_local_provider(const std::string& name,
                                     const std::string& model_path) {
        std::string payload = build_json({
            {"name",       quote(name)},
            {"model_path", quote(model_path)},
        });
        return {invoke_async("add_local_provider", payload), "provider_id"};
    }

    VoidAwaiter delete_provider(const std::string& provider_id) {
        std::string payload = build_json({
            {"provider_id", quote(provider_id)},
        });
        return {invoke_async("delete_provider", payload)};
    }

    VoidAwaiter set_route_rules(const std::string& virtual_model_id,
                                const std::string& display_name,
                                const std::string& strategy,
                                const std::string& provider_id,
                                const std::string& model_id) {
        std::string targets_json =
            "[{\"provider_id\":" + quote(provider_id) +
            ",\"model_id\":"    + quote(model_id) + "}]";

        std::string payload =
            "{\"virtual_model_id\":" + quote(virtual_model_id) +
            ",\"display_name\":"     + quote(display_name) +
            ",\"strategy\":"         + quote(strategy) +
            ",\"targets\":"          + targets_json + "}";
        return {invoke_async("set_route_rules", payload)};
    }

    VoidAwaiter delete_route(const std::string& virtual_model_id) {
        std::string payload = build_json({
            {"virtual_model_id", quote(virtual_model_id)},
        });
        return {invoke_async("delete_route", payload)};
    }

    VoidAwaiter remove_from_allowlist(const std::string& app_path) {
        std::string payload = build_json({
            {"app_path", quote(app_path)},
        });
        return {invoke_async("remove_from_allowlist", payload)};
    }

private:
    std::unique_ptr<sdbus::IConnection> connection_;
    std::unique_ptr<sdbus::IProxy>      proxy_;
    detail::ScopedJsonParser parser_;

    detail::ScopedJsonParser& ensure_parser() {
        parser_ = detail::ScopedJsonParser();
        return parser_;
    }

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

    static std::string quote(const std::string& s) {
        return "\"" + escape(s) + "\"";
    }

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