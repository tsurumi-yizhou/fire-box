#pragma once
/// @file window.hpp
/// Main application window — wraps AdwApplicationWindow.

#include <adwaita.h>
#include <sdbus-c++/sdbus-c++.h>
#include <memory>

namespace firebox::frontend {

class DashboardPage;
class SettingsPage;
class RoutePage;
class AllowlistPage;
class ConnectionsPage;

class MainWindow {
public:
    explicit MainWindow(GtkApplication* app);
    ~MainWindow();

    void present();
    void set_visible(bool visible);

private:
    void setup_dbus();
    void setup_ui();

    AdwApplicationWindow* win_   = nullptr;
    GtkStack*             stack_ = nullptr;

    std::unique_ptr<sdbus::IConnection> dbus_connection_;
    std::unique_ptr<sdbus::IProxy>      control_proxy_;

    std::unique_ptr<DashboardPage>   dashboard_page_;
    std::unique_ptr<SettingsPage>    settings_page_;
    std::unique_ptr<RoutePage>       route_page_;
    std::unique_ptr<AllowlistPage>   allowlist_page_;
    std::unique_ptr<ConnectionsPage> connections_page_;
};

} // namespace firebox::frontend

