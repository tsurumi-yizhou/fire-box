/// @file capability_adaptor.cpp
#include "capability_adaptor.hpp"
#include "service.hpp"
#include "common/dbus_types.hpp"
#include <spdlog/spdlog.h>
#include <unistd.h>
#include <filesystem>

namespace firebox::backend {

CapabilityAdaptor::CapabilityAdaptor(Service& svc, sdbus::IConnection& conn,
                                     sdbus::ObjectPath object_path)
    : sdbus::AdaptorInterfaces<org::firebox::Capability_adaptor>(conn, std::move(object_path))
    , svc_(svc) {
    registerAdaptor();
    spdlog::info("CapabilityAdaptor registered");
}

CapabilityAdaptor::~CapabilityAdaptor() {
    unregisterAdaptor();
}

// ── D-Bus caller identity helpers ────────────────────────────────

std::string CapabilityAdaptor::get_caller_exe_path() {
    try {
        auto msg = getObject().getCurrentlyProcessedMessage();
        auto sender = msg.getSender();

        // Ask the D-Bus daemon for the PID of the sender
        auto dbus_proxy = sdbus::createProxy(
            svc_.dbus_connection(),
            sdbus::ServiceName{"org.freedesktop.DBus"},
            sdbus::ObjectPath{"/org/freedesktop/DBus"});

        uint32_t pid = 0;
        dbus_proxy->callMethod("GetConnectionUnixProcessID")
            .onInterface("org.freedesktop.DBus")
            .withArguments(sender)
            .storeResultsTo(pid);

        // Resolve exe path via /proc.
        std::error_code ec;
        auto exe_path = std::filesystem::read_symlink(
            "/proc/" + std::to_string(pid) + "/exe", ec);
        if (!ec) return exe_path.string();

        // /proc not accessible — fall back to a UID-scoped identifier.
        // This still triggers TOFU for unknown UIDs while staying functional.
        spdlog::debug("Capability: /proc/{}/exe not accessible ({}), using UID fallback",
                      pid, ec.message());
        uint32_t caller_uid = ~0u;
        dbus_proxy->callMethod("GetConnectionUnixUser")
            .onInterface("org.freedesktop.DBus")
            .withArguments(sender)
            .storeResultsTo(caller_uid);
        // Use a synthetic path so the allowlist can recognise this caller.
        return "uid://" + std::to_string(caller_uid);
    } catch (const sdbus::Error& e) {
        spdlog::warn("Failed to resolve caller identity: {}", e.what());
        return {};
    }
}

std::string CapabilityAdaptor::check_caller_auth() {
    auto exe_path = get_caller_exe_path();
    if (exe_path.empty()) {
        return "Failed to identify caller process";
    }
    if (!svc_.check_authorization(exe_path, exe_path)) {
        return "Application not authorized. Authorization required via TOFU.";
    }
    return {};
}

// ── Method Implementations ───────────────────────────────────────

using ModelStruct = sdbus::Struct<
    std::string, std::string, bool, bool, bool, bool, bool,
    int32_t, int32_t, std::string, std::string, std::vector<std::string>>;

std::tuple<bool, std::string, std::vector<ModelStruct>>
CapabilityAdaptor::ListAvailableModels() {
    spdlog::debug("ListAvailableModels called");

    auto auth_err = check_caller_auth();
    if (!auth_err.empty()) {
        return {false, std::move(auth_err), {}};
    }

    auto rules = svc_.storage().list_route_rules();
    std::vector<ModelStruct> models;
    for (auto& r : rules) {
        models.emplace_back(ModelStruct{
            r.virtual_model_id, r.display_name,
            r.capabilities.chat, r.capabilities.streaming,
            r.capabilities.embeddings, r.capabilities.vision,
            r.capabilities.tool_calling,
            r.metadata.context_window, int32_t(0),
            r.metadata.pricing_tier, r.metadata.description,
            r.metadata.strengths});
    }
    return {true, "", std::move(models)};
}

std::tuple<bool, std::string, std::string, std::string,
    bool, bool, bool, bool, bool, int32_t,
    std::string, std::string, std::vector<std::string>>
CapabilityAdaptor::GetModelMetadata(const std::string& model_id) {
    spdlog::debug("GetModelMetadata: {}", model_id);

    auto auth_err = check_caller_auth();
    if (!auth_err.empty()) {
        return {false, std::move(auth_err), "", "",
                false, false, false, false, false, 0,
                "", "", {}};
    }

    auto rule = svc_.storage().get_route_rule(model_id);
    if (rule.virtual_model_id.empty()) {
        return {false, "Model not found", "", "",
                false, false, false, false, false, 0,
                "", "", {}};
    }
    return {true, "",
            rule.virtual_model_id, rule.display_name,
            rule.capabilities.chat, rule.capabilities.streaming,
            rule.capabilities.embeddings, rule.capabilities.vision,
            rule.capabilities.tool_calling,
            rule.metadata.context_window,
            rule.metadata.pricing_tier, rule.metadata.description,
            rule.metadata.strengths};
}

std::tuple<bool, std::string, bool, std::string>
CapabilityAdaptor::GetAuthStatus() {
    auto exe_path = get_caller_exe_path();
    if (exe_path.empty()) {
        return {true, "", false, ""};
    }
    bool authorized = svc_.check_authorization(exe_path, exe_path);
    return {true, "", authorized, exe_path};
}

std::tuple<bool, std::string, std::string, int32_t, int32_t, int32_t, std::string>
CapabilityAdaptor::Complete(const std::string& model_id,
                            const std::string& messages_json,
                            const std::string& tools_json,
                            const double& temperature,
                            const int32_t& max_tokens) {
    spdlog::debug("Complete: model_id={}", model_id);

    auto auth_err = check_caller_auth();
    if (!auth_err.empty()) {
        return {false, std::move(auth_err), "{}", 0, 0, 0, ""};
    }

    auto [provider_id, physical_model] = svc_.resolve_route(model_id);
    if (provider_id.empty()) {
        return {false, "Model not found or no route targets configured", "{}", 0, 0, 0, ""};
    }

    // Forward to provider via HTTP (not yet implemented — return NotImplemented)
    (void)messages_json; (void)tools_json; (void)temperature; (void)max_tokens;
    return {false, "Provider HTTP forwarding not yet implemented", "{}", 0, 0, 0, ""};
}

std::tuple<bool, std::string, std::string>
CapabilityAdaptor::CreateStream(const std::string& model_id,
                                const double& temperature,
                                const int32_t& max_tokens) {
    spdlog::debug("CreateStream: model_id={}", model_id);

    auto auth_err = check_caller_auth();
    if (!auth_err.empty()) {
        return {false, std::move(auth_err), ""};
    }

    auto [provider_id, physical_model] = svc_.resolve_route(model_id);
    if (provider_id.empty()) {
        return {false, "Model not found or no route targets configured", ""};
    }

    std::lock_guard lock(streams_mutex_);
    auto stream_id = "stream-" + std::to_string(++next_stream_id_);
    StreamState state;
    state.model_id = model_id;
    state.provider_id = provider_id;
    state.physical_model = physical_model;
    state.temperature = temperature;
    state.max_tokens = max_tokens;
    state.open = true;
    streams_[stream_id] = std::move(state);

    spdlog::info("Stream created: {} -> {}/{}", stream_id, provider_id, physical_model);
    return {true, "", stream_id};
}

std::tuple<bool, std::string>
CapabilityAdaptor::SendMessage(const std::string& stream_id,
                               const std::string& message_json,
                               const std::string& tools_json) {
    spdlog::debug("SendMessage: stream_id={}", stream_id);

    std::lock_guard lock(streams_mutex_);
    auto it = streams_.find(stream_id);
    if (it == streams_.end()) {
        return {false, "Stream not found: " + stream_id};
    }
    if (!it->second.open) {
        return {false, "Stream is closed: " + stream_id};
    }

    // Provider HTTP streaming not yet implemented
    (void)message_json; (void)tools_json;
    return {false, "Provider HTTP streaming not yet implemented"};
}

std::tuple<bool, std::string, std::string, bool, int32_t, int32_t, int32_t, std::string>
CapabilityAdaptor::ReceiveStream(const std::string& stream_id,
                                 const int32_t& timeout_ms) {
    spdlog::debug("ReceiveStream: stream_id={}", stream_id);

    std::lock_guard lock(streams_mutex_);
    auto it = streams_.find(stream_id);
    if (it == streams_.end()) {
        return {false, "Stream not found: " + stream_id, "{}", true, 0, 0, 0, ""};
    }
    if (!it->second.open) {
        return {false, "Stream is closed: " + stream_id, "{}", true, 0, 0, 0, ""};
    }

    // Provider HTTP streaming not yet implemented
    (void)timeout_ms;
    return {false, "Provider HTTP streaming not yet implemented", "{}", true, 0, 0, 0, ""};
}

std::tuple<bool, std::string>
CapabilityAdaptor::CloseStream(const std::string& stream_id) {
    spdlog::debug("CloseStream: {}", stream_id);

    std::lock_guard lock(streams_mutex_);
    auto it = streams_.find(stream_id);
    if (it == streams_.end()) {
        return {false, "Stream not found: " + stream_id};
    }
    it->second.open = false;
    streams_.erase(it);
    spdlog::info("Stream closed: {}", stream_id);
    return {true, ""};
}

std::tuple<bool, std::string, std::string, int32_t, int32_t>
CapabilityAdaptor::Embed(const std::string& model_id,
                         const std::vector<std::string>& inputs,
                         const std::string& encoding_format) {
    spdlog::debug("Embed: model_id={}, inputs={}", model_id, inputs.size());

    auto auth_err = check_caller_auth();
    if (!auth_err.empty()) {
        return {false, std::move(auth_err), "[]", 0, 0};
    }

    auto [provider_id, physical_model] = svc_.resolve_route(model_id);
    if (provider_id.empty()) {
        return {false, "Model not found or no route targets configured", "[]", 0, 0};
    }

    // Provider HTTP embedding not yet implemented
    (void)inputs; (void)encoding_format;
    return {false, "Provider HTTP embedding not yet implemented", "[]", 0, 0};
}

} // namespace firebox::backend
