module;

#include <adwaita.h>
#include <gtk/gtk.h>
#include <string>
#include <vector>

export module routes;
import dbus_client;
import i18n;

export class RoutesView {
public:
    RoutesView() {
        scrolled_ = gtk_scrolled_window_new();
        gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scrolled_),
                                       GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);

        stack_ = gtk_stack_new();
        gtk_stack_set_transition_type(GTK_STACK(stack_),
                                      GTK_STACK_TRANSITION_TYPE_CROSSFADE);
        gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scrolled_), stack_);

        GtkWidget* empty_page = adw_status_page_new();
        adw_status_page_set_icon_name(ADW_STATUS_PAGE(empty_page),
                                      "preferences-other-symbolic");
        adw_status_page_set_title(ADW_STATUS_PAGE(empty_page),
                                  _("No routes configured"));
        adw_status_page_set_description(ADW_STATUS_PAGE(empty_page),
            _("Routes map virtual model names to real providers."));
        gtk_stack_add_named(GTK_STACK(stack_), empty_page, "empty");

        GtkWidget* list_clamp = adw_clamp_new();
        adw_clamp_set_maximum_size(ADW_CLAMP(list_clamp), 600);

        GtkWidget* list_vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
        gtk_widget_set_margin_top(list_vbox, 24);
        gtk_widget_set_margin_bottom(list_vbox, 24);
        gtk_widget_set_margin_start(list_vbox, 12);
        gtk_widget_set_margin_end(list_vbox, 12);
        adw_clamp_set_child(ADW_CLAMP(list_clamp), list_vbox);

        group_ = adw_preferences_group_new();
        adw_preferences_group_set_title(ADW_PREFERENCES_GROUP(group_),
                                        _("Routes"));
        gtk_box_append(GTK_BOX(list_vbox), group_);
        gtk_stack_add_named(GTK_STACK(stack_), list_clamp, "list");

        gtk_stack_set_visible_child_name(GTK_STACK(stack_), "empty");
    }

    GtkWidget* widget() const { return scrolled_; }

    Task refresh(FireBoxDbusClient* client) {
        for (auto* row : rows_) {
            adw_preferences_group_remove(ADW_PREFERENCES_GROUP(group_), row);
        }
        rows_.clear();

        try {
            auto rules = co_await client->get_route_rules();

            if (rules.empty()) {
                gtk_stack_set_visible_child_name(GTK_STACK(stack_), "empty");
                co_return;
            }

            for (const auto& rule : rules) {
                GtkWidget* row = adw_action_row_new();
                adw_preferences_row_set_title(ADW_PREFERENCES_ROW(row),
                                              rule.virtual_model_id.c_str());

                std::string subtitle = rule.display_name + " (" +
                                       rule.strategy + ")";
                adw_action_row_set_subtitle(ADW_ACTION_ROW(row),
                                            subtitle.c_str());

                GtkWidget* del_btn = gtk_button_new_from_icon_name(
                    "user-trash-symbolic");
                gtk_widget_set_valign(del_btn, GTK_ALIGN_CENTER);
                gtk_widget_add_css_class(del_btn, "flat");
                gtk_widget_add_css_class(del_btn, "error");
                gtk_widget_set_tooltip_text(del_btn, _("Delete route"));

                auto* ctx = new DeleteContext{this, client,
                                              rule.virtual_model_id};
                g_object_set_data_full(G_OBJECT(del_btn), "ctx", ctx,
                    [](gpointer p) { delete static_cast<DeleteContext*>(p); });
                g_signal_connect(del_btn, "clicked",
                                 G_CALLBACK(on_delete_clicked), nullptr);

                adw_action_row_add_suffix(ADW_ACTION_ROW(row), del_btn);
                adw_preferences_group_add(ADW_PREFERENCES_GROUP(group_), row);
                rows_.push_back(row);
            }

            gtk_stack_set_visible_child_name(GTK_STACK(stack_), "list");

        } catch (const std::exception& e) {
            gtk_stack_set_visible_child_name(GTK_STACK(stack_), "empty");
        }
    }

    void show_add_dialog(GtkWindow* parent, FireBoxDbusClient* client) {
        GtkWidget* dlg = adw_message_dialog_new(
            parent,
            _("Add Route"),
            _("Map a virtual model name to a real provider and model."));
        adw_message_dialog_add_response(ADW_MESSAGE_DIALOG(dlg), "cancel",
                                        _("Cancel"));
        adw_message_dialog_add_response(ADW_MESSAGE_DIALOG(dlg), "add",
                                        _("Add"));
        adw_message_dialog_set_response_appearance(
            ADW_MESSAGE_DIALOG(dlg), "add", ADW_RESPONSE_SUGGESTED);
        adw_message_dialog_set_default_response(ADW_MESSAGE_DIALOG(dlg), "add");
        adw_message_dialog_set_close_response(ADW_MESSAGE_DIALOG(dlg), "cancel");

        GtkWidget* box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 8);
        gtk_widget_set_margin_start(box, 12);
        gtk_widget_set_margin_end(box, 12);

        GtkWidget* vm_entry = gtk_entry_new();
        gtk_entry_set_placeholder_text(GTK_ENTRY(vm_entry),
                                       _("Virtual model ID (e.g. my-gpt4)"));
        gtk_box_append(GTK_BOX(box), vm_entry);

        GtkWidget* dn_entry = gtk_entry_new();
        gtk_entry_set_placeholder_text(GTK_ENTRY(dn_entry),
                                       _("Display name"));
        gtk_box_append(GTK_BOX(box), dn_entry);

        GtkWidget* strategy_label = gtk_label_new(_("Strategy"));
        gtk_widget_set_halign(strategy_label, GTK_ALIGN_START);
        gtk_widget_add_css_class(strategy_label, "caption");
        gtk_box_append(GTK_BOX(box), strategy_label);

        const char* strategies[] = {"failover", "random", nullptr};
        GtkWidget* strategy_combo = gtk_drop_down_new_from_strings(strategies);
        gtk_box_append(GTK_BOX(box), strategy_combo);

        GtkWidget* pid_entry = gtk_entry_new();
        gtk_entry_set_placeholder_text(GTK_ENTRY(pid_entry),
                                       _("Provider ID"));
        gtk_box_append(GTK_BOX(box), pid_entry);

        GtkWidget* mid_entry = gtk_entry_new();
        gtk_entry_set_placeholder_text(GTK_ENTRY(mid_entry),
                                       _("Model ID (e.g. gpt-4o)"));
        gtk_box_append(GTK_BOX(box), mid_entry);

        adw_message_dialog_set_extra_child(ADW_MESSAGE_DIALOG(dlg), box);

        auto* add_ctx = new AddDialogWidgets{
            vm_entry, dn_entry, strategy_combo, pid_entry, mid_entry,
            this, parent, client};
        g_signal_connect(dlg, "response",
                         G_CALLBACK(on_add_response), add_ctx);
        gtk_window_present(GTK_WINDOW(dlg));
    }

private:
    GtkWidget* scrolled_ = nullptr;
    GtkWidget* stack_    = nullptr;
    GtkWidget* group_    = nullptr;

    std::vector<GtkWidget*> rows_;

    struct DeleteContext {
        RoutesView*        view;
        FireBoxDbusClient* client;
        std::string        route_id;
    };

    static void on_delete_clicked(GtkButton* button, gpointer /*unused*/) {
        auto* ctx = static_cast<DeleteContext*>(
            g_object_get_data(G_OBJECT(button), "ctx"));
        if (!ctx) return;

        GtkRoot* root = gtk_widget_get_root(GTK_WIDGET(button));
        GtkWindow* win = GTK_IS_WINDOW(root) ? GTK_WINDOW(root) : nullptr;

        AdwMessageDialog* dlg = ADW_MESSAGE_DIALOG(
            adw_message_dialog_new(win,
                                   _("Delete Route"),
                                   _("Are you sure you want to delete this route?")));
        adw_message_dialog_add_response(dlg, "cancel", _("Cancel"));
        adw_message_dialog_add_response(dlg, "delete", _("Delete"));
        adw_message_dialog_set_response_appearance(
            dlg, "delete", ADW_RESPONSE_DESTRUCTIVE);
        adw_message_dialog_set_default_response(dlg, "cancel");
        adw_message_dialog_set_close_response(dlg, "cancel");

        auto* confirm_ctx = new DeleteContext{ctx->view, ctx->client,
                                              ctx->route_id};
        g_signal_connect(dlg, "response",
                         G_CALLBACK(on_delete_confirmed), confirm_ctx);
        gtk_window_present(GTK_WINDOW(dlg));
    }

    static Task do_delete(DeleteContext* ctx) {
        try {
            co_await ctx->client->delete_route(ctx->route_id);
            co_await ctx->view->refresh(ctx->client);
        } catch (const std::exception& e) {}
        delete ctx;
    }

    static void on_delete_confirmed(AdwMessageDialog* dlg,
                                    const char* response,
                                    gpointer user_data) {
        auto* ctx = static_cast<DeleteContext*>(user_data);
        if (g_strcmp0(response, "delete") == 0 && ctx) {
            do_delete(ctx);
        } else {
            delete ctx;
        }
        gtk_window_destroy(GTK_WINDOW(dlg));
    }

    struct AddDialogWidgets {
        GtkWidget* vm_entry;
        GtkWidget* dn_entry;
        GtkWidget* strategy_combo;
        GtkWidget* pid_entry;
        GtkWidget* mid_entry;
        RoutesView*        view;
        GtkWindow*         parent;
        FireBoxDbusClient* client;
    };

    static Task do_add_route(AddDialogWidgets* w) {
        std::string vm_id    = gtk_editable_get_text(GTK_EDITABLE(w->vm_entry));
        std::string dn       = gtk_editable_get_text(GTK_EDITABLE(w->dn_entry));
        guint strat_idx      = gtk_drop_down_get_selected(GTK_DROP_DOWN(w->strategy_combo));
        std::string pid      = gtk_editable_get_text(GTK_EDITABLE(w->pid_entry));
        std::string mid      = gtk_editable_get_text(GTK_EDITABLE(w->mid_entry));

        const char* strat_slugs[] = {"failover", "random"};
        std::string strategy = (strat_idx < 2) ? strat_slugs[strat_idx]
                                               : "failover";

        try {
            co_await w->client->set_route_rules(vm_id, dn, strategy, pid, mid);
            co_await w->view->refresh(w->client);
        } catch (const std::exception& e) {}
        
        delete w;
    }

    static void on_add_response(AdwMessageDialog* dlg,
                                const char* response,
                                gpointer user_data) {
        auto* w = static_cast<AddDialogWidgets*>(user_data);
        if (g_strcmp0(response, "add") == 0 && w) {
            do_add_route(w);
        } else {
            delete w;
        }
        gtk_window_destroy(GTK_WINDOW(dlg));
    }
};