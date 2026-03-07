/// @file http_client.cpp
#include "http_client.hpp"
#include <spdlog/spdlog.h>

namespace firebox::backend {

HttpClient::HttpClient() {
    session_ = soup_session_new();
    spdlog::debug("HttpClient: libsoup3 session created");
}

HttpClient::~HttpClient() {
    if (session_) g_object_unref(session_);
}

firebox::Task<HttpResponse> HttpClient::post(
    const std::string& url,
    const std::string& body,
    const std::unordered_map<std::string, std::string>& headers) {

    auto* msg = soup_message_new("POST", url.c_str());
    if (!msg) {
        spdlog::error("HttpClient: Invalid URL: {}", url);
        co_return HttpResponse{0, "Invalid URL", {}};
    }

    // Set body via GBytes
    auto* body_bytes = g_bytes_new(body.c_str(), body.size());
    soup_message_set_request_body_from_bytes(msg, "application/json", body_bytes);
    g_bytes_unref(body_bytes);

    // Set headers
    auto* hdrs = soup_message_get_request_headers(msg);
    for (auto& [k, v] : headers) {
        soup_message_headers_replace(hdrs, k.c_str(), v.c_str());
    }

    HttpResponse response = co_await firebox::gio_async<HttpResponse>(
        [this, msg](GAsyncReadyCallback cb, gpointer ud) {
            soup_session_send_and_read_async(
                session_, msg, G_PRIORITY_DEFAULT, nullptr, cb, ud);
        },
        [msg](GAsyncResult* res) -> HttpResponse {
            HttpResponse resp;
            GError* error = nullptr;
            auto* session = SOUP_SESSION(g_async_result_get_source_object(res));
            auto* bytes = soup_session_send_and_read_finish(session, res, &error);

            if (error) {
                resp.status_code = 0;
                resp.body = error->message;
                g_error_free(error);
                g_object_unref(session);
                return resp;
            }

            resp.status_code = soup_message_get_status(msg);

            gsize size = 0;
            auto* data = static_cast<const char*>(g_bytes_get_data(bytes, &size));
            if (data && size > 0) {
                resp.body.assign(data, size);
            }
            g_bytes_unref(bytes);
            g_object_unref(msg);
            g_object_unref(session);
            return resp;
        });

    co_return response;
}

firebox::Task<HttpResponse> HttpClient::get(
    const std::string& url,
    const std::unordered_map<std::string, std::string>& headers) {

    auto* msg = soup_message_new("GET", url.c_str());
    if (!msg) {
        co_return HttpResponse{0, "Invalid URL", {}};
    }

    auto* hdrs = soup_message_get_request_headers(msg);
    for (auto& [k, v] : headers) {
        soup_message_headers_replace(hdrs, k.c_str(), v.c_str());
    }

    HttpResponse response = co_await firebox::gio_async<HttpResponse>(
        [this, msg](GAsyncReadyCallback cb, gpointer ud) {
            soup_session_send_and_read_async(
                session_, msg, G_PRIORITY_DEFAULT, nullptr, cb, ud);
        },
        [msg](GAsyncResult* res) -> HttpResponse {
            HttpResponse resp;
            GError* error = nullptr;
            auto* session = SOUP_SESSION(g_async_result_get_source_object(res));
            auto* bytes = soup_session_send_and_read_finish(session, res, &error);

            if (error) {
                resp.status_code = 0;
                resp.body = error->message;
                g_error_free(error);
                g_object_unref(session);
                return resp;
            }

            resp.status_code = soup_message_get_status(msg);

            gsize size = 0;
            auto* data = static_cast<const char*>(g_bytes_get_data(bytes, &size));
            if (data && size > 0) {
                resp.body.assign(data, size);
            }
            g_bytes_unref(bytes);
            g_object_unref(msg);
            g_object_unref(session);
            return resp;
        });

    co_return response;
}

HttpResponse HttpClient::get_sync(
    const std::string& url,
    const std::unordered_map<std::string, std::string>& headers) {

    struct State {
        SoupMessage* msg = nullptr;
        HttpResponse resp;
        bool done = false;
    };
    State state;

    state.msg = soup_message_new("GET", url.c_str());
    if (!state.msg) return {0, "Invalid URL", {}};

    auto* hdrs = soup_message_get_request_headers(state.msg);
    for (auto& [k, v] : headers)
        soup_message_headers_replace(hdrs, k.c_str(), v.c_str());

    // Push a private context so libsoup dispatches callbacks here, not on
    // the GLib main thread's context.
    GMainContext* ctx = g_main_context_new();
    g_main_context_push_thread_default(ctx);

    soup_session_send_and_read_async(
        session_, state.msg, G_PRIORITY_DEFAULT, nullptr,
        +[](GObject* src, GAsyncResult* res, gpointer ud) {
            auto* s = static_cast<State*>(ud);
            GError* err = nullptr;
            auto* bytes = soup_session_send_and_read_finish(
                SOUP_SESSION(src), res, &err);
            if (err) {
                s->resp.status_code = 0;
                s->resp.body = err->message;
                g_error_free(err);
            } else {
                s->resp.status_code = soup_message_get_status(s->msg);
                gsize sz = 0;
                auto* data = static_cast<const char*>(g_bytes_get_data(bytes, &sz));
                if (data && sz > 0) s->resp.body.assign(data, sz);
                g_bytes_unref(bytes);
            }
            s->done = true;
        },
        &state);

    while (!state.done)
        g_main_context_iteration(ctx, TRUE);

    g_main_context_pop_thread_default(ctx);
    g_main_context_unref(ctx);
    g_object_unref(state.msg);
    return state.resp;
}

} // namespace firebox::backend
