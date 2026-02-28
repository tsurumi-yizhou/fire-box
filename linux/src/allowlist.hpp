#pragma once

#include "dbus_client.hpp"

#include <adwaita.h>
#include <gtk/gtk.h>
#include <libintl.h>

#include <string>
#include <vector>

#define _(S) gettext(S)

/// Allowlist view — lists applications that have been granted access to Fire Box.
///
/// GtkStack pages:
///   - "empty" : Adw.StatusPage placeholder
///   - "list"  : Adw.PreferencesGroup with one Adw.ActionRow per entry
class AllowlistView {
public:
    AllowlistView() {
        scrolled_ = gtk_scrolled_window_new();
        gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scrolled_),
                                       GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);

        stack_ = gtk_stack_new();
        gtk_stack_set_transition_type(GTK_STACK(stack_),
                                      GTK_STACK_TRANSITION_TYPE_CROSSFADE);
        gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scrolled_), stack_);

        // ---- empty state --------------------------------------------------
        GtkWidget* empty_page = adw_status_page_new();
        adw_status_page_set_icon_name(ADW_STATUS_PAGE(empty_page),
                                      "security-high-symbolic");
        adw_status_page_set_title(ADW_STATUS_PAGE(empty_page),
                                  _("No applications allowed"));
        adw_status_page_set_description(ADW_STATUS_PAGE(empty_page),
            _("Applications will appear here once they are granted access."));
        gtk_stack_add_named(GTK_STACK(stack_), empty_page, "empty");

        // ---- list state ---------------------------------------------------
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
                                        _("Allowed Applications"));
        gtk_box_append(GTK_BOX(list_vbox), group_);
        gtk_stack_add_named(GTK_STACK(stack_), list_clamp, "list");

        gtk_stack_set_visible_child_name(GTK_STACK(stack_), "empty");
    }

    GtkWidget* widget() const { return scrolled_; }

    /// Refresh the allowlist from the service.
    void refresh(FireBoxDbusClient& client) {
        for (auto* row : rows_) {
            adw_preferences_group_remove(ADW_PREFERENCES_GROUP(group_), row);
        }
        rows_.clear();

        try {
            auto entries = client.get_allowlist();

            if (entries.empty()) {
                gtk_stack_set_visible_child_name(GTK_STACK(stack_), "empty");
                return;
            }

            for (const auto& entry : entries) {
                GtkWidget* row = adw_action_row_new();
                adw_preferences_row_set_title(ADW_PREFERENCES_ROW(row),
                                              entry.display_name.c_str());
                adw_action_row_set_subtitle(ADW_ACTION_ROW(row),
                                            entry.app_path.c_str());

                // "Revoke" button suffix
                GtkWidget* revoke_btn = gtk_button_new_with_label(_("Revoke"));
                gtk_widget_set_valign(revoke_btn, GTK_ALIGN_CENTER);
                gtk_widget_add_css_class(revoke_btn, "destructive-action");
                gtk_widget_set_tooltip_text(revoke_btn,
                                            _("Revoke access for this application"));

                auto* ctx = new RevokeContext{this, &client, entry.app_path};
                g_object_set_data_full(G_OBJECT(revoke_btn), "ctx", ctx,
                    [](gpointer p) { delete static_cast<RevokeContext*>(p); });
                g_signal_connect(revoke_btn, "clicked",
                                 G_CALLBACK(on_revoke_clicked), nullptr);

                adw_action_row_add_suffix(ADW_ACTION_ROW(row), revoke_btn);
                adw_preferences_group_add(ADW_PREFERENCES_GROUP(group_), row);
                rows_.push_back(row);
            }

            gtk_stack_set_visible_child_name(GTK_STACK(stack_), "list");

        } catch (const std::exception& e) {
            gtk_stack_set_visible_child_name(GTK_STACK(stack_), "empty");
        }
    }

private:
    GtkWidget* scrolled_ = nullptr;
    GtkWidget* stack_    = nullptr;
    GtkWidget* group_    = nullptr;

    std::vector<GtkWidget*> rows_;

    struct RevokeContext {
        AllowlistView*     view;
        FireBoxDbusClient* client;
        std::string        app_path;
    };

    static void on_revoke_clicked(GtkButton* button, gpointer /*unused*/) {
        auto* ctx = static_cast<RevokeContext*>(
            g_object_get_data(G_OBJECT(button), "ctx"));
        if (!ctx) return;

        GtkRoot* root = gtk_widget_get_root(GTK_WIDGET(button));
        GtkWindow* win = GTK_IS_WINDOW(root) ? GTK_WINDOW(root) : nullptr;

        AdwMessageDialog* dlg = ADW_MESSAGE_DIALOG(
            adw_message_dialog_new(win,
                                   _("Revoke Access"),
                                   _("Are you sure you want to revoke access "
                                     "for this application? It will need to "
                                     "request permission again.")));
        adw_message_dialog_add_response(dlg, "cancel", _("Cancel"));
        adw_message_dialog_add_response(dlg, "revoke", _("Revoke"));
        adw_message_dialog_set_response_appearance(
            dlg, "revoke", ADW_RESPONSE_DESTRUCTIVE);
        adw_message_dialog_set_default_response(dlg, "cancel");
        adw_message_dialog_set_close_response(dlg, "cancel");

        auto* confirm_ctx = new RevokeContext{ctx->view, ctx->client,
                                              ctx->app_path};
        g_signal_connect(dlg, "response",
                         G_CALLBACK(on_revoke_confirmed), confirm_ctx);
        gtk_window_present(GTK_WINDOW(dlg));
    }

    static void on_revoke_confirmed(AdwMessageDialog* dlg,
                                    const char* response,
                                    gpointer user_data) {
        auto* ctx = static_cast<RevokeContext*>(user_data);
        if (g_strcmp0(response, "revoke") == 0 && ctx) {
            try {
                ctx->client->remove_from_allowlist(ctx->app_path);
                ctx->view->refresh(*ctx->client);
            } catch (const std::exception& e) {
                show_error(GTK_WINDOW(dlg), e.what());
            }
        }
        delete ctx;
        gtk_window_destroy(GTK_WINDOW(dlg));
    }

    static void show_error(GtkWindow* parent, const char* message) {
        GtkWidget* err_dlg = adw_message_dialog_new(
            parent, _("Error"), message);
        adw_message_dialog_add_response(ADW_MESSAGE_DIALOG(err_dlg), "ok",
                                        _("OK"));
        adw_message_dialog_set_default_response(ADW_MESSAGE_DIALOG(err_dlg),
                                                "ok");
        adw_message_dialog_set_close_response(ADW_MESSAGE_DIALOG(err_dlg),
                                              "ok");
        gtk_window_present(GTK_WINDOW(err_dlg));
    }
};

#undef _
