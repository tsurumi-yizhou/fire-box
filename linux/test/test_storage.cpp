/// @file test_storage.cpp
/// Unit tests for the SQLite storage layer.

#include <boost/ut.hpp>
#include "common/storage.hpp"
#include <filesystem>
#include <chrono>

namespace ut = boost::ut;
using namespace firebox;

int main() {
    using namespace ut;

    // Use a temporary database
    auto db_path = std::filesystem::temp_directory_path() / "firebox_test.db";
    std::filesystem::remove(db_path);  // clean slate

    "storage_create_and_open"_test = [&] {
        expect(nothrow([&] { Storage db(db_path.string()); }));
    };

    "provider_crud"_test = [&] {
        Storage db(db_path.string());

        Provider p;
        p.provider_id = "test-id-1";
        p.name = "Test OpenAI";
        p.type = ProviderType::ApiKey;
        p.base_url = "https://api.openai.com/v1";
        p.enabled = true;

        auto id = db.upsert_provider(p);
        expect(id == "test-id-1"_b);

        auto providers = db.list_providers();
        expect(providers.size() == 1_ul);
        expect(providers[0].name == "Test OpenAI");

        // Upsert (same name) updates
        p.provider_id = "test-id-1";
        p.base_url = "https://custom.endpoint.com";
        db.upsert_provider(p);
        providers = db.list_providers();
        expect(providers.size() == 1_ul);
        expect(providers[0].base_url == "https://custom.endpoint.com");

        // Delete
        auto ok = db.delete_provider("test-id-1");
        expect(ok);
        expect(db.list_providers().empty());
    };

    "model_crud"_test = [&] {
        Storage db(db_path.string());

        // Need a provider first
        Provider p;
        p.provider_id = "prov-1";
        p.name = "Provider 1";
        p.type = ProviderType::ApiKey;
        db.upsert_provider(p);

        Model m;
        m.model_id = "gpt-4";
        m.provider_id = "prov-1";
        m.enabled = true;
        m.capability_chat = true;
        m.capability_streaming = true;

        db.upsert_model(m);

        auto models = db.get_models("prov-1");
        expect(models.size() == 1_ul);
        expect(models[0].model_id == "gpt-4");

        // Toggle enabled
        db.set_model_enabled("prov-1", "gpt-4", false);
        models = db.get_models("prov-1");
        expect(!models[0].enabled);

        db.delete_provider("prov-1");
    };

    "allowlist_crud"_test = [&] {
        Storage db(db_path.string());

        auto now = std::chrono::duration_cast<std::chrono::milliseconds>(
                       std::chrono::system_clock::now().time_since_epoch()).count();

        AllowedApp app;
        app.app_path = "/usr/bin/test-app";
        app.display_name = "Test App";
        app.first_seen_ms = now;
        app.last_used_ms = now;

        db.add_to_allowlist(app);
        expect(db.is_allowed("/usr/bin/test-app"));
        expect(!db.is_allowed("/usr/bin/other-app"));

        auto list = db.get_allowlist();
        expect(list.size() == 1_ul);
        expect(list[0].display_name == "Test App");

        db.remove_from_allowlist("/usr/bin/test-app");
        expect(!db.is_allowed("/usr/bin/test-app"));
    };

    "route_rules_crud"_test = [&] {
        Storage db(db_path.string());

        RouteRule rule;
        rule.virtual_model_id = "coding-assistant";
        rule.display_name = "Coding Assistant";
        rule.capabilities.chat = true;
        rule.capabilities.streaming = true;
        rule.capabilities.tool_calling = true;
        rule.strategy = RouteStrategy::Failover;
        rule.targets.push_back({"prov-a", "gpt-4"});
        rule.targets.push_back({"prov-b", "claude-3"});

        db.set_route_rule(rule);

        auto rules = db.list_route_rules();
        expect(rules.size() >= 1_ul);

        auto r = db.get_route_rule("coding-assistant");
        expect(r.display_name == "Coding Assistant");
        expect(r.targets.size() == 2_ul);
        expect(r.capabilities.tool_calling);

        // Delete (empty targets)
        db.delete_route_rule("coding-assistant");
        r = db.get_route_rule("coding-assistant");
        expect(r.virtual_model_id.empty());
    };

    "metrics_record_and_query"_test = [&] {
        Storage db(db_path.string());

        MetricsSnapshot snap;
        snap.window_start_ms = 1000;
        snap.window_end_ms = 2000;
        snap.requests_total = 10;
        snap.requests_failed = 1;
        snap.prompt_tokens = 500;
        snap.completion_tokens = 200;
        snap.cost_total = 0.05;

        db.record_metrics(snap);

        auto snapshot = db.get_metrics_snapshot();
        expect(snapshot.requests_total >= 10_i);

        auto range = db.get_metrics_range(0, 10000);
        expect(!range.empty());
    };

    // Clean up
    std::filesystem::remove(db_path);
}
