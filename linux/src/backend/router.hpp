#pragma once
/// @file router.hpp
/// Request routing engine — resolves virtual model IDs to physical targets.

#include "common/dbus_types.hpp"
#include "common/storage.hpp"
#include <algorithm>
#include <random>
#include <string>
#include <utility>
#include <vector>

namespace firebox::backend {

class Router {
public:
    explicit Router(Storage& storage) : storage_(storage) {}

    /// Resolve a virtual model ID to a list of physical targets in order.
    /// For Failover: returns targets in defined order.
    /// For Random: shuffles and returns all.
    std::vector<RouteTarget> resolve(const std::string& virtual_model_id) {
        auto rule = storage_.get_route_rule(virtual_model_id);
        if (rule.targets.empty()) return {};

        auto targets = rule.targets;

        if (rule.strategy == RouteStrategy::Random && targets.size() > 1) {
            std::shuffle(targets.begin(), targets.end(), rng_);
        }

        return targets;
    }

    /// Get the capability contract for a virtual model.
    ModelCapabilities get_capabilities(const std::string& virtual_model_id) {
        auto rule = storage_.get_route_rule(virtual_model_id);
        return rule.capabilities;
    }

private:
    Storage& storage_;
    std::mt19937 rng_{std::random_device{}()};
};

} // namespace firebox::backend
