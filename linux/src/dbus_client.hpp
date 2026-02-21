#include <sdbus-c++/sdbus-c++.h>
#include <coroutine>
#include <string>
#include <memory>

// D-Bus 异步调用的 coroutine 包装
template<typename T>
struct DbusAsyncCall {
    struct promise_type {
        T value;
        std::exception_ptr exception;

        DbusAsyncCall get_return_object() {
            return DbusAsyncCall{std::coroutine_handle<promise_type>::from_promise(*this)};
        }

        std::suspend_never initial_suspend() { return {}; }
        std::suspend_always final_suspend() noexcept { return {}; }

        void return_value(T v) { value = std::move(v); }

        void unhandled_exception() {
            exception = std::current_exception();
        }
    };

    std::coroutine_handle<promise_type> handle;

    DbusAsyncCall(std::coroutine_handle<promise_type> h) : handle(h) {}

    ~DbusAsyncCall() {
        if (handle) handle.destroy();
    }

    T get() {
        if (handle.promise().exception) {
            std::rethrow_exception(handle.promise().exception);
        }
        return handle.promise().value;
    }
};

// Fire Box D-Bus 服务接口
class FireBoxDbusClient {
public:
    FireBoxDbusClient()
        : connection_(sdbus::createSessionBusConnection()),
          proxy_(sdbus::createProxy(*connection_, "com.example.firebox.service", "/com/example/firebox")) {
        connection_->enterEventLoopAsync();
    }

    // 使用 coroutine 进行异步 D-Bus 调用
    DbusAsyncCall<std::string> get_status_async() {
        std::string status;
        proxy_->callMethod("GetStatus")
              .onInterface("com.example.firebox.Service")
              .storeResultsTo(status);
        co_return status;
    }

    DbusAsyncCall<bool> start_service_async() {
        bool success;
        proxy_->callMethod("Start")
              .onInterface("com.example.firebox.Service")
              .storeResultsTo(success);
        co_return success;
    }

    DbusAsyncCall<bool> stop_service_async() {
        bool success;
        proxy_->callMethod("Stop")
              .onInterface("com.example.firebox.Service")
              .storeResultsTo(success);
        co_return success;
    }

    // 监听 D-Bus 信号
    void on_status_changed(std::function<void(const std::string&)> callback) {
        proxy_->uponSignal("StatusChanged")
              .onInterface("com.example.firebox.Service")
              .call([callback](const std::string& new_status) {
                  callback(new_status);
              });
    }

private:
    std::unique_ptr<sdbus::IConnection> connection_;
    std::unique_ptr<sdbus::IProxy> proxy_;
};

// 使用示例
/*
auto client = std::make_unique<FireBoxDbusClient>();

// 异步获取状态
auto status_task = client->get_status_async();
std::string status = status_task.get();

// 监听状态变化
client->on_status_changed([](const std::string& status) {
    g_print("Status changed: %s\n", status.c_str());
});

// 启动服务
auto start_task = client->start_service_async();
bool success = start_task.get();
*/
