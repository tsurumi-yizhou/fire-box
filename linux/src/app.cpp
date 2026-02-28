// ---------------------------------------------------------------------------
// app.cpp — Fire Box Linux GUI entry point
//
// Creates an Adwaita application window with sidebar navigation (GtkStackSidebar)
// and a content stack hosting the five management panels.  Connects to the
// FireBox Rust service over D-Bus.
// ---------------------------------------------------------------------------

#include "dbus_client.hpp"
#include "dashboard.hpp"
#include "connections.hpp"
#include "providers.hpp"
#include "routes.hpp"
#include "allowlist.hpp"

#include <adwaita.h>
#include <gtk/gtk.h>
#include <libintl.h>
#include <locale.h>

#include <memory>

#define _(S) gettext(S)

// ---------------------------------------------------------------------------
// Application state — owned by the activate callback, destroyed with the app
// ---------------------------------------------------------------------------

struct AppState {
    std::unique_ptr<FireBoxDbusClient> client;
    std::unique_ptr<DashboardView>     dashboard;
    std::unique_ptr<ConnectionsView>   connections;
    std::unique_ptr<ProvidersView>     providers;
    std::unique_ptr<RoutesView>        routes;
    std::unique_ptr<AllowlistView>     allowlist;
    GtkWidget*                         window = nullptr;
};

// ---------------------------------------------------------------------------
// Action callbacks
// ---------------------------------------------------------------------------

static void on_add_provider(GSimpleAction* /*action*/, GVariant* /*param*/,
                            gpointer user_data) {
    auto* state = static_cast<AppState*>(user_data);
    state->providers->show_add_dialog(GTK_WINDOW(state->window),
                                      *state->client);
}

static void on_add_route(GSimpleAction* /*action*/, GVariant* /*param*/,
                         gpointer user_data) {
    auto* state = static_cast<AppState*>(user_data);
    state->routes->show_add_dialog(GTK_WINDOW(state->window),
                                   *state->client);
}

static void on_about(GSimpleAction* /*action*/, GVariant* /*param*/,
                     gpointer user_data) {
    auto* state = static_cast<AppState*>(user_data);
    AdwAboutWindow* about = ADW_ABOUT_WINDOW(adw_about_window_new());

    adw_about_window_set_application_name(about, "Fire Box");
    adw_about_window_set_application_icon(about, "application-x-executable");
    adw_about_window_set_version(about, "1.0.0");
    adw_about_window_set_comments(about,
        _("A cross-platform AI gateway service"));
    adw_about_window_set_developer_name(about, "Fire Box Project");

    gtk_window_set_transient_for(GTK_WINDOW(about),
                                 GTK_WINDOW(state->window));
    gtk_window_present(GTK_WINDOW(about));
}

// ---------------------------------------------------------------------------
// Stack page switching — show/hide header bar buttons contextually
// ---------------------------------------------------------------------------

struct HeaderCtx {
    GtkWidget* add_provider_btn;
    GtkWidget* add_route_btn;
    GtkWidget* stack;
};

static void on_visible_child_changed(GObject* /*obj*/, GParamSpec* /*pspec*/,
                                     gpointer user_data) {
    auto* hctx = static_cast<HeaderCtx*>(user_data);
    const char* name = gtk_stack_get_visible_child_name(
        GTK_STACK(hctx->stack));

    bool show_provider_btn = (g_strcmp0(name, "providers") == 0);
    bool show_route_btn    = (g_strcmp0(name, "routes") == 0);

    gtk_widget_set_visible(hctx->add_provider_btn, show_provider_btn);
    gtk_widget_set_visible(hctx->add_route_btn, show_route_btn);
}

// ---------------------------------------------------------------------------
// Application activate
// ---------------------------------------------------------------------------

static void on_activate(GtkApplication* gtk_app, gpointer user_data) {
    auto* state = static_cast<AppState*>(user_data);

    // ---- D-Bus client -----------------------------------------------------
    try {
        state->client = std::make_unique<FireBoxDbusClient>();
    } catch (const std::exception& e) {
        g_warning("Failed to connect to D-Bus: %s", e.what());
        // Continue — panels will show disconnected state on first refresh
        state->client = nullptr;
    }

    // ---- Window -----------------------------------------------------------
    state->window = adw_application_window_new(gtk_app);
    gtk_window_set_title(GTK_WINDOW(state->window), _("Fire Box"));
    gtk_window_set_default_size(GTK_WINDOW(state->window), 900, 640);

    // ---- Content stack ----------------------------------------------------
    GtkWidget* stack = gtk_stack_new();
    gtk_stack_set_transition_type(GTK_STACK(stack),
                                  GTK_STACK_TRANSITION_TYPE_CROSSFADE);

    // Create views
    state->dashboard   = std::make_unique<DashboardView>(state->client.get());
    state->connections = std::make_unique<ConnectionsView>();
    state->providers   = std::make_unique<ProvidersView>();
    state->routes      = std::make_unique<RoutesView>();
    state->allowlist   = std::make_unique<AllowlistView>();

    gtk_stack_add_titled(GTK_STACK(stack), state->dashboard->widget(),
                         "dashboard", _("Dashboard"));
    gtk_stack_add_titled(GTK_STACK(stack), state->connections->widget(),
                         "connections", _("Connections"));
    gtk_stack_add_titled(GTK_STACK(stack), state->providers->widget(),
                         "providers", _("Providers"));
    gtk_stack_add_titled(GTK_STACK(stack), state->routes->widget(),
                         "routes", _("Routes"));
    gtk_stack_add_titled(GTK_STACK(stack), state->allowlist->widget(),
                         "allowlist", _("Allowlist"));

    // ---- Sidebar ----------------------------------------------------------
    GtkWidget* sidebar = gtk_stack_sidebar_new();
    gtk_stack_sidebar_set_stack(GTK_STACK_SIDEBAR(sidebar), GTK_STACK(stack));
    gtk_widget_set_size_request(sidebar, 180, -1);

    // ---- Header bar -------------------------------------------------------
    GtkWidget* header = adw_header_bar_new();

    // "Add Provider" button — only visible on the Providers page
    GtkWidget* add_provider_btn = gtk_button_new_from_icon_name(
        "list-add-symbolic");
    gtk_widget_set_tooltip_text(add_provider_btn, _("Add Provider"));
    gtk_actionable_set_action_name(GTK_ACTIONABLE(add_provider_btn),
                                   "app.add-provider");
    gtk_widget_set_visible(add_provider_btn, FALSE);
    adw_header_bar_pack_start(ADW_HEADER_BAR(header), add_provider_btn);

    // "Add Route" button — only visible on the Routes page
    GtkWidget* add_route_btn = gtk_button_new_from_icon_name(
        "list-add-symbolic");
    gtk_widget_set_tooltip_text(add_route_btn, _("Add Route"));
    gtk_actionable_set_action_name(GTK_ACTIONABLE(add_route_btn),
                                   "app.add-route");
    gtk_widget_set_visible(add_route_btn, FALSE);
    adw_header_bar_pack_start(ADW_HEADER_BAR(header), add_route_btn);

    // Menu button
    GMenu* menu = g_menu_new();
    g_menu_append(menu, _("About Fire Box"), "app.about");

    GtkWidget* menu_btn = gtk_menu_button_new();
    gtk_menu_button_set_icon_name(GTK_MENU_BUTTON(menu_btn),
                                  "open-menu-symbolic");
    gtk_menu_button_set_menu_model(GTK_MENU_BUTTON(menu_btn),
                                   G_MENU_MODEL(menu));
    adw_header_bar_pack_end(ADW_HEADER_BAR(header), menu_btn);

    g_object_unref(menu);

    // Track page changes for contextual header buttons
    auto* hctx = new HeaderCtx{add_provider_btn, add_route_btn, stack};
    g_signal_connect(stack, "notify::visible-child",
                     G_CALLBACK(on_visible_child_changed), hctx);

    // ---- Layout assembly --------------------------------------------------
    // ToolbarView wraps the header bar + content area
    GtkWidget* toolbar_view = adw_toolbar_view_new();
    adw_toolbar_view_add_top_bar(ADW_TOOLBAR_VIEW(toolbar_view), header);

    // Horizontal box: sidebar | separator | stack
    GtkWidget* hbox = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
    gtk_box_append(GTK_BOX(hbox), sidebar);
    gtk_box_append(GTK_BOX(hbox), gtk_separator_new(GTK_ORIENTATION_VERTICAL));

    // Let the stack expand to fill remaining space
    gtk_widget_set_hexpand(stack, TRUE);
    gtk_widget_set_vexpand(stack, TRUE);
    gtk_box_append(GTK_BOX(hbox), stack);

    adw_toolbar_view_set_content(ADW_TOOLBAR_VIEW(toolbar_view), hbox);
    adw_application_window_set_content(
        ADW_APPLICATION_WINDOW(state->window), toolbar_view);

    // ---- GActions ---------------------------------------------------------
    GActionEntry actions[] = {
        {"add-provider", on_add_provider, nullptr, nullptr, nullptr, {0}},
        {"add-route",    on_add_route,    nullptr, nullptr, nullptr, {0}},
        {"about",        on_about,        nullptr, nullptr, nullptr, {0}},
    };
    g_action_map_add_action_entries(G_ACTION_MAP(gtk_app), actions,
                                   G_N_ELEMENTS(actions), state);

    // ---- Wire connections refresh button ----------------------------------
    if (state->client) {
        struct RefreshCtx {
            ConnectionsView*   view;
            FireBoxDbusClient* client;
        };
        auto* rctx = new RefreshCtx{state->connections.get(),
                                    state->client.get()};
        g_signal_connect(state->connections->refresh_button(), "clicked",
            G_CALLBACK(+[](GtkButton* /*btn*/, gpointer ud) {
                auto* rc = static_cast<RefreshCtx*>(ud);
                rc->view->refresh(*rc->client);
            }), rctx);
    }

    // ---- Initial data load ------------------------------------------------
    if (state->client) {
        state->dashboard->refresh(*state->client);
        state->connections->refresh(*state->client);
        state->providers->refresh(*state->client);
        state->routes->refresh(*state->client);
        state->allowlist->refresh(*state->client);
    }

    // ---- Present ----------------------------------------------------------
    gtk_window_present(GTK_WINDOW(state->window));
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

auto main(int argc, char* argv[]) -> int {
    // Initialize i18n
    setlocale(LC_ALL, "");
    bindtextdomain("fire-box", LOCALEDIR);
    textdomain("fire-box");

    // Initialize libadwaita
    adw_init();

    AdwApplication* app = adw_application_new("com.firebox.App",
                                              G_APPLICATION_DEFAULT_FLAGS);

    // Allocate state that lives for the entire application lifetime
    auto state = std::make_unique<AppState>();
    g_signal_connect(app, "activate", G_CALLBACK(on_activate), state.get());

    int status = g_application_run(G_APPLICATION(app), argc, argv);

    g_object_unref(app);
    return status;
}

#undef _
