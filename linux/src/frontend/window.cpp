/// @file window.cpp
#include "window.hpp"
#include "dashboard_page.hpp"
#include "settings_page.hpp"
#include "route_page.hpp"
#include "allowlist_page.hpp"
#include "connections_page.hpp"
#include <spdlog/spdlog.h>

namespace firebox::frontend {

struct NavEntry { const char* icon; const char* label; const char* key; };
static constexpr NavEntry NAV_ENTRIES[] = {
    { "org.gnome.Software-symbolic",        "Dashboard",   "dashboard"   },
    { "preferences-system-symbolic",        "Settings",    "settings"    },
    { "network-workgroup-symbolic",          "Routes",      "routes"      },
    { "security-medium-symbolic",           "Allowlist",   "allowlist"   },
    { "network-transmit-receive-symbolic",  "Connections", "connections" },
};

// Wrap a content widget in AdwToolbarView + AdwHeaderBar
static GtkWidget* make_toolbar_page(const char* title, GtkWidget* content) {
    auto* tv = adw_toolbar_view_new();
    auto* hb = adw_header_bar_new();
    adw_header_bar_set_title_widget(ADW_HEADER_BAR(hb),
        GTK_WIDGET(adw_window_title_new(title, nullptr)));
    adw_toolbar_view_add_top_bar(ADW_TOOLBAR_VIEW(tv), GTK_WIDGET(hb));
    adw_toolbar_view_set_content(ADW_TOOLBAR_VIEW(tv), content);
    return GTK_WIDGET(tv);
}

MainWindow::MainWindow(GtkApplication* app) {
    setup_dbus();

    win_ = ADW_APPLICATION_WINDOW(adw_application_window_new(app));
    gtk_window_set_title(GTK_WINDOW(win_), "FireBox");
    gtk_window_set_default_size(GTK_WINDOW(win_), 1100, 720);

    g_signal_connect(win_, "close-request",
        G_CALLBACK(+[](AdwApplicationWindow* w, gpointer) -> gboolean {
            gtk_widget_set_visible(GTK_WIDGET(w), FALSE);
            return TRUE;
        }), nullptr);

    setup_ui();
}

MainWindow::~MainWindow() = default;

void MainWindow::present() { gtk_window_present(GTK_WINDOW(win_)); }
void MainWindow::set_visible(bool v) {
    gtk_widget_set_visible(GTK_WIDGET(win_), v ? TRUE : FALSE);
}

void MainWindow::setup_dbus() {
    try {
        dbus_connection_ = sdbus::createSessionBusConnection();
        control_proxy_ = sdbus::createProxy(
            *dbus_connection_,
            sdbus::ServiceName{"org.firebox"},
            sdbus::ObjectPath{"/org/firebox"});
        spdlog::info("Connected to FireBox backend via D-Bus");
    } catch (const sdbus::Error& e) {
        spdlog::warn("Could not connect to backend: {}", e.what());
    }
}

void MainWindow::setup_ui() {
    // ── Pages ────────────────────────────────────────────────────
    dashboard_page_   = std::make_unique<DashboardPage>(control_proxy_.get());
    settings_page_    = std::make_unique<SettingsPage>(control_proxy_.get(),
                                                        GTK_WINDOW(win_));
    route_page_       = std::make_unique<RoutePage>(control_proxy_.get());
    allowlist_page_   = std::make_unique<AllowlistPage>(control_proxy_.get());
    connections_page_ = std::make_unique<ConnectionsPage>(control_proxy_.get());

    // ── Content stack ─────────────────────────────────────────────
    stack_ = GTK_STACK(gtk_stack_new());
    gtk_stack_set_transition_type(stack_, GTK_STACK_TRANSITION_TYPE_CROSSFADE);
    gtk_stack_set_transition_duration(stack_, 200);

    gtk_stack_add_named(stack_,
        make_toolbar_page("Dashboard",   GTK_WIDGET(dashboard_page_->gobj())),
        "dashboard");
    gtk_stack_add_named(stack_,
        make_toolbar_page("Settings",    settings_page_->widget()),
        "settings");
    gtk_stack_add_named(stack_,
        make_toolbar_page("Routes",      GTK_WIDGET(route_page_->gobj())),
        "routes");
    gtk_stack_add_named(stack_,
        make_toolbar_page("Allowlist",   GTK_WIDGET(allowlist_page_->gobj())),
        "allowlist");
    gtk_stack_add_named(stack_,
        make_toolbar_page("Connections", GTK_WIDGET(connections_page_->gobj())),
        "connections");

    // ── Sidebar nav list ──────────────────────────────────────────
    auto* nav_list = gtk_list_box_new();
    gtk_widget_add_css_class(nav_list, "navigation-sidebar");
    gtk_list_box_set_selection_mode(GTK_LIST_BOX(nav_list), GTK_SELECTION_SINGLE);

    for (const auto& e : NAV_ENTRIES) {
        auto* row = gtk_list_box_row_new();
        auto* hbox = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 10);
        gtk_widget_set_margin_start(hbox, 8);
        gtk_widget_set_margin_end(hbox, 8);
        gtk_widget_set_margin_top(hbox, 7);
        gtk_widget_set_margin_bottom(hbox, 7);

        auto* icon = gtk_image_new_from_icon_name(e.icon);
        gtk_image_set_icon_size(GTK_IMAGE(icon), GTK_ICON_SIZE_NORMAL);
        gtk_box_append(GTK_BOX(hbox), icon);

        auto* lbl = gtk_label_new(e.label);
        gtk_label_set_xalign(GTK_LABEL(lbl), 0.0f);
        gtk_widget_set_hexpand(lbl, TRUE);
        gtk_box_append(GTK_BOX(hbox), lbl);

        gtk_list_box_row_set_child(GTK_LIST_BOX_ROW(row), hbox);
        g_object_set_data(G_OBJECT(row), "page-key",
            const_cast<char*>(e.key));
        gtk_list_box_append(GTK_LIST_BOX(nav_list), row);
    }

    // Connect selection → stack switch
    g_signal_connect(nav_list, "row-selected",
        G_CALLBACK(+[](GtkListBox*, GtkListBoxRow* row, gpointer ud) {
            if (!row) return;
            auto* self = static_cast<MainWindow*>(ud);
            auto* key  = static_cast<const char*>(
                g_object_get_data(G_OBJECT(row), "page-key"));
            if (key) gtk_stack_set_visible_child_name(self->stack_, key);
        }), this);

    // Select first row
    if (auto* first = gtk_list_box_get_row_at_index(GTK_LIST_BOX(nav_list), 0))
        gtk_list_box_select_row(GTK_LIST_BOX(nav_list), first);

    // ── Sidebar pane (AdwToolbarView + AdwHeaderBar + nav list) ───
    auto* sidebar_scroll = gtk_scrolled_window_new();
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(sidebar_scroll),
        GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(sidebar_scroll),
        nav_list);

    auto* sidebar_tv = adw_toolbar_view_new();
    auto* sidebar_hb = adw_header_bar_new();
    adw_header_bar_set_title_widget(ADW_HEADER_BAR(sidebar_hb),
        GTK_WIDGET(adw_window_title_new("FireBox", nullptr)));
    adw_toolbar_view_add_top_bar(ADW_TOOLBAR_VIEW(sidebar_tv),
        GTK_WIDGET(sidebar_hb));
    adw_toolbar_view_set_content(ADW_TOOLBAR_VIEW(sidebar_tv),
        sidebar_scroll);

    // ── NavigationSplitView ───────────────────────────────────────
    auto* sidebar_np = adw_navigation_page_new(
        GTK_WIDGET(sidebar_tv), "Navigation");
    auto* content_np = adw_navigation_page_new(
        GTK_WIDGET(stack_), "Content");

    auto* split = ADW_NAVIGATION_SPLIT_VIEW(adw_navigation_split_view_new());
    adw_navigation_split_view_set_sidebar(split, sidebar_np);
    adw_navigation_split_view_set_content(split, content_np);
    adw_navigation_split_view_set_sidebar_width_fraction(split, 0.22);

    adw_application_window_set_content(win_, GTK_WIDGET(split));
}

} // namespace firebox::frontend
