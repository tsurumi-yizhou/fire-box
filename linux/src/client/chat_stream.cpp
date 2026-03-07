/// @file chat_stream.cpp
#include "chat_stream.hpp"
#include <spdlog/spdlog.h>

namespace firebox::client {

ChatStream::ChatStream(sdbus::IProxy* proxy, std::string stream_id)
    : proxy_(proxy), stream_id_(std::move(stream_id)) {
    spdlog::debug("ChatStream created: {}", stream_id_);
}

ChatStream::~ChatStream() {
    if (open_) {
        try { close(); } catch (...) {}
    }
}

static constexpr const char* CAP_IFACE = "org.firebox.Capability";

void ChatStream::send(const std::string& message_json,
                      const std::string& tools_json) {
    if (!open_)
        throw FireboxError(ErrorCode::StreamClosed, "Stream is closed");

    std::tuple<bool, std::string> result;
    proxy_->callMethod("SendMessage")
        .onInterface(CAP_IFACE)
        .withArguments(stream_id_, message_json, tools_json)
        .storeResultsTo(result);

    auto& [success, message] = result;
    if (!success)
        throw FireboxError(ErrorCode::BackendError, message);
}

StreamChunk ChatStream::receive(int timeout_ms) {
    if (!open_)
        throw FireboxError(ErrorCode::StreamClosed, "Stream is closed");

    std::tuple<bool, std::string, std::string, bool,
               int32_t, int32_t, int32_t, std::string> result;
    proxy_->callMethod("ReceiveStream")
        .onInterface(CAP_IFACE)
        .withArguments(stream_id_, static_cast<int32_t>(timeout_ms))
        .storeResultsTo(result);

    auto& [success, message, content_json, done, pt, ct, tt, finish_reason] = result;
    if (!success)
        throw FireboxError(ErrorCode::BackendError, message);

    StreamChunk chunk;
    chunk.content_json = content_json;
    chunk.done = done;
    chunk.usage.prompt_tokens = pt;
    chunk.usage.completion_tokens = ct;
    chunk.usage.total_tokens = tt;
    chunk.finish_reason = finish_reason;
    return chunk;
}

void ChatStream::close() {
    if (!open_) return;

    try {
        proxy_->callMethod("CloseStream")
            .onInterface(CAP_IFACE)
            .withArguments(stream_id_);
    } catch (const sdbus::Error& e) {
        spdlog::debug("CloseStream failed: {}", e.what());
    }
    open_ = false;
    spdlog::debug("ChatStream closed: {}", stream_id_);
}

} // namespace firebox::client
