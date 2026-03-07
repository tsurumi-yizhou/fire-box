/// @file route_page.cpp
#include "route_page.hpp"
#include <spdlog/spdlog.h>
#include <nlohmann/json.hpp>

namespace firebox::frontend {

// ── Context structs ──────────────────────────────────────────────
struct RouteDialogCtx {
    RoutePage*   page;
    AdwDialog*   dlg;
    AdwEntryRow* id_row;
    AdwEntryRow* name_row;
    AdwComboRow* strategy_row;
    AdwComboRow* provider_row;
    GtkWidget*   models_box;
    GtkWidget*   targets_list;
    std::vector<std::string> provider_ids;
    std::vector<std::pair<std::string, std::string>> targets;
};

// ── Callbacks ────────────────────────────────────────────────────

static void route_dlg_closed(AdwDialog*, gpointer ud) {
    delete static_cast<RouteDialogCtx*>(ud);
}

static void refresh_models_list(RouteDialogCtx* c);

static void provider_changed(AdwComboRow*, GParamSpec*, gpointer ud) {
    auto* c = static_cast<RouteDialogCtx*>(ud);
    refresh_models_list(c);
}

static void refresh_models_list(RouteDialogCtx* c) {
    // Clear old model checkboxes
    GtkWidget* child;
    while ((child = gtk_widget_get_first_child(c->models_box)) != nullptr)
        gtk_box_remove(GTK_BOX(c->models_box), child);

    if (!c->page->proxy()) return;

    int provider_idx = adw_combo_row_get_selected(c->provider_row);
    if (provider_idx >= (int)c->provider_ids.size() || provider_idx < 0) return;

    std::string selected_provider_id = c->provider_ids[provider_idx];

    // Get models for this provider
    try {
        using ModelTuple = sdbus::Struct<std::string, std::string, bool, bool, bool>;
        std::tuple<bool, std::string, std::vector<ModelTuple>> mod_result;
        c->page->proxy()->callMethod("GetAllModels")
            .onInterface("org.firebox.Control")
            .withArguments(selected_provider_id)
            .storeResultsTo(mod_result);

        auto& [mod_ok, mod_msg, models] = mod_result;
        if (!mod_ok) return;

        // Create checkboxes for each enabled model
        for (const auto& m : models) {
            const auto& model_id = std::get<0>(m);
            bool enabled = std::get<2>(m);

            if (!enabled) continue;  // Only show enabled models

            auto* check_btn = gtk_check_button_new_with_label(model_id.c_str());
            gtk_widget_set_margin_start(check_btn, 12);
            gtk_widget_set_margin_end(check_btn, 12);
            gtk_widget_set_margin_top(check_btn, 4);
            gtk_widget_set_margin_bottom(check_btn, 4);

            // Store provider_id and model_id on the button for later retrieval
            g_object_set_data_full(G_OBJECT(check_btn), "provider-id",
                g_strdup(selected_provider_id.c_str()), g_free);
            g_object_set_data_full(G_OBJECT(check_btn), "model-id",
                g_strdup(model_id.c_str()), g_free);

            // When toggled, update targets list
            g_signal_connect(check_btn, "toggled",
                G_CALLBACK(+[](GtkCheckButton* btn, gpointer ud) {
                    auto* c = static_cast<RouteDialogCtx*>(ud);
                    const char* pid = static_cast<const char*>(
                        g_object_get_data(G_OBJECT(btn), "provider-id"));
                    const char* mid = static_cast<const char*>(
                        g_object_get_data(G_OBJECT(btn), "model-id"));
                    if (!pid || !mid) return;

                    if (gtk_check_button_get_active(btn)) {
                        // Add target
                        bool exists = false;
                        for (auto& t : c->targets)
                            if (t.first == pid && t.second == mid) { exists = true; break; }
                        if (!exists)
                            c->targets.push_back({pid, mid});
                    } else {
                        // Remove target
                        for (auto it = c->targets.begin(); it != c->targets.end(); ++it)
                            if (it->first == pid && it->second == mid) {
                                c->targets.erase(it);
                                break;
                            }
                    }

                    // Refresh targets display
                    GtkWidget* child;
                    while ((child = gtk_widget_get_first_child(c->targets_list)) != nullptr)
                        gtk_list_box_remove(GTK_LIST_BOX(c->targets_list), child);

                    if (c->targets.empty()) {
                        auto* empty = adw_action_row_new();
                        adw_preferences_row_set_title(ADW_PREFERENCES_ROW(empty),
                            "No targets selected");
                        gtk_widget_set_sensitive(GTK_WIDGET(empty), FALSE);
                        gtk_list_box_append(GTK_LIST_BOX(c->targets_list),
                            GTK_WIDGET(empty));
                    } else {
                        for (const auto& tgt : c->targets) {
                            auto* row = adw_action_row_new();
                            adw_preferences_row_set_title(ADW_PREFERENCES_ROW(row),
                                tgt.second.c_str());
                            adw_action_row_set_subtitle(ADW_ACTION_ROW(row),
                                tgt.first.c_str());
                            gtk_list_box_append(GTK_LIST_BOX(c->targets_list),
                                GTK_WIDGET(row));
                        }
                    }
                }), c);

            gtk_box_append(GTK_BOX(c->models_box), check_btn);
        }

        if (models.empty()) {
            auto* no_models = gtk_label_new("No enabled models for this provider");
            gtk_widget_add_css_class(no_models, "dim-label");
            gtk_widget_set_margin_top(no_models, 8);
            gtk_box_append(GTK_BOX(c->models_box), no_models);
        }
    } catch (const sdbus::Error& e) {
        spdlog::error("Failed to get models: {}", e.what());
    }
}

static void route_save_clicked(GtkButton*, gpointer ud) {
    auto* c = static_cast<RouteDialogCtx*>(ud);
    if (!c->page->proxy()) { adw_dialog_close(c->dlg); return; }
    if (c->targets.empty()) {
        spdlog::warn("Route must have at least one target");
        return;
    }
    try {
        std::string vid  = gtk_editable_get_text(GTK_EDITABLE(c->id_row));
        std::string name = gtk_editable_get_text(GTK_EDITABLE(c->name_row));
        if (vid.empty()) {
            vid = name;
            for (auto& ch : vid) if (ch == ' ') ch = '-';
        }
        int32_t strategy = adw_combo_row_get_selected(c->strategy_row) == 0 ? 1 : 2;

        // Convert targets vector to D-Bus struct vector
        std::vector<sdbus::Struct<std::string, std::string>> dbus_targets;
        for (const auto& [provider_id, model_id] : c->targets)
            dbus_targets.push_back(sdbus::Struct<std::string, std::string>{provider_id, model_id});

        c->page->proxy()->callMethod("SetRouteRules")
            .onInterface("org.firebox.Control")
            .withArguments(
                vid, name,
                sdbus::Struct<bool,bool,bool,bool,bool>{true,true,false,false,false},
                std::string("{}"),
                dbus_targets,
                strategy);
        c->page->refresh_rules();
    } catch (const sdbus::Error& e) {
        spdlog::error("SetRouteRules failed: {}", e.what());
    }
    adw_dialog_close(c->dlg);
}

// ── Edit button context ───────────────────────────────────────────
struct RouteEditCtx { RoutePage* page; std::string vid; std::string name; };

// ── Page ─────────────────────────────────────────────────────────
RoutePage::RoutePage(sdbus::IProxy* proxy)
    : Gtk::Box(Gtk::Orientation::VERTICAL, 0)
    , proxy_(proxy) {
    setup_ui();
    refresh_rules();
}

void RoutePage::setup_ui() {
    auto* page = adw_preferences_page_new();
    gtk_box_append(GTK_BOX(gobj()), GTK_WIDGET(page));

    group_ = ADW_PREFERENCES_GROUP(adw_preferences_group_new());
    adw_preferences_group_set_title(group_, "Virtual Models");
    adw_preferences_group_set_description(group_,
        "Map virtual model IDs to physical provider/model targets");

    auto* add_btn = gtk_button_new_with_label("+ New Route");
    gtk_widget_add_css_class(add_btn, "flat");
    g_signal_connect(add_btn, "clicked",
        G_CALLBACK(+[](GtkButton*, gpointer ud) {
            static_cast<RoutePage*>(ud)->show_create_dialog();
        }), this);
    adw_preferences_group_set_header_suffix(group_, add_btn);

    adw_preferences_page_add(ADW_PREFERENCES_PAGE(page), group_);
}

void RoutePage::refresh_rules() {
    for (auto* w : rows_)
        adw_preferences_group_remove(group_, w);
    rows_.clear();

    if (!proxy_) return;

    try {
        std::tuple<bool, std::string, std::string> result;
        proxy_->callMethod("ListRouteRules")
            .onInterface("org.firebox.Control")
            .storeResultsTo(result);

        auto& [success, message, rules_json] = result;
        if (!success) return;

        auto rules = nlohmann::json::parse(rules_json, nullptr, false);
        if (rules.is_discarded() || !rules.is_array()) {
            spdlog::warn("Failed to parse route rules JSON: {}", rules_json);
            return;
        }

        if (rules.empty()) {
            auto* empty = adw_action_row_new();
            adw_preferences_row_set_title(ADW_PREFERENCES_ROW(empty),
                "No routes configured");
            adw_action_row_set_subtitle(ADW_ACTION_ROW(empty),
                "Create a new route above");
            gtk_widget_set_sensitive(GTK_WIDGET(empty), FALSE);
            adw_preferences_group_add(group_, GTK_WIDGET(empty));
            rows_.push_back(GTK_WIDGET(empty));
            return;
        }

        for (const auto& rule : rules) {
            std::string vid = rule.value("virtual_model_id", "");
            std::string name = rule.value("display_name", vid);
            int strategy = rule.value("strategy", 1);
            int targets = rule.value("targets_count", 0);

            auto* row = adw_action_row_new();
            adw_preferences_row_set_title(ADW_PREFERENCES_ROW(row), name.c_str());
            adw_action_row_set_subtitle(ADW_ACTION_ROW(row), vid.c_str());

            // Strategy badge
            auto* strat_lbl = gtk_label_new(strategy == 1 ? "Failover" : "Random");
            gtk_widget_add_css_class(strat_lbl, "dim-label");
            gtk_widget_add_css_class(strat_lbl, "caption-heading");
            gtk_widget_set_valign(strat_lbl, GTK_ALIGN_CENTER);
            adw_action_row_add_suffix(ADW_ACTION_ROW(row), strat_lbl);

            // Targets count badge
            auto  tgt_str = std::to_string(targets) + (targets != 1 ? " targets" : " target");
            auto* tgt_lbl = gtk_label_new(tgt_str.c_str());
            gtk_widget_add_css_class(tgt_lbl, targets > 0 ? "success" : "warning");
            gtk_widget_add_css_class(tgt_lbl, "caption-heading");
            gtk_widget_set_valign(tgt_lbl, GTK_ALIGN_CENTER);
            adw_action_row_add_suffix(ADW_ACTION_ROW(row), tgt_lbl);

            // Edit button
            auto* edit_btn = gtk_button_new_from_icon_name("document-edit-symbolic");
            gtk_widget_add_css_class(edit_btn, "flat");
            gtk_widget_set_valign(edit_btn, GTK_ALIGN_CENTER);
            gtk_widget_set_tooltip_text(edit_btn, "Edit route");

            auto* ectx = new RouteEditCtx{this, vid, name};
            g_signal_connect(edit_btn, "clicked",
                G_CALLBACK(+[](GtkButton*, gpointer ud) {
                    auto* c = static_cast<RouteEditCtx*>(ud);
                    c->page->show_create_dialog(c->vid, c->name);
                }), ectx);
            g_object_set_data_full(G_OBJECT(edit_btn), "edit-ctx", ectx,
                [](gpointer p){ delete static_cast<RouteEditCtx*>(p); });

            adw_action_row_add_suffix(ADW_ACTION_ROW(row), edit_btn);
            adw_action_row_set_activatable_widget(ADW_ACTION_ROW(row), edit_btn);

            adw_preferences_group_add(group_, GTK_WIDGET(row));
            rows_.push_back(GTK_WIDGET(row));
        }
    } catch (const sdbus::Error& e) {
        spdlog::warn("ListRouteRules failed: {}", e.what());
    }
}

void RoutePage::show_create_dialog(const std::string& prefill_id,
                                    const std::string& prefill_name) {
    bool is_edit = !prefill_id.empty();
    auto* dlg = adw_dialog_new();
    adw_dialog_set_title(dlg, is_edit ? "Edit Route" : "New Route");
    adw_dialog_set_content_width(dlg, 500);
    adw_dialog_set_content_height(dlg, 700);

    auto* tv = adw_toolbar_view_new();
    auto* hb = adw_header_bar_new();
    auto* save_btn = gtk_button_new_with_label(is_edit ? "Save" : "Create");
    gtk_widget_add_css_class(save_btn, "suggested-action");
    adw_header_bar_pack_end(ADW_HEADER_BAR(hb), save_btn);
    adw_toolbar_view_add_top_bar(ADW_TOOLBAR_VIEW(tv), GTK_WIDGET(hb));

    auto* scroll = gtk_scrolled_window_new();
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scroll),
        GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
    gtk_widget_set_vexpand(scroll, TRUE);

    auto* pp    = adw_preferences_page_new();
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scroll), GTK_WIDGET(pp));
    adw_toolbar_view_set_content(ADW_TOOLBAR_VIEW(tv), scroll);
    adw_dialog_set_child(dlg, GTK_WIDGET(tv));

    // ── Route Settings Group ─────────────────────────────────────
    auto* grp = ADW_PREFERENCES_GROUP(adw_preferences_group_new());
    adw_preferences_group_set_title(grp, "Route Settings");
    adw_preferences_page_add(ADW_PREFERENCES_PAGE(pp), grp);

    auto* id_row = adw_entry_row_new();
    adw_preferences_row_set_title(ADW_PREFERENCES_ROW(id_row),
        "Virtual Model ID");
    if (!prefill_id.empty())
        gtk_editable_set_text(GTK_EDITABLE(id_row), prefill_id.c_str());
    adw_preferences_group_add(grp, GTK_WIDGET(id_row));

    auto* name_row = adw_entry_row_new();
    adw_preferences_row_set_title(ADW_PREFERENCES_ROW(name_row), "Display Name");
    if (!prefill_name.empty())
        gtk_editable_set_text(GTK_EDITABLE(name_row), prefill_name.c_str());
    adw_preferences_group_add(grp, GTK_WIDGET(name_row));

    // ── Strategy Group ───────────────────────────────────────────
    auto* strat_grp = ADW_PREFERENCES_GROUP(adw_preferences_group_new());
    adw_preferences_group_set_title(strat_grp, "Strategy");
    adw_preferences_page_add(ADW_PREFERENCES_PAGE(pp), strat_grp);

    const char* strategies[] = {
        "Failover – try targets in order",
        "Random – load balance across targets",
        nullptr};
    auto* strat_list = gtk_string_list_new(strategies);
    auto* strat_row  = adw_combo_row_new();
    adw_preferences_row_set_title(ADW_PREFERENCES_ROW(strat_row), "Routing Strategy");
    adw_combo_row_set_model(ADW_COMBO_ROW(strat_row), G_LIST_MODEL(strat_list));
    g_object_unref(strat_list);
    adw_preferences_group_add(strat_grp, GTK_WIDGET(strat_row));

    // ── Targets Selection Group ──────────────────────────────────
    auto* targets_grp = ADW_PREFERENCES_GROUP(adw_preferences_group_new());
    adw_preferences_group_set_title(targets_grp, "Targets (Provider/Model pairs)");
    adw_preferences_group_set_description(targets_grp,
        "Select provider and models to route requests to");
    adw_preferences_page_add(ADW_PREFERENCES_PAGE(pp), targets_grp);

    // Provider selector
    auto* provider_list = gtk_string_list_new(nullptr);
    std::vector<std::string> provider_ids;
    if (proxy_) {
        try {
            using ProvTuple = sdbus::Struct<
                std::string, std::string, int32_t, std::string, std::string, bool>;
            std::tuple<bool, std::string, std::vector<ProvTuple>> result;
            proxy_->callMethod("ListProviders")
                .onInterface("org.firebox.Control")
                .storeResultsTo(result);

            auto& [ok, msg, providers] = result;
            if (ok) {
                for (const auto& p : providers) {
                    bool enabled = std::get<5>(p);
                    if (enabled) {
                        gtk_string_list_append(provider_list,
                            std::get<1>(p).c_str());
                        provider_ids.push_back(std::get<0>(p));
                    }
                }
            }
        } catch (const sdbus::Error& e) {
            spdlog::error("ListProviders failed: {}", e.what());
        }
    }
    auto* provider_row = adw_combo_row_new();
    adw_preferences_row_set_title(ADW_PREFERENCES_ROW(provider_row), "Provider");
    adw_combo_row_set_model(ADW_COMBO_ROW(provider_row), G_LIST_MODEL(provider_list));
    g_object_unref(provider_list);
    if (!provider_ids.empty())
        adw_combo_row_set_selected(ADW_COMBO_ROW(provider_row), 0);
    adw_preferences_group_add(targets_grp, GTK_WIDGET(provider_row));

    // Models box (for holding checkboxes)
    auto* models_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_widget_set_margin_start(models_box, 12);
    gtk_widget_set_margin_end(models_box, 12);
    gtk_widget_set_margin_top(models_box, 8);
    gtk_widget_set_margin_bottom(models_box, 8);
    adw_preferences_group_add(targets_grp, models_box);

    // Targets list (showing selected targets)
    auto* targets_list = gtk_list_box_new();
    gtk_widget_add_css_class(targets_list, "boxed-list");
    gtk_list_box_set_selection_mode(GTK_LIST_BOX(targets_list), GTK_SELECTION_NONE);
    gtk_widget_set_margin_start(targets_list, 12);
    gtk_widget_set_margin_end(targets_list, 12);
    gtk_widget_set_margin_top(targets_list, 8);
    gtk_widget_set_margin_bottom(targets_list, 8);
    gtk_widget_set_vexpand(targets_list, FALSE);

    // Initial empty state in targets list
    auto* empty = adw_action_row_new();
    adw_preferences_row_set_title(ADW_PREFERENCES_ROW(empty), "No targets selected");
    gtk_widget_set_sensitive(GTK_WIDGET(empty), FALSE);
    gtk_list_box_append(GTK_LIST_BOX(targets_list), GTK_WIDGET(empty));

    adw_preferences_group_add(targets_grp, targets_list);

    auto* ctx = new RouteDialogCtx{
        this, ADW_DIALOG(dlg),
        ADW_ENTRY_ROW(id_row), ADW_ENTRY_ROW(name_row),
        ADW_COMBO_ROW(strat_row),
        ADW_COMBO_ROW(provider_row),
        models_box,
        targets_list,
        std::move(provider_ids),
        std::vector<std::pair<std::string, std::string>>()
    };

    g_signal_connect(provider_row, "notify::selected",
        G_CALLBACK(provider_changed), ctx);

    // Populate initial models list if there are providers
    refresh_models_list(ctx);

    g_signal_connect(save_btn, "clicked", G_CALLBACK(route_save_clicked), ctx);
    g_signal_connect(dlg, "closed", G_CALLBACK(route_dlg_closed), ctx);

    auto* win = gtk_widget_get_root(GTK_WIDGET(gobj()));
    adw_dialog_present(dlg, GTK_WIDGET(win));
}

} // namespace firebox::frontend
