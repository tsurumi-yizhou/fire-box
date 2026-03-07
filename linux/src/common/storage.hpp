#pragma once
/// @file storage.hpp
/// SQLite storage layer for metrics, allowlist, providers, and routes.

#include "dbus_types.hpp"
#include <sqlite3.h>
#include <memory>
#include <mutex>
#include <string>
#include <vector>

namespace firebox {

/// Thread-safe RAII wrapper around sqlite3*
class Storage {
public:
    /// Opens (or creates) the database at the given path.
    explicit Storage(const std::string& db_path);
    ~Storage();

    Storage(const Storage&) = delete;
    Storage& operator=(const Storage&) = delete;

    // ── Providers ────────────────────────────────────────────
    std::string upsert_provider(const Provider& p);
    bool update_provider(const std::string& provider_id,
                         const std::string& name,
                         const std::string& base_url);
    bool delete_provider(const std::string& provider_id);
    std::vector<Provider> list_providers();

    // ── Models ───────────────────────────────────────────────
    void upsert_model(const Model& m);
    bool set_model_enabled(const std::string& provider_id,
                           const std::string& model_id, bool enabled);
    std::vector<Model> get_models(const std::string& provider_id = "");

    // ── Routes ───────────────────────────────────────────────
    void set_route_rule(const RouteRule& rule);
    bool delete_route_rule(const std::string& virtual_model_id);
    std::vector<RouteRule> list_route_rules();
    RouteRule get_route_rule(const std::string& virtual_model_id);

    // ── Metrics ──────────────────────────────────────────────
    void record_metrics(const MetricsSnapshot& snap);
    MetricsSnapshot get_metrics_snapshot();
    std::vector<MetricsSnapshot> get_metrics_range(int64_t start_ms, int64_t end_ms);

    // ── Allowlist ────────────────────────────────────────────
    void add_to_allowlist(const AllowedApp& app);
    bool remove_from_allowlist(const std::string& app_path);
    std::vector<AllowedApp> get_allowlist();
    bool is_allowed(const std::string& app_path);
    void update_last_used(const std::string& app_path, int64_t timestamp_ms);

private:
    void create_tables();
    void exec(const char* sql);

    struct SqliteDeleter {
        void operator()(sqlite3* db) const noexcept { sqlite3_close(db); }
    };
    std::unique_ptr<sqlite3, SqliteDeleter> db_;
    mutable std::mutex mutex_;
};

} // namespace firebox
