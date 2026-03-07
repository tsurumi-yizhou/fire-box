/// @file test_coroutine.cpp
/// Unit tests for the C++23 coroutine wrappers.

#include <boost/ut.hpp>
#include "common/coroutine.hpp"
#include <thread>

namespace ut = boost::ut;
using namespace firebox;

// Simple coroutine that returns a value
Task<int> simple_task() {
    co_return 42;
}

// Coroutine that chains another coroutine
Task<int> chained_task() {
    int val = co_await simple_task();
    co_return val + 8;
}

// Coroutine using delay
Task<int> delayed_task() {
    co_await delay_ms(10);
    co_return 99;
}

// Void coroutine
Task<void> void_task(int& out) {
    out = 100;
    co_return;
}

int main() {
    using namespace ut;

    "task_basic_value"_test = [] {
        // Run on a GLib main loop in a separate thread
        auto* loop = g_main_loop_new(nullptr, FALSE);
        int result = 0;

        auto test_coro = [&]() -> Task<void> {
            result = co_await simple_task();
            g_main_loop_quit(loop);
        };

        g_idle_add([](gpointer data) -> gboolean {
            auto* fn = static_cast<std::function<Task<void>()>*>(data);
            auto task = (*fn)();
            task.start();
            return G_SOURCE_REMOVE;
        }, new std::function<Task<void>()>(test_coro));

        // Run with a timeout to prevent hanging
        g_timeout_add(2000, [](gpointer data) -> gboolean {
            g_main_loop_quit(static_cast<GMainLoop*>(data));
            return G_SOURCE_REMOVE;
        }, loop);

        g_main_loop_run(loop);
        g_main_loop_unref(loop);

        expect(result == 42_i);
    };

    "task_chained"_test = [] {
        auto* loop = g_main_loop_new(nullptr, FALSE);
        int result = 0;

        auto test_coro = [&]() -> Task<void> {
            result = co_await chained_task();
            g_main_loop_quit(loop);
        };

        g_idle_add([](gpointer data) -> gboolean {
            auto* fn = static_cast<std::function<Task<void>()>*>(data);
            auto task = (*fn)();
            task.start();
            return G_SOURCE_REMOVE;
        }, new std::function<Task<void>()>(test_coro));

        g_timeout_add(2000, [](gpointer data) -> gboolean {
            g_main_loop_quit(static_cast<GMainLoop*>(data));
            return G_SOURCE_REMOVE;
        }, loop);

        g_main_loop_run(loop);
        g_main_loop_unref(loop);

        expect(result == 50_i);
    };

    "task_void"_test = [] {
        auto* loop = g_main_loop_new(nullptr, FALSE);
        int value = 0;

        auto test_coro = [&]() -> Task<void> {
            co_await void_task(value);
            g_main_loop_quit(loop);
        };

        g_idle_add([](gpointer data) -> gboolean {
            auto* fn = static_cast<std::function<Task<void>()>*>(data);
            auto task = (*fn)();
            task.start();
            return G_SOURCE_REMOVE;
        }, new std::function<Task<void>()>(test_coro));

        g_timeout_add(2000, [](gpointer data) -> gboolean {
            g_main_loop_quit(static_cast<GMainLoop*>(data));
            return G_SOURCE_REMOVE;
        }, loop);

        g_main_loop_run(loop);
        g_main_loop_unref(loop);

        expect(value == 100_i);
    };

    "delay_awaitable"_test = [] {
        auto* loop = g_main_loop_new(nullptr, FALSE);
        int result = 0;

        // This coroutine uses co_await delay_ms() which suspends and resumes
        // asynchronously via a GLib timer. The Task must outlive the current
        // scope — use spawn() which heap-manages the Task via detach_and_start().
        auto* result_ptr = &result;
        auto* loop_ptr   = loop;
        spawn(([result_ptr, loop_ptr]() -> Task<void> {
            *result_ptr = co_await delayed_task();
            g_main_loop_quit(loop_ptr);
        })());

        g_timeout_add(2000, [](gpointer data) -> gboolean {
            g_main_loop_quit(static_cast<GMainLoop*>(data));
            return G_SOURCE_REMOVE;
        }, loop);

        g_main_loop_run(loop);
        g_main_loop_unref(loop);

        expect(result == 99_i);
    };

    "spawn_fire_and_forget"_test = [] {
        // Verify spawn() works for a synchronously-completing task.
        auto* loop = g_main_loop_new(nullptr, FALSE);
        int value  = 0;

        auto* value_ptr = &value;
        auto* loop_ptr  = loop;
        spawn(([value_ptr, loop_ptr]() -> Task<void> {
            *value_ptr = co_await simple_task();
            g_main_loop_quit(loop_ptr);
        })());

        g_timeout_add(2000, [](gpointer data) -> gboolean {
            g_main_loop_quit(static_cast<GMainLoop*>(data));
            return G_SOURCE_REMOVE;
        }, loop);

        g_main_loop_run(loop);
        g_main_loop_unref(loop);

        expect(value == 42_i);
    };
}
