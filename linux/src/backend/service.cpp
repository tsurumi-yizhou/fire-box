/// @file service.cpp
#include "service.hpp"
#include "capability_adaptor.hpp"
#include "control_adaptor.hpp"
#include "common/log.hpp"
#include "common/credential.hpp"

#include <spdlog/spdlog.h>
#include <filesystem>
#include <chrono>

namespace fs = std::filesystem;

namespace firebox::backend {

namespace {

fs::path safe_home_dir() {
    const char* home = std::getenv("HOME");
    if (home && home[0] != '\0') return fs::path(home);
    spdlog::warn("HOME environment variable not set, using /tmp");
    return fs::path("/tmp");
}

std::string data_dir() {
    const char* xdg = std::getenv("XDG_DATA_HOME");
    fs::path base = (xdg && xdg[0] != '\0')
        ? fs::path(xdg)
        : safe_home_dir() / ".local" / "share";
    auto dir = base / "firebox";
    fs::create_directories(dir);
    return dir.string();
}

} // namespace

Service::Service() {
    log::init("firebox-backend");
    spdlog::info("FireBox backend starting…");

    // Open database
    auto db_path = data_dir() + "/firebox.db";
    storage_ = std::make_unique<Storage>(db_path);
    spdlog::info("Database opened: {}", db_path);

    router_ = std::make_unique<Router>(*storage_);

    // Connect to session bus
    dbus_connection_ = sdbus::createSessionBusConnection(sdbus::ServiceName{"org.firebox"});

    // Create adaptors on the D-Bus object path
    capability_adaptor_ = std::make_unique<CapabilityAdaptor>(
        *this, *dbus_connection_, sdbus::ObjectPath{"/org/firebox"});
    control_adaptor_ = std::make_unique<ControlAdaptor>(
        *this, *dbus_connection_, sdbus::ObjectPath{"/org/firebox"});

    spdlog::info("D-Bus adaptors registered on org.firebox");
}

Service::~Service() {
    if (main_loop_) g_main_loop_unref(main_loop_);
}

void Service::run() {
    // Integrate sdbus event loop with GLib main loop using idle callbacks
    // sdbus-c++ can work in its own thread; we'll run it in a separate thread
    // while GLib main loop runs on the main thread.

    main_loop_ = g_main_loop_new(nullptr, FALSE);

    // Run sdbus event loop in a background thread
    std::thread dbus_thread([this] {
        dbus_connection_->enterEventLoopAsync();
    });

    spdlog::info("FireBox backend running");
    g_main_loop_run(main_loop_);

    // Cleanup
    dbus_connection_->leaveEventLoop();
    if (dbus_thread.joinable()) dbus_thread.join();
    spdlog::info("FireBox backend stopped");
}

void Service::quit() {
    if (main_loop_) g_main_loop_quit(main_loop_);
}

// ── Routing ──────────────────────────────────────────────────────

std::pair<std::string, std::string> Service::resolve_route(
    const std::string& virtual_model_id) {
    auto targets = router_->resolve(virtual_model_id);
    if (targets.empty()) return {};
    return {targets[0].provider_id, targets[0].model_id};
}

// ── TOFU ─────────────────────────────────────────────────────────

bool Service::check_authorization(const std::string& app_path,
                                  const std::string& /*app_name*/) {
    return storage_->is_allowed(app_path);
}

// ── Connection tracking ──────────────────────────────────────────

std::string Service::register_connection(const std::string& client_name,
                                         int32_t pid) {
    std::lock_guard lock(conn_mutex_);
    auto id = "conn-" + std::to_string(next_conn_id_++);
    ConnectionInfo info;
    info.connection_id = id;
    info.client_name   = client_name;
    info.pid           = pid;
    info.connected_at_ms =
        std::chrono::duration_cast<std::chrono::milliseconds>(
            std::chrono::system_clock::now().time_since_epoch())
            .count();
    info.requests_count = 0;
    connections_[id] = std::move(info);
    return id;
}

void Service::unregister_connection(const std::string& connection_id) {
    std::lock_guard lock(conn_mutex_);
    connections_.erase(connection_id);
}

std::vector<ConnectionInfo> Service::list_connections() const {
    std::lock_guard lock(conn_mutex_);
    std::vector<ConnectionInfo> result;
    result.reserve(connections_.size());
    for (auto& [_, c] : connections_) result.push_back(c);
    return result;
}

void Service::increment_request_count(const std::string& connection_id) {
    std::lock_guard lock(conn_mutex_);
    if (auto it = connections_.find(connection_id); it != connections_.end())
        ++it->second.requests_count;
}

} // namespace firebox::backend
