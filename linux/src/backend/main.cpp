/// @file main.cpp — FireBox backend service entry point.
#include "service.hpp"
#include <csignal>
#include <cstdlib>

static firebox::backend::Service* g_service = nullptr;

static void signal_handler(int /*sig*/) {
    if (g_service) g_service->quit();
}

int main() {
    std::signal(SIGINT, signal_handler);
    std::signal(SIGTERM, signal_handler);

    firebox::backend::Service service;
    g_service = &service;
    service.run();
    g_service = nullptr;
    return 0;
}
