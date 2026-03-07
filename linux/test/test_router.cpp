/// @file test_router.cpp
/// Unit tests for the routing engine.

#include <boost/ut.hpp>
#include "backend/router.hpp"
#include "common/storage.hpp"
#include <filesystem>

namespace ut = boost::ut;
using namespace firebox;
using namespace firebox::backend;

int main() {
    using namespace ut;

    auto db_path = std::filesystem::temp_directory_path() / "firebox_router_test.db";
    std::filesystem::remove(db_path);

    "router_failover_resolve"_test = [&] {
        Storage db(db_path.string());
        Router router(db);

        RouteRule rule;
        rule.virtual_model_id = "test-model";
        rule.display_name = "Test Model";
        rule.strategy = RouteStrategy::Failover;
        rule.targets.push_back({"provider-a", "gpt-4"});
        rule.targets.push_back({"provider-b", "claude-3"});
        db.set_route_rule(rule);

        auto targets = router.resolve("test-model");
        expect(targets.size() == 2_ul);
        // Failover: first target should be provider-a
        expect(targets[0].provider_id == "provider-a");
        expect(targets[0].model_id == "gpt-4");

        db.delete_route_rule("test-model");
    };

    "router_random_resolve"_test = [&] {
        Storage db(db_path.string());
        Router router(db);

        RouteRule rule;
        rule.virtual_model_id = "random-model";
        rule.display_name = "Random Model";
        rule.strategy = RouteStrategy::Random;
        rule.targets.push_back({"provider-a", "gpt-4"});
        rule.targets.push_back({"provider-b", "claude-3"});
        db.set_route_rule(rule);

        // Random should return all targets (possibly shuffled)
        auto targets = router.resolve("random-model");
        expect(targets.size() == 2_ul);

        db.delete_route_rule("random-model");
    };

    "router_nonexistent_model"_test = [&] {
        Storage db(db_path.string());
        Router router(db);

        auto targets = router.resolve("nonexistent");
        expect(targets.empty());
    };

    "router_get_capabilities"_test = [&] {
        Storage db(db_path.string());
        Router router(db);

        RouteRule rule;
        rule.virtual_model_id = "cap-test";
        rule.display_name = "Capability Test";
        rule.capabilities.chat = true;
        rule.capabilities.streaming = true;
        rule.capabilities.embeddings = false;
        rule.capabilities.vision = true;
        rule.capabilities.tool_calling = true;
        rule.targets.push_back({"prov", "model"});
        db.set_route_rule(rule);

        auto caps = router.get_capabilities("cap-test");
        expect(caps.chat);
        expect(caps.streaming);
        expect(!caps.embeddings);
        expect(caps.vision);
        expect(caps.tool_calling);

        db.delete_route_rule("cap-test");
    };

    std::filesystem::remove(db_path);
}
