/// @file main.cpp — FireBox frontend GUI entry point.
#include "application.hpp"
#include <adwaita.h>

int main(int argc, char* argv[]) {
    // Initialize libadwaita before creating the application
    adw_init();

    auto app = firebox::frontend::Application::create();
    return app->run(argc, argv);
}
