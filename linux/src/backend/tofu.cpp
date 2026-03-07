/// @file tofu.cpp
#include "tofu.hpp"
#include <polkit/polkit.h>
#include <spdlog/spdlog.h>
#include <chrono>

namespace firebox::backend {

firebox::Task<bool> authorize_tofu(
    firebox::Storage& storage,
    const std::string& app_path,
    const std::string& app_name,
    uint32_t pid) {

    // Fast path: already authorized
    if (storage.is_allowed(app_path)) {
        auto now = std::chrono::duration_cast<std::chrono::milliseconds>(
                       std::chrono::system_clock::now().time_since_epoch())
                       .count();
        storage.update_last_used(app_path, now);
        spdlog::info("TOFU: {} is already authorized", app_path);
        co_return true;
    }

    spdlog::info("TOFU: Requesting polkit authorization for '{}' (PID {})",
                 app_name, pid);

    // Use GIO polkit async check via coroutine wrapper
    bool authorized = co_await firebox::gio_async<bool>(
        [&](GAsyncReadyCallback cb, gpointer ud) {
            auto* authority = polkit_authority_get_sync(nullptr, nullptr);
            if (!authority) {
                // polkit not available: deny
                // We need to invoke cb manually with a "failed" result
                spdlog::error("TOFU: Failed to get polkit authority");
                // Create a simple GTask to pass back
                auto* task = g_task_new(nullptr, nullptr, cb, ud);
                g_task_return_boolean(task, FALSE);
                g_object_unref(task);
                return;
            }

            auto* subject = polkit_unix_process_new_for_owner(
                static_cast<gint>(pid), 0, -1);

            polkit_authority_check_authorization(
                authority,
                subject,
                "org.firebox.authorize-client",
                nullptr,  // details
                POLKIT_CHECK_AUTHORIZATION_FLAGS_ALLOW_USER_INTERACTION,
                nullptr,  // cancellable
                cb, ud);

            g_object_unref(subject);
            g_object_unref(authority);
        },
        [](GAsyncResult* res) -> bool {
            GError* error = nullptr;

            // Try to get polkit result
            auto* authority = POLKIT_AUTHORITY(g_async_result_get_source_object(res));
            if (authority) {
                auto* result = polkit_authority_check_authorization_finish(
                    authority, res, &error);
                if (error) {
                    spdlog::error("TOFU: polkit error: {}", error->message);
                    g_error_free(error);
                    return false;
                }
                bool auth = polkit_authorization_result_get_is_authorized(result);
                g_object_unref(result);
                return auth;
            }

            // Fallback: GTask boolean result
            return g_task_propagate_boolean(G_TASK(res), nullptr) != FALSE;
        });

    if (authorized) {
        auto now = std::chrono::duration_cast<std::chrono::milliseconds>(
                       std::chrono::system_clock::now().time_since_epoch())
                       .count();
        firebox::AllowedApp app;
        app.app_path = app_path;
        app.display_name = app_name;
        app.first_seen_ms = now;
        app.last_used_ms = now;
        storage.add_to_allowlist(app);
        spdlog::info("TOFU: '{}' authorized and added to allowlist", app_name);
    } else {
        spdlog::warn("TOFU: '{}' was denied by user", app_name);
    }

    co_return authorized;
}

} // namespace firebox::backend
