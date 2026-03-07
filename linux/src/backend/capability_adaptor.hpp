#pragma once
/// @file capability_adaptor.hpp
/// D-Bus adaptor implementing org.firebox.Capability for client apps.

#include "capability_adaptor.h"  // generated
#include <sdbus-c++/sdbus-c++.h>
#include <string>
#include <vector>
#include <tuple>
#include <mutex>
#include <unordered_map>

namespace firebox::backend {

class Service;

struct StreamState {
    std::string model_id;
    std::string provider_id;
    std::string physical_model;
    double temperature = 0.0;
    int32_t max_tokens = 0;
    bool open = true;
};

class CapabilityAdaptor final
    : public sdbus::AdaptorInterfaces<org::firebox::Capability_adaptor> {
public:
    CapabilityAdaptor(Service& svc, sdbus::IConnection& conn,
                      sdbus::ObjectPath object_path);
    ~CapabilityAdaptor();

private:
    /// Resolve calling process exe path via D-Bus sender.
    std::string get_caller_exe_path();

    /// Check TOFU authorization for the current D-Bus caller.
    /// Returns empty string on success, error message on denial.
    std::string check_caller_auth();

    // Generated virtual method overrides
    std::tuple<bool, std::string, std::vector<sdbus::Struct<
        std::string, std::string, bool, bool, bool, bool, bool,
        int32_t, int32_t, std::string, std::string, std::vector<std::string>>>>
    ListAvailableModels() override;

    std::tuple<bool, std::string, std::string, std::string,
        bool, bool, bool, bool, bool, int32_t,
        std::string, std::string, std::vector<std::string>>
    GetModelMetadata(const std::string& model_id) override;

    std::tuple<bool, std::string, bool, std::string>
    GetAuthStatus() override;

    std::tuple<bool, std::string, std::string, int32_t, int32_t, int32_t, std::string>
    Complete(const std::string& model_id, const std::string& messages_json,
             const std::string& tools_json, const double& temperature,
             const int32_t& max_tokens) override;

    std::tuple<bool, std::string, std::string>
    CreateStream(const std::string& model_id, const double& temperature,
                 const int32_t& max_tokens) override;

    std::tuple<bool, std::string>
    SendMessage(const std::string& stream_id, const std::string& message_json,
                const std::string& tools_json) override;

    std::tuple<bool, std::string, std::string, bool, int32_t, int32_t, int32_t, std::string>
    ReceiveStream(const std::string& stream_id, const int32_t& timeout_ms) override;

    std::tuple<bool, std::string>
    CloseStream(const std::string& stream_id) override;

    std::tuple<bool, std::string, std::string, int32_t, int32_t>
    Embed(const std::string& model_id, const std::vector<std::string>& inputs,
          const std::string& encoding_format) override;

    Service& svc_;

    // Stream state tracking
    mutable std::mutex streams_mutex_;
    std::unordered_map<std::string, StreamState> streams_;
    int next_stream_id_ = 0;
};

} // namespace firebox::backend
