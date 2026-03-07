#pragma once
/// @file application.hpp
/// Main application class.

#include <adwaita.h>
#include <gtkmm.h>
#include <memory>

namespace firebox::frontend {

class MainWindow;

class Application : public Gtk::Application {
public:
    static Glib::RefPtr<Application> create();

protected:
    Application();
    ~Application() override;  // defined in .cpp where MainWindow is complete

    void on_activate() override;
    void on_startup() override;

private:
    std::unique_ptr<MainWindow> window_;
};

} // namespace firebox::frontend
