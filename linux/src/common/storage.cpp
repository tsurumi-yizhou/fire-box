/// @file storage.cpp
#include "storage.hpp"
#include <spdlog/spdlog.h>
#include <stdexcept>
#include <chrono>

namespace firebox {

namespace {
/// Returns empty string instead of crashing when sqlite3_column_text returns NULL.
std::string safe_column_text(sqlite3_stmt* stmt, int col) {
    auto ptr = sqlite3_column_text(stmt, col);
    return ptr ? reinterpret_cast<const char*>(ptr) : std::string{};
}
} // namespace

Storage::Storage(const std::string& db_path) {
    sqlite3* raw = nullptr;
    int rc = sqlite3_open(db_path.c_str(), &raw);
    db_.reset(raw);
    if (rc != SQLITE_OK) {
        throw std::runtime_error(
            std::string("Failed to open database: ") + sqlite3_errmsg(raw));
    }
    exec("PRAGMA journal_mode=WAL");
    exec("PRAGMA foreign_keys=ON");
    create_tables();
}

Storage::~Storage() = default;

void Storage::exec(const char* sql) {
    char* err = nullptr;
    int rc = sqlite3_exec(db_.get(), sql, nullptr, nullptr, &err);
    if (rc != SQLITE_OK) {
        std::string msg = err ? err : "unknown error";
        sqlite3_free(err);
        throw std::runtime_error("SQL error: " + msg);
    }
}

void Storage::create_tables() {
    exec(R"SQL(
        CREATE TABLE IF NOT EXISTS providers (
            provider_id TEXT PRIMARY KEY,
            name        TEXT NOT NULL UNIQUE,
            type        INTEGER NOT NULL,
            base_url    TEXT DEFAULT '',
            local_path  TEXT DEFAULT '',
            enabled     INTEGER DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS models (
            model_id    TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            enabled     INTEGER DEFAULT 1,
            cap_chat    INTEGER DEFAULT 1,
            cap_streaming INTEGER DEFAULT 1,
            PRIMARY KEY (provider_id, model_id),
            FOREIGN KEY (provider_id) REFERENCES providers(provider_id)
                ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS route_rules (
            virtual_model_id TEXT PRIMARY KEY,
            display_name     TEXT NOT NULL,
            cap_chat         INTEGER DEFAULT 1,
            cap_streaming    INTEGER DEFAULT 1,
            cap_embeddings   INTEGER DEFAULT 0,
            cap_vision       INTEGER DEFAULT 0,
            cap_tool_calling INTEGER DEFAULT 0,
            context_window   INTEGER DEFAULT 0,
            pricing_tier     TEXT DEFAULT '',
            description      TEXT DEFAULT '',
            strengths        TEXT DEFAULT '',
            strategy         INTEGER DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS route_targets (
            virtual_model_id TEXT NOT NULL,
            provider_id      TEXT NOT NULL,
            model_id         TEXT NOT NULL,
            sort_order       INTEGER DEFAULT 0,
            FOREIGN KEY (virtual_model_id) REFERENCES route_rules(virtual_model_id)
                ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS metrics (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            window_start_ms  INTEGER,
            window_end_ms    INTEGER,
            requests_total   INTEGER DEFAULT 0,
            requests_failed  INTEGER DEFAULT 0,
            prompt_tokens    INTEGER DEFAULT 0,
            completion_tokens INTEGER DEFAULT 0,
            cost_total       REAL DEFAULT 0.0
        );

        CREATE TABLE IF NOT EXISTS allowlist (
            app_path      TEXT PRIMARY KEY,
            display_name  TEXT NOT NULL,
            first_seen_ms INTEGER NOT NULL,
            last_used_ms  INTEGER NOT NULL
        );
    )SQL");
    // Migration: add subtype column if it doesn't exist yet
    char* mig_err = nullptr;
    sqlite3_exec(db_.get(),
        "ALTER TABLE providers ADD COLUMN subtype TEXT DEFAULT ''",
        nullptr, nullptr, &mig_err);
    sqlite3_free(mig_err); // ignore error if column already exists
}

// ── Providers ────────────────────────────────────────────────────

std::string Storage::upsert_provider(const Provider& p) {
    std::lock_guard<std::mutex> lock(mutex_);
    const char* sql = R"SQL(
        INSERT INTO providers (provider_id, name, type, subtype, base_url, local_path, enabled)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(name) DO UPDATE SET
            provider_id = excluded.provider_id,
            type        = excluded.type,
            subtype     = excluded.subtype,
            base_url    = excluded.base_url,
            local_path  = excluded.local_path,
            enabled     = excluded.enabled
    )SQL";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    sqlite3_bind_text(stmt, 1, p.provider_id.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_text(stmt, 2, p.name.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_int(stmt, 3, static_cast<int>(p.type));
    sqlite3_bind_text(stmt, 4, p.subtype.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_text(stmt, 5, p.base_url.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_text(stmt, 6, p.local_path.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_int(stmt, 7, p.enabled ? 1 : 0);
    sqlite3_step(stmt);
    sqlite3_finalize(stmt);
    return p.provider_id;
}

bool Storage::update_provider(const std::string& provider_id,
                              const std::string& name,
                              const std::string& base_url) {
    std::lock_guard<std::mutex> lock(mutex_);
    const char* sql =
        "UPDATE providers SET name = ?2, base_url = ?3 WHERE provider_id = ?1";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    sqlite3_bind_text(stmt, 1, provider_id.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_text(stmt, 2, name.c_str(),        -1, SQLITE_TRANSIENT);
    sqlite3_bind_text(stmt, 3, base_url.c_str(),    -1, SQLITE_TRANSIENT);
    sqlite3_step(stmt);
    bool changed = sqlite3_changes(db_.get()) > 0;
    sqlite3_finalize(stmt);
    return changed;
}

bool Storage::delete_provider(const std::string& provider_id) {
    std::lock_guard<std::mutex> lock(mutex_);
    const char* sql = "DELETE FROM providers WHERE provider_id = ?1";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    sqlite3_bind_text(stmt, 1, provider_id.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_step(stmt);
    int changes = sqlite3_changes(db_.get());
    sqlite3_finalize(stmt);
    return changes > 0;
}

std::vector<Provider> Storage::list_providers() {
    std::lock_guard<std::mutex> lock(mutex_);
    std::vector<Provider> result;
    const char* sql =
        "SELECT provider_id, name, type, subtype, base_url, local_path, enabled FROM providers";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    while (sqlite3_step(stmt) == SQLITE_ROW) {
        Provider p;
        p.provider_id = safe_column_text(stmt, 0);
        p.name        = safe_column_text(stmt, 1);
        p.type        = static_cast<ProviderType>(sqlite3_column_int(stmt, 2));
        p.subtype     = safe_column_text(stmt, 3);
        p.base_url    = safe_column_text(stmt, 4);
        p.local_path  = safe_column_text(stmt, 5);
        p.enabled     = sqlite3_column_int(stmt, 6) != 0;
        result.push_back(std::move(p));
    }
    sqlite3_finalize(stmt);
    return result;
}

// ── Models ───────────────────────────────────────────────────────

void Storage::upsert_model(const Model& m) {
    std::lock_guard<std::mutex> lock(mutex_);
    const char* sql = R"SQL(
        INSERT INTO models (model_id, provider_id, enabled, cap_chat, cap_streaming)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(provider_id, model_id) DO UPDATE SET
            enabled = excluded.enabled,
            cap_chat = excluded.cap_chat,
            cap_streaming = excluded.cap_streaming
    )SQL";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    sqlite3_bind_text(stmt, 1, m.model_id.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_text(stmt, 2, m.provider_id.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_int(stmt, 3, m.enabled ? 1 : 0);
    sqlite3_bind_int(stmt, 4, m.capability_chat ? 1 : 0);
    sqlite3_bind_int(stmt, 5, m.capability_streaming ? 1 : 0);
    sqlite3_step(stmt);
    sqlite3_finalize(stmt);
}

bool Storage::set_model_enabled(const std::string& provider_id,
                                const std::string& model_id, bool enabled) {
    std::lock_guard<std::mutex> lock(mutex_);
    const char* sql =
        "UPDATE models SET enabled = ?3 WHERE provider_id = ?1 AND model_id = ?2";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    sqlite3_bind_text(stmt, 1, provider_id.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_text(stmt, 2, model_id.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_int(stmt, 3, enabled ? 1 : 0);
    sqlite3_step(stmt);
    int changes = sqlite3_changes(db_.get());
    sqlite3_finalize(stmt);
    return changes > 0;
}

std::vector<Model> Storage::get_models(const std::string& provider_id) {
    std::lock_guard<std::mutex> lock(mutex_);
    std::vector<Model> result;
    std::string sql = "SELECT model_id, provider_id, enabled, cap_chat, cap_streaming FROM models";
    if (!provider_id.empty()) sql += " WHERE provider_id = ?1";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql.c_str(), -1, &stmt, nullptr);
    if (!provider_id.empty())
        sqlite3_bind_text(stmt, 1, provider_id.c_str(), -1, SQLITE_TRANSIENT);
    while (sqlite3_step(stmt) == SQLITE_ROW) {
        Model m;
        m.model_id            = safe_column_text(stmt, 0);
        m.provider_id         = safe_column_text(stmt, 1);
        m.enabled             = sqlite3_column_int(stmt, 2) != 0;
        m.capability_chat     = sqlite3_column_int(stmt, 3) != 0;
        m.capability_streaming = sqlite3_column_int(stmt, 4) != 0;
        result.push_back(std::move(m));
    }
    sqlite3_finalize(stmt);
    return result;
}

// ── Routes ───────────────────────────────────────────────────────

void Storage::set_route_rule(const RouteRule& rule) {
    std::lock_guard<std::mutex> lock(mutex_);
    // Delete existing first (cascade deletes targets) — call exec directly, not public method
    {
        const char* del_sql = "DELETE FROM route_rules WHERE virtual_model_id = ?1";
        sqlite3_stmt* del_stmt = nullptr;
        sqlite3_prepare_v2(db_.get(), del_sql, -1, &del_stmt, nullptr);
        sqlite3_bind_text(del_stmt, 1, rule.virtual_model_id.c_str(), -1, SQLITE_TRANSIENT);
        sqlite3_step(del_stmt);
        sqlite3_finalize(del_stmt);
    }

    if (rule.targets.empty()) return; // empty targets → delete only

    const char* sql = R"SQL(
        INSERT INTO route_rules
            (virtual_model_id, display_name,
             cap_chat, cap_streaming, cap_embeddings, cap_vision, cap_tool_calling,
             context_window, pricing_tier, description, strengths, strategy)
        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
    )SQL";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    sqlite3_bind_text(stmt, 1, rule.virtual_model_id.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_text(stmt, 2, rule.display_name.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_int(stmt, 3, rule.capabilities.chat ? 1 : 0);
    sqlite3_bind_int(stmt, 4, rule.capabilities.streaming ? 1 : 0);
    sqlite3_bind_int(stmt, 5, rule.capabilities.embeddings ? 1 : 0);
    sqlite3_bind_int(stmt, 6, rule.capabilities.vision ? 1 : 0);
    sqlite3_bind_int(stmt, 7, rule.capabilities.tool_calling ? 1 : 0);
    sqlite3_bind_int(stmt, 8, rule.metadata.context_window);
    sqlite3_bind_text(stmt, 9, rule.metadata.pricing_tier.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_text(stmt, 10, rule.metadata.description.c_str(), -1, SQLITE_TRANSIENT);
    // Join strengths with comma
    std::string strengths_str;
    for (size_t i = 0; i < rule.metadata.strengths.size(); ++i) {
        if (i > 0) strengths_str += ',';
        strengths_str += rule.metadata.strengths[i];
    }
    sqlite3_bind_text(stmt, 11, strengths_str.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_int(stmt, 12, static_cast<int>(rule.strategy));
    sqlite3_step(stmt);
    sqlite3_finalize(stmt);

    // Insert targets
    const char* tgt_sql = R"SQL(
        INSERT INTO route_targets (virtual_model_id, provider_id, model_id, sort_order)
        VALUES (?1, ?2, ?3, ?4)
    )SQL";
    for (int i = 0; i < static_cast<int>(rule.targets.size()); ++i) {
        sqlite3_prepare_v2(db_.get(), tgt_sql, -1, &stmt, nullptr);
        sqlite3_bind_text(stmt, 1, rule.virtual_model_id.c_str(), -1, SQLITE_TRANSIENT);
        sqlite3_bind_text(stmt, 2, rule.targets[i].provider_id.c_str(), -1, SQLITE_TRANSIENT);
        sqlite3_bind_text(stmt, 3, rule.targets[i].model_id.c_str(), -1, SQLITE_TRANSIENT);
        sqlite3_bind_int(stmt, 4, i);
        sqlite3_step(stmt);
        sqlite3_finalize(stmt);
    }
}

bool Storage::delete_route_rule(const std::string& virtual_model_id) {
    std::lock_guard<std::mutex> lock(mutex_);
    const char* sql = "DELETE FROM route_rules WHERE virtual_model_id = ?1";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    sqlite3_bind_text(stmt, 1, virtual_model_id.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_step(stmt);
    int changes = sqlite3_changes(db_.get());
    sqlite3_finalize(stmt);
    return changes > 0;
}

std::vector<RouteRule> Storage::list_route_rules() {
    std::lock_guard<std::mutex> lock(mutex_);
    std::vector<RouteRule> result;
    const char* sql = R"SQL(
        SELECT virtual_model_id, display_name,
               cap_chat, cap_streaming, cap_embeddings, cap_vision, cap_tool_calling,
               context_window, pricing_tier, description, strengths, strategy
        FROM route_rules
    )SQL";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    while (sqlite3_step(stmt) == SQLITE_ROW) {
        RouteRule r;
        r.virtual_model_id = safe_column_text(stmt, 0);
        r.display_name     = safe_column_text(stmt, 1);
        r.capabilities.chat         = sqlite3_column_int(stmt, 2) != 0;
        r.capabilities.streaming    = sqlite3_column_int(stmt, 3) != 0;
        r.capabilities.embeddings   = sqlite3_column_int(stmt, 4) != 0;
        r.capabilities.vision       = sqlite3_column_int(stmt, 5) != 0;
        r.capabilities.tool_calling = sqlite3_column_int(stmt, 6) != 0;
        r.metadata.context_window = sqlite3_column_int(stmt, 7);
        r.metadata.pricing_tier   = safe_column_text(stmt, 8);
        r.metadata.description    = safe_column_text(stmt, 9);
        // Parse strengths
        std::string s = safe_column_text(stmt, 10);
        if (!s.empty()) {
            size_t pos = 0;
            while ((pos = s.find(',')) != std::string::npos) {
                r.metadata.strengths.push_back(s.substr(0, pos));
                s.erase(0, pos + 1);
            }
            r.metadata.strengths.push_back(s);
        }
        r.strategy = static_cast<RouteStrategy>(sqlite3_column_int(stmt, 11));

        // Load targets
        const char* tgt_sql =
            "SELECT provider_id, model_id FROM route_targets "
            "WHERE virtual_model_id = ?1 ORDER BY sort_order";
        sqlite3_stmt* tgt_stmt = nullptr;
        sqlite3_prepare_v2(db_.get(), tgt_sql, -1, &tgt_stmt, nullptr);
        sqlite3_bind_text(tgt_stmt, 1, r.virtual_model_id.c_str(), -1, SQLITE_TRANSIENT);
        while (sqlite3_step(tgt_stmt) == SQLITE_ROW) {
            RouteTarget t;
            t.provider_id = safe_column_text(tgt_stmt, 0);
            t.model_id    = safe_column_text(tgt_stmt, 1);
            r.targets.push_back(std::move(t));
        }
        sqlite3_finalize(tgt_stmt);

        result.push_back(std::move(r));
    }
    sqlite3_finalize(stmt);
    return result;
}

RouteRule Storage::get_route_rule(const std::string& virtual_model_id) {
    std::lock_guard<std::mutex> lock(mutex_);
    RouteRule r;
    const char* sql = R"SQL(
        SELECT virtual_model_id, display_name,
               cap_chat, cap_streaming, cap_embeddings, cap_vision, cap_tool_calling,
               context_window, pricing_tier, description, strengths, strategy
        FROM route_rules WHERE virtual_model_id = ?1
    )SQL";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    sqlite3_bind_text(stmt, 1, virtual_model_id.c_str(), -1, SQLITE_TRANSIENT);
    if (sqlite3_step(stmt) != SQLITE_ROW) {
        sqlite3_finalize(stmt);
        return r;
    }
    r.virtual_model_id = safe_column_text(stmt, 0);
    r.display_name     = safe_column_text(stmt, 1);
    r.capabilities.chat         = sqlite3_column_int(stmt, 2) != 0;
    r.capabilities.streaming    = sqlite3_column_int(stmt, 3) != 0;
    r.capabilities.embeddings   = sqlite3_column_int(stmt, 4) != 0;
    r.capabilities.vision       = sqlite3_column_int(stmt, 5) != 0;
    r.capabilities.tool_calling = sqlite3_column_int(stmt, 6) != 0;
    r.metadata.context_window = sqlite3_column_int(stmt, 7);
    r.metadata.pricing_tier   = safe_column_text(stmt, 8);
    r.metadata.description    = safe_column_text(stmt, 9);
    std::string s = safe_column_text(stmt, 10);
    if (!s.empty()) {
        size_t pos = 0;
        while ((pos = s.find(',')) != std::string::npos) {
            r.metadata.strengths.push_back(s.substr(0, pos));
            s.erase(0, pos + 1);
        }
        r.metadata.strengths.push_back(s);
    }
    r.strategy = static_cast<RouteStrategy>(sqlite3_column_int(stmt, 11));
    sqlite3_finalize(stmt);

    // Load targets
    const char* tgt_sql =
        "SELECT provider_id, model_id FROM route_targets "
        "WHERE virtual_model_id = ?1 ORDER BY sort_order";
    sqlite3_stmt* tgt_stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), tgt_sql, -1, &tgt_stmt, nullptr);
    sqlite3_bind_text(tgt_stmt, 1, r.virtual_model_id.c_str(), -1, SQLITE_TRANSIENT);
    while (sqlite3_step(tgt_stmt) == SQLITE_ROW) {
        RouteTarget t;
        t.provider_id = safe_column_text(tgt_stmt, 0);
        t.model_id    = safe_column_text(tgt_stmt, 1);
        r.targets.push_back(std::move(t));
    }
    sqlite3_finalize(tgt_stmt);

    return r;
}

// ── Metrics ──────────────────────────────────────────────────────

void Storage::record_metrics(const MetricsSnapshot& snap) {
    std::lock_guard<std::mutex> lock(mutex_);
    const char* sql = R"SQL(
        INSERT INTO metrics
            (window_start_ms, window_end_ms, requests_total, requests_failed,
             prompt_tokens, completion_tokens, cost_total)
        VALUES (?1,?2,?3,?4,?5,?6,?7)
    )SQL";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    sqlite3_bind_int64(stmt, 1, snap.window_start_ms);
    sqlite3_bind_int64(stmt, 2, snap.window_end_ms);
    sqlite3_bind_int64(stmt, 3, snap.requests_total);
    sqlite3_bind_int64(stmt, 4, snap.requests_failed);
    sqlite3_bind_int64(stmt, 5, snap.prompt_tokens);
    sqlite3_bind_int64(stmt, 6, snap.completion_tokens);
    sqlite3_bind_double(stmt, 7, snap.cost_total);
    sqlite3_step(stmt);
    sqlite3_finalize(stmt);
}

MetricsSnapshot Storage::get_metrics_snapshot() {
    std::lock_guard<std::mutex> lock(mutex_);
    MetricsSnapshot snap;
    const char* sql = R"SQL(
        SELECT COALESCE(SUM(requests_total),0),
               COALESCE(SUM(requests_failed),0),
               COALESCE(SUM(prompt_tokens),0),
               COALESCE(SUM(completion_tokens),0),
               COALESCE(SUM(cost_total),0.0),
               COALESCE(MIN(window_start_ms),0),
               COALESCE(MAX(window_end_ms),0)
        FROM metrics
    )SQL";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    if (sqlite3_step(stmt) == SQLITE_ROW) {
        snap.requests_total    = sqlite3_column_int64(stmt, 0);
        snap.requests_failed   = sqlite3_column_int64(stmt, 1);
        snap.prompt_tokens     = sqlite3_column_int64(stmt, 2);
        snap.completion_tokens = sqlite3_column_int64(stmt, 3);
        snap.cost_total        = sqlite3_column_double(stmt, 4);
        snap.window_start_ms   = sqlite3_column_int64(stmt, 5);
        snap.window_end_ms     = sqlite3_column_int64(stmt, 6);
    }
    sqlite3_finalize(stmt);
    return snap;
}

std::vector<MetricsSnapshot> Storage::get_metrics_range(int64_t start_ms, int64_t end_ms) {
    std::lock_guard<std::mutex> lock(mutex_);
    std::vector<MetricsSnapshot> result;
    const char* sql = R"SQL(
        SELECT window_start_ms, window_end_ms,
               requests_total, requests_failed,
               prompt_tokens, completion_tokens, cost_total
        FROM metrics
        WHERE window_start_ms >= ?1 AND window_end_ms <= ?2
        ORDER BY window_start_ms
    )SQL";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    sqlite3_bind_int64(stmt, 1, start_ms);
    sqlite3_bind_int64(stmt, 2, end_ms);
    while (sqlite3_step(stmt) == SQLITE_ROW) {
        MetricsSnapshot s;
        s.window_start_ms   = sqlite3_column_int64(stmt, 0);
        s.window_end_ms     = sqlite3_column_int64(stmt, 1);
        s.requests_total    = sqlite3_column_int64(stmt, 2);
        s.requests_failed   = sqlite3_column_int64(stmt, 3);
        s.prompt_tokens     = sqlite3_column_int64(stmt, 4);
        s.completion_tokens = sqlite3_column_int64(stmt, 5);
        s.cost_total        = sqlite3_column_double(stmt, 6);
        result.push_back(s);
    }
    sqlite3_finalize(stmt);
    return result;
}

// ── Allowlist ────────────────────────────────────────────────────

void Storage::add_to_allowlist(const AllowedApp& app) {
    std::lock_guard<std::mutex> lock(mutex_);
    const char* sql = R"SQL(
        INSERT INTO allowlist (app_path, display_name, first_seen_ms, last_used_ms)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(app_path) DO UPDATE SET
            display_name = excluded.display_name,
            last_used_ms = excluded.last_used_ms
    )SQL";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    sqlite3_bind_text(stmt, 1, app.app_path.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_text(stmt, 2, app.display_name.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_int64(stmt, 3, app.first_seen_ms);
    sqlite3_bind_int64(stmt, 4, app.last_used_ms);
    sqlite3_step(stmt);
    sqlite3_finalize(stmt);
}

bool Storage::remove_from_allowlist(const std::string& app_path) {
    std::lock_guard<std::mutex> lock(mutex_);
    const char* sql = "DELETE FROM allowlist WHERE app_path = ?1";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    sqlite3_bind_text(stmt, 1, app_path.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_step(stmt);
    int changes = sqlite3_changes(db_.get());
    sqlite3_finalize(stmt);
    return changes > 0;
}

std::vector<AllowedApp> Storage::get_allowlist() {
    std::lock_guard<std::mutex> lock(mutex_);
    std::vector<AllowedApp> result;
    const char* sql =
        "SELECT app_path, display_name, first_seen_ms, last_used_ms FROM allowlist";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    while (sqlite3_step(stmt) == SQLITE_ROW) {
        AllowedApp a;
        a.app_path      = safe_column_text(stmt, 0);
        a.display_name  = safe_column_text(stmt, 1);
        a.first_seen_ms = sqlite3_column_int64(stmt, 2);
        a.last_used_ms  = sqlite3_column_int64(stmt, 3);
        result.push_back(std::move(a));
    }
    sqlite3_finalize(stmt);
    return result;
}

bool Storage::is_allowed(const std::string& app_path) {
    std::lock_guard<std::mutex> lock(mutex_);
    const char* sql = "SELECT 1 FROM allowlist WHERE app_path = ?1 LIMIT 1";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    sqlite3_bind_text(stmt, 1, app_path.c_str(), -1, SQLITE_TRANSIENT);
    bool found = sqlite3_step(stmt) == SQLITE_ROW;
    sqlite3_finalize(stmt);
    return found;
}

void Storage::update_last_used(const std::string& app_path, int64_t timestamp_ms) {
    std::lock_guard<std::mutex> lock(mutex_);
    const char* sql = "UPDATE allowlist SET last_used_ms = ?2 WHERE app_path = ?1";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(db_.get(), sql, -1, &stmt, nullptr);
    sqlite3_bind_text(stmt, 1, app_path.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_int64(stmt, 2, timestamp_ms);
    sqlite3_step(stmt);
    sqlite3_finalize(stmt);
}

} // namespace firebox
