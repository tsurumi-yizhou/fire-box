#pragma once
/// @file coroutine.hpp
/// C++23 coroutine awaitables wrapping GLib async, libsoup3, and D-Bus callbacks.
/// All awaitables resume on the GLib main loop thread.

#include <gio/gio.h>
#include <coroutine>
#include <expected>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <variant>

namespace firebox {

// ─── GLib deleters ───────────────────────────────────────────────
struct GObjectDeleter {
    void operator()(gpointer p) const noexcept {
        if (p) g_object_unref(p);
    }
};
template <typename T>
using GObjectPtr = std::unique_ptr<T, GObjectDeleter>;

struct GErrorDeleter {
    void operator()(GError* e) const noexcept {
        if (e) g_error_free(e);
    }
};
using GErrorPtr = std::unique_ptr<GError, GErrorDeleter>;

// ─── Task<T> — a lazy coroutine that co_awaits other awaitables ──
template <typename T = void>
class Task;

namespace detail {

struct TaskPromiseBase {
    std::coroutine_handle<> continuation{};
    bool detached_ = false; // set by detach_and_start() for spawn() fire-and-forget

    auto initial_suspend() noexcept { return std::suspend_always{}; }

    struct FinalAwaiter {
        bool await_ready() const noexcept { return false; }
        template <typename P>
        std::coroutine_handle<> await_suspend(
            std::coroutine_handle<P> h) noexcept {
            if (h.promise().continuation)
                return h.promise().continuation;
            // Only self-destroy for spawn() fire-and-forget tasks;
            // scope-managed tasks are destroyed by ~Task().
            if (h.promise().detached_)
                h.destroy();
            return std::noop_coroutine();
        }
        void await_resume() noexcept {}
    };
    auto final_suspend() noexcept { return FinalAwaiter{}; }

    void unhandled_exception() { exception_ = std::current_exception(); }

    std::exception_ptr exception_{};
};

template <typename T>
struct TaskPromise : TaskPromiseBase {
    Task<T> get_return_object();
    void return_value(T value) { result_.emplace(std::move(value)); }

    T& value() {
        if (exception_) std::rethrow_exception(exception_);
        return *result_;
    }

    std::optional<T> result_;
};

template <>
struct TaskPromise<void> : TaskPromiseBase {
    Task<void> get_return_object();
    void return_void() {}
    void value() {
        if (exception_) std::rethrow_exception(exception_);
    }
};

} // namespace detail

template <typename T>
class [[nodiscard]] Task {
public:
    using promise_type = detail::TaskPromise<T>;

    explicit Task(std::coroutine_handle<promise_type> h) : handle_(h) {}
    Task(Task&& o) noexcept : handle_(std::exchange(o.handle_, {})) {}
    ~Task() {
        if (handle_) handle_.destroy();
    }
    Task& operator=(Task&& o) noexcept {
        if (this != &o) {
            if (handle_) handle_.destroy();
            handle_ = std::exchange(o.handle_, {});
        }
        return *this;
    }
    Task(const Task&) = delete;
    Task& operator=(const Task&) = delete;

    // Awaitable interface
    bool await_ready() const noexcept { return false; }

    std::coroutine_handle<> await_suspend(
        std::coroutine_handle<> caller) noexcept {
        handle_.promise().continuation = caller;
        return handle_;
    }

    decltype(auto) await_resume() { return handle_.promise().value(); }

    /// Start without detaching — used when Task is scope-managed.
    void start() {
        if (handle_ && !handle_.done()) handle_.resume();
    }

    /// Mark as fire-and-forget, release ownership, and start the coroutine.
    /// The frame self-destroys via FinalAwaiter when it completes.
    /// Must be called instead of start() inside spawn().
    void detach_and_start() noexcept {
        if (!handle_) return;
        handle_.promise().detached_ = true;
        auto h = std::exchange(handle_, {}); // clear Task ownership before resuming
        if (!h.done()) h.resume();           // frame self-manages from here
    }

private:
    std::coroutine_handle<promise_type> handle_;
};

namespace detail {
template <typename T>
Task<T> TaskPromise<T>::get_return_object() {
    return Task<T>{
        std::coroutine_handle<TaskPromise<T>>::from_promise(*this)};
}
inline Task<void> TaskPromise<void>::get_return_object() {
    return Task<void>{
        std::coroutine_handle<TaskPromise<void>>::from_promise(*this)};
}
} // namespace detail

// ─── spawn() — launch a Task on the GLib main loop ──────────────
/// Schedules the given Task to run on the default GLib main context.
template <typename T>
void spawn(Task<T> task) {
    // Move the task to the heap so the Task wrapper outlives this call.
    auto* p = new Task<T>(std::move(task));
    g_idle_add_full(
        G_PRIORITY_DEFAULT,
        [](gpointer data) -> gboolean {
            auto* t = static_cast<Task<T>*>(data);
            // detach_and_start(): marks detached_, clears handle_, then resumes.
            // Frame self-destroys in FinalAwaiter when done.
            // ~Task() is a no-op because handle_ was cleared.
            t->detach_and_start();
            delete t;
            return G_SOURCE_REMOVE;
        },
        p, nullptr);
}

// ─── GAsync awaitable — wraps any GIO-style async/finish pair ───
/// Usage:
///   auto* result = co_await gio_async<GObject*>(
///       [&](GAsyncReadyCallback cb, gpointer ud) {
///           g_some_async_op(..., cb, ud);
///       },
///       [&](GAsyncResult* res) -> GObject* {
///           GError* err = nullptr;
///           auto* obj = g_some_finish(res, &err);
///           if (err) throw std::runtime_error(err->message);
///           return obj;
///       });
template <typename T>
class GAsyncAwaitable {
public:
    using StartFn  = std::function<void(GAsyncReadyCallback, gpointer)>;
    using FinishFn = std::function<T(GAsyncResult*)>;

    GAsyncAwaitable(StartFn start, FinishFn finish)
        : start_(std::move(start)), finish_(std::move(finish)) {}

    bool await_ready() const noexcept { return false; }

    void await_suspend(std::coroutine_handle<> h) {
        handle_ = h;
        start_(
            [](GObject* /*src*/, GAsyncResult* res, gpointer data) {
                auto* self = static_cast<GAsyncAwaitable*>(data);
                try {
                    self->result_.emplace(self->finish_(res));
                } catch (...) {
                    self->exception_ = std::current_exception();
                }
                // Resume the coroutine on the GLib main loop thread.
                self->handle_.resume();
            },
            this);
    }

    T await_resume() {
        if (exception_) std::rethrow_exception(exception_);
        return std::move(*result_);
    }

private:
    StartFn start_;
    FinishFn finish_;
    std::coroutine_handle<> handle_{};
    std::optional<T> result_;
    std::exception_ptr exception_{};
};

/// Convenience factory
template <typename T>
auto gio_async(
    std::function<void(GAsyncReadyCallback, gpointer)> start,
    std::function<T(GAsyncResult*)> finish) {
    return GAsyncAwaitable<T>(std::move(start), std::move(finish));
}

// ─── GLib timeout awaitable ──────────────────────────────────────
/// co_await delay_ms(500); — suspends for the given duration
class DelayAwaitable {
public:
    explicit DelayAwaitable(unsigned int ms) : ms_(ms) {}

    bool await_ready() const noexcept { return ms_ == 0; }

    void await_suspend(std::coroutine_handle<> h) {
        g_timeout_add(ms_,
            [](gpointer data) -> gboolean {
                auto h = std::coroutine_handle<>::from_address(data);
                h.resume();
                return G_SOURCE_REMOVE;
            },
            h.address());
    }

    void await_resume() noexcept {}

private:
    unsigned int ms_;
};

inline auto delay_ms(unsigned int ms) { return DelayAwaitable(ms); }

// ─── GLib idle awaitable ─────────────────────────────────────────
/// co_await yield(); — yields to the main loop and resumes later
class YieldAwaitable {
public:
    bool await_ready() const noexcept { return false; }
    void await_suspend(std::coroutine_handle<> h) {
        g_idle_add(
            [](gpointer data) -> gboolean {
                auto h = std::coroutine_handle<>::from_address(data);
                h.resume();
                return G_SOURCE_REMOVE;
            },
            h.address());
    }
    void await_resume() noexcept {}
};

inline auto yield() { return YieldAwaitable{}; }

} // namespace firebox
