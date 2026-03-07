/// @file control_adaptor.cpp
#include "control_adaptor.hpp"
#include "service.hpp"
#include "http_client.hpp"
#include "common/dbus_types.hpp"
#include "common/credential.hpp"
#include <spdlog/spdlog.h>
#include <nlohmann/json.hpp>
#include <unistd.h>
#include <algorithm>
#include <chrono>
#include <random>
#include <filesystem>

namespace firebox::backend {

namespace {

std::string generate_id() {
    static std::mt19937 rng(std::random_device{}());
    static const char alphanum[] =
        "0123456789abcdefghijklmnopqrstuvwxyz";
    std::string id;
    id.reserve(12);
    for (int i = 0; i < 12; ++i)
        id += alphanum[rng() % (sizeof(alphanum) - 1)];
    return id;
}

} // namespace

ControlAdaptor::ControlAdaptor(Service& svc, sdbus::IConnection& conn,
                               sdbus::ObjectPath object_path)
    : sdbus::AdaptorInterfaces<org::firebox::Control_adaptor>(conn, std::move(object_path))
    , svc_(svc) {
    registerAdaptor();
    spdlog::info("ControlAdaptor registered");
}

ControlAdaptor::~ControlAdaptor() {
    unregisterAdaptor();
}

// ── Caller verification ──────────────────────────────────────────

std::string ControlAdaptor::verify_frontend_caller() {
    try {
        auto msg = getObject().getCurrentlyProcessedMessage();
        auto sender = msg.getSender();

        auto dbus_proxy = sdbus::createProxy(
            svc_.dbus_connection(),
            sdbus::ServiceName{"org.freedesktop.DBus"},
            sdbus::ObjectPath{"/org/freedesktop/DBus"});

        uint32_t pid = 0;
        dbus_proxy->callMethod("GetConnectionUnixProcessID")
            .onInterface("org.freedesktop.DBus")
            .withArguments(sender)
            .storeResultsTo(pid);

        // Try to resolve the caller's executable path via /proc.
        // Falls back to UID check if /proc is not accessible (e.g. Yama ptrace_scope).
        std::error_code ec;
        auto exe_path = std::filesystem::read_symlink(
            "/proc/" + std::to_string(pid) + "/exe", ec);

        if (!ec) {
            auto exe_name = exe_path.filename().string();
            if (exe_name != "firebox") {
                spdlog::warn("Control: unauthorized caller '{}' (PID {})", exe_path.string(), pid);
                return "Permission denied: only the FireBox frontend may access the Control interface";
            }
            return {};
        }

        // /proc read failed — fall back to UID verification.
        // On the session bus, same-UID callers are trusted for the Control interface.
        spdlog::debug("Control: /proc/{}/exe not accessible ({}), falling back to UID check",
                      pid, ec.message());
        uint32_t caller_uid = ~0u;
        dbus_proxy->callMethod("GetConnectionUnixUser")
            .onInterface("org.freedesktop.DBus")
            .withArguments(sender)
            .storeResultsTo(caller_uid);
        if (caller_uid != static_cast<uint32_t>(getuid())) {
            spdlog::warn("Control: UID mismatch (caller={}, us={})", caller_uid, getuid());
            return "Permission denied: caller UID does not match";
        }
        return {};
    } catch (const sdbus::Error& e) {
        spdlog::warn("Control: caller verification failed: {}", e.what());
        return "Failed to verify caller identity";
    }
}

// ── Provider Management ──────────────────────────────────────────

std::tuple<bool, std::string, std::string>
ControlAdaptor::AddApiKeyProvider(const std::string& name,
                                  const std::string& provider_type,
                                  const std::string& api_key,
                                  const std::string& base_url) {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err), ""};

    spdlog::info("AddApiKeyProvider: name={} type={}", name, provider_type);
    auto pid = generate_id();
    Provider p;
    p.provider_id = pid;
    p.name = name;
    p.type = ProviderType::ApiKey;
    p.subtype = provider_type;
    p.base_url = base_url;
    p.enabled = true;
    svc_.storage().upsert_provider(p);
    if (!api_key.empty())
        credential_store("provider-" + pid + "-apikey", api_key);
    return {true, "", pid};
}

std::tuple<bool, std::string, std::string, std::string, std::string, std::string, int32_t, int32_t>
ControlAdaptor::AddOAuthProvider(const std::string& name,
                                 const std::string& /*provider_type*/) {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err), "", "", "", "", 0, 0};

    spdlog::info("AddOAuthProvider: name={}", name);
    auto pid = generate_id();

    Provider p;
    p.provider_id = pid;
    p.name = name;
    p.type = ProviderType::OAuth;
    p.enabled = false;  // Not enabled until OAuth completes
    svc_.storage().upsert_provider(p);

    // OAuth device flow not yet implemented — return explicit error
    return {false, "OAuth device flow not yet implemented", pid, "", "", "", 0, 0};
}

std::tuple<bool, std::string, int32_t, std::string>
ControlAdaptor::CompleteOAuth(const std::string& provider_id) {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err), 0, "{}"};

    spdlog::debug("CompleteOAuth: {}", provider_id);
    // OAuth polling not yet implemented
    return {false, "OAuth polling not yet implemented",
            static_cast<int32_t>(OAuthStatus::Pending), "{}"};
}

std::tuple<bool, std::string, std::vector<sdbus::Struct<
    std::string, std::string, int32_t, std::string, std::string, bool>>>
ControlAdaptor::ListProviders() {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err), {}};

    auto providers = svc_.storage().list_providers();
    using ProvTuple = sdbus::Struct<
        std::string, std::string, int32_t, std::string, std::string, bool>;
    std::vector<ProvTuple> result;
    for (auto& p : providers) {
        result.emplace_back(ProvTuple{
            p.provider_id, p.name, static_cast<int32_t>(p.type),
            p.base_url, p.local_path, p.enabled});
    }
    return {true, "", std::move(result)};
}

std::tuple<bool, std::string>
ControlAdaptor::UpdateProvider(const std::string& provider_id,
                                const std::string& name,
                                const std::string& api_key,
                                const std::string& base_url) {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err)};

    spdlog::info("UpdateProvider: id={} name={}", provider_id, name);
    bool ok = svc_.storage().update_provider(provider_id, name, base_url);
    if (!ok) return {false, "Provider not found"};
    if (!api_key.empty())
        credential_store("provider-" + provider_id + "-apikey", api_key);
    return {true, ""};
}

std::tuple<bool, std::string>
ControlAdaptor::DeleteProvider(const std::string& provider_id) {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err)};

    spdlog::info("DeleteProvider: {}", provider_id);
    bool ok = svc_.storage().delete_provider(provider_id);
    credential_delete("provider-" + provider_id + "-apikey");
    return {ok, ok ? "" : "Provider not found"};
}

// ── Model Discovery ──────────────────────────────────────────────

std::tuple<bool, std::string, int32_t>
ControlAdaptor::DiscoverModels(const std::string& provider_id) {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err), 0};

    // Look up provider
    auto providers = svc_.storage().list_providers();
    auto it = std::find_if(providers.begin(), providers.end(),
        [&](const Provider& p){ return p.provider_id == provider_id; });
    if (it == providers.end())
        return {false, "Provider not found", 0};
    const Provider& prov = *it;

    auto api_key = credential_load("provider-" + provider_id + "-apikey");
    const auto& sub = prov.subtype;

    // Build URL and headers from subtype.
    // If base_url is set by the user it overrides the canonical host.
    std::string url;
    std::unordered_map<std::string, std::string> req_headers;

    if (sub == "openai") {
        const std::string host = prov.base_url.empty()
            ? "https://api.openai.com/v1" : prov.base_url;
        url = host + "/models";
        req_headers["Authorization"] = "Bearer " + api_key;
    } else if (sub == "anthropic") {
        const std::string host = prov.base_url.empty()
            ? "https://api.anthropic.com" : prov.base_url;
        url = host + "/v1/models?limit=100";
        req_headers["x-api-key"] = api_key;
        req_headers["anthropic-version"] = "2023-06-01";
    } else if (sub == "gemini") {
        const std::string host = prov.base_url.empty()
            ? "https://generativelanguage.googleapis.com" : prov.base_url;
        url = host + "/v1beta/models?key=" + api_key;
    } else {
        return {false, "DiscoverModels: unknown provider subtype '" + sub + "'", 0};
    }

    spdlog::info("DiscoverModels: GET {} (provider={})", url, provider_id);
    HttpClient http;
    auto resp = http.get_sync(url, req_headers);

    if (resp.status_code != 200) {
        auto snippet = resp.body.substr(0, 300);
        spdlog::warn("DiscoverModels: HTTP {} — {}", resp.status_code, snippet);
        return {false,
            "Provider returned HTTP " + std::to_string(resp.status_code)
                + ": " + snippet,
            0};
    }

    int32_t count = 0;
    try {
        auto j = nlohmann::json::parse(resp.body);

        if (sub == "openai") {
            for (auto& m : j.at("data")) {
                auto id = m.at("id").get<std::string>();
                // Distinguish embedding / moderation from chat models by id suffix
                bool is_embed = id.find("embedding") != std::string::npos
                             || id.find("embed")     != std::string::npos
                             || id.find("moderation")!= std::string::npos;
                Model mdl;
                mdl.model_id             = id;
                mdl.provider_id          = provider_id;
                mdl.enabled              = true;
                mdl.capability_chat      = !is_embed;
                mdl.capability_streaming = !is_embed;
                svc_.storage().upsert_model(mdl);
                ++count;
            }
        } else if (sub == "anthropic") {
            for (auto& m : j.at("data")) {
                Model mdl;
                mdl.model_id             = m.at("id").get<std::string>();
                mdl.provider_id          = provider_id;
                mdl.enabled              = true;
                mdl.capability_chat      = true;
                mdl.capability_streaming = true;
                svc_.storage().upsert_model(mdl);
                ++count;
            }
        } else if (sub == "gemini") {
            for (auto& m : j.at("models")) {
                // "name" is like "models/gemini-2.0-flash"
                auto full_name = m.at("name").get<std::string>();
                auto id = full_name.substr(full_name.rfind('/') + 1);
                bool chat = false, streaming = false;
                if (m.contains("supportedGenerationMethods")) {
                    for (auto& method : m["supportedGenerationMethods"]) {
                        auto ms = method.get<std::string>();
                        if (ms == "generateContent")       chat      = true;
                        if (ms == "streamGenerateContent") streaming = true;
                    }
                }
                if (!chat) continue; // skip non-generative models
                Model mdl;
                mdl.model_id             = id;
                mdl.provider_id          = provider_id;
                mdl.enabled              = true;
                mdl.capability_chat      = true;
                mdl.capability_streaming = streaming;
                svc_.storage().upsert_model(mdl);
                ++count;
            }
        }
    } catch (const nlohmann::json::exception& e) {
        return {false, std::string("JSON parse error: ") + e.what(), 0};
    }

    spdlog::info("DiscoverModels: stored {} models for provider {}", count, provider_id);
    return {true, "", count};
}

// ── Model Configuration ──────────────────────────────────────────

std::tuple<bool, std::string, std::vector<sdbus::Struct<
    std::string, std::string, bool, bool, bool>>>
ControlAdaptor::GetAllModels(const std::string& provider_id) {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err), {}};

    auto models = svc_.storage().get_models(provider_id);
    using ModelTuple = sdbus::Struct<std::string, std::string, bool, bool, bool>;
    std::vector<ModelTuple> result;
    for (auto& m : models) {
        result.emplace_back(ModelTuple{
            m.model_id, m.provider_id, m.enabled,
            m.capability_chat, m.capability_streaming});
    }
    return {true, "", std::move(result)};
}

std::tuple<bool, std::string>
ControlAdaptor::SetModelEnabled(const std::string& provider_id,
                                const std::string& model_id,
                                const bool& enabled) {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err)};

    spdlog::info("SetModelEnabled: {}/{} = {}", provider_id, model_id, enabled);
    bool ok = svc_.storage().set_model_enabled(provider_id, model_id, enabled);
    return {ok, ok ? "" : "Model not found"};
}

// ── Routing ──────────────────────────────────────────────────────

std::tuple<bool, std::string>
ControlAdaptor::SetRouteRules(
    const std::string& virtual_model_id, const std::string& display_name,
    const sdbus::Struct<bool, bool, bool, bool, bool>& capabilities,
    const std::string& /*metadata_json*/,
    const std::vector<sdbus::Struct<std::string, std::string>>& targets,
    const int32_t& strategy) {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err)};

    spdlog::info("SetRouteRules: {}", virtual_model_id);
    RouteRule rule;
    rule.virtual_model_id = virtual_model_id;
    rule.display_name = display_name;
    rule.capabilities.chat         = std::get<0>(capabilities);
    rule.capabilities.streaming    = std::get<1>(capabilities);
    rule.capabilities.embeddings   = std::get<2>(capabilities);
    rule.capabilities.vision       = std::get<3>(capabilities);
    rule.capabilities.tool_calling = std::get<4>(capabilities);
    rule.strategy = static_cast<RouteStrategy>(strategy);
    for (auto& t : targets)
        rule.targets.push_back({std::get<0>(t), std::get<1>(t)});
    svc_.storage().set_route_rule(rule);
    return {true, ""};
}

std::tuple<bool, std::string, std::string,
           sdbus::Struct<bool, bool, bool, bool, bool>,
           std::string,
           std::vector<sdbus::Struct<std::string, std::string>>, int32_t>
ControlAdaptor::GetRouteRules(const std::string& virtual_model_id) {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err), "", sdbus::Struct<bool,bool,bool,bool,bool>{}, "", {}, 0};

    auto rule = svc_.storage().get_route_rule(virtual_model_id);
    sdbus::Struct<bool, bool, bool, bool, bool> caps{
        rule.capabilities.chat, rule.capabilities.streaming,
        rule.capabilities.embeddings, rule.capabilities.vision,
        rule.capabilities.tool_calling};
    std::vector<sdbus::Struct<std::string, std::string>> targets;
    for (auto& t : rule.targets)
        targets.emplace_back(sdbus::Struct<std::string, std::string>{
            t.provider_id, t.model_id});
    return {true, "", rule.display_name, caps, "",
            std::move(targets), static_cast<int32_t>(rule.strategy)};
}

std::tuple<bool, std::string, std::string>
ControlAdaptor::ListRouteRules() {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err), "[]"};

    auto rules = svc_.storage().list_route_rules();
    nlohmann::json j = nlohmann::json::array();
    for (auto& r : rules) {
        j.push_back({
            {"virtual_model_id", r.virtual_model_id},
            {"display_name", r.display_name},
            {"strategy", static_cast<int>(r.strategy)},
            {"targets_count", r.targets.size()}
        });
    }
    return {true, "", j.dump()};
}

// ── Metrics ──────────────────────────────────────────────────────

std::tuple<bool, std::string, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, double>
ControlAdaptor::GetMetricsSnapshot() {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err), 0, 0, 0, 0, 0, 0, 0.0};

    auto snap = svc_.storage().get_metrics_snapshot();
    return {true, "",
            snap.window_start_ms, snap.window_end_ms,
            snap.requests_total, snap.requests_failed,
            snap.prompt_tokens, snap.completion_tokens,
            snap.cost_total};
}

std::tuple<bool, std::string, std::string>
ControlAdaptor::GetMetricsRange(const int64_t& start_ms, const int64_t& end_ms) {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err), "[]"};

    auto snapshots = svc_.storage().get_metrics_range(start_ms, end_ms);
    nlohmann::json j = nlohmann::json::array();
    for (auto& s : snapshots) {
        j.push_back({
            {"window_start_ms", s.window_start_ms},
            {"window_end_ms", s.window_end_ms},
            {"requests_total", s.requests_total},
            {"requests_failed", s.requests_failed},
            {"prompt_tokens", s.prompt_tokens},
            {"completion_tokens", s.completion_tokens},
            {"cost_total", s.cost_total}
        });
    }
    return {true, "", j.dump()};
}

// ── Connections ──────────────────────────────────────────────────

std::tuple<bool, std::string, std::vector<sdbus::Struct<
    std::string, std::string, int32_t, int64_t, int64_t>>>
ControlAdaptor::ListConnections() {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err), {}};

    auto conns = svc_.list_connections();
    using ConnTuple = sdbus::Struct<
        std::string, std::string, int32_t, int64_t, int64_t>;
    std::vector<ConnTuple> result;
    for (auto& c : conns)
        result.emplace_back(ConnTuple{
            c.connection_id, c.client_name, c.pid,
            c.connected_at_ms, c.requests_count});
    return {true, "", std::move(result)};
}

// ── Allowlist ────────────────────────────────────────────────────

std::tuple<bool, std::string, std::vector<sdbus::Struct<
    std::string, std::string, int64_t, int64_t>>>
ControlAdaptor::GetAllowlist() {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err), {}};

    auto apps = svc_.storage().get_allowlist();
    using AppTuple = sdbus::Struct<std::string, std::string, int64_t, int64_t>;
    std::vector<AppTuple> result;
    for (auto& a : apps)
        result.emplace_back(AppTuple{
            a.app_path, a.display_name, a.first_seen_ms, a.last_used_ms});
    return {true, "", std::move(result)};
}

std::tuple<bool, std::string>
ControlAdaptor::RemoveFromAllowlist(const std::string& app_path) {
    auto err = verify_frontend_caller();
    if (!err.empty()) return {false, std::move(err)};

    spdlog::info("RemoveFromAllowlist: {}", app_path);
    bool ok = svc_.storage().remove_from_allowlist(app_path);
    return {ok, ok ? "" : "App not found"};
}

} // namespace firebox::backend
