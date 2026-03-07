#pragma once
/// @file allowlist_page.hpp
/// Application allowlist management page.

#include <adwaita.h>
#include <gtkmm.h>
#include <sdbus-c++/sdbus-c++.h>
#include <vector>

namespace firebox::frontend {

class AllowlistPage : public Gtk::Box {
public:
    explicit AllowlistPage(sdbus::IProxy* proxy);
    sdbus::IProxy* proxy() const { return proxy_; }
    void refresh_allowlist();

private:
    void setup_ui();

    sdbus::IProxy*       proxy_;
    AdwPreferencesGroup* group_ = nullptr;
    std::vector<GtkWidget*> rows_;
};

} // namespace firebox::frontend
