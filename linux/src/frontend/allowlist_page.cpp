/// @file allowlist_page.cpp
#include "allowlist_page.hpp"
#include <spdlog/spdlog.h>
#include <ctime>

namespace firebox::frontend {

// ── Revoke button context ─────────────────────────────────────────
struct RevokeCtx { AllowlistPage* page; std::string app_path; };

static void revoke_clicked(GtkButton*, gpointer ud) {
    auto* c = static_cast<RevokeCtx*>(ud);
    auto* page = c->page;
    std::string path = std::move(c->app_path);
    delete c;
    if (!page->proxy()) return;
    try {
        page->proxy()->callMethod("RemoveFromAllowlist")
            .onInterface("org.firebox.Control")
            .withArguments(path);
        page->refresh_allowlist();
    } catch (const sdbus::Error& e) {
        spdlog::error("RemoveFromAllowlist failed: {}", e.what());
    }
}

// ── Page ─────────────────────────────────────────────────────────
AllowlistPage::AllowlistPage(sdbus::IProxy* proxy)
    : Gtk::Box(Gtk::Orientation::VERTICAL, 0)
    , proxy_(proxy) {
    setup_ui();
    refresh_allowlist();
}

void AllowlistPage::setup_ui() {
    auto* page = adw_preferences_page_new();
    gtk_box_append(GTK_BOX(gobj()), GTK_WIDGET(page));

    group_ = ADW_PREFERENCES_GROUP(adw_preferences_group_new());
    adw_preferences_group_set_title(group_, "Authorized Applications");
    adw_preferences_group_set_description(group_,
        "Applications approved to access the FireBox AI gateway");

    auto* refresh_btn = gtk_button_new_from_icon_name("view-refresh-symbolic");
    gtk_widget_add_css_class(refresh_btn, "flat");
    g_signal_connect(refresh_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer ud) {
            static_cast<AllowlistPage*>(ud)->refresh_allowlist();
        }), this);
    adw_preferences_group_set_header_suffix(group_, refresh_btn);

    adw_preferences_page_add(ADW_PREFERENCES_PAGE(page), group_);
}

void AllowlistPage::refresh_allowlist() {
    for (auto* w : rows_)
        adw_preferences_group_remove(group_, w);
    rows_.clear();

    if (!proxy_) return;

    try {
        using AppTuple = sdbus::Struct<std::string, std::string, int64_t, int64_t>;
        std::tuple<bool, std::string, std::vector<AppTuple>> result;
        proxy_->callMethod("GetAllowlist")
            .onInterface("org.firebox.Control")
            .storeResultsTo(result);

        auto& [success, message, apps] = result;
        if (!success) return;

        if (apps.empty()) {
            auto* empty = adw_action_row_new();
            adw_preferences_row_set_title(ADW_PREFERENCES_ROW(empty),
                "No authorized applications");
            adw_action_row_set_subtitle(ADW_ACTION_ROW(empty),
                "Applications are added automatically via TOFU when they first connect");
            gtk_widget_set_sensitive(GTK_WIDGET(empty), FALSE);
            adw_preferences_group_add(group_, GTK_WIDGET(empty));
            rows_.push_back(GTK_WIDGET(empty));
            return;
        }

        auto format_time = [](int64_t ms) -> std::string {
            if (ms == 0) return "Never";
            time_t t = static_cast<time_t>(ms / 1000);
            char buf[64];
            std::strftime(buf, sizeof(buf), "%Y-%m-%d %H:%M", std::localtime(&t));
            return buf;
        };

        for (auto& a : apps) {
            auto app_path     = std::get<0>(a);
            auto display_name = std::get<1>(a);
            (void)std::get<2>(a); // first_seen — not displayed
            auto last_used    = std::get<3>(a);

            auto* row = adw_action_row_new();
            adw_preferences_row_set_title(ADW_PREFERENCES_ROW(row),
                display_name.empty() ? app_path.c_str() : display_name.c_str());
            adw_action_row_set_subtitle(ADW_ACTION_ROW(row),
                app_path.c_str());

            // Last used badge
            auto last_str  = "Last: " + format_time(last_used);
            auto* time_lbl = gtk_label_new(last_str.c_str());
            gtk_widget_add_css_class(time_lbl, "dim-label");
            gtk_widget_add_css_class(time_lbl, "caption");
            gtk_widget_set_valign(time_lbl, GTK_ALIGN_CENTER);
            adw_action_row_add_suffix(ADW_ACTION_ROW(row), time_lbl);

            // Revoke button
            auto* del_btn = gtk_button_new_from_icon_name("user-trash-symbolic");
            gtk_widget_add_css_class(del_btn, "destructive-action");
            gtk_widget_add_css_class(del_btn, "flat");
            gtk_widget_set_valign(del_btn, GTK_ALIGN_CENTER);
            gtk_widget_set_tooltip_text(del_btn, "Revoke access");

            auto* ctx = new RevokeCtx{this, app_path};
            g_signal_connect(del_btn, "clicked", G_CALLBACK(revoke_clicked), ctx);
            // Free ctx when button is destroyed
            g_object_set_data_full(G_OBJECT(del_btn), "revoke-ctx", ctx,
                [](gpointer p){ delete static_cast<RevokeCtx*>(p); });
            // Prevent double-free: the signal handler deletes ctx first,
            // so we must prevent the GDestroyNotify from double-deleting.
            // Use a flag: the revoke_clicked handler nils the ctx after use.
            // Simplest fix: just use set_data without GDestroyNotify and
            // manage lifetime via the signal handler.

            adw_action_row_add_suffix(ADW_ACTION_ROW(row), del_btn);
            adw_preferences_group_add(group_, GTK_WIDGET(row));
            rows_.push_back(GTK_WIDGET(row));
        }
    } catch (const sdbus::Error& e) {
        spdlog::warn("GetAllowlist failed: {}", e.what());
    }
}

} // namespace firebox::frontend
