/// @file firebox_client.cpp
#include "firebox_client.hpp"
#include <spdlog/spdlog.h>
#include <stdexcept>

namespace firebox::client {

static constexpr const char* CAP_IFACE = "org.firebox.Capability";

std::unique_ptr<FireBoxClient> FireBoxClient::connect(int timeout_ms) {
    return connect("org.firebox", "/org/firebox", timeout_ms);
}

std::unique_ptr<FireBoxClient> FireBoxClient::connect(
    const std::string& bus_name,
    const std::string& object_path,
    int /*timeout_ms*/) {

    auto client = std::unique_ptr<FireBoxClient>(new FireBoxClient());

    try {
        client->connection_ = sdbus::createSessionBusConnection();
        client->proxy_ = sdbus::createProxy(
            *client->connection_,
            sdbus::ServiceName{bus_name},
            sdbus::ObjectPath{object_path});
        client->connected_ = true;
        spdlog::info("FireBoxClient: Connected to {} at {}", bus_name, object_path);
    } catch (const sdbus::Error& e) {
        throw FireboxError(ErrorCode::ServiceNotFound,
            std::string("Cannot connect to FireBox: ") + e.what());
    }

    return client;
}

FireBoxClient::~FireBoxClient() {
    close();
}

void FireBoxClient::close() {
    if (connected_) {
        proxy_.reset();
        connection_.reset();
        connected_ = false;
        spdlog::debug("FireBoxClient: Closed");
    }
}

// ── Discovery ────────────────────────────────────────────────────

std::vector<ServiceModel> FireBoxClient::list_models() {
    if (!connected_)
        throw FireboxError(ErrorCode::ClientClosed, "Client is closed");

    using ModelTuple = sdbus::Struct<
        std::string, std::string,
        bool, bool, bool, bool, bool,
        int32_t, int32_t, std::string, std::string,
        std::vector<std::string>>;

    std::tuple<bool, std::string, std::vector<ModelTuple>> result;
    try {
        proxy_->callMethod("ListAvailableModels")
            .onInterface(CAP_IFACE)
            .storeResultsTo(result);
    } catch (const sdbus::Error& e) {
        throw FireboxError(ErrorCode::BackendError, e.what());
    }

    auto& [success, message, tuples] = result;
    if (!success)
        throw FireboxError(ErrorCode::BackendError, message);

    std::vector<ServiceModel> models;
    for (auto& t : tuples) {
        ServiceModel m;
        m.id           = std::get<0>(t);
        m.display_name = std::get<1>(t);
        m.capabilities.chat         = std::get<2>(t);
        m.capabilities.streaming    = std::get<3>(t);
        m.capabilities.embeddings   = std::get<4>(t);
        m.capabilities.vision       = std::get<5>(t);
        m.capabilities.tool_calling = std::get<6>(t);
        m.metadata.context_window   = std::get<7>(t);
        // std::get<8>(t) is int32_t (max_tokens placeholder)
        m.metadata.pricing_tier     = std::get<9>(t);
        m.metadata.description      = std::get<10>(t);
        m.metadata.strengths        = std::get<11>(t);
        models.push_back(std::move(m));
    }
    return models;
}

ServiceModel FireBoxClient::get_model_metadata(const std::string& model_id) {
    if (!connected_)
        throw FireboxError(ErrorCode::ClientClosed, "Client is closed");

    std::tuple<bool, std::string, std::string, std::string,
        bool, bool, bool, bool, bool, int32_t,
        std::string, std::string, std::vector<std::string>> result;
    try {
        proxy_->callMethod("GetModelMetadata")
            .onInterface(CAP_IFACE)
            .withArguments(model_id)
            .storeResultsTo(result);
    } catch (const sdbus::Error& e) {
        throw FireboxError(ErrorCode::BackendError, e.what());
    }

    auto& [success, message, id, display_name,
           chat, streaming, embeddings, vision, tool_calling,
           context_window, pricing_tier, description, strengths] = result;

    if (!success)
        throw FireboxError(ErrorCode::ModelNotFound, message);

    ServiceModel m;
    m.id = id;
    m.display_name = display_name;
    m.capabilities.chat = chat;
    m.capabilities.streaming = streaming;
    m.capabilities.embeddings = embeddings;
    m.capabilities.vision = vision;
    m.capabilities.tool_calling = tool_calling;
    m.metadata.context_window = context_window;
    m.metadata.pricing_tier = pricing_tier;
    m.metadata.description = description;
    m.metadata.strengths = strengths;
    return m;
}

// ── Auth ─────────────────────────────────────────────────────────

FireBoxClient::AuthStatus FireBoxClient::get_auth_status() {
    if (!connected_)
        throw FireboxError(ErrorCode::ClientClosed, "Client is closed");

    std::tuple<bool, std::string, bool, std::string> result;
    try {
        proxy_->callMethod("GetAuthStatus")
            .onInterface(CAP_IFACE)
            .storeResultsTo(result);
    } catch (const sdbus::Error& e) {
        throw FireboxError(ErrorCode::BackendError, e.what());
    }

    auto& [success, message, authorized, app_name] = result;
    return {authorized, app_name};
}

// ── Completion ───────────────────────────────────────────────────

FireBoxClient::CompleteResult FireBoxClient::complete(
    const std::string& model_id,
    const std::string& messages_json,
    const std::string& tools_json,
    double temperature,
    int max_tokens) {

    if (!connected_)
        throw FireboxError(ErrorCode::ClientClosed, "Client is closed");

    std::tuple<bool, std::string, std::string, int32_t, int32_t, int32_t, std::string> result;
    try {
        proxy_->callMethod("Complete")
            .onInterface(CAP_IFACE)
            .withArguments(model_id, messages_json, tools_json,
                           temperature, static_cast<int32_t>(max_tokens))
            .storeResultsTo(result);
    } catch (const sdbus::Error& e) {
        throw FireboxError(ErrorCode::BackendError, e.what());
    }

    auto& [success, message, completion_json, pt, ct, tt, finish_reason] = result;
    if (!success)
        throw FireboxError(ErrorCode::BackendError, message);

    CompleteResult cr;
    cr.completion_json = completion_json;
    cr.usage.prompt_tokens = pt;
    cr.usage.completion_tokens = ct;
    cr.usage.total_tokens = tt;
    cr.finish_reason = finish_reason;
    return cr;
}

// ── Streaming ────────────────────────────────────────────────────

std::unique_ptr<ChatStream> FireBoxClient::create_stream(
    const std::string& model_id,
    double temperature,
    int max_tokens) {

    if (!connected_)
        throw FireboxError(ErrorCode::ClientClosed, "Client is closed");

    std::tuple<bool, std::string, std::string> result;
    try {
        proxy_->callMethod("CreateStream")
            .onInterface(CAP_IFACE)
            .withArguments(model_id, temperature, static_cast<int32_t>(max_tokens))
            .storeResultsTo(result);
    } catch (const sdbus::Error& e) {
        throw FireboxError(ErrorCode::BackendError, e.what());
    }

    auto& [success, message, stream_id] = result;
    if (!success)
        throw FireboxError(ErrorCode::BackendError, message);

    return std::make_unique<ChatStream>(proxy_.get(), stream_id);
}

// ── Embeddings ───────────────────────────────────────────────────

FireBoxClient::EmbedResult FireBoxClient::embed(
    const std::string& model_id,
    const std::vector<std::string>& inputs,
    const std::string& encoding_format) {

    if (!connected_)
        throw FireboxError(ErrorCode::ClientClosed, "Client is closed");

    std::tuple<bool, std::string, std::string, int32_t, int32_t> result;
    try {
        proxy_->callMethod("Embed")
            .onInterface(CAP_IFACE)
            .withArguments(model_id, inputs, encoding_format)
            .storeResultsTo(result);
    } catch (const sdbus::Error& e) {
        throw FireboxError(ErrorCode::BackendError, e.what());
    }

    auto& [success, message, embeddings_json, pt, tt] = result;
    if (!success)
        throw FireboxError(ErrorCode::BackendError, message);

    EmbedResult er;
    er.embeddings_json = embeddings_json;
    er.prompt_tokens = pt;
    er.total_tokens = tt;
    return er;
}

} // namespace firebox::client
