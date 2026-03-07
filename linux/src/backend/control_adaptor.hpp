#pragma once
/// @file control_adaptor.hpp
/// D-Bus adaptor implementing org.firebox.Control for the frontend.

#include "control_adaptor.h"  // generated
#include <sdbus-c++/sdbus-c++.h>
#include <string>

namespace firebox::backend {

class Service;

class ControlAdaptor final
    : public sdbus::AdaptorInterfaces<org::firebox::Control_adaptor> {
public:
    ControlAdaptor(Service& svc, sdbus::IConnection& conn,
                   sdbus::ObjectPath object_path);
    ~ControlAdaptor();

private:
    /// Verify the D-Bus caller is the authorized frontend process.
    /// Returns empty string on success, error message on denial.
    std::string verify_frontend_caller();

    // Generated virtual method overrides
    std::tuple<bool, std::string, std::string>
    AddApiKeyProvider(const std::string& name, const std::string& provider_type,
                      const std::string& api_key, const std::string& base_url) override;

    std::tuple<bool, std::string, std::string, std::string, std::string, std::string, int32_t, int32_t>
    AddOAuthProvider(const std::string& name, const std::string& provider_type) override;

    std::tuple<bool, std::string, int32_t, std::string>
    CompleteOAuth(const std::string& provider_id) override;

    std::tuple<bool, std::string, std::vector<sdbus::Struct<
        std::string, std::string, int32_t, std::string, std::string, bool>>>
    ListProviders() override;

    std::tuple<bool, std::string>
    UpdateProvider(const std::string& provider_id, const std::string& name,
                   const std::string& api_key, const std::string& base_url) override;

    std::tuple<bool, std::string>
    DeleteProvider(const std::string& provider_id) override;

    std::tuple<bool, std::string, int32_t>
    DiscoverModels(const std::string& provider_id) override;

    std::tuple<bool, std::string, std::vector<sdbus::Struct<
        std::string, std::string, bool, bool, bool>>>
    GetAllModels(const std::string& provider_id) override;

    std::tuple<bool, std::string>
    SetModelEnabled(const std::string& provider_id, const std::string& model_id,
                    const bool& enabled) override;

    std::tuple<bool, std::string>
    SetRouteRules(const std::string& virtual_model_id, const std::string& display_name,
                  const sdbus::Struct<bool, bool, bool, bool, bool>& capabilities,
                  const std::string& metadata_json,
                  const std::vector<sdbus::Struct<std::string, std::string>>& targets,
                  const int32_t& strategy) override;

    std::tuple<bool, std::string, std::string,
               sdbus::Struct<bool, bool, bool, bool, bool>,
               std::string,
               std::vector<sdbus::Struct<std::string, std::string>>, int32_t>
    GetRouteRules(const std::string& virtual_model_id) override;

    std::tuple<bool, std::string, std::string>
    ListRouteRules() override;

    std::tuple<bool, std::string, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, double>
    GetMetricsSnapshot() override;

    std::tuple<bool, std::string, std::string>
    GetMetricsRange(const int64_t& start_ms, const int64_t& end_ms) override;

    std::tuple<bool, std::string, std::vector<sdbus::Struct<
        std::string, std::string, int32_t, int64_t, int64_t>>>
    ListConnections() override;

    std::tuple<bool, std::string, std::vector<sdbus::Struct<
        std::string, std::string, int64_t, int64_t>>>
    GetAllowlist() override;

    std::tuple<bool, std::string>
    RemoveFromAllowlist(const std::string& app_path) override;

    Service& svc_;
};

} // namespace firebox::backend
