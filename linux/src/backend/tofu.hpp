#pragma once
/// @file tofu.hpp
/// TOFU (Trust On First Use) authorization via Polkit.

#include "common/coroutine.hpp"
#include "common/storage.hpp"
#include <gio/gio.h>
#include <string>

namespace firebox::backend {

/// Perform a polkit-based TOFU authorization check.
/// If the app_path is already in the allowlist, returns true immediately.
/// Otherwise, invokes polkit to prompt the user.
///
/// @param storage  reference to the storage layer
/// @param app_path filesystem path of the calling executable
/// @param app_name display name of the calling application
/// @param pid      PID of the calling process
/// @return Task<bool> — true if authorized, false if denied
firebox::Task<bool> authorize_tofu(
    firebox::Storage& storage,
    const std::string& app_path,
    const std::string& app_name,
    uint32_t pid);

} // namespace firebox::backend
