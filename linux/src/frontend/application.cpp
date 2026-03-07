/// @file application.cpp
#include "application.hpp"
#include "window.hpp"
#include "common/log.hpp"
#include <spdlog/spdlog.h>

namespace firebox::frontend {

Application::Application()
    : Gtk::Application("org.firebox.Frontend",
                       Gio::Application::Flags::DEFAULT_FLAGS) {
}

Glib::RefPtr<Application> Application::create() {
    return Glib::make_refptr_for_instance<Application>(new Application());
}

Application::~Application() = default;

void Application::on_startup() {
    Gtk::Application::on_startup();

    // Initialize libadwaita
    adw_init();

    log::init("firebox-frontend");
    spdlog::info("FireBox frontend starting");
}

void Application::on_activate() {
    if (!window_) {
        window_ = std::make_unique<MainWindow>(GTK_APPLICATION(gobj()));
    }
    window_->present();
}

} // namespace firebox::frontend
