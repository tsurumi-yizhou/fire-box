/// @file connections_page.cpp
#include "connections_page.hpp"
#include <spdlog/spdlog.h>
#include <chrono>

namespace firebox::frontend {

ConnectionsPage::ConnectionsPage(sdbus::IProxy* proxy)
    : Gtk::Box(Gtk::Orientation::VERTICAL, 0)
    , proxy_(proxy) {
    setup_ui();
    timer_ = Glib::signal_timeout().connect(
        sigc::mem_fun(*this, &ConnectionsPage::on_auto_refresh), 5000);
    refresh_connections();
}

void ConnectionsPage::setup_ui() {
    auto* page = adw_preferences_page_new();
    gtk_box_append(GTK_BOX(gobj()), GTK_WIDGET(page));

    group_ = ADW_PREFERENCES_GROUP(adw_preferences_group_new());
    adw_preferences_group_set_title(group_, "Active Connections");
    adw_preferences_group_set_description(group_,
        "Client applications currently connected to the FireBox service");

    auto* refresh_btn = gtk_button_new_from_icon_name("view-refresh-symbolic");
    gtk_widget_add_css_class(refresh_btn, "flat");
    g_signal_connect(refresh_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer ud) {
            static_cast<ConnectionsPage*>(ud)->refresh_connections();
        }), this);
    adw_preferences_group_set_header_suffix(group_, refresh_btn);

    adw_preferences_page_add(ADW_PREFERENCES_PAGE(page), group_);
}

void ConnectionsPage::refresh_connections() {
    for (auto* w : rows_)
        adw_preferences_group_remove(group_, w);
    rows_.clear();

    if (!proxy_) return;

    try {
        using ConnTuple = sdbus::Struct<
            std::string, std::string, int32_t, int64_t, int64_t>;
        std::tuple<bool, std::string, std::vector<ConnTuple>> result;
        proxy_->callMethod("ListConnections")
            .onInterface("org.firebox.Control")
            .storeResultsTo(result);

        auto& [success, message, connections] = result;
        if (!success) return;

        if (connections.empty()) {
            auto* empty = adw_action_row_new();
            adw_preferences_row_set_title(ADW_PREFERENCES_ROW(empty),
                "No active connections");
            adw_action_row_set_subtitle(ADW_ACTION_ROW(empty),
                "Client applications will appear here when connected");
            gtk_widget_set_sensitive(GTK_WIDGET(empty), FALSE);
            adw_preferences_group_add(group_, GTK_WIDGET(empty));
            rows_.push_back(GTK_WIDGET(empty));
            return;
        }

        auto now_ms = std::chrono::duration_cast<std::chrono::milliseconds>(
            std::chrono::system_clock::now().time_since_epoch()).count();

        for (auto& c : connections) {
            auto client_name  = std::get<1>(c);
            auto pid          = std::get<2>(c);
            auto connected_at = std::get<3>(c);
            auto req_count    = std::get<4>(c);

            auto secs = (now_ms - connected_at) / 1000;
            std::string duration;
            if      (secs < 60)   duration = std::to_string(secs) + "s";
            else if (secs < 3600) duration = std::to_string(secs / 60) + "m";
            else                  duration = std::to_string(secs / 3600) + "h";

            auto* row = adw_action_row_new();
            adw_preferences_row_set_title(ADW_PREFERENCES_ROW(row),
                client_name.c_str());
            adw_action_row_set_subtitle(ADW_ACTION_ROW(row),
                ("PID " + std::to_string(pid)).c_str());

            auto* dur_lbl = gtk_label_new(duration.c_str());
            gtk_widget_add_css_class(dur_lbl, "dim-label");
            gtk_widget_add_css_class(dur_lbl, "caption");
            gtk_widget_set_valign(dur_lbl, GTK_ALIGN_CENTER);
            adw_action_row_add_suffix(ADW_ACTION_ROW(row), dur_lbl);

            auto req_str  = std::to_string(req_count) + " req";
            auto* req_lbl = gtk_label_new(req_str.c_str());
            gtk_widget_add_css_class(req_lbl, req_count > 0 ? "success" : "dim-label");
            gtk_widget_add_css_class(req_lbl, "caption-heading");
            gtk_widget_set_valign(req_lbl, GTK_ALIGN_CENTER);
            adw_action_row_add_suffix(ADW_ACTION_ROW(row), req_lbl);

            adw_preferences_group_add(group_, GTK_WIDGET(row));
            rows_.push_back(GTK_WIDGET(row));
        }
    } catch (const sdbus::Error& e) {
        spdlog::warn("ListConnections failed: {}", e.what());
    }
}

bool ConnectionsPage::on_auto_refresh() {
    refresh_connections();
    return true;
}

} // namespace firebox::frontend
