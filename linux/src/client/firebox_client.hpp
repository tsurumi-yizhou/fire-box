#pragma once
/// @file firebox_client.hpp
/// FireBox Client SDK — C++ D-Bus based client for consuming AI capabilities.

#include "chat_stream.hpp"
#include "common/dbus_types.hpp"
#include "common/error.hpp"

#include <sdbus-c++/sdbus-c++.h>
#include <memory>
#include <string>
#include <vector>

namespace firebox::client {

/// The main client entry point. Manages D-Bus connection and exposes
/// all Capability Protocol operations.
class FireBoxClient {
public:
    /// Connect to the FireBox backend via Session Bus.
    /// @param timeout_ms  Maximum time to wait for authorization (TOFU).
    /// @return connected client instance
    /// @throws FireboxError on failure
    static std::unique_ptr<FireBoxClient> connect(int timeout_ms = 60000);

    /// Connect to an explicit D-Bus bus name and object path.
    static std::unique_ptr<FireBoxClient> connect(
        const std::string& bus_name,
        const std::string& object_path,
        int timeout_ms = 60000);

    ~FireBoxClient();
    FireBoxClient(const FireBoxClient&) = delete;
    FireBoxClient& operator=(const FireBoxClient&) = delete;

    // ── Discovery ────────────────────────────────────────────
    std::vector<ServiceModel> list_models();
    ServiceModel get_model_metadata(const std::string& model_id);

    // ── Auth Status ──────────────────────────────────────────
    struct AuthStatus {
        bool authorized = false;
        std::string app_name;
    };
    AuthStatus get_auth_status();

    // ── Non-streaming Completion ─────────────────────────────
    struct CompleteResult {
        std::string completion_json;
        Usage usage;
        std::string finish_reason;
    };
    CompleteResult complete(
        const std::string& model_id,
        const std::string& messages_json,
        const std::string& tools_json = "[]",
        double temperature = 0.7,
        int max_tokens = 4096);

    // ── Streaming ────────────────────────────────────────────
    std::unique_ptr<ChatStream> create_stream(
        const std::string& model_id,
        double temperature = 0.7,
        int max_tokens = 4096);

    // ── Embeddings ───────────────────────────────────────────
    struct EmbedResult {
        std::string embeddings_json;
        int prompt_tokens = 0;
        int total_tokens = 0;
    };
    EmbedResult embed(
        const std::string& model_id,
        const std::vector<std::string>& inputs,
        const std::string& encoding_format = "float");

    /// Close the client connection.
    void close();

    /// Check if client is connected.
    [[nodiscard]] bool is_connected() const { return connected_; }

private:
    FireBoxClient() = default;

    std::unique_ptr<sdbus::IConnection> connection_;
    std::unique_ptr<sdbus::IProxy> proxy_;
    bool connected_ = false;
};

} // namespace firebox::client
