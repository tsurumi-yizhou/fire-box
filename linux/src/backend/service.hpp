#pragma once
/// @file service.hpp
/// Main FireBox backend service orchestrator.

#include "common/coroutine.hpp"
#include "common/storage.hpp"
#include "common/dbus_types.hpp"
#include "router.hpp"

#include <sdbus-c++/sdbus-c++.h>
#include <memory>
#include <mutex>
#include <string>
#include <unordered_map>

namespace firebox::backend {

class CapabilityAdaptor;
class ControlAdaptor;

/// The central backend service.  Owns storage, router, and D-Bus adaptors.
class Service {
public:
    Service();
    ~Service();

    /// Run the GLib + D-Bus main loop (blocks).
    void run();

    /// Stop the service.
    void quit();

    // ── Access for adaptors ──────────────────────────────────
    Storage& storage() { return *storage_; }
    Router& router() { return *router_; }

    /// D-Bus connection for caller identity checks.
    sdbus::IConnection& dbus_connection() { return *dbus_connection_; }

    /// Resolve a virtual model id to the first physical (provider_id, model_id).
    std::pair<std::string, std::string> resolve_route(
        const std::string& virtual_model_id);

    /// TOFU: check if a caller (by exe path) is authorized.
    bool check_authorization(const std::string& app_path,
                             const std::string& app_name);

    /// Register an active connection.
    std::string register_connection(const std::string& client_name, int32_t pid);
    void unregister_connection(const std::string& connection_id);
    std::vector<ConnectionInfo> list_connections() const;
    void increment_request_count(const std::string& connection_id);

private:
    std::unique_ptr<Storage> storage_;
    std::unique_ptr<Router> router_;
    std::unique_ptr<sdbus::IConnection> dbus_connection_;
    std::unique_ptr<CapabilityAdaptor> capability_adaptor_;
    std::unique_ptr<ControlAdaptor> control_adaptor_;

    // Active connections
    mutable std::mutex conn_mutex_;
    std::unordered_map<std::string, ConnectionInfo> connections_;
    int next_conn_id_ = 1;

    GMainLoop* main_loop_ = nullptr;
};

} // namespace firebox::backend
