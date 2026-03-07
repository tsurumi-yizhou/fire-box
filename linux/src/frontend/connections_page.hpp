#pragma once
/// @file connections_page.hpp
/// Active connections monitoring page.

#include <adwaita.h>
#include <gtkmm.h>
#include <sdbus-c++/sdbus-c++.h>
#include <vector>

namespace firebox::frontend {

class ConnectionsPage : public Gtk::Box {
public:
    explicit ConnectionsPage(sdbus::IProxy* proxy);
    void refresh_connections();

private:
    void setup_ui();
    bool on_auto_refresh();

    sdbus::IProxy*       proxy_;
    AdwPreferencesGroup* group_ = nullptr;
    std::vector<GtkWidget*> rows_;
    sigc::connection     timer_;
};

} // namespace firebox::frontend
