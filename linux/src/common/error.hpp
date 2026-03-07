#pragma once
/// @file error.hpp
/// Structured error types matching the Client SDK error specification.

#include <stdexcept>
#include <string>

namespace firebox {

enum class ErrorCode {
    // Connection errors
    ServiceNotFound,
    ConnectionDenied,
    ConnectionTimeout,
    TransportError,
    // Operational errors
    InvalidRequest,
    ModelNotFound,
    UnsupportedCapability,
    StreamBusy,
    StreamClosed,
    BackendError,
    RateLimited,
    // Client state errors
    ClientClosed,
    Disconnected,
    // Internal
    PermissionDenied,
    InternalError,
};

[[nodiscard]] constexpr bool is_retryable(ErrorCode code) noexcept {
    switch (code) {
    case ErrorCode::TransportError:
    case ErrorCode::BackendError:
    case ErrorCode::RateLimited:
    case ErrorCode::ConnectionTimeout:
        return true;
    default:
        return false;
    }
}

[[nodiscard]] constexpr const char* error_code_string(ErrorCode code) noexcept {
    switch (code) {
    case ErrorCode::ServiceNotFound:       return "ServiceNotFound";
    case ErrorCode::ConnectionDenied:      return "ConnectionDenied";
    case ErrorCode::ConnectionTimeout:     return "ConnectionTimeout";
    case ErrorCode::TransportError:        return "TransportError";
    case ErrorCode::InvalidRequest:        return "InvalidRequest";
    case ErrorCode::ModelNotFound:         return "ModelNotFound";
    case ErrorCode::UnsupportedCapability: return "UnsupportedCapability";
    case ErrorCode::StreamBusy:            return "StreamBusy";
    case ErrorCode::StreamClosed:          return "StreamClosed";
    case ErrorCode::BackendError:          return "BackendError";
    case ErrorCode::RateLimited:           return "RateLimited";
    case ErrorCode::ClientClosed:          return "ClientClosed";
    case ErrorCode::Disconnected:          return "Disconnected";
    case ErrorCode::PermissionDenied:      return "PermissionDenied";
    case ErrorCode::InternalError:         return "InternalError";
    }
    return "Unknown";
}

class FireboxError : public std::runtime_error {
public:
    FireboxError(ErrorCode code, std::string message)
        : std::runtime_error(std::move(message)), code_(code) {}

    [[nodiscard]] ErrorCode code() const noexcept { return code_; }
    [[nodiscard]] bool retryable() const noexcept { return is_retryable(code_); }
    [[nodiscard]] const char* code_string() const noexcept {
        return error_code_string(code_);
    }

private:
    ErrorCode code_;
};

} // namespace firebox
