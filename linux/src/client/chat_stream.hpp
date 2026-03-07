#pragma once
/// @file chat_stream.hpp
/// Streaming session handle for the FireBox Client SDK.

#include "common/dbus_types.hpp"
#include "common/error.hpp"

#include <sdbus-c++/sdbus-c++.h>
#include <string>

namespace firebox::client {

/// A received stream chunk.
struct StreamChunk {
    std::string content_json;  // The chunk content as JSON
    bool done = false;
    Usage usage;
    std::string finish_reason;
};

/// Stateful streaming session.
class ChatStream {
public:
    ChatStream(sdbus::IProxy* proxy, std::string stream_id);
    ~ChatStream();

    ChatStream(const ChatStream&) = delete;
    ChatStream& operator=(const ChatStream&) = delete;

    /// Send a message into the stream.
    void send(const std::string& message_json,
              const std::string& tools_json = "[]");

    /// Receive the next chunk (blocking with timeout).
    StreamChunk receive(int timeout_ms = 5000);

    /// Close the stream and release server-side resources.
    void close();

    [[nodiscard]] bool is_open() const { return open_; }
    [[nodiscard]] const std::string& stream_id() const { return stream_id_; }

private:
    sdbus::IProxy* proxy_;
    std::string stream_id_;
    bool open_ = true;
};

} // namespace firebox::client
