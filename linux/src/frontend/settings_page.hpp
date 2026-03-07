#pragma once
/// @file settings_page.hpp
/// Provider configuration page using AdwPreferencesPage.

#include <adwaita.h>
#include <sdbus-c++/sdbus-c++.h>
#include <vector>

namespace firebox::frontend {

class SettingsPage {
public:
    SettingsPage(sdbus::IProxy* proxy, GtkWindow* parent);
    ~SettingsPage() = default;

    /// Returns the root AdwPreferencesPage widget to place in the stack.
    GtkWidget* widget() const;

    void refresh_providers();

    // Expose for C-style callbacks
    void show_add_api_key_dialog();
    void show_add_oauth_dialog();
    void show_models_dialog(const std::string& provider_id,
                            const std::string& provider_name);
    void show_edit_provider_dialog(const std::string& provider_id,
                                    const std::string& name,
                                    int type_id,
                                    const std::string& base_url,
                                    bool enabled);
    sdbus::IProxy* proxy() const { return proxy_; }
    GtkWindow* parent() const { return parent_; }
private:
    sdbus::IProxy* proxy_;
    GtkWindow*     parent_;

    AdwPreferencesPage*  page_widget_     = nullptr;
    AdwPreferencesGroup* providers_group_ = nullptr;

    // Tracked provider rows so we can clear them on refresh
    std::vector<GtkWidget*> provider_rows_;
};

} // namespace firebox::frontend
