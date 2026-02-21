#include <gtkmm/application.h>
#include <gtkmm/applicationwindow.h>
#include <gtkmm/box.h>
#include <gtkmm/button.h>
#include <gtkmm/label.h>
#include <gtkmm/builder.h>
#include <adwaita.h>
#include <coroutine>
#include <string>
#include <exception>
#include <future>
#include <thread>
#include <chrono>
#include <libintl.h>
#include <locale.h>

#define _(String) gettext(String)

// Simple coroutine task for async operations
template<typename T>
struct AsyncTask {
    struct promise_type {
        T value;
        std::exception_ptr exception;

        AsyncTask get_return_object() {
            return AsyncTask{std::coroutine_handle<promise_type>::from_promise(*this)};
        }

        std::suspend_never initial_suspend() { return {}; }
        std::suspend_always final_suspend() noexcept { return {}; }

        void return_value(T v) { value = std::move(v); }

        void unhandled_exception() {
            exception = std::current_exception();
        }
    };

    std::coroutine_handle<promise_type> handle;

    AsyncTask(std::coroutine_handle<promise_type> h) : handle(h) {}

    ~AsyncTask() {
        if (handle) handle.destroy();
    }

    T get() {
        if (handle.promise().exception) {
            std::rethrow_exception(handle.promise().exception);
        }
        return handle.promise().value;
    }
};

// Example async operation using coroutine
AsyncTask<std::string> fetch_data_async() {
    // Simulate async operation
    std::this_thread::sleep_for(std::chrono::milliseconds(100));
    co_return "Data fetched successfully using C++20 coroutines!";
}

class MainWindow : public Gtk::ApplicationWindow {
public:
    MainWindow() {
        set_title(_("Fire Box"));
        set_default_size(800, 600);

        // Setup UI manually (Blueprint UI can be loaded via GtkBuilder)
        auto box = Gtk::make_managed<Gtk::Box>(Gtk::Orientation::VERTICAL, 12);
        box->set_margin(12);

        auto label = Gtk::make_managed<Gtk::Label>(_("Welcome to Fire Box"));
        label->add_css_class("title-1");

        result_label = Gtk::make_managed<Gtk::Label>("");
        result_label->add_css_class("title-2");

        auto button = Gtk::make_managed<Gtk::Button>(_("Fetch Data"));
        button->signal_clicked().connect(sigc::mem_fun(*this, &MainWindow::on_fetch_clicked));

        box->append(*label);
        box->append(*button);
        box->append(*result_label);

        set_child(*box);
    }

private:
    Gtk::Label* result_label;

    void on_fetch_clicked() {
        result_label->set_text("Data fetched successfully!");
    }
};

class Application : public Gtk::Application {
public:
    static Glib::RefPtr<Application> create() {
        return Glib::make_refptr_for_instance<Application>(
            new Application()
        );
    }

protected:
    Application()
        : Gtk::Application("com.example.firebox") {
    }

    void on_activate() override {
        auto window = new MainWindow();
        add_window(*window);
        window->present();
    }
};

auto main(int argc, char* argv[]) -> int {
    // Initialize i18n
    setlocale(LC_ALL, "");
    bindtextdomain("fire-box", LOCALEDIR);
    textdomain("fire-box");

    // Initialize libadwaita
    adw_init();

    auto app = Application::create();
    return app->run(argc, argv);
}