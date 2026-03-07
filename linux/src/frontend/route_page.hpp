#pragma once
/// @file route_page.hpp
/// Route rules configuration page.

#include <adwaita.h>
#include <gtkmm.h>
#include <sdbus-c++/sdbus-c++.h>
#include <string>
#include <vector>

namespace firebox::frontend {

class RoutePage : public Gtk::Box {
public:
    explicit RoutePage(sdbus::IProxy* proxy);
    sdbus::IProxy* proxy() const { return proxy_; }
    void refresh_rules();
    void show_create_dialog(const std::string& prefill_id  = "",
                            const std::string& prefill_name = "");

private:
    void setup_ui();

    sdbus::IProxy*       proxy_;
    AdwPreferencesGroup* group_ = nullptr;
    std::vector<GtkWidget*> rows_;
};

} // namespace firebox::frontend
