# Fire Box Linux 客户端 - 技术集成示例

## 完整技术栈集成

本项目展示了如何将以下现代 C++ 和 GNOME 技术整合在一起：

### 1. C++20 Coroutines + gtkmm

```cpp
// 定义异步任务类型
template<typename T>
struct AsyncTask {
    struct promise_type {
        T value;
        AsyncTask get_return_object() {
            return AsyncTask{std::coroutine_handle<promise_type>::from_promise(*this)};
        }
        std::suspend_never initial_suspend() { return {}; }
        std::suspend_always final_suspend() noexcept { return {}; }
        void return_value(T v) { value = std::move(v); }
    };

    std::coroutine_handle<promise_type> handle;
    T get() { return handle.promise().value; }
};

// 异步操作
AsyncTask<std::string> fetch_data() {
    co_return "Data";
}

// 在 gtkmm 中使用
void on_button_click() {
    std::thread([this]() {
        auto task = fetch_data();
        auto result = task.get();

        // 回到主线程更新 UI
        Glib::signal_idle().connect_once([this, result]() {
            label->set_text(result);
        });
    }).detach();
}
```

### 2. Blueprint UI 设计

Blueprint 提供声明式 UI 语法，编译时转换为 GTK UI 文件：

```blueprint
using Gtk 4.0;
using Adw 1;

template $MainWindow : Adw.ApplicationWindow {
  Adw.ToolbarView {
    [top]
    Adw.HeaderBar {}

    content: Box {
      orientation: vertical;

      Button {
        label: _("Click Me");
        clicked => $on_button_clicked();
      }
    };
  }
}
```

### 3. sdbus-c++ D-Bus 集成

```cpp
#include <sdbus-c++/sdbus-c++.h>

class ServiceClient {
    std::unique_ptr<sdbus::IProxy> proxy_;

public:
    ServiceClient() {
        auto conn = sdbus::createSessionBusConnection();
        proxy_ = sdbus::createProxy(*conn, "com.example.service", "/path");
    }

    // 同步调用
    std::string get_status() {
        std::string status;
        proxy_->callMethod("GetStatus")
              .onInterface("com.example.Interface")
              .storeResultsTo(status);
        return status;
    }

    // 异步调用 + coroutine
    AsyncTask<std::string> get_status_async() {
        co_return get_status();
    }

    // 监听信号
    void on_signal(std::function<void(std::string)> callback) {
        proxy_->uponSignal("StatusChanged")
              .onInterface("com.example.Interface")
              .call(callback);
    }
};
```

### 4. libadwaita 现代 UI

```cpp
#include <adwaita.h>

// 初始化
adw_init();

// 使用 Adwaita 组件
auto window = Adw::ApplicationWindow();
auto status_page = Adw::StatusPage();
status_page.set_title("Welcome");
status_page.set_icon_name("application-x-executable-symbolic");

auto toast_overlay = Adw::ToastOverlay();
auto toast = Adw::Toast::create("Operation completed");
toast_overlay.add_toast(toast);
```

### 5. 系统托盘 (libayatana-appindicator)

```cpp
#include <libayatana-appindicator/app-indicator.h>

auto indicator = app_indicator_new(
    "fire-box",
    "application-icon",
    APP_INDICATOR_CATEGORY_APPLICATION_STATUS
);

app_indicator_set_status(indicator, APP_INDICATOR_STATUS_ACTIVE);
app_indicator_set_menu(indicator, GTK_MENU(menu));
```

### 6. 国际化 (gettext)

```cpp
#include <libintl.h>
#include <locale.h>

#define _(String) gettext(String)

// 初始化
setlocale(LC_ALL, "");
bindtextdomain("fire-box", LOCALEDIR);
textdomain("fire-box");

// 使用
auto label = Gtk::Label(_("Hello World"));
```

## 完整工作流程

1. **UI 设计**: 使用 Blueprint 设计界面
2. **构建时**: Blueprint 编译为 GTK UI，打包进 GResource
3. **运行时**:
   - 加载 GResource 中的 UI
   - 使用 gtkmm 绑定信号处理
   - 通过 sdbus-c++ 与后端服务通信
   - 使用 coroutine 处理异步操作
   - libadwaita 提供现代化外观
   - gettext 提供多语言支持

## 架构建议

```
┌─────────────────────────────────────┐
│         GTK/Adwaita UI              │
│    (Blueprint + gtkmm)              │
└──────────────┬──────────────────────┘
               │
               │ Signal/Slot
               │
┌──────────────▼──────────────────────┐
│      Application Logic              │
│   (C++20 Coroutines)                │
└──────────────┬──────────────────────┘
               │
               │ D-Bus (sdbus-c++)
               │
┌──────────────▼──────────────────────┐
│      Backend Service                │
│   (Rust service from ../service)    │
└─────────────────────────────────────┘
```

## 性能优化建议

1. **异步操作**: 所有耗时操作使用 coroutine 异步执行
2. **D-Bus**: 使用异步调用避免阻塞 UI 线程
3. **资源管理**: UI 资源打包进 GResource，减少文件 I/O
4. **编译优化**: 使用 `-O2` 或 `-O3` 编译选项

## 下一步

- [ ] 实现完整的 D-Bus 服务接口
- [ ] 添加更多 Blueprint UI 组件
- [ ] 实现系统托盘菜单
- [ ] 添加更多语言翻译
- [ ] 集成 Rust 后端服务
