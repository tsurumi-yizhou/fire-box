#pragma once
/// @file log.hpp
/// Logging setup: spdlog with systemd journald sink.

#include <spdlog/spdlog.h>
#include <spdlog/sinks/stdout_color_sinks.h>
#include <memory>

#ifdef SD_JOURNAL_SUPPRESS_LOCATION
#include <spdlog/sinks/systemd_sink.h>
#endif

namespace firebox::log {

inline void init(const std::string& name) {
    std::vector<spdlog::sink_ptr> sinks;

    // Always add console sink
    sinks.push_back(std::make_shared<spdlog::sinks::stderr_color_sink_mt>());

#ifdef SD_JOURNAL_SUPPRESS_LOCATION
    // If systemd headers available, also log to journald
    sinks.push_back(std::make_shared<spdlog::sinks::systemd_sink_mt>(name));
#endif

    auto logger = std::make_shared<spdlog::logger>(name, sinks.begin(), sinks.end());
    logger->set_level(spdlog::level::info);
    spdlog::set_default_logger(logger);
}

} // namespace firebox::log
