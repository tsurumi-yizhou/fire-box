#pragma once
/// @file http_client.hpp
/// Async HTTP client wrapping libsoup3, with C++23 coroutine support.

#include "common/coroutine.hpp"
#include <libsoup/soup.h>
#include <string>
#include <unordered_map>

namespace firebox::backend {

struct HttpResponse {
    int status_code = 0;
    std::string body;
    std::unordered_map<std::string, std::string> headers;
};

/// Async HTTP client using libsoup3 integrated with the GLib main loop.
class HttpClient {
public:
    HttpClient();
    ~HttpClient();

    HttpClient(const HttpClient&) = delete;
    HttpClient& operator=(const HttpClient&) = delete;

    /// Perform an async HTTP POST request.
    firebox::Task<HttpResponse> post(
        const std::string& url,
        const std::string& body,
        const std::unordered_map<std::string, std::string>& headers = {});

    /// Perform an async HTTP GET request.
    firebox::Task<HttpResponse> get(
        const std::string& url,
        const std::unordered_map<std::string, std::string>& headers = {});

    /// Perform a synchronous HTTP GET by spinning a private GMainContext.
    /// Safe to call from any non-main thread (e.g. the D-Bus handler thread).
    HttpResponse get_sync(
        const std::string& url,
        const std::unordered_map<std::string, std::string>& headers = {});

private:
    SoupSession* session_;
};

} // namespace firebox::backend
