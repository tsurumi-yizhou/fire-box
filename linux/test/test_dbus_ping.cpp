/// @file test_dbus_ping.cpp
/// Minimal D-Bus round-trip test: register a Ping method and call it.
/// This validates the sdbus-c++ v2 pipeline works end-to-end.

#include <boost/ut.hpp>
#include <sdbus-c++/sdbus-c++.h>
#include <thread>
#include <chrono>

namespace ut = boost::ut;

int main() {
    using namespace ut;

    "dbus_ping_pong"_test = [] {
        // Create a private session bus connection for testing
        auto server_conn = sdbus::createSessionBusConnection(
            sdbus::ServiceName{"org.firebox.test.ping"});

        // Create the server object
        auto server_obj = sdbus::createObject(
            *server_conn, sdbus::ObjectPath{"/org/firebox/test"});

        const auto iface = sdbus::InterfaceName{"org.firebox.test.PingPong"};
        server_obj->addVTable(
            sdbus::registerMethod("Ping")
                .implementedAs([](std::string input) -> std::string {
                    return "Pong: " + input;
                })
        ).forInterface(iface);

        // Run the server in a background thread
        server_conn->enterEventLoopAsync();

        // Give the server a moment to start
        std::this_thread::sleep_for(std::chrono::milliseconds(100));

        // Create client and call Ping
        auto client_conn = sdbus::createSessionBusConnection();
        auto proxy = sdbus::createProxy(
            *client_conn,
            sdbus::ServiceName{"org.firebox.test.ping"},
            sdbus::ObjectPath{"/org/firebox/test"});

        std::string result;
        proxy->callMethod("Ping")
            .onInterface("org.firebox.test.PingPong")
            .withArguments(std::string("Hello"))
            .storeResultsTo(result);

        expect(result == "Pong: Hello");

        // Cleanup
        server_conn->leaveEventLoop();
    };
}
