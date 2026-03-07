/// @file settings_page.cpp
#include "settings_page.hpp"
#include <spdlog/spdlog.h>

namespace firebox::frontend {

// ── Helper C-style contexts and callbacks ────────────────────────
struct DelCtx { SettingsPage* page; std::string provider_id; };

struct ApiKeyCtx {
    SettingsPage* page;
    AdwDialog*    dlg;
    AdwEntryRow*  name_row;
    AdwComboRow*  type_row;
    GtkWidget*    key_row;
    AdwEntryRow*  url_row;
};

struct OAuthCtx {
    SettingsPage* page;
    AdwDialog*    dlg;
    AdwEntryRow*  name_row;
    AdwComboRow*  type_row;
    AdwActionRow* code_row;
};

// ── New contexts for models & edit dialogs ────────────────────────
struct ModelsBtnCtx {
    SettingsPage* page;
    std::string   provider_id;
    std::string   provider_name;
};

struct RefreshModelsBtnCtx {
    SettingsPage* page;
    std::string   provider_id;
    GtkListBox*   list_box;
    AdwDialog*    dlg;
};

struct EditBtnCtx {
    SettingsPage* page;
    std::string   provider_id;
    std::string   name;
    int           type_id;
    std::string   base_url;
    bool          enabled;
};

struct TogCtx {
    SettingsPage* page;
    std::string   provider_id;
    std::string   model_id;
};

struct EditProvCtx {
    SettingsPage* page;
    std::string   provider_id;
    int           type_id;
    AdwDialog*    dlg;
    AdwEntryRow*  name_row;
    AdwEntryRow*  url_row;       // null for OAuth
    GtkWidget*    apikey_row;    // null for OAuth (AdwPasswordEntryRow)
};

// ────────────────────────────────────────────────────────────────────
// Provider delete
// ────────────────────────────────────────────────────────────────────

static gboolean idle_refresh_providers(gpointer ud) {
    static_cast<SettingsPage*>(ud)->refresh_providers();
    return G_SOURCE_REMOVE;
}

static void del_btn_clicked(GtkButton*, gpointer ud) {
    auto* c = static_cast<DelCtx*>(ud);
    auto* page = c->page;
    std::string provider_id = std::move(c->provider_id);
    delete c;
    if (!page->proxy()) return;
    try {
        page->proxy()->callMethod("DeleteProvider")
            .onInterface("org.firebox.Control")
            .withArguments(provider_id);
        g_idle_add(idle_refresh_providers, page);
    } catch (const sdbus::Error& e) {
        spdlog::error("DeleteProvider: {}", e.what());
    }
}

// ────────────────────────────────────────────────────────────────────
// Add API Key dialog
// ────────────────────────────────────────────────────────────────────

static void api_dialog_closed(AdwDialog*, gpointer ud) {
    delete static_cast<ApiKeyCtx*>(ud);
}

static void api_save_btn_clicked(GtkButton*, gpointer ud) {
    auto* c = static_cast<ApiKeyCtx*>(ud);
    if (!c->page->proxy()) { adw_dialog_close(c->dlg); return; }
    try {
        const char* type_strings[] = {"openai", "anthropic", "gemini", nullptr};
        auto sel  = adw_combo_row_get_selected(c->type_row);
        std::string type = sel < 3u ? type_strings[sel] : "openai";

        std::tuple<bool, std::string, std::string> add_result;
        c->page->proxy()->callMethod("AddApiKeyProvider")
            .onInterface("org.firebox.Control")
            .withArguments(
                std::string(gtk_editable_get_text(GTK_EDITABLE(c->name_row))),
                type,
                std::string(gtk_editable_get_text(GTK_EDITABLE(c->key_row))),
                std::string(gtk_editable_get_text(GTK_EDITABLE(c->url_row))))
            .storeResultsTo(add_result);

        auto& [ok, msg, pid] = add_result;
        if (ok && !pid.empty()) {
            // Discover models from the provider API (best-effort)
            try {
                c->page->proxy()->callMethod("DiscoverModels")
                    .onInterface("org.firebox.Control")
                    .withArguments(pid);
            } catch (const sdbus::Error& e) {
                spdlog::warn("DiscoverModels after add failed: {}", e.what());
            }
        }
        c->page->refresh_providers();
    } catch (const sdbus::Error& e) {
        spdlog::error("AddApiKeyProvider: {}", e.what());
    }
    adw_dialog_close(c->dlg);
}

// ────────────────────────────────────────────────────────────────────
// Add OAuth dialog
// ────────────────────────────────────────────────────────────────────

static void oauth_dialog_closed(AdwDialog*, gpointer ud) {
    delete static_cast<OAuthCtx*>(ud);
}

static void auth_btn_clicked(GtkButton* btn, gpointer ud) {
    auto* c = static_cast<OAuthCtx*>(ud);
    if (!c->page->proxy()) return;
    gtk_widget_set_sensitive(GTK_WIDGET(btn), FALSE);
    try {
        const char* types[] = {"copilot", "dashscope", nullptr};
        auto sel = adw_combo_row_get_selected(c->type_row);
        std::string type = sel < 2 ? types[sel] : "copilot";

        std::tuple<bool, std::string, std::string, std::string, std::string,
                   std::string, int32_t, int32_t> reply;
        c->page->proxy()->callMethod("AddOAuthProvider")
            .onInterface("org.firebox.Control")
            .withArguments(
                std::string(gtk_editable_get_text(GTK_EDITABLE(c->name_row))),
                type)
            .storeResultsTo(reply);

        auto& [ok, msg, pid, device_code, user_code,
               verify_uri, expires, interval] = reply;
        if (ok) {
            auto code_text = "Code: " + user_code + " — " + verify_uri;
            adw_preferences_row_set_title(
                ADW_PREFERENCES_ROW(c->code_row), code_text.c_str());
            gtk_widget_set_sensitive(GTK_WIDGET(c->code_row), TRUE);
            c->page->refresh_providers();
        } else {
            adw_preferences_row_set_title(
                ADW_PREFERENCES_ROW(c->code_row),
                ("Error: " + msg).c_str());
        }
    } catch (const sdbus::Error& e) {
        adw_preferences_row_set_title(ADW_PREFERENCES_ROW(c->code_row), e.what());
    }
}

static void add_api_btn_clicked(GtkButton*, gpointer ud) {
    static_cast<SettingsPage*>(ud)->show_add_api_key_dialog();
}

static void add_oauth_btn_clicked(GtkButton*, gpointer ud) {
    static_cast<SettingsPage*>(ud)->show_add_oauth_dialog();
}

// ────────────────────────────────────────────────────────────────────
// Models row button
// ────────────────────────────────────────────────────────────────────

static void models_btn_clicked(GtkButton*, gpointer ud) {
    auto* c = static_cast<ModelsBtnCtx*>(ud);
    c->page->show_models_dialog(c->provider_id, c->provider_name);
}

// ────────────────────────────────────────────────────────────────────
// Edit provider row button
// ────────────────────────────────────────────────────────────────────

static void edit_btn_clicked(GtkButton*, gpointer ud) {
    auto* c = static_cast<EditBtnCtx*>(ud);
    c->page->show_edit_provider_dialog(
        c->provider_id, c->name, c->type_id, c->base_url, c->enabled);
}

// ────────────────────────────────────────────────────────────────────
// Edit provider dialog save/close
// ────────────────────────────────────────────────────────────────────

static void edit_prov_dlg_closed(AdwDialog*, gpointer ud) {
    delete static_cast<EditProvCtx*>(ud);
}

static void edit_prov_save_clicked(GtkButton*, gpointer ud) {
    auto* c = static_cast<EditProvCtx*>(ud);
    if (!c->page->proxy()) { adw_dialog_close(c->dlg); return; }
    try {
        std::string new_name = gtk_editable_get_text(GTK_EDITABLE(c->name_row));
        std::string new_url  = c->url_row
            ? gtk_editable_get_text(GTK_EDITABLE(c->url_row)) : "";
        std::string new_key  = c->apikey_row
            ? gtk_editable_get_text(GTK_EDITABLE(c->apikey_row)) : "";

        c->page->proxy()->callMethod("UpdateProvider")
            .onInterface("org.firebox.Control")
            .withArguments(c->provider_id, new_name, new_key, new_url);
        c->page->refresh_providers();
    } catch (const sdbus::Error& e) {
        spdlog::error("UpdateProvider: {}", e.what());
    }
    adw_dialog_close(c->dlg);
}

// ── Construction ─────────────────────────────────────────────────
SettingsPage::SettingsPage(sdbus::IProxy* proxy, GtkWindow* parent)
    : proxy_(proxy), parent_(parent) {

    page_widget_ = ADW_PREFERENCES_PAGE(adw_preferences_page_new());
    adw_preferences_page_set_icon_name(page_widget_, "preferences-system-symbolic");

    // ── Providers group ───────────────────────────────────────────
    providers_group_ = ADW_PREFERENCES_GROUP(adw_preferences_group_new());
    adw_preferences_group_set_title(providers_group_, "AI Providers");
    adw_preferences_group_set_description(providers_group_,
        "Configure API keys and OAuth tokens for AI providers");

    auto* btn_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 4);

    auto* add_api_btn = gtk_button_new_with_label("+ API Key");
    gtk_widget_add_css_class(add_api_btn, "flat");
    g_signal_connect(add_api_btn, "clicked",
        G_CALLBACK(add_api_btn_clicked), this);
    gtk_box_append(GTK_BOX(btn_box), add_api_btn);

    auto* add_oauth_btn = gtk_button_new_with_label("+ OAuth");
    gtk_widget_add_css_class(add_oauth_btn, "flat");
    g_signal_connect(add_oauth_btn, "clicked",
        G_CALLBACK(add_oauth_btn_clicked), this);
    gtk_box_append(GTK_BOX(btn_box), add_oauth_btn);

    adw_preferences_group_set_header_suffix(providers_group_, btn_box);
    adw_preferences_page_add(page_widget_, providers_group_);

    refresh_providers();
}

GtkWidget* SettingsPage::widget() const {
    return GTK_WIDGET(page_widget_);
}

// ── Providers list ────────────────────────────────────────────────
void SettingsPage::refresh_providers() {
    for (auto* row : provider_rows_)
        adw_preferences_group_remove(providers_group_, row);
    provider_rows_.clear();

    if (!proxy_) return;

    try {
        using ProvTuple = sdbus::Struct<
            std::string, std::string, int32_t, std::string, std::string, bool>;
        std::tuple<bool, std::string, std::vector<ProvTuple>> result;
        proxy_->callMethod("ListProviders")
            .onInterface("org.firebox.Control")
            .storeResultsTo(result);

        auto& [success, message, providers] = result;
        if (!success) {
            spdlog::warn("ListProviders: {}", message);
            return;
        }

        for (auto& p : providers) {
            const auto& pid     = std::get<0>(p);
            const auto& name    = std::get<1>(p);
            const auto  type_id = std::get<2>(p);
            const auto& base_url = std::get<3>(p);
            const bool  enabled = std::get<5>(p);

            const char* type_str =
                type_id == 1 ? "API Key" : (type_id == 2 ? "OAuth" : "Local");

            auto* row = adw_action_row_new();
            adw_preferences_row_set_title(ADW_PREFERENCES_ROW(row), name.c_str());
            adw_action_row_set_subtitle(ADW_ACTION_ROW(row), type_str);

            // Status badge
            auto* status_lbl = gtk_label_new(enabled ? "Enabled" : "Disabled");
            gtk_widget_add_css_class(status_lbl, enabled ? "success" : "warning");
            gtk_widget_set_valign(status_lbl, GTK_ALIGN_CENTER);
            adw_action_row_add_suffix(ADW_ACTION_ROW(row), status_lbl);

            // Models button (only for non-local providers)
            {
                auto* mctx = new ModelsBtnCtx{this, pid, name};
                auto* models_btn = gtk_button_new_from_icon_name("view-list-symbolic");
                gtk_widget_add_css_class(models_btn, "flat");
                gtk_widget_set_valign(models_btn, GTK_ALIGN_CENTER);
                gtk_widget_set_tooltip_text(models_btn, "Manage models");
                g_signal_connect(models_btn, "clicked",
                    G_CALLBACK(models_btn_clicked), mctx);
                g_object_set_data_full(G_OBJECT(models_btn), "mctx", mctx,
                    [](gpointer p){ delete static_cast<ModelsBtnCtx*>(p); });
                adw_action_row_add_suffix(ADW_ACTION_ROW(row), models_btn);
            }

            // Edit button
            {
                auto* ectx = new EditBtnCtx{this, pid, name, type_id, base_url, enabled};
                auto* edit_btn = gtk_button_new_from_icon_name("document-edit-symbolic");
                gtk_widget_add_css_class(edit_btn, "flat");
                gtk_widget_set_valign(edit_btn, GTK_ALIGN_CENTER);
                gtk_widget_set_tooltip_text(edit_btn, "Edit provider");
                g_signal_connect(edit_btn, "clicked",
                    G_CALLBACK(edit_btn_clicked), ectx);
                g_object_set_data_full(G_OBJECT(edit_btn), "ectx", ectx,
                    [](gpointer p){ delete static_cast<EditBtnCtx*>(p); });
                adw_action_row_add_suffix(ADW_ACTION_ROW(row), edit_btn);
            }

            // Delete button
            {
                auto* del_ctx = new DelCtx{this, pid};
                auto* del_btn = gtk_button_new_from_icon_name("user-trash-symbolic");
                gtk_widget_add_css_class(del_btn, "destructive-action");
                gtk_widget_add_css_class(del_btn, "flat");
                gtk_widget_set_valign(del_btn, GTK_ALIGN_CENTER);
                adw_action_row_add_suffix(ADW_ACTION_ROW(row), del_btn);
                g_signal_connect(del_btn, "clicked",
                    G_CALLBACK(del_btn_clicked), del_ctx);
            }

            adw_preferences_group_add(providers_group_, GTK_WIDGET(row));
            provider_rows_.push_back(GTK_WIDGET(row));
        }

        if (providers.empty()) {
            auto* empty_row = adw_action_row_new();
            adw_preferences_row_set_title(ADW_PREFERENCES_ROW(empty_row),
                "No providers configured");
            adw_action_row_set_subtitle(ADW_ACTION_ROW(empty_row),
                "Add an API key or OAuth provider above");
            gtk_widget_set_sensitive(GTK_WIDGET(empty_row), FALSE);
            adw_preferences_group_add(providers_group_, GTK_WIDGET(empty_row));
            provider_rows_.push_back(GTK_WIDGET(empty_row));
        }

    } catch (const sdbus::Error& e) {
        spdlog::warn("ListProviders failed: {}", e.what());
    }
}

// ── Add API Key dialog ─────────────────────────────────────────────
void SettingsPage::show_add_api_key_dialog() {
    auto* dlg = adw_dialog_new();
    adw_dialog_set_title(dlg, "Add API Key Provider");
    adw_dialog_set_content_width(dlg, 480);

    auto* tv = adw_toolbar_view_new();
    auto* hb = adw_header_bar_new();

    auto* save_btn = gtk_button_new_with_label("Add");
    gtk_widget_add_css_class(save_btn, "suggested-action");
    adw_header_bar_pack_end(ADW_HEADER_BAR(hb), save_btn);
    adw_toolbar_view_add_top_bar(ADW_TOOLBAR_VIEW(tv), GTK_WIDGET(hb));

    auto* pp    = adw_preferences_page_new();
    auto* group = adw_preferences_group_new();
    adw_preferences_page_add(ADW_PREFERENCES_PAGE(pp),
                              ADW_PREFERENCES_GROUP(group));
    adw_toolbar_view_set_content(ADW_TOOLBAR_VIEW(tv), GTK_WIDGET(pp));
    adw_dialog_set_child(dlg, GTK_WIDGET(tv));

    auto* name_row = adw_entry_row_new();
    adw_preferences_row_set_title(ADW_PREFERENCES_ROW(name_row), "Name");
    adw_preferences_group_add(ADW_PREFERENCES_GROUP(group), GTK_WIDGET(name_row));

    const char* type_strings[] = {"openai", "anthropic", "gemini", nullptr};
    auto* type_list = gtk_string_list_new(type_strings);
    auto* type_row  = adw_combo_row_new();
    adw_preferences_row_set_title(ADW_PREFERENCES_ROW(type_row), "Provider Type");
    adw_combo_row_set_model(ADW_COMBO_ROW(type_row), G_LIST_MODEL(type_list));
    g_object_unref(type_list);
    adw_preferences_group_add(ADW_PREFERENCES_GROUP(group), GTK_WIDGET(type_row));

    auto* key_row = adw_password_entry_row_new();
    adw_preferences_row_set_title(ADW_PREFERENCES_ROW(key_row), "API Key");
    adw_preferences_group_add(ADW_PREFERENCES_GROUP(group), GTK_WIDGET(key_row));

    auto* url_row = adw_entry_row_new();
    adw_preferences_row_set_title(ADW_PREFERENCES_ROW(url_row), "Base URL (optional)");
    adw_preferences_group_add(ADW_PREFERENCES_GROUP(group), GTK_WIDGET(url_row));

    auto* ctx = new ApiKeyCtx{this, dlg,
        ADW_ENTRY_ROW(name_row), ADW_COMBO_ROW(type_row),
        key_row, ADW_ENTRY_ROW(url_row)};

    g_signal_connect(save_btn, "clicked",
        G_CALLBACK(api_save_btn_clicked), ctx);
    g_signal_connect(dlg, "closed",
        G_CALLBACK(api_dialog_closed), ctx);

    adw_dialog_present(dlg, GTK_WIDGET(parent_));
}

// ── Add OAuth dialog ───────────────────────────────────────────────
void SettingsPage::show_add_oauth_dialog() {
    auto* dlg = adw_dialog_new();
    adw_dialog_set_title(dlg, "Add OAuth Provider");
    adw_dialog_set_content_width(dlg, 480);

    auto* tv = adw_toolbar_view_new();
    auto* hb = adw_header_bar_new();

    auto* auth_btn = gtk_button_new_with_label("Authenticate");
    gtk_widget_add_css_class(auth_btn, "suggested-action");
    adw_header_bar_pack_end(ADW_HEADER_BAR(hb), auth_btn);
    adw_toolbar_view_add_top_bar(ADW_TOOLBAR_VIEW(tv), GTK_WIDGET(hb));

    auto* pp         = adw_preferences_page_new();
    auto* input_grp  = adw_preferences_group_new();
    adw_preferences_page_add(ADW_PREFERENCES_PAGE(pp),
                              ADW_PREFERENCES_GROUP(input_grp));

    auto* status_grp = ADW_PREFERENCES_GROUP(adw_preferences_group_new());
    adw_preferences_group_set_title(status_grp, "Authentication Code");
    adw_preferences_page_add(ADW_PREFERENCES_PAGE(pp),
                              ADW_PREFERENCES_GROUP(status_grp));

    adw_toolbar_view_set_content(ADW_TOOLBAR_VIEW(tv), GTK_WIDGET(pp));
    adw_dialog_set_child(dlg, GTK_WIDGET(tv));

    auto* name_row = adw_entry_row_new();
    adw_preferences_row_set_title(ADW_PREFERENCES_ROW(name_row), "Name");
    adw_preferences_group_add(ADW_PREFERENCES_GROUP(input_grp), GTK_WIDGET(name_row));

    const char* type_strings[] = {"copilot", "dashscope", nullptr};
    auto* type_list = gtk_string_list_new(type_strings);
    auto* type_row  = adw_combo_row_new();
    adw_preferences_row_set_title(ADW_PREFERENCES_ROW(type_row), "Provider");
    adw_combo_row_set_model(ADW_COMBO_ROW(type_row), G_LIST_MODEL(type_list));
    g_object_unref(type_list);
    adw_preferences_group_add(ADW_PREFERENCES_GROUP(input_grp), GTK_WIDGET(type_row));

    auto* code_row = adw_action_row_new();
    adw_preferences_row_set_title(ADW_PREFERENCES_ROW(code_row),
        "Waiting for authentication\xe2\x80\xa6");
    gtk_widget_set_sensitive(GTK_WIDGET(code_row), FALSE);
    adw_preferences_group_add(ADW_PREFERENCES_GROUP(status_grp),
        GTK_WIDGET(code_row));

    auto* ctx = new OAuthCtx{this, dlg,
        ADW_ENTRY_ROW(name_row), ADW_COMBO_ROW(type_row),
        ADW_ACTION_ROW(code_row)};

    g_signal_connect(auth_btn, "clicked",
        G_CALLBACK(auth_btn_clicked), ctx);
    g_signal_connect(dlg, "closed",
        G_CALLBACK(oauth_dialog_closed), ctx);

    adw_dialog_present(dlg, GTK_WIDGET(parent_));
}

// ── Models dialog ─────────────────────────────────────────────────

/// Clears list_box and repopulates it from the DB (via GetAllModels).
static void populate_model_rows(GtkListBox* list_box,
                                 sdbus::IProxy* proxy,
                                 SettingsPage* page,
                                 const std::string& provider_id) {
    // Remove all existing rows
    while (GtkWidget* child = gtk_widget_get_first_child(GTK_WIDGET(list_box)))
        gtk_list_box_remove(list_box, child);

    if (!proxy) return;
    try {
        using ModelTuple = sdbus::Struct<std::string, std::string, bool, bool, bool>;
        std::tuple<bool, std::string, std::vector<ModelTuple>> result;
        proxy->callMethod("GetAllModels")
            .onInterface("org.firebox.Control")
            .withArguments(provider_id)
            .storeResultsTo(result);

        auto& [ok, msg, models] = result;
        if (ok && !models.empty()) {
            for (auto& m : models) {
                const auto& mid      = std::get<0>(m);
                bool  menabled       = std::get<2>(m);
                bool  cap_chat       = std::get<3>(m);
                bool  cap_streaming  = std::get<4>(m);

                std::string caps;
                if (cap_chat)      caps += "Chat";
                if (cap_streaming) { if (!caps.empty()) caps += " \xc2\xb7 "; caps += "Streaming"; }

                auto* switch_row = adw_switch_row_new();
                adw_preferences_row_set_title(ADW_PREFERENCES_ROW(switch_row), mid.c_str());
                if (!caps.empty())
                    adw_action_row_set_subtitle(ADW_ACTION_ROW(switch_row), caps.c_str());
                adw_switch_row_set_active(ADW_SWITCH_ROW(switch_row), menabled);

                g_object_set_data_full(G_OBJECT(switch_row), "model-id",
                    g_strdup(mid.c_str()), g_free);

                auto* tctx = new TogCtx{page, provider_id, mid};
                g_object_set_data_full(G_OBJECT(switch_row), "tog-ctx", tctx,
                    [](gpointer p){ delete static_cast<TogCtx*>(p); });

                g_signal_connect(switch_row, "notify::active",
                    G_CALLBACK(+[](AdwSwitchRow* row, GParamSpec*, gpointer ud) {
                        auto* c = static_cast<TogCtx*>(ud);
                        if (!c->page->proxy()) return;
                        bool active = adw_switch_row_get_active(row);
                        try {
                            c->page->proxy()->callMethod("SetModelEnabled")
                                .onInterface("org.firebox.Control")
                                .withArguments(c->provider_id, c->model_id, active);
                        } catch (const sdbus::Error& e) {
                            spdlog::error("SetModelEnabled: {}", e.what());
                        }
                    }), tctx);

                gtk_list_box_append(list_box, GTK_WIDGET(switch_row));
            }
        } else if (ok) {
            auto* empty = adw_action_row_new();
            adw_preferences_row_set_title(ADW_PREFERENCES_ROW(empty),
                "No models — press Refresh to discover from provider API");
            gtk_widget_set_sensitive(GTK_WIDGET(empty), FALSE);
            gtk_list_box_append(list_box, GTK_WIDGET(empty));
        }
    } catch (const sdbus::Error& e) {
        spdlog::error("GetAllModels: {}", e.what());
    }
}

static void refresh_models_btn_clicked(GtkButton* btn, gpointer ud) {
    auto* c = static_cast<RefreshModelsBtnCtx*>(ud);
    if (!c->page->proxy()) return;
    gtk_widget_set_sensitive(GTK_WIDGET(btn), FALSE);
    try {
        c->page->proxy()->callMethod("DiscoverModels")
            .onInterface("org.firebox.Control")
            .withArguments(c->provider_id);
    } catch (const sdbus::Error& e) {
        spdlog::warn("DiscoverModels: {}", e.what());
    }
    populate_model_rows(c->list_box, c->page->proxy(), c->page, c->provider_id);
    gtk_list_box_invalidate_filter(c->list_box);
    gtk_widget_set_sensitive(GTK_WIDGET(btn), TRUE);
}

void SettingsPage::show_models_dialog(const std::string& provider_id,
                                       const std::string& provider_name) {
    auto* dlg = adw_dialog_new();
    adw_dialog_set_title(dlg, ("Models \xe2\x80\x94 " + provider_name).c_str());
    adw_dialog_set_content_width(dlg, 460);
    adw_dialog_set_content_height(dlg, 560);

    auto* tv = adw_toolbar_view_new();
    auto* hb = adw_header_bar_new();

    auto* refresh_btn = gtk_button_new_from_icon_name("view-refresh-symbolic");
    gtk_widget_set_tooltip_text(refresh_btn, "Refresh model list from provider API");
    adw_header_bar_pack_end(ADW_HEADER_BAR(hb), refresh_btn);

    adw_toolbar_view_add_top_bar(ADW_TOOLBAR_VIEW(tv), GTK_WIDGET(hb));

    auto* search_entry = gtk_search_entry_new();
    gtk_widget_set_hexpand(search_entry, TRUE);
    gtk_widget_set_margin_start(search_entry, 12);
    gtk_widget_set_margin_end(search_entry, 12);
    gtk_widget_set_margin_top(search_entry, 4);
    gtk_widget_set_margin_bottom(search_entry, 8);
    adw_toolbar_view_add_top_bar(ADW_TOOLBAR_VIEW(tv), search_entry);

    auto* scroll = gtk_scrolled_window_new();
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scroll),
        GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
    gtk_widget_set_vexpand(scroll, TRUE);

    auto* list_box = gtk_list_box_new();
    gtk_widget_add_css_class(list_box, "boxed-list");
    gtk_list_box_set_selection_mode(GTK_LIST_BOX(list_box), GTK_SELECTION_NONE);
    gtk_widget_set_margin_start(list_box, 12);
    gtk_widget_set_margin_end(list_box, 12);
    gtk_widget_set_margin_top(list_box, 8);
    gtk_widget_set_margin_bottom(list_box, 8);

    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scroll), list_box);
    adw_toolbar_view_set_content(ADW_TOOLBAR_VIEW(tv), scroll);
    adw_dialog_set_child(dlg, GTK_WIDGET(tv));

    gtk_list_box_set_filter_func(GTK_LIST_BOX(list_box),
        +[](GtkListBoxRow* row, gpointer data) -> gboolean {
            const char* text = gtk_editable_get_text(
                GTK_EDITABLE(static_cast<GtkWidget*>(data)));
            if (!text || *text == '\0') return TRUE;
            const char* mid = static_cast<const char*>(
                g_object_get_data(G_OBJECT(row), "model-id"));
            if (!mid) return FALSE;
            gchar* ltext = g_utf8_casefold(text, -1);
            gchar* lmid  = g_utf8_casefold(mid,  -1);
            gboolean match = (strstr(lmid, ltext) != nullptr);
            g_free(ltext);
            g_free(lmid);
            return match;
        }, search_entry, nullptr);

    g_signal_connect_swapped(search_entry, "search-changed",
        G_CALLBACK(gtk_list_box_invalidate_filter), list_box);

    // Wire up the Refresh button
    auto* rctx = new RefreshModelsBtnCtx{this, provider_id,
        GTK_LIST_BOX(list_box), ADW_DIALOG(dlg)};
    g_signal_connect(refresh_btn, "clicked",
        G_CALLBACK(refresh_models_btn_clicked), rctx);
    g_object_set_data_full(G_OBJECT(refresh_btn), "rctx", rctx,
        [](gpointer p){ delete static_cast<RefreshModelsBtnCtx*>(p); });

    // Initial population from DB
    populate_model_rows(GTK_LIST_BOX(list_box), proxy_, this, provider_id);

    adw_dialog_present(dlg, GTK_WIDGET(parent_));
}

// ── Edit Provider dialog ──────────────────────────────────────────
void SettingsPage::show_edit_provider_dialog(const std::string& provider_id,
                                              const std::string& name,
                                              int type_id,
                                              const std::string& base_url,
                                              bool /*enabled*/) {
    auto* dlg = adw_dialog_new();
    adw_dialog_set_title(dlg, "Edit Provider");
    adw_dialog_set_content_width(dlg, 460);

    auto* tv = adw_toolbar_view_new();
    auto* hb = adw_header_bar_new();

    auto* save_btn = gtk_button_new_with_label("Save");
    gtk_widget_add_css_class(save_btn, "suggested-action");
    adw_header_bar_pack_end(ADW_HEADER_BAR(hb), save_btn);
    adw_toolbar_view_add_top_bar(ADW_TOOLBAR_VIEW(tv), GTK_WIDGET(hb));

    auto* pp    = adw_preferences_page_new();
    auto* group = adw_preferences_group_new();
    adw_preferences_page_add(ADW_PREFERENCES_PAGE(pp),
                              ADW_PREFERENCES_GROUP(group));
    adw_toolbar_view_set_content(ADW_TOOLBAR_VIEW(tv), GTK_WIDGET(pp));
    adw_dialog_set_child(dlg, GTK_WIDGET(tv));

    // Name row (always editable)
    auto* name_row = adw_entry_row_new();
    adw_preferences_row_set_title(ADW_PREFERENCES_ROW(name_row), "Name");
    gtk_editable_set_text(GTK_EDITABLE(name_row), name.c_str());
    adw_preferences_group_add(ADW_PREFERENCES_GROUP(group), GTK_WIDGET(name_row));

    // API key providers: show base_url and api_key fields
    AdwEntryRow* url_row  = nullptr;
    GtkWidget*   key_row  = nullptr;
    if (type_id == 1) {
        auto* ur = adw_entry_row_new();
        adw_preferences_row_set_title(ADW_PREFERENCES_ROW(ur), "Base URL");
        gtk_editable_set_text(GTK_EDITABLE(ur), base_url.c_str());
        adw_preferences_group_add(ADW_PREFERENCES_GROUP(group), GTK_WIDGET(ur));
        url_row = ADW_ENTRY_ROW(ur);

        auto* kr = adw_password_entry_row_new();
        adw_preferences_row_set_title(ADW_PREFERENCES_ROW(kr),
            "API Key (leave blank to keep current)");
        adw_preferences_group_add(ADW_PREFERENCES_GROUP(group), kr);
        key_row = GTK_WIDGET(kr);
    }

    auto* ctx = new EditProvCtx{
        this, provider_id, type_id, ADW_DIALOG(dlg),
        ADW_ENTRY_ROW(name_row), url_row, key_row};

    g_signal_connect(save_btn, "clicked",
        G_CALLBACK(edit_prov_save_clicked), ctx);
    g_signal_connect(dlg, "closed",
        G_CALLBACK(edit_prov_dlg_closed), ctx);

    adw_dialog_present(dlg, GTK_WIDGET(parent_));
}

} // namespace firebox::frontend
