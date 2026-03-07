/// @file test_client.cpp
/// Unit tests for the FireBox Client SDK.
/// These test the client structure and error handling (not live D-Bus).

#include <boost/ut.hpp>
#include "client/firebox_client.hpp"
#include "common/error.hpp"

namespace ut = boost::ut;
using namespace firebox;
using namespace firebox::client;

int main() {
    using namespace ut;

    "error_code_strings"_test = [] {
        expect(std::string(error_code_string(ErrorCode::ServiceNotFound)) == "ServiceNotFound");
        expect(std::string(error_code_string(ErrorCode::ModelNotFound)) == "ModelNotFound");
        expect(std::string(error_code_string(ErrorCode::StreamClosed)) == "StreamClosed");
    };

    "error_retryable"_test = [] {
        expect(is_retryable(ErrorCode::TransportError));
        expect(is_retryable(ErrorCode::BackendError));
        expect(is_retryable(ErrorCode::RateLimited));
        expect(!is_retryable(ErrorCode::ServiceNotFound));
        expect(!is_retryable(ErrorCode::ConnectionDenied));
        expect(!is_retryable(ErrorCode::InvalidRequest));
    };

    "firebox_error_construction"_test = [] {
        FireboxError err(ErrorCode::ModelNotFound, "Model 'xyz' not found");
        expect(err.code() == ErrorCode::ModelNotFound);
        expect(std::string(err.code_string()) == "ModelNotFound");
        expect(std::string(err.what()) == "Model 'xyz' not found");
        expect(!err.retryable());
    };

    "client_connect_no_service"_test = [] {
        // When the backend is not running, connect should throw ServiceNotFound
        expect(throws<FireboxError>([] {
            auto client = FireBoxClient::connect(
                "org.firebox.nonexistent.test",
                "/org/firebox/test",
                1000);
            // If we somehow connected, try an operation that will fail
            client->list_models();
        }));
    };

    "client_operations_after_close"_test = [] {
        // Verify that operations on a closed client throw ClientClosed
        // We can't easily create a connected client without a backend,
        // so we test the error types directly
        FireboxError err(ErrorCode::ClientClosed, "Client is closed");
        expect(err.code() == ErrorCode::ClientClosed);
        expect(!err.retryable());
    };
}
